use crate::cores::{
    as_obj_or_empty, extract_protocol_users, safe_sni, str_or_empty, validate_inbound, CoreUser,
};
use mypanel_proto::links::js_hash_code;
use mypanel_proto::{InboundConfig, Protocol};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

/// Maximum buffered bytes while peeking a TLS `ClientHello` or HTTP request header (`16 KiB`).
pub const MAX_HELLO_BYTES: usize = 16 * 1024;
/// Maximum inspection timeout before falling back to the default backend (`5s`).
pub const INSPECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Maximum concurrent SNI inspection handshakes per port-share listener (`4096`).
pub const MAX_CONCURRENT_CONNECTIONS: usize = 4096;
/// Upper bound for connecting to the loopback backend core.
const UPSTREAM_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// TCP keepalive on proxied sockets: reaps half-open peers so they cannot hold permits forever.
const KEEPALIVE_IDLE: Duration = Duration::from_secs(60);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(15);

fn enable_keepalive(stream: &TcpStream) {
    let ka = socket2::TcpKeepalive::new()
        .with_time(KEEPALIVE_IDLE)
        .with_interval(KEEPALIVE_INTERVAL);
    let _ = socket2::SockRef::from(stream).set_tcp_keepalive(&ka);
    let _ = stream.set_nodelay(true);
}

/// Loopback base port for port-shared backend inbounds (``: `11000`, never collides with `XRAY_API_PORT = 10085`).
pub const BASE_PORT: u16 = 11000;
/// Port allocation span above [`BASE_PORT`] (`11000..16000`).
pub const PORT_SPAN: u16 = 5000;
/// Legacy per-protocol hash span (`90`) used as the primary deterministic slot before linear collision probing.
const HASH_SPAN: u32 = 90;

/// Single SNI / Host / Path route target mapped to a local loopback backend port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareMapping {
    pub share_id: String,
    pub public_port: u16,
    pub protocol: Protocol,
    pub tag: String,
    pub host: Option<String>,
    pub path: Option<String>,
    pub sni: String,
    pub backend_port: u16,
    pub upstream: String,
}

/// Complete port-sharing route table for a single public TCP port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortShareGroup {
    pub public_port: u16,
    pub listen: String,
    pub default_backend_port: u16,
    pub mappings: Vec<ShareMapping>,
}

/// Result of analyzing and remapping [`InboundConfig`] entries for SNI/HTTP port-sharing.
#[derive(Debug, Clone)]
pub struct RemappedConfig {
    pub inbounds: Vec<InboundConfig>,
    pub groups: Vec<PortShareGroup>,
}

fn protocol_offset(proto: Protocol) -> u16 {
    match proto {
        Protocol::Vless => 0,
        Protocol::Vmess => 100,
        Protocol::Trojan => 200,
        Protocol::Shadowsocks => 300,
        Protocol::Hysteria2 => 400,
        Protocol::Naiveproxy => 500,
        Protocol::Mieru => 600,
        Protocol::Tuic => 700,
        Protocol::Anytls => 800,
        Protocol::Shadowtls => 900,
        Protocol::Tgweb => 1000,
    }
}

/// Assigns a deterministic loopback backend port in `BASE_PORT..BASE_PORT + PORT_SPAN` (`11000..16000`)
/// using linear collision probing against `used_ports`.
pub fn assign_backend_port(share_id: &str, used_ports: &mut HashSet<u16>) -> u16 {
    assign_backend_port_with_proto(None, share_id, used_ports)
}

/// Assigns a deterministic loopback backend port honoring protocol offset when provided,
/// with linear collision probing inside `BASE_PORT..BASE_PORT + PORT_SPAN`.
pub fn assign_backend_port_with_proto(
    protocol: Option<Protocol>,
    key: &str,
    used_ports: &mut HashSet<u16>,
) -> u16 {
    let hash = js_hash_code(key);
    let initial_offset = match protocol {
        Some(p) => {
            let base_off = u32::from(protocol_offset(p));
            (base_off + (hash % HASH_SPAN)) % u32::from(PORT_SPAN)
        }
        None => hash % u32::from(PORT_SPAN),
    } as u16;

    for step in 0..PORT_SPAN {
        let candidate = BASE_PORT + ((initial_offset + step) % PORT_SPAN);
        if used_ports.insert(candidate) {
            return candidate;
        }
    }
    BASE_PORT
}

/// Analyzes enabled port-shares on all inbounds and remaps parent + child share inbounds
/// to collision-free loopback ports (`127.0.0.1:<backend_port>`), returning both the remapped
/// [`InboundConfig`] list and the [`PortShareGroup`] table for the native Tokio SNI router.
pub fn remap_inbounds(inbounds: &[InboundConfig]) -> Result<RemappedConfig, String> {
    let mut used_ports: HashSet<u16> = inbounds
        .iter()
        .filter(|i| i.enable && i.port > 0)
        .map(|i| i.port)
        .collect();
    // Never allocate the Xray gRPC port even if an someone lowers BASE_PORT
    used_ports.insert(crate::config::DEFAULT_XRAY_API_PORT);

    let mut groups_by_port: HashMap<u16, PortShareGroup> = HashMap::new();
    let mut remapped_inbounds: Vec<InboundConfig> = Vec::new();
    let mut synthesized_shares: Vec<InboundConfig> = Vec::new();

    for inbound in inbounds {
        if !inbound.enable {
            remapped_inbounds.push(inbound.clone());
            continue;
        }

        let enabled_shares: Vec<_> = inbound
            .port_shares
            .iter()
            .filter(|ps| ps.enable && !matches!(ps.protocol, Protocol::Tuic | Protocol::Mieru | Protocol::Tgweb))
            .collect();

        if enabled_shares.is_empty() {
            remapped_inbounds.push(inbound.clone());
            continue;
        }

        // Allocate a loopback backend port for the parent inbound itself so the public port
        // can be bound by the Tokio SNI multiplexer without EADDRINUSE.
        let parent_key = format!("parent-{}-{}", inbound.id, inbound.tag);
        let parent_backend_port =
            assign_backend_port_with_proto(Some(inbound.protocol), &parent_key, &mut used_ports);

        let mut parent_remapped = inbound.clone();
        parent_remapped.listen = "127.0.0.1".to_string();
        parent_remapped.port = parent_backend_port;
        parent_remapped.port_shares.clear();

        let parent_sni = extract_inbound_sni(inbound);
        let listen_addr = if inbound.listen.trim().is_empty() {
            "0.0.0.0".to_string()
        } else {
            inbound.listen.clone()
        };

        let group = groups_by_port
            .entry(inbound.port)
            .or_insert_with(|| PortShareGroup {
                public_port: inbound.port,
                listen: listen_addr,
                default_backend_port: parent_backend_port,
                mappings: Vec::new(),
            });

        if !parent_sni.is_empty() {
            let _ = safe_sni(&parent_sni)?;
            group.mappings.push(ShareMapping {
                share_id: inbound.id.clone(),
                public_port: inbound.port,
                protocol: inbound.protocol,
                tag: inbound.tag.clone(),
                host: Some(parent_sni.clone()),
                path: extract_inbound_path(inbound),
                sni: parent_sni,
                backend_port: parent_backend_port,
                upstream: format!("127.0.0.1:{parent_backend_port}"),
            });
        }

        remapped_inbounds.push(parent_remapped);

        for ps in enabled_shares {
            let key = if !ps.tag.trim().is_empty() {
                ps.tag.as_str()
            } else {
                ps.id.as_str()
            };
            let share_port =
                assign_backend_port_with_proto(Some(ps.protocol), key, &mut used_ports);
            let sni = ps
                .host
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    ps.settings
                        .get("sni")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                })
                .or_else(|| {
                    ps.stream
                        .get("sni")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                })
                .unwrap_or("")
                .to_string();

            if !sni.is_empty() {
                let _ = safe_sni(&sni)?;
            }

            let path = ps
                .path
                .as_deref()
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(ToString::to_string);

            group.mappings.push(ShareMapping {
                share_id: ps.id.clone(),
                public_port: inbound.port,
                protocol: ps.protocol,
                tag: ps.tag.clone(),
                host: ps.host.clone(),
                path: path.clone(),
                sni: sni.clone(),
                backend_port: share_port,
                upstream: format!("127.0.0.1:{share_port}"),
            });

            let child_inbound = materialize_share_inbound(inbound, ps, share_port, &sni, path.as_deref())?;
            validate_inbound(&child_inbound)?;
            synthesized_shares.push(child_inbound);
        }
    }

    remapped_inbounds.extend(synthesized_shares);
    let mut groups: Vec<PortShareGroup> = groups_by_port.into_values().collect();
    groups.sort_by_key(|g| g.public_port);

    Ok(RemappedConfig {
        inbounds: remapped_inbounds,
        groups,
    })
}

fn extract_inbound_sni(inbound: &InboundConfig) -> String {
    let s1 = str_or_empty(&inbound.stream, "sni");
    if !s1.is_empty() {
        return s1.to_string();
    }
    if let Some(tls) = inbound.stream.get("tlsSettings") {
        let s2 = str_or_empty(tls, "serverName");
        if !s2.is_empty() {
            return s2.to_string();
        }
    }
    if let Some(reality) = inbound.stream.get("realitySettings") {
        if let Some(arr) = reality.get("serverNames").and_then(Value::as_array) {
            if let Some(first) = arr.iter().filter_map(Value::as_str).map(str::trim).find(|s| !s.is_empty()) {
                return first.to_string();
            }
        }
    }
    str_or_empty(&inbound.settings, "sni").to_string()
}

fn extract_inbound_path(inbound: &InboundConfig) -> Option<String> {
    let net = inbound
        .stream
        .get("network")
        .and_then(Value::as_str)
        .unwrap_or("tcp");
    let p = match net {
        "ws" => inbound
            .stream
            .get("wsSettings")
            .and_then(|w| w.get("path"))
            .and_then(Value::as_str),
        "xhttp" => inbound
            .stream
            .get("xhttpSettings")
            .and_then(|x| x.get("path"))
            .and_then(Value::as_str),
        "httpupgrade" => inbound
            .stream
            .get("httpupgradeSettings")
            .and_then(|h| h.get("path"))
            .and_then(Value::as_str),
        "grpc" => inbound
            .stream
            .get("grpcSettings")
            .and_then(|g| g.get("serviceName"))
            .and_then(Value::as_str),
        _ => inbound.stream.get("path").and_then(Value::as_str),
    };
    p.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

fn materialize_share_inbound(
    parent: &InboundConfig,
    ps: &mypanel_proto::PortShareConfig,
    backend_port: u16,
    sni: &str,
    path: Option<&str>,
) -> Result<InboundConfig, String> {
    let mut settings = as_obj_or_empty(&parent.settings);
    for (k, v) in as_obj_or_empty(&ps.settings) {
        settings.insert(k, v);
    }

    let users = if !ps.users.is_empty() {
        ps.users.clone()
    } else if !parent.users.is_empty() {
        parent.users.clone()
    } else {
        let extracted: Vec<CoreUser> = extract_protocol_users(&[], &Value::Object(settings.clone()), false)?;
        extracted
            .into_iter()
            .map(|u| mypanel_proto::NodeUser {
                id: u.id.clone(),
                username: u.username,
                uuid: u.id,
                email: u.email,
                name: u.name,
                password: if u.password.is_empty() {
                    None
                } else {
                    Some(u.password.clone())
                },
                auth: if u.password.is_empty() {
                    None
                } else {
                    Some(u.password)
                },
                flow: if u.flow.is_empty() { None } else { Some(u.flow) },
                level: u.level,
            })
            .collect()
    };

    let mut stream = as_obj_or_empty(&parent.stream);
    for (k, v) in as_obj_or_empty(&ps.stream) {
        stream.insert(k, v);
    }
    if !sni.is_empty() {
        stream.insert("sni".into(), json!(sni));
    }
    if let Some(p) = path.filter(|p| !p.is_empty()) {
        let net = stream
            .get("network")
            .and_then(Value::as_str)
            .unwrap_or("tcp")
            .to_string();
        match net.as_str() {
            "grpc" => {
                let mut g = stream
                    .get("grpcSettings")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                g.insert("serviceName".into(), json!(p));
                stream.insert("grpcSettings".into(), Value::Object(g));
            }
            "xhttp" => {
                let mut x = stream
                    .get("xhttpSettings")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                x.insert("path".into(), json!(p));
                stream.insert("xhttpSettings".into(), Value::Object(x));
            }
            "httpupgrade" => {
                let mut h = stream
                    .get("httpupgradeSettings")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                h.insert("path".into(), json!(p));
                stream.insert("httpupgradeSettings".into(), Value::Object(h));
            }
            "h2" => {
                let mut h = stream
                    .get("httpSettings")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                h.insert("path".into(), json!(p));
                stream.insert("httpSettings".into(), Value::Object(h));
            }
            _ => {
                let mut w = stream
                    .get("wsSettings")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                w.insert("path".into(), json!(p));
                stream.insert("wsSettings".into(), Value::Object(w));
            }
        }
    }

    Ok(InboundConfig {
        id: format!("ps-{}", ps.id),
        tag: ps.tag.clone(),
        protocol: ps.protocol,
        listen: "127.0.0.1".to_string(),
        port: backend_port,
        settings: Value::Object(settings),
        stream: Value::Object(stream),
        routing: Value::Object(Map::new()),
        sniffing: parent.sniffing,
        remark: Some(format!("PortShare {}", ps.tag)),
        enable: true,
        users,
        port_shares: Vec::new(),
        hosts: Vec::new(),
    })
}

/// TLS `ClientHello` / HTTP header inspection metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientInspection {
    pub sni: Option<String>,
    pub alpn: Option<String>,
    pub http_host: Option<String>,
    pub http_path: Option<String>,
}

/// Outcome of peeking initial client bytes: `Complete`, `NeedMore` (truncated record), or `NotTlsOrHttp`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseStatus {
    Complete(ClientInspection),
    NeedMore,
    NotTlsOrHttp,
}

/// Parses a TLS `ClientHello` record to extract SNI (`0x0000`) and ALPN (`0x0010`), or falls back
/// to parsing an HTTP request's `Host` header and request `path`.
pub fn inspect_client_bytes(buf: &[u8]) -> ParseStatus {
    if buf.is_empty() {
        return ParseStatus::NeedMore;
    }
    if buf[0] == 0x16 {
        return parse_tls_client_hello(buf);
    }
    if is_http_method_prefix(buf) {
        return parse_http_header(buf);
    }
    ParseStatus::NotTlsOrHttp
}

/// Extracts the SNI server_name from a TLS `ClientHello` byte buffer (`None` if not TLS or missing SNI).
pub fn parse_client_hello_sni(buf: &[u8]) -> Option<String> {
    match parse_tls_client_hello(buf) {
        ParseStatus::Complete(info) => info.sni,
        _ => None,
    }
}

fn parse_tls_client_hello(buf: &[u8]) -> ParseStatus {
    if buf.len() < 5 {
        return ParseStatus::NeedMore;
    }
    if buf[0] != 0x16 || buf[1] != 0x03 || buf[2] > 0x04 {
        return ParseStatus::NotTlsOrHttp;
    }
    let record_len = u16::from_be_bytes([buf[3], buf[4]]) as usize;
    if record_len == 0 || record_len > MAX_HELLO_BYTES {
        return ParseStatus::NotTlsOrHttp;
    }
    if buf.len() < 5 + record_len {
        return ParseStatus::NeedMore;
    }

    let payload = &buf[5..5 + record_len];
    if payload.len() < 4 {
        return ParseStatus::NotTlsOrHttp;
    }
    // HandshakeType == ClientHello (1)
    if payload[0] != 0x01 {
        return ParseStatus::NotTlsOrHttp;
    }
    let hs_len =
        ((payload[1] as usize) << 16) | ((payload[2] as usize) << 8) | (payload[3] as usize);
    if payload.len() < 4 + hs_len {
        return ParseStatus::NeedMore;
    }

    let body = &payload[4..4 + hs_len];
    // client_version (2) + random (32) = 34 bytes
    if body.len() < 35 {
        return ParseStatus::NotTlsOrHttp;
    }
    let mut pos = 34;

    // session_id
    let sid_len = body[pos] as usize;
    pos += 1;
    if pos + sid_len + 2 > body.len() {
        return ParseStatus::NotTlsOrHttp;
    }
    pos += sid_len;

    // cipher_suites
    let cs_len = u16::from_be_bytes([body[pos], body[pos + 1]]) as usize;
    pos += 2;
    if pos + cs_len + 1 > body.len() {
        return ParseStatus::NotTlsOrHttp;
    }
    pos += cs_len;

    // compression_methods
    let comp_len = body[pos] as usize;
    pos += 1;
    if pos + comp_len > body.len() {
        return ParseStatus::NotTlsOrHttp;
    }
    pos += comp_len;

    if pos == body.len() {
        return ParseStatus::Complete(ClientInspection::default());
    }
    if pos + 2 > body.len() {
        return ParseStatus::NotTlsOrHttp;
    }
    let ext_total_len = u16::from_be_bytes([body[pos], body[pos + 1]]) as usize;
    pos += 2;
    if pos + ext_total_len > body.len() {
        return ParseStatus::NotTlsOrHttp;
    }

    let ext_end = pos + ext_total_len;
    let mut info = ClientInspection::default();

    while pos + 4 <= ext_end {
        let ext_type = u16::from_be_bytes([body[pos], body[pos + 1]]);
        let ext_len = u16::from_be_bytes([body[pos + 2], body[pos + 3]]) as usize;
        pos += 4;
        if pos + ext_len > ext_end {
            return ParseStatus::NotTlsOrHttp;
        }
        let ext_data = &body[pos..pos + ext_len];
        pos += ext_len;

        if ext_type == 0x0000 && ext_data.len() >= 2 {
            // server_name extension
            let list_len = u16::from_be_bytes([ext_data[0], ext_data[1]]) as usize;
            let mut lpos = 2;
            let lend = (2 + list_len).min(ext_data.len());
            while lpos + 3 <= lend {
                let name_type = ext_data[lpos];
                let name_len = u16::from_be_bytes([ext_data[lpos + 1], ext_data[lpos + 2]]) as usize;
                lpos += 3;
                if lpos + name_len > lend {
                    break;
                }
                if name_type == 0 {
                    if let Ok(host) = std::str::from_utf8(&ext_data[lpos..lpos + name_len]) {
                        let clean = host.trim().to_ascii_lowercase();
                        if !clean.is_empty() && safe_sni(&clean).is_ok() {
                            info.sni = Some(clean);
                            break;
                        }
                    }
                }
                lpos += name_len;
            }
        } else if ext_type == 0x0010 && ext_data.len() >= 3 {
            // ALPN extension
            let proto_len = ext_data[2] as usize;
            if 3 + proto_len <= ext_data.len() {
                if let Ok(alpn) = std::str::from_utf8(&ext_data[3..3 + proto_len]) {
                    info.alpn = Some(alpn.to_string());
                }
            }
        }
    }

    ParseStatus::Complete(info)
}

fn is_http_method_prefix(buf: &[u8]) -> bool {
    const METHODS: &[&[u8]] = &[
        b"GET ",
        b"POST ",
        b"PUT ",
        b"HEAD ",
        b"OPTIONS ",
        b"DELETE ",
        b"PATCH ",
        b"CONNECT ",
        b"PRI ",
    ];
    for m in METHODS {
        let cmp_len = buf.len().min(m.len());
        if buf[..cmp_len].eq_ignore_ascii_case(&m[..cmp_len]) {
            return true;
        }
    }
    false
}

fn parse_http_header(buf: &[u8]) -> ParseStatus {
    let Some(end_idx) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
        if buf.len() >= MAX_HELLO_BYTES {
            return ParseStatus::NotTlsOrHttp;
        }
        return ParseStatus::NeedMore;
    };
    let Ok(header_str) = std::str::from_utf8(&buf[..end_idx]) else {
        return ParseStatus::NotTlsOrHttp;
    };
    let mut lines = header_str.split("\r\n");
    let Some(req_line) = lines.next() else {
        return ParseStatus::NotTlsOrHttp;
    };
    let mut parts = req_line.split_whitespace();
    let _method = parts.next();
    let http_path = parts.next().map(str::to_string);

    let mut http_host = None;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("host") {
                let raw_host = v.trim();
                let host_only = raw_host
                    .split_once(':')
                    .map(|(h, _)| h)
                    .unwrap_or(raw_host)
                    .trim()
                    .to_ascii_lowercase();
                if !host_only.is_empty() {
                    http_host = Some(host_only);
                }
                break;
            }
        }
    }

    ParseStatus::Complete(ClientInspection {
        sni: None,
        alpn: None,
        http_host,
        http_path,
    })
}

/// Selects the target loopback backend port from a [`PortShareGroup`] given a [`ClientInspection`].
///
/// Matching order:
/// 1. Exact SNI (or HTTP `Host`) + Path prefix match
/// 2. Exact SNI (or HTTP `Host`) match
/// 3. Wildcard `*.domain` SNI (or HTTP `Host`) match
/// 4. HTTP Path prefix match
/// 5. Group `default_backend_port`
pub fn select_backend_port(group: &PortShareGroup, inspection: &ClientInspection) -> u16 {
    let target_host = inspection
        .sni
        .as_deref()
        .or(inspection.http_host.as_deref())
        .map(|h| h.trim().to_ascii_lowercase());
    let target_path = inspection.http_path.as_deref();

    // 1. Exact host + path prefix
    if let (Some(ref host), Some(path)) = (&target_host, target_path) {
        for m in &group.mappings {
            if !m.sni.is_empty() && m.sni.eq_ignore_ascii_case(host) {
                if let Some(ref mp) = m.path {
                    if path.starts_with(mp.as_str()) {
                        return m.backend_port;
                    }
                }
            }
        }
    }

    // 2. Exact host match
    if let Some(ref host) = target_host {
        for m in &group.mappings {
            if !m.sni.is_empty() && !m.sni.starts_with("*.") && m.sni.eq_ignore_ascii_case(host) {
                return m.backend_port;
            }
        }
        // 3. Wildcard `*.suffix` match
        for m in &group.mappings {
            if let Some(suffix) = m.sni.strip_prefix("*.") {
                let suffix_lower = suffix.to_ascii_lowercase();
                if host.ends_with(&format!(".{suffix_lower}")) && host.len() > suffix_lower.len() + 1 {
                    return m.backend_port;
                }
            }
        }
    }

    // 4. HTTP path prefix fallback
    if let Some(path) = target_path {
        for m in &group.mappings {
            if let Some(ref mp) = m.path {
                if !mp.is_empty() && path.starts_with(mp.as_str()) {
                    return m.backend_port;
                }
            }
        }
    }

    group.default_backend_port
}

/// Runtime manager for active Tokio SNI/HTTP port-share listeners.
pub struct PortShareRuntime {
    cancel: Option<CancellationToken>,
    handles: Vec<tokio::task::JoinHandle<()>>,
}

impl Default for PortShareRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl PortShareRuntime {
    pub fn new() -> Self {
        Self {
            cancel: None,
            handles: Vec::new(),
        }
    }

    /// Stops all active port-share TCP listeners and restarts new ones for `groups`.
    pub async fn apply(&mut self, groups: &[PortShareGroup]) -> Result<(), String> {
        self.stop().await;
        if groups.is_empty() {
            return Ok(());
        }

        let token = CancellationToken::new();
        let mut handles = Vec::with_capacity(groups.len());

        for group in groups.iter().cloned() {
            let bind_addr = format!("{}:{}", group.listen, group.public_port);
            let listener = TcpListener::bind(&bind_addr)
                .await
                .map_err(|e| format!("Failed to bind SNI router on {bind_addr}: {e}"))?;
            let child_token = token.child_token();
            let sem = Arc::new(Semaphore::new(MAX_CONCURRENT_CONNECTIONS));
            let group_arc = Arc::new(group);

            let handle = tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = child_token.cancelled() => break,
                        res = listener.accept() => {
                            let Ok((client, peer)) = res else {
                                continue;
                            };
                            let Ok(permit) = sem.clone().try_acquire_owned() else {
                                tracing::warn!(peer = %peer, "PortShare connection cap reached (4096)");
                                continue;
                            };
                            let g = group_arc.clone();
                            tokio::spawn(async move {
                                let _permit = permit;
                                let _ = proxy_connection(client, &g).await;
                            });
                        }
                    }
                }
            });
            handles.push(handle);
        }

        self.cancel = Some(token);
        self.handles = handles;
        Ok(())
    }

    /// Cancels and awaits all running SNI router tasks.
    pub async fn stop(&mut self) {
        if let Some(token) = self.cancel.take() {
            token.cancel();
        }
        for h in self.handles.drain(..) {
            let _ = h.await;
        }
    }
}

async fn proxy_connection(mut client: TcpStream, group: &PortShareGroup) -> std::io::Result<()> {
    let mut buf = Vec::with_capacity(2048);
    let mut chunk = [0u8; 4096];

    let inspection = tokio::time::timeout(INSPECT_TIMEOUT, async {
        loop {
            let n = client.read(&mut chunk).await?;
            if n == 0 {
                return Ok::<ClientInspection, std::io::Error>(ClientInspection::default());
            }
            let take = n.min(MAX_HELLO_BYTES.saturating_sub(buf.len()));
            buf.extend_from_slice(&chunk[..take]);
            match inspect_client_bytes(&buf) {
                ParseStatus::Complete(info) => return Ok(info),
                ParseStatus::NotTlsOrHttp => return Ok(ClientInspection::default()),
                ParseStatus::NeedMore => {
                    if buf.len() >= MAX_HELLO_BYTES {
                        return Ok(ClientInspection::default());
                    }
                }
            }
        }
    })
    .await
    .unwrap_or_else(|_| Ok(ClientInspection::default()))?;

    if buf.is_empty() {
        return Ok(());
    }

    let backend_port = select_backend_port(group, &inspection);
    let addr = SocketAddr::from(([127, 0, 0, 1], backend_port));
    enable_keepalive(&client);
    let mut upstream = tokio::time::timeout(UPSTREAM_CONNECT_TIMEOUT, TcpStream::connect(addr))
        .await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "upstream connect timed out"))??;
    enable_keepalive(&upstream);
    upstream.write_all(&buf).await?;
    let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
    Ok(())
}
