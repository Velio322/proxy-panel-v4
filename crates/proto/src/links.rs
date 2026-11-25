use crate::wire::{HostOverrideConfig, Protocol};
use base64::{engine::general_purpose, Engine as _};
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use thiserror::Error;
use uuid::Uuid;

/// Alias preserving compatibility with downstream callers referencing `EntryProtocol`.
pub type EntryProtocol = Protocol;

const STANDARD: general_purpose::GeneralPurpose = general_purpose::STANDARD;
const URL_SAFE_NO_PAD: general_purpose::GeneralPurpose = general_purpose::URL_SAFE_NO_PAD;
const URL_SAFE: general_purpose::GeneralPurpose = general_purpose::URL_SAFE;

/// Maximum number of individual ports allowed in a Hysteria2 port-hopping specification.
pub const MAX_HY2_HOP_PORTS: usize = 64;

/// Characters escaped by JS `encodeURIComponent` (leaves `A-Za-z0-9 - _ . ! ~ * '` unescaped).
const URI_COMPONENT_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'$')
    .add(b'%')
    .add(b'&')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

/// Characters escaped by JS `URLSearchParams` (after `' '` -> `'+'`; leaves `A-Za-z0-9 * - . _` unescaped).
const FORM_ENCODE_SET: &AsciiSet = &URI_COMPONENT_SET
    .add(b'!')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'~');

/// Errors raised during link extraction, validation, or subscription rendering.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SubError {
    #[error("{0}")]
    Unsupported(String),
    #[error("{0}")]
    Invalid(String),
}

/// Context provided by the panel when materializing a user's [`SubEntry`] from an inbound.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntryContext {
    pub node_host: String,
    pub port: u16,
    pub client_uuid: String,
    pub client_id: String,
    pub secret: String,
}

/// Borrowed view of a port-share entry to merge onto its parent inbound.
#[derive(Debug, Clone)]
pub struct PortShareInput<'a> {
    pub protocol: Protocol,
    pub tag: &'a str,
    pub host: Option<&'a str>,
    pub path: Option<&'a str>,
    pub settings: &'a Value,
    pub stream: &'a Value,
}

/// Normalized proxy entry consumed by URI builders and subscription renderers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubEntry {
    pub protocol: Protocol,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inbound_id: Option<String>,
    pub tag: String,
    /// Unbracketed host (IPv4, hostname, or raw IPv6 without `[...]`).
    pub host: String,
    pub port: u16,
    pub uuid: String,
    /// User credential (`uPSK` / password / AnyTLS password / TUIC password).
    pub password: String,
    /// Shadowsocks-2022 server `iPSK` from `inbound.settings.password`, if configured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_ipsk: Option<String>,
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security: Option<String>,
    /// VMess cipher from `settings.security` (`auto`, `aes-128-gcm`, `chacha20-poly1305`, `none`, `zero`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vmess_cipher: Option<String>,
    #[serde(default)]
    pub alter_id: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sni: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpn: Option<String>,
    #[serde(default)]
    pub allow_insecure: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spider_x: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ws_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ws_host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grpc_service_name: Option<String>,
    #[serde(default)]
    pub grpc_multi_mode: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h2_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h2_host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub httpupgrade_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub httpupgrade_host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xhttp_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xhttp_host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xhttp_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xhttp_extra: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kcp_header_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kcp_seed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tcp_header_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tcp_header_host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tcp_header_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obfs_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obfs_password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obfs_min_pkt: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obfs_max_pkt: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upload_mbps: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_mbps: Option<u64>,
    #[serde(default)]
    pub ignore_client_bandwidth: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub congestion_control: Option<String>,
    #[serde(default)]
    pub zero_rtt: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tuic_auth_timeout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tuic_heartbeat: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tuic_udp_relay_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anytls_idle_check: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anytls_idle_timeout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anytls_min_idle: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub naive_proto: Option<String>,
    /// Opt-in `insecure-concurrency` for the naive client config (parallel TCP connections).
    pub naive_concurrency: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mieru_transport: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mieru_port_range: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mieru_domain: Option<String>,
    /// Telegram WEB proxy: `plain` or `dd` (FakeTLS `ee` secrets are not supported by WEB mode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tgweb_secret_mode: Option<String>,
    /// Telegram WEB proxy: optional URL base path (`telegram/web`), without slashes at the ends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tgweb_base_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_range: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ech_config: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vless_enc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_mask: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mldsa65_verify: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_peer_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_peer_cert_by_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vless_route_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbr_profile: Option<String>,
    #[serde(default)]
    pub disable_chrome_parrot: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragment: Option<String>,
    #[serde(default)]
    pub is_cdn: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadowtls_version: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadowtls_password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadowtls_detour: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mihomo_ip_version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub host_tags: Vec<String>,
    /// Original node host from `EntryContext` before host overrides or IP resolution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_host: Option<String>,
    /// Resolved IPv4/IPv6 address of the node or host override target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_ip: Option<String>,
    #[serde(default)]
    pub raw: String,
}

impl Default for SubEntry {
    fn default() -> Self {
        Self {
            protocol: Protocol::Vless,
            inbound_id: None,
            tag: String::new(),
            host: String::new(),
            port: 0,
            uuid: String::new(),
            password: String::new(),
            server_ipsk: None,
            username: String::new(),
            flow: None,
            method: None,
            network: None,
            security: None,
            vmess_cipher: None,
            alter_id: 0,
            sni: None,
            fingerprint: None,
            alpn: None,
            allow_insecure: false,
            public_key: None,
            short_id: None,
            spider_x: None,
            ws_path: None,
            ws_host: None,
            grpc_service_name: None,
            grpc_multi_mode: false,
            h2_path: None,
            h2_host: None,
            httpupgrade_path: None,
            httpupgrade_host: None,
            xhttp_path: None,
            xhttp_host: None,
            xhttp_mode: None,
            xhttp_extra: None,
            kcp_header_type: None,
            kcp_seed: None,
            tcp_header_type: None,
            tcp_header_host: None,
            tcp_header_path: None,
            stream_path: None,
            obfs_type: None,
            obfs_password: None,
            obfs_min_pkt: None,
            obfs_max_pkt: None,
            upload_mbps: None,
            download_mbps: None,
            ignore_client_bandwidth: false,
            congestion_control: None,
            zero_rtt: false,
            tuic_auth_timeout: None,
            tuic_heartbeat: None,
            tuic_udp_relay_mode: None,
            anytls_idle_check: None,
            anytls_idle_timeout: None,
            anytls_min_idle: None,
            naive_proto: None,
            naive_concurrency: None,
            mieru_transport: None,
            mieru_port_range: None,
            mieru_domain: None,
            tgweb_secret_mode: None,
            tgweb_base_path: None,
            port_range: None,
            ech_config: None,
            vless_enc: None,
            final_mask: None,
            mldsa65_verify: None,
            pinned_peer_sha256: None,
            verify_peer_cert_by_name: None,
            vless_route_id: None,
            bbr_profile: None,
            disable_chrome_parrot: false,
            fragment: None,
            is_cdn: false,
            shadowtls_version: None,
            shadowtls_password: None,
            shadowtls_detour: None,
            mihomo_ip_version: None,
            host_tags: Vec::new(),
            node_host: None,
            resolved_ip: None,
            raw: String::new(),
        }
    }
}

impl SubEntry {
    /// Returns normalized network transport (`"raw"` mapped to `"tcp"` when requested, or raw value).
    pub fn net(&self) -> &str {
        self.network.as_deref().unwrap_or("tcp")
    }

    /// Returns normalized network transport with `"raw"` folded to `"tcp"`.
    pub fn normalized_net(&self) -> &str {
        match self.network.as_deref().unwrap_or("tcp") {
            "raw" => "tcp",
            other => other,
        }
    }

    /// Returns `true` if TLS or Reality security is enabled on this entry.
    pub fn is_tls(&self) -> bool {
        matches!(self.security.as_deref(), Some("tls" | "reality"))
    }
}

/// Exact Java / JavaScript UTF-16 `String.prototype.hashCode` unsigned magnitude (`Math.abs(h | 0)`).
pub fn js_hash_code(s: &str) -> u32 {
    let mut h: i32 = 0;
    for unit in s.encode_utf16() {
        h = h.wrapping_shl(5).wrapping_sub(h).wrapping_add(unit as i32);
    }
    h.unsigned_abs()
}

/// Strips surrounding `[` and `]` from an IPv6 literal if present.
pub fn strip_brackets(host: &str) -> String {
    let trimmed = host.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.len() >= 2 {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

pub(crate) fn is_ip_literal(s: &str) -> bool {
    let clean = strip_brackets(s);
    !clean.is_empty() && clean.parse::<std::net::IpAddr>().is_ok()
}

fn strip_host_only(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with('[') {
        if let Some(end) = trimmed.find(']') {
            return trimmed[1..end].trim().to_string();
        }
    }
    if trimmed.parse::<std::net::IpAddr>().is_ok() {
        return trimmed.to_string();
    }
    if let Some((host_part, port_part)) = trimmed.rsplit_once(':') {
        if !host_part.is_empty()
            && !host_part.contains(':')
            && !port_part.is_empty()
            && port_part.chars().all(|c| c.is_ascii_digit())
        {
            return host_part.trim().to_string();
        }
    }
    trimmed.to_string()
}

pub(crate) fn same_host_ignore_case(a: &str, b: &str) -> bool {
    let ha = strip_host_only(a);
    let hb = strip_host_only(b);
    !ha.is_empty() && !hb.is_empty() && ha.eq_ignore_ascii_case(&hb)
}

/// Brackets bare IPv6 addresses (`:` present and not already bracketed) for URI authorities.
pub fn bracket_host(host: &str) -> String {
    let trimmed = host.trim();
    if trimmed.contains(':') && !trimmed.starts_with('[') {
        format!("[{trimmed}]")
    } else {
        trimmed.to_string()
    }
}

/// Exact JS `encodeURIComponent` equivalent.
pub fn encode_uri_component(input: &str) -> String {
    utf8_percent_encode(input, URI_COMPONENT_SET).to_string()
}

/// Exact JS `URLSearchParams.prototype.toString` equivalent preserving key insertion order.
pub fn form_urlencode(pairs: &[(&str, String)]) -> String {
    let mut out = String::new();
    for (idx, (k, v)) in pairs.iter().enumerate() {
        if idx > 0 {
            out.push('&');
        }
        let ek = encode_form_part(k);
        let ev = encode_form_part(v);
        out.push_str(&ek);
        out.push('=');
        out.push_str(&ev);
    }
    out
}

fn encode_form_part(s: &str) -> String {
    let encoded = utf8_percent_encode(s, FORM_ENCODE_SET).to_string();
    encoded.replace("%20", "+")
}

/// Parses a comma-separated port / range specification (e.g. `"20000-20063"` or `"443,8443-8445"`),
/// returning sorted unique ports with a hard cap of [`MAX_HY2_HOP_PORTS`] (64).
pub fn parse_port_range(spec: &str) -> Result<Vec<u16>, SubError> {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return Err(SubError::Invalid("Empty port range".into()));
    }
    let mut ports = BTreeSet::new();
    for token in trimmed.split(',') {
        let part = token.trim();
        if part.is_empty() {
            return Err(SubError::Invalid(format!(
                "Invalid empty token in port range: {spec}"
            )));
        }
        if let Some((a, b)) = part.split_once('-') {
            let start: u16 = a
                .trim()
                .parse()
                .map_err(|_| SubError::Invalid(format!("Invalid start port in range: {part}")))?;
            let end: u16 = b
                .trim()
                .parse()
                .map_err(|_| SubError::Invalid(format!("Invalid end port in range: {part}")))?;
            if start == 0 || end == 0 || start > end {
                return Err(SubError::Invalid(format!(
                    "Invalid port range bounds: {part}"
                )));
            }
            let span = usize::from(end - start) + 1;
            if ports.len().saturating_add(span) > MAX_HY2_HOP_PORTS {
                return Err(SubError::Invalid(format!(
                    "Port range exceeds maximum of {MAX_HY2_HOP_PORTS} ports"
                )));
            }
            for p in start..=end {
                ports.insert(p);
            }
        } else {
            let port: u16 = part
                .parse()
                .map_err(|_| SubError::Invalid(format!("Invalid port: {part}")))?;
            if port == 0 {
                return Err(SubError::Invalid("Port 0 is invalid".into()));
            }
            ports.insert(port);
            if ports.len() > MAX_HY2_HOP_PORTS {
                return Err(SubError::Invalid(format!(
                    "Port range exceeds maximum of {MAX_HY2_HOP_PORTS} ports"
                )));
            }
        }
    }
    if ports.is_empty() {
        return Err(SubError::Invalid("Port range produced no ports".into()));
    }
    Ok(ports.into_iter().collect())
}

/// Encodes a `vlessRoute` 16-bit route ID (decimal or `0x...` hex) into UUID bytes `[6..8]` (big-endian).
pub fn apply_vless_route(uuid_str: &str, route_id: &str) -> Result<String, SubError> {
    let parsed = Uuid::parse_str(uuid_str.trim())
        .map_err(|e| SubError::Invalid(format!("Invalid UUID for vlessRoute: {e}")))?;
    let trimmed = route_id.trim();
    let route_num: u16 = if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u16::from_str_radix(hex, 16)
            .map_err(|_| SubError::Invalid(format!("Invalid hex vlessRoute id: {route_id}")))?
    } else {
        trimmed
            .parse::<u16>()
            .map_err(|_| SubError::Invalid(format!("Invalid numeric vlessRoute id: {route_id}")))?
    };
    let mut bytes = *parsed.as_bytes();
    let be = route_num.to_be_bytes();
    bytes[6] = be[0];
    bytes[7] = be[1];
    Ok(Uuid::from_bytes(bytes).hyphenated().to_string())
}

/// Returns `true` if the Shadowsocks method is an AEAD-2022 cipher (`2022-blake3-*`).
pub fn is_ss2022(method: &str) -> bool {
    method.trim().starts_with("2022-blake3-")
}

/// Required raw PSK byte length for Shadowsocks-2022 ciphers.
pub fn ss2022_key_len(method: &str) -> usize {
    if method.trim() == "2022-blake3-aes-128-gcm" {
        16
    } else {
        32
    }
}

/// Derives/normalizes a Shadowsocks-2022 PSK into standard Base64 of exact cipher length (16 or 32 bytes).
pub fn derive_ss2022_psk(raw: &str, method: &str) -> String {
    let need = ss2022_key_len(method);
    let trimmed = raw.trim();
    let decoded = STANDARD
        .decode(trimmed)
        .or_else(|_| URL_SAFE_NO_PAD.decode(trimmed))
        .or_else(|_| URL_SAFE.decode(trimmed));
    if let Ok(ref bytes) = decoded {
        if bytes.len() >= need {
            return STANDARD.encode(&bytes[..need]);
        }
    }
    let digest = Sha256::digest(trimmed.as_bytes());
    STANDARD.encode(&digest[..need])
}

/// Formats the effective Shadowsocks password (`<iPSK>:<uPSK>` for multi-user SS-2022 when `server_ipsk` differs).
pub fn format_ss2022_key(entry: &SubEntry) -> String {
    let method = entry.method.as_deref().unwrap_or("aes-256-gcm");
    if is_ss2022(method) {
        if let Some(ipsk) = entry.server_ipsk.as_deref().filter(|s| !s.is_empty()) {
            if !entry.password.is_empty() && ipsk != entry.password {
                return format!("{ipsk}:{}", entry.password);
            }
            if entry.password.is_empty() {
                return ipsk.to_string();
            }
        }
    }
    entry.password.clone()
}

fn at<'a>(root: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cur = root;
    for seg in path {
        cur = cur.get(*seg)?;
    }
    Some(cur)
}

fn jstr(root: &Value, path: &[&str]) -> Option<String> {
    at(root, path)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

/// Like [`jstr`] but preserves explicitly configured empty strings (`Some("")`).
///
/// Used for Reality `shortId` where `""` is a valid identifier (clients connecting
/// with an empty short ID must be admitted exactly like in Remnawave's `shortIds: [""]`).
fn jstr_keep_empty(root: &Value, path: &[&str]) -> Option<String> {
    at(root, path)
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
}

/// Like [`jstr_first`] but preserves an explicitly configured empty first entry (`Some("")`).
fn jstr_first_keep_empty(root: &Value, path: &[&str]) -> Option<String> {
    match at(root, path)? {
        Value::String(s) => Some(s.trim().to_string()),
        Value::Array(arr) => arr
            .iter()
            .filter_map(Value::as_str)
            .map(|s| s.trim().to_string())
            .next(),
        _ => None,
    }
}

/// Resolves Reality `shortId` preserving an explicitly configured empty value.
fn extract_short_id(root: &Value) -> Option<String> {
    for path in [
        &["realitySettings", "shortId"][..],
        &["realitySettings", "shortIds"][..],
        &["shortId"][..],
        &["shortIds"][..],
    ] {
        let found = if path.last() == Some(&"shortIds") {
            jstr_first_keep_empty(root, path)
        } else {
            jstr_keep_empty(root, path)
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

/// Derives the X25519 public key for a Reality `privateKey` (base64url scalar).
///
/// Returns the base64url-encoded public key so subscriptions always carry `pbk`,
/// matching Remnawave where the keypair is stored complete even when the inbound
/// stream only holds `privateKey`.
pub fn derive_reality_public_key(private_key: &str) -> Option<String> {
    let trimmed = private_key.trim();
    if trimmed.is_empty() {
        return None;
    }
    let raw = URL_SAFE_NO_PAD
        .decode(trimmed)
        .or_else(|_| URL_SAFE.decode(trimmed))
        .or_else(|_| STANDARD.decode(trimmed))
        .ok()?;
    let bytes: [u8; 32] = raw.as_slice().try_into().ok()?;
    let secret = x25519_dalek::StaticSecret::from(bytes);
    let public = x25519_dalek::PublicKey::from(&secret);
    Some(URL_SAFE_NO_PAD.encode(public.to_bytes()))
}

fn jstr_or_comma_arr(root: &Value, path: &[&str]) -> Option<String> {
    let val = at(root, path)?;
    match val {
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }
        Value::Array(arr) => {
            let parts: Vec<String> = arr
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
                .collect();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(","))
            }
        }
        _ => None,
    }
}

fn jstr_first(root: &Value, path: &[&str]) -> Option<String> {
    let val = at(root, path)?;
    match val {
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }
        Value::Array(arr) => arr
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .find(|s| !s.is_empty())
            .map(ToString::to_string),
        _ => None,
    }
}

fn jbool(root: &Value, path: &[&str]) -> bool {
    match at(root, path) {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => matches!(s.trim().to_ascii_lowercase().as_str(), "true" | "1"),
        Some(Value::Number(n)) => n.as_i64() == Some(1),
        _ => false,
    }
}

fn ju64(root: &Value, path: &[&str]) -> Option<u64> {
    let v = at(root, path)?;
    v.as_u64()
        .or_else(|| v.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64))
        .or_else(|| v.as_str().and_then(|s| s.trim().parse::<u64>().ok()))
}

fn ju32(root: &Value, path: &[&str]) -> Option<u32> {
    ju64(root, path).and_then(|n| u32::try_from(n).ok())
}

fn obj_of(v: Option<&Value>) -> Map<String, Value> {
    v.and_then(Value::as_object).cloned().unwrap_or_default()
}

fn merge_objects(base: &Value, overlay: &Value) -> Map<String, Value> {
    let mut out = obj_of(Some(base));
    if let Some(ov) = overlay.as_object() {
        for (k, v) in ov {
            out.insert(k.clone(), v.clone());
        }
    }
    out
}

fn jstr_list(root: &Value, path: &[&str]) -> Vec<String> {
    let Some(val) = at(root, path) else {
        return Vec::new();
    };
    match val {
        Value::String(s) => s
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(ToString::to_string)
            .collect(),
        Value::Array(arr) => arr
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn extract_dest_host(stream: &Value) -> Option<String> {
    for path in [
        &["realitySettings", "dest"][..],
        &["realitySettings", "target"][..],
        &["dest"][..],
        &["target"][..],
    ] {
        if let Some(raw) = jstr(stream, path) {
            let host = strip_host_only(&raw);
            if !host.is_empty() && !is_ip_literal(&host) {
                return Some(host);
            }
        }
    }
    None
}

/// Builds a normalized [`SubEntry`] from inbound configuration and user context.
pub fn build_entry(
    protocol: Protocol,
    tag: &str,
    settings: &Value,
    stream: &Value,
    ctx: &EntryContext,
) -> Result<Option<SubEntry>, SubError> {
    let clean_node_host = strip_brackets(&ctx.node_host);
    let mut e = SubEntry {
        protocol,
        tag: tag.to_string(),
        host: clean_node_host.clone(),
        port: ctx.port,
        uuid: ctx.client_uuid.clone(),
        username: ctx.client_id.clone(),
        node_host: Some(clean_node_host.clone()),
        ..Default::default()
    };

    // extract security, cipher, alterId, TLS settings, and all transport parameters
    e.network = jstr(stream, &["network"]);
    e.security = jstr(stream, &["security"]);
    e.vmess_cipher = jstr(settings, &["security"]);
    e.alter_id = at(settings, &["alterId"])
        .or_else(|| at(stream, &["alterId"]))
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
        })
        .unwrap_or(0);

    let is_reality = e
        .security
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case("reality"));
    if is_reality {
        let mut server_names = jstr_list(stream, &["realitySettings", "serverNames"]);
        server_names.extend(jstr_list(stream, &["serverNames"]));
        let valid_server_names: Vec<String> = server_names
            .into_iter()
            .filter(|c| !is_ip_literal(c))
            .collect();

        let dest_host = extract_dest_host(stream);

        let mut fallback_snis = Vec::new();
        for path in [&["sni"][..], &["tlsSettings", "serverName"][..]] {
            if let Some(v) = jstr(stream, path) {
                if !is_ip_literal(&v) {
                    fallback_snis.push(v);
                }
            }
        }
        if let Some(v) = jstr(settings, &["sni"]) {
            if !is_ip_literal(&v) {
                fallback_snis.push(v);
            }
        }

        let chosen = valid_server_names
            .iter()
            .find(|c| !same_host_ignore_case(c, &clean_node_host))
            .cloned()
            .or_else(|| {
                dest_host
                    .as_ref()
                    .filter(|d| !same_host_ignore_case(d, &clean_node_host))
                    .cloned()
            })
            .or_else(|| valid_server_names.into_iter().next())
            .or(dest_host)
            .or_else(|| {
                fallback_snis
                    .iter()
                    .find(|c| !same_host_ignore_case(c, &clean_node_host))
                    .cloned()
            })
            .or_else(|| fallback_snis.into_iter().next())
            .unwrap_or_else(|| "www.google.com".to_string());
        e.sni = Some(chosen);
    } else {
        e.sni = jstr(stream, &["sni"])
            .or_else(|| jstr(stream, &["tlsSettings", "serverName"]))
            .or_else(|| jstr(settings, &["sni"]))
            .or_else(|| jstr_first(stream, &["realitySettings", "serverNames"]));
    }
    e.fingerprint = jstr(stream, &["fingerprint"])
        .or_else(|| jstr(stream, &["tlsSettings", "fingerprint"]))
        .or_else(|| jstr(stream, &["realitySettings", "fingerprint"]));
    e.alpn =
        jstr_or_comma_arr(stream, &["alpn"]).or_else(|| jstr_or_comma_arr(stream, &["tlsSettings", "alpn"]));
    e.allow_insecure = jbool(stream, &["allowInsecure"])
        || jbool(stream, &["tlsSettings", "allowInsecure"])
        || jbool(settings, &["allowInsecure"])
        || jbool(settings, &["allow_insecure"]);

    e.flow = jstr(settings, &["flow"])
        .or_else(|| jstr(stream, &["flow"]))
        .filter(|f| !f.eq_ignore_ascii_case("none"));

    // Reality + ML-DSA-65 verification
    e.public_key =
        jstr(stream, &["publicKey"]).or_else(|| jstr(stream, &["realitySettings", "publicKey"]));
    if e.public_key.is_none() {
        let priv_key = jstr(stream, &["privateKey"])
            .or_else(|| jstr(stream, &["realitySettings", "privateKey"]));
        if let Some(pk) = priv_key.as_deref().and_then(derive_reality_public_key) {
            e.public_key = Some(pk);
        }
    }
    e.short_id = extract_short_id(stream);
    e.spider_x =
        jstr(stream, &["spiderX"]).or_else(|| jstr(stream, &["realitySettings", "spiderX"]));
    e.mldsa65_verify = jstr(stream, &["mldsa65Verify"])
        .or_else(|| jstr(stream, &["realitySettings", "mldsa65Verify"]));

    // Transports
    e.stream_path = jstr(stream, &["path"]);
    e.ws_path = jstr(stream, &["wsSettings", "path"]).or_else(|| e.stream_path.clone());
    e.ws_host =
        jstr(stream, &["wsSettings", "host"]).or_else(|| jstr(stream, &["wsSettings", "headers", "Host"]));

    e.grpc_service_name =
        jstr(stream, &["grpcSettings", "serviceName"]).or_else(|| jstr(stream, &["serviceName"]));
    e.grpc_multi_mode = jbool(stream, &["grpcSettings", "multiMode"]);

    e.h2_path = jstr(stream, &["httpSettings", "path"]).or_else(|| e.stream_path.clone());
    e.h2_host = jstr_first(stream, &["httpSettings", "host"]);

    e.httpupgrade_path =
        jstr(stream, &["httpupgradeSettings", "path"]).or_else(|| e.stream_path.clone());
    e.httpupgrade_host = jstr(stream, &["httpupgradeSettings", "host"])
        .or_else(|| jstr(stream, &["httpupgradeSettings", "headers", "Host"]));

    e.xhttp_path = jstr(stream, &["xhttpSettings", "path"]).or_else(|| e.stream_path.clone());
    let raw_xhttp_host = jstr(stream, &["xhttpSettings", "host"])
        .or_else(|| jstr(stream, &["xhttpSettings", "headers", "Host"]));
    e.xhttp_host = if is_reality {
        match raw_xhttp_host {
            Some(ref h) if !is_ip_literal(h) => {
                if same_host_ignore_case(h, &clean_node_host)
                    && e.sni
                        .as_deref()
                        .is_some_and(|s| !same_host_ignore_case(s, &clean_node_host))
                {
                    e.sni.clone()
                } else {
                    Some(h.clone())
                }
            }
            _ => None,
        }
    } else {
        raw_xhttp_host
    };
    e.xhttp_mode = jstr(stream, &["xhttpSettings", "mode"]);
    e.xhttp_extra = build_xhttp_extra(stream);

    e.kcp_header_type = jstr(stream, &["kcpSettings", "headerType"])
        .or_else(|| jstr(stream, &["kcpSettings", "header", "type"]));
    e.kcp_seed = jstr(stream, &["kcpSettings", "seed"]);

    let tcp_obj = at(stream, &["tcpSettings"]).or_else(|| at(stream, &["rawSettings"]));
    if let Some(t) = tcp_obj {
        e.tcp_header_type = jstr(t, &["header", "type"]).or_else(|| jstr(t, &["headerType"]));
        e.tcp_header_path = jstr_first(t, &["header", "request", "path"]);
        e.tcp_header_host = jstr_first(t, &["header", "request", "headers", "Host"]);
    }

    // Extended Xray / TLS fields
    e.vless_enc = jstr(settings, &["encryption"]).or_else(|| jstr(settings, &["vlessEnc"]));
    e.vless_route_id =
        jstr(settings, &["vlessRouteId"]).or_else(|| jstr(settings, &["vlessRoute"]));
    e.final_mask = at(stream, &["finalmask"])
        .or_else(|| at(stream, &["finalMask"]))
        .or_else(|| at(settings, &["finalMask"]))
        .filter(|v| !v.is_null())
        .cloned();
    e.pinned_peer_sha256 = jstr(stream, &["pinnedPeerCertSha256"])
        .or_else(|| jstr(stream, &["tlsSettings", "pinnedPeerCertSha256"]))
        .or_else(|| jstr(settings, &["pinSHA256"]))
        .or_else(|| jstr(settings, &["pinnedPeerCertSha256"]));
    e.verify_peer_cert_by_name = jstr(stream, &["verifyPeerCertByName"])
        .or_else(|| jstr(stream, &["tlsSettings", "verifyPeerCertByName"]));
    e.ech_config = jstr(settings, &["echConfig"])
        .or_else(|| jstr(stream, &["echConfig"]))
        .or_else(|| jstr(stream, &["tlsSettings", "echConfigList"]))
        .or_else(|| jstr(settings, &["ech"]));

    // Protocol-specific fields
    match protocol {
        Protocol::Vless | Protocol::Vmess => {
            if let Some(id) = jstr(settings, &["id"]) {
                if e.uuid.is_empty() {
                    e.uuid = id;
                }
            }
        }
        Protocol::Trojan => {
            e.password = if !ctx.secret.is_empty() {
                ctx.secret.clone()
            } else {
                jstr(settings, &["password"]).unwrap_or_default()
            };
            if e.security.is_none() {
                e.security = Some("tls".into());
            }
        }
        Protocol::Shadowsocks => {
            let method = jstr(settings, &["method"]).unwrap_or_else(|| "aes-256-gcm".into());
            let raw_server_pw = jstr(settings, &["password"]);
            let raw_user_pw = if !ctx.secret.is_empty() {
                ctx.secret.clone()
            } else {
                raw_server_pw.clone().unwrap_or_default()
            };
            if is_ss2022(&method) {
                e.server_ipsk = raw_server_pw.map(|p| derive_ss2022_psk(&p, &method));
                e.password = derive_ss2022_psk(&raw_user_pw, &method);
            } else {
                e.server_ipsk = raw_server_pw;
                e.password = raw_user_pw;
            }
            e.method = Some(method);
            if let Some(st_pw) = jstr(settings, &["shadowtlsPassword"]) {
                e.shadowtls_password = Some(st_pw);
                e.shadowtls_version = ju32(settings, &["shadowtlsVersion"])
                    .and_then(|v| u8::try_from(v).ok())
                    .or(Some(3));
            }
        }
        Protocol::Hysteria2 => {
            e.password = if !ctx.secret.is_empty() {
                ctx.secret.clone()
            } else {
                jstr(settings, &["password"]).unwrap_or_default()
            };
            e.obfs_type = jstr(settings, &["obfs", "type"]);
            e.obfs_password = jstr(settings, &["obfs", "password"]);
            e.obfs_min_pkt = ju32(settings, &["obfs", "minPacketSize"])
                .or_else(|| ju32(settings, &["obfs", "min_packet_size"]));
            e.obfs_max_pkt = ju32(settings, &["obfs", "maxPacketSize"])
                .or_else(|| ju32(settings, &["obfs", "max_packet_size"]));
            // Server settings are from the server's point of view: its upload is the client's
            // download. Client configs need the opposite mapping.
            e.upload_mbps = ju64(settings, &["downloadMbps"]);
            e.download_mbps = ju64(settings, &["uploadMbps"]);
            e.ignore_client_bandwidth = jbool(settings, &["ignoreClientBandwidth"])
                || jbool(settings, &["ignore_client_bandwidth"]);
            e.port_range =
                jstr(settings, &["portRange"]).or_else(|| jstr(settings, &["port_range"]));
            if let Some(ref pr) = e.port_range {
                parse_port_range(pr)?;
            }
            e.bbr_profile =
                jstr(settings, &["bbrProfile"]).or_else(|| jstr(settings, &["bbr_profile"]));
            e.disable_chrome_parrot = jbool(settings, &["disableChromeParrot"])
                || jbool(settings, &["disable_chrome_parrot"]);
            if e.security.is_none() {
                e.security = Some("tls".into());
            }
        }
        Protocol::Tuic => {
            e.password = if !ctx.secret.is_empty() {
                ctx.secret.clone()
            } else {
                jstr(settings, &["password"]).unwrap_or_default()
            };
            if e.uuid.is_empty() {
                e.uuid = jstr(settings, &["uuid"]).unwrap_or_default();
            }
            e.congestion_control = jstr(settings, &["congestion_control"])
                .or_else(|| jstr(settings, &["congestionControl"]))
                .or_else(|| Some("bbr".into()));
            e.zero_rtt = jbool(settings, &["zeroRttHandshake"])
                || jbool(settings, &["zero_rtt_handshake"])
                || jbool(settings, &["zero_rtt"])
                || jbool(settings, &["zeroRtt"]);
            e.tuic_auth_timeout =
                jstr(settings, &["authTimeout"]).or_else(|| jstr(settings, &["auth_timeout"]));
            e.tuic_heartbeat = jstr(settings, &["heartbeat"]);
            e.tuic_udp_relay_mode =
                jstr(settings, &["udpRelayMode"]).or_else(|| jstr(settings, &["udp_relay_mode"]));
            if e.security.is_none() {
                e.security = Some("tls".into());
            }
        }
        Protocol::Naiveproxy => {
            e.password = if !ctx.secret.is_empty() {
                ctx.secret.clone()
            } else {
                jstr(settings, &["password"]).unwrap_or_default()
            };
            if e.username.is_empty() {
                e.username = jstr(settings, &["username"]).unwrap_or_else(|| "user".into());
            }
            e.naive_proto = Some(
                jstr(settings, &["proto"])
                    .unwrap_or_else(|| "https".into())
                    .to_ascii_lowercase(),
            );
            // The certificate is issued for the naive domain, so clients must connect by that
            // name (the node host may be a bare IP, which breaks SNI and validation).
            let naive_domain = jstr(settings, &["domain"])
                .or_else(|| jstr(settings, &["sni"]))
                .or_else(|| jstr(stream, &["sni"]))
                .map(|d| d.trim().to_string())
                .filter(|d| !d.is_empty() && !d.contains(':') && !is_ip_literal(d));
            if let Some(d) = naive_domain {
                e.host = d.clone();
                e.sni = Some(d);
            }
            e.naive_concurrency = ju32(settings, &["insecureConcurrency"]).filter(|n| *n > 1);
            if e.security.is_none() {
                e.security = Some("tls".into());
            }
        }
        Protocol::Mieru => {
            e.password = if !ctx.secret.is_empty() {
                ctx.secret.clone()
            } else {
                jstr(settings, &["password"]).unwrap_or_default()
            };
            if e.username.is_empty() {
                e.username = jstr(settings, &["username"]).unwrap_or_else(|| "user".into());
            }
            e.mieru_transport =
                jstr(settings, &["transport"]).or_else(|| jstr(settings, &["protocol"]));
            e.mieru_port_range = jstr(settings, &["portRange"]);
            e.mieru_domain = jstr(settings, &["domain"]).or_else(|| e.sni.clone());
        }
        Protocol::Tgweb => {
            e.password = if !ctx.secret.is_empty() {
                ctx.secret.clone()
            } else {
                jstr(settings, &["secret"]).unwrap_or_default()
            };
            e.tgweb_secret_mode = Some(tgweb_secret_mode(settings));
            e.tgweb_base_path = jstr(settings, &["basePath"])
                .map(|p| p.trim().trim_matches('/').to_string())
                .filter(|p| !p.is_empty());
            // The link must carry the public hostname that owns the certificate. WEB mode is
            // always served over HTTPS on 443, so the node host (often a bare IP) is not enough.
            let domain = jstr(settings, &["domain"])
                .or_else(|| jstr(settings, &["host"]))
                .or_else(|| jstr(settings, &["sni"]))
                .map(|d| d.trim().to_string())
                .filter(|d| !d.is_empty() && !d.contains(':') && !is_ip_literal(d));
            if let Some(d) = domain {
                e.host = d.clone();
                e.sni = Some(d);
            }
            e.port = 443;
            if e.security.is_none() {
                e.security = Some("tls".into());
            }
        }
        Protocol::Anytls => {
            e.password = if !ctx.secret.is_empty() {
                ctx.secret.clone()
            } else {
                jstr(settings, &["password"]).unwrap_or_default()
            };
            e.anytls_idle_check = jstr(settings, &["idleSessionCheckInterval"])
                .or_else(|| jstr(settings, &["idle_session_check_interval"]));
            e.anytls_idle_timeout = jstr(settings, &["idleSessionTimeout"])
                .or_else(|| jstr(settings, &["idle_session_timeout"]));
            e.anytls_min_idle = ju32(settings, &["minIdleSession"])
                .or_else(|| ju32(settings, &["min_idle_session"]));
            if e.security.is_none() {
                e.security = Some("tls".into());
            }
        }
        Protocol::Shadowtls => {
            e.shadowtls_version = ju32(settings, &["version"])
                .or_else(|| ju32(settings, &["shadowtlsVersion"]))
                .and_then(|v| u8::try_from(v).ok())
                .or(Some(3));
            e.shadowtls_password = if !ctx.secret.is_empty() {
                Some(ctx.secret.clone())
            } else {
                jstr(settings, &["password"]).or_else(|| jstr(settings, &["shadowtlsPassword"]))
            };
            e.shadowtls_detour = jstr(settings, &["detour"]);
            e.password = e.shadowtls_password.clone().unwrap_or_default();
            if e.security.is_none() {
                e.security = Some("tls".into());
            }
        }
    }

    e.raw = build_uri(&e)?;
    Ok(Some(e))
}

fn build_xhttp_extra(stream: &Value) -> Option<Value> {
    let xh = at(stream, &["xhttpSettings"]);
    let extra = at(stream, &["xhttpSettings", "extra"]).filter(|v| !v.is_null());
    let mut merged = match extra {
        Some(Value::Object(map)) => map.clone(),
        Some(other) => return Some(other.clone()),
        None => Map::new(),
    };

    if let Some(xh_val) = xh {
        for key in [
            "xmux",
            "downloadSettings",
            "headers",
            "xPaddingBytes",
            "scMaxEachPostBytes",
            "scMinPostsIntervalMs",
            "scMaxBufferedPosts",
            "scStreamUpServerSecs",
            "noGRPCHeader",
            "noSSEHeader",
        ] {
            if !merged.contains_key(key) {
                if let Some(v) = xh_val.get(key).filter(|v| !v.is_null()) {
                    let is_empty_str = v.as_str().is_some_and(|s| s.trim().is_empty());
                    if !is_empty_str {
                        merged.insert(key.into(), v.clone());
                    }
                }
            }
        }
    }

    if merged.is_empty() {
        None
    } else {
        Some(Value::Object(merged))
    }
}

/// Builds a [`SubEntry`] for a [`PortShareInput`] layered on top of its parent inbound.
///
/// Returns `Ok(None)` for protocols unsupported on SNI port-sharing (`TUIC`, `MIERU`).
pub fn build_port_share_entry(
    inbound_settings: &Value,
    inbound_stream: &Value,
    ps: &PortShareInput<'_>,
    ctx: &EntryContext,
) -> Result<Option<SubEntry>, SubError> {
    if matches!(ps.protocol, Protocol::Tuic | Protocol::Mieru) {
        return Ok(None);
    }

    let mut merged_settings = merge_objects(inbound_settings, ps.settings);
    if !ctx.secret.is_empty() {
        merged_settings.insert("password".into(), json!(ctx.secret));
    }
    if !ctx.client_id.is_empty() {
        merged_settings.insert("username".into(), json!(ctx.client_id));
    }

    let mut merged_stream = merge_objects(inbound_stream, ps.stream);
    if let Some(host) = ps.host.map(str::trim).filter(|h| !h.is_empty()) {
        merged_stream.insert("sni".into(), json!(host));
    }

    if let Some(path) = ps.path.map(str::trim).filter(|p| !p.is_empty()) {
        let network = merged_stream
            .get("network")
            .and_then(Value::as_str)
            .unwrap_or("tcp");
        match network {
            "grpc" => {
                let mut g = obj_of(merged_stream.get("grpcSettings"));
                g.insert("serviceName".into(), json!(path));
                merged_stream.insert("grpcSettings".into(), Value::Object(g));
            }
            "xhttp" => {
                let mut x = obj_of(merged_stream.get("xhttpSettings"));
                x.insert("path".into(), json!(path));
                merged_stream.insert("xhttpSettings".into(), Value::Object(x));
            }
            "httpupgrade" => {
                let mut h = obj_of(merged_stream.get("httpupgradeSettings"));
                h.insert("path".into(), json!(path));
                merged_stream.insert("httpupgradeSettings".into(), Value::Object(h));
            }
            "h2" => {
                let mut h = obj_of(merged_stream.get("httpSettings"));
                h.insert("path".into(), json!(path));
                merged_stream.insert("httpSettings".into(), Value::Object(h));
                merged_stream.insert("path".into(), json!(path));
            }
            _ => {
                let mut w = obj_of(merged_stream.get("wsSettings"));
                w.insert("path".into(), json!(path));
                merged_stream.insert("wsSettings".into(), Value::Object(w));
                merged_stream.insert("path".into(), json!(path));
            }
        }
    }

    build_entry(
        ps.protocol,
        ps.tag,
        &Value::Object(merged_settings),
        &Value::Object(merged_stream),
        ctx,
    )
}

/// Applies a [`HostOverrideConfig`] onto a [`SubEntry`] and refreshes `entry.raw`.
pub fn apply_host_override(
    entry: &mut SubEntry,
    host_cfg: &HostOverrideConfig,
) -> Result<(), SubError> {
    if !host_cfg.enable {
        return Ok(());
    }

    if let Some(ref addr_spec) = host_cfg.address {
        let candidates: Vec<&str> = addr_spec
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if !candidates.is_empty() {
            let chosen = if host_cfg.shuffle_host && candidates.len() > 1 {
                let idx = (js_hash_code(&entry.tag) as usize) % candidates.len();
                candidates[idx]
            } else {
                candidates[0]
            };
            entry.host = strip_brackets(chosen);
            entry.resolved_ip = None;
        }
    }

    if let Some(port) = host_cfg.port.filter(|p| *p > 0) {
        entry.port = port;
    }
    if let Some(sni) = host_cfg.sni.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        entry.sni = Some(sni.to_string());
    }
    if let Some(alpn) = host_cfg
        .alpn
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        entry.alpn = Some(alpn.to_string());
    }
    if let Some(fp) = host_cfg
        .fingerprint
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        entry.fingerprint = Some(fp.to_string());
    }
    if let Some(ref extra) = host_cfg.xhttp_extra {
        entry.xhttp_extra = Some(extra.clone());
    }
    if let Some(ref fm) = host_cfg.final_mask {
        entry.final_mask = Some(fm.clone());
    }
    if let Some(rid) = host_cfg
        .vless_route_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        entry.vless_route_id = Some(rid.to_string());
    }
    if let Some(frag) = host_cfg
        .fragment
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        entry.fragment = Some(frag.to_string());
    }
    if let Some(allow_ins) = host_cfg.allow_insecure {
        entry.allow_insecure = allow_ins;
    }
    if let Some(pcs) = host_cfg
        .pinned_peer_cert_sha256
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        entry.pinned_peer_sha256 = Some(pcs.to_string());
    }
    if let Some(vcn) = host_cfg
        .verify_peer_cert_by_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        entry.verify_peer_cert_by_name = Some(vcn.to_string());
    }
    if let Some(ip_ver) = host_cfg
        .mihomo_ip_version
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        entry.mihomo_ip_version = Some(ip_ver.to_string());
    }
    if !host_cfg.tags.is_empty() {
        entry.host_tags = host_cfg.tags.clone();
    }
    if host_cfg.is_cdn {
        entry.is_cdn = true;
    }

    if let Some(h_hdr) = host_cfg
        .host
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        match entry.normalized_net() {
            "ws" => entry.ws_host = Some(h_hdr.to_string()),
            "httpupgrade" => entry.httpupgrade_host = Some(h_hdr.to_string()),
            "xhttp" => entry.xhttp_host = Some(h_hdr.to_string()),
            "h2" => entry.h2_host = Some(h_hdr.to_string()),
            _ => entry.ws_host = Some(h_hdr.to_string()),
        }
    }

    if let Some(p_hdr) = host_cfg
        .path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        match entry.normalized_net() {
            "ws" => entry.ws_path = Some(p_hdr.to_string()),
            "httpupgrade" => entry.httpupgrade_path = Some(p_hdr.to_string()),
            "xhttp" => entry.xhttp_path = Some(p_hdr.to_string()),
            "h2" => entry.h2_path = Some(p_hdr.to_string()),
            "grpc" => entry.grpc_service_name = Some(p_hdr.to_string()),
            _ => entry.stream_path = Some(p_hdr.to_string()),
        }
    }

    if let Some(remark) = host_cfg
        .remark
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        entry.tag = remark.to_string();
    }

    entry.raw = build_uri(entry)?;
    Ok(())
}

/// Expands base [`SubEntry`] items against matching enabled [`HostOverrideConfig`] overrides.
///
/// If an entry has no matching host overrides, it is kept as-is. When overrides match
/// (`h.inbound_id.is_none` or matches `entry.inbound_id`), one cloned entry is emitted per override.
pub fn expand_with_hosts(
    entries: &[SubEntry],
    hosts: &[HostOverrideConfig],
) -> Result<Vec<SubEntry>, SubError> {
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        let matching: Vec<&HostOverrideConfig> = hosts
            .iter()
            .filter(|h| h.enable)
            .filter(|h| match (&h.inbound_id, &entry.inbound_id) {
                (Some(hid), Some(eid)) => hid == eid,
                (None, _) => true,
                (Some(_), None) => true,
            })
            .collect();
        if matching.is_empty() {
            out.push(entry.clone());
        } else {
            for h in matching {
                let mut cloned = entry.clone();
                apply_host_override(&mut cloned, h)?;
                out.push(cloned);
            }
        }
    }
    Ok(out)
}

/// Builds the canonical share URI (or JSON string for Mieru) for a [`SubEntry`].
pub fn build_uri(entry: &SubEntry) -> Result<String, SubError> {
    match entry.protocol {
        Protocol::Vless => vless_uri(entry),
        Protocol::Vmess => Ok(vmess_uri(entry)),
        Protocol::Trojan => trojan_uri(entry),
        Protocol::Shadowsocks => Ok(ss_uri(entry)),
        Protocol::Hysteria2 => hysteria2_uri(entry),
        Protocol::Tuic => Ok(tuic_uri(entry)),
        Protocol::Naiveproxy => Ok(naive_uri(entry)),
        Protocol::Mieru => mieru_config(entry).map(|v| v.to_string()),
        Protocol::Anytls => Ok(anytls_uri(entry)),
        Protocol::Shadowtls => Ok(shadowtls_uri(entry)),
        Protocol::Tgweb => tgweb_link(entry),
    }
}

/// Builds a user-displayable share link (`naive+https://...`, `mieru://...`, etc.).
pub fn display_link(entry: &SubEntry) -> Result<String, SubError> {
    match entry.protocol {
        Protocol::Naiveproxy => Ok(naive_link(entry)),
        Protocol::Mieru => mieru_link(entry),
        _ => build_uri(entry),
    }
}

/// Returns the canonical share link used across both single "Nodes & QR" links
/// and `/sub/:token` subscription payloads.
pub fn share_link(entry: &SubEntry) -> Result<String, SubError> {
    match entry.protocol {
        Protocol::Naiveproxy | Protocol::Mieru | Protocol::Tgweb => display_link(entry),
        _ if !entry.raw.is_empty() => Ok(entry.raw.clone()),
        _ => build_uri(entry),
    }
}

fn push_transport_params(params: &mut Vec<(&'static str, String)>, e: &SubEntry) {
    match e.net() {
        "ws" => {
            params.push((
                "path",
                e.ws_path
                    .clone()
                    .or_else(|| e.stream_path.clone())
                    .unwrap_or_else(|| "/".into()),
            ));
            if let Some(ref h) = e.ws_host {
                params.push(("host", h.clone()));
            }
        }
        "grpc" => {
            params.push((
                "serviceName",
                e.grpc_service_name.clone().unwrap_or_default(),
            ));
            if e.grpc_multi_mode {
                params.push(("mode", "multi".into()));
            }
        }
        "h2" => {
            params.push((
                "path",
                e.h2_path
                    .clone()
                    .or_else(|| e.stream_path.clone())
                    .unwrap_or_else(|| "/".into()),
            ));
            if let Some(ref h) = e.h2_host {
                params.push(("host", h.clone()));
            }
        }
        "httpupgrade" => {
            params.push((
                "path",
                e.httpupgrade_path
                    .clone()
                    .or_else(|| e.stream_path.clone())
                    .unwrap_or_else(|| "/".into()),
            ));
            if let Some(ref h) = e.httpupgrade_host {
                params.push(("host", h.clone()));
            }
        }
        "xhttp" => {
            params.push((
                "path",
                e.xhttp_path
                    .clone()
                    .or_else(|| e.stream_path.clone())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "/".into()),
            ));
            if let Some(ref h) = e.xhttp_host {
                params.push(("host", h.clone()));
            }
            if let Some(ref m) = e.xhttp_mode {
                params.push(("mode", m.clone()));
            }
            if let Some(ref extra) = e.xhttp_extra {
                if let Ok(compact) = serde_json::to_string(extra) {
                    params.push(("extra", compact));
                }
            }
        }
        "kcp" => {
            if let Some(ref ht) = e.kcp_header_type {
                params.push(("headerType", ht.clone()));
            }
            if let Some(ref seed) = e.kcp_seed {
                params.push(("seed", seed.clone()));
            }
        }
        "tcp" | "raw" => {
            if let Some(ref ht) = e.tcp_header_type {
                if ht != "none" && !ht.is_empty() {
                    params.push(("headerType", ht.clone()));
                }
            }
            if let Some(ref h) = e.tcp_header_host {
                params.push(("host", h.clone()));
            }
            if let Some(ref p) = e.tcp_header_path {
                params.push(("path", p.clone()));
            }
        }
        _ => {}
    }
}

/// Formats a `vless://` URI with Reality, XHTTP `extra`, `vlessenc`, `FinalMask`, `pcs`/`vcn`, `pqv`, and `vlessRoute`.
pub fn vless_uri(e: &SubEntry) -> Result<String, SubError> {
    let uuid = if let Some(ref route_id) = e.vless_route_id {
        apply_vless_route(&e.uuid, route_id)?
    } else {
        e.uuid.clone()
    };

    let net = e.net();
    let sec = e.security.as_deref().unwrap_or("none");
    let mut params: Vec<(&'static str, String)> = vec![
        ("type", net.to_string()),
        (
            "encryption",
            e.vless_enc.clone().unwrap_or_else(|| "none".into()),
        ),
        ("security", sec.to_string()),
    ];

    if sec == "tls" {
        if let Some(ref sni) = e.sni {
            params.push(("sni", sni.clone()));
        }
        if let Some(ref fp) = e.fingerprint {
            params.push(("fp", fp.clone()));
        }
        if let Some(ref alpn) = e.alpn {
            params.push(("alpn", alpn.clone()));
        }
        if e.allow_insecure {
            params.push(("allowInsecure", "1".into()));
        }
        if let Some(ref pcs) = e.pinned_peer_sha256 {
            params.push(("pcs", pcs.clone()));
        }
        if let Some(ref vcn) = e.verify_peer_cert_by_name {
            params.push(("vcn", vcn.clone()));
        }
        if let Some(ref ech) = e.ech_config {
            params.push(("ech", ech.clone()));
        }
    } else if sec == "reality" {
        if let Some(ref sni) = e.sni {
            params.push(("sni", sni.clone()));
        }
        if let Some(ref fp) = e.fingerprint {
            params.push(("fp", fp.clone()));
        }
        if let Some(ref pbk) = e.public_key {
            params.push(("pbk", pbk.clone()));
        }
        if let Some(ref sid) = e.short_id {
            params.push(("sid", sid.clone()));
        }
        if let Some(ref spx) = e.spider_x {
            params.push(("spx", spx.clone()));
        }
        if let Some(ref pqv) = e.mldsa65_verify {
            params.push(("pqv", pqv.clone()));
        }
    }

    if let Some(ref flow) = e.flow {
        params.push(("flow", flow.clone()));
    }
    push_transport_params(&mut params, e);

    if let Some(ref fm) = e.final_mask {
        if let Ok(compact) = serde_json::to_string(fm) {
            params.push(("fm", compact));
        }
    }
    if let Some(ref frag) = e.fragment {
        if !frag.trim().is_empty() {
            params.push(("fragment", frag.clone()));
        }
    }

    let qs = form_urlencode(&params);
    Ok(format!(
        "vless://{uuid}@{}:{}?{qs}#{}",
        bracket_host(&e.host),
        e.port,
        encode_uri_component(&e.tag)
    ))
}

/// Formats a `vmess://` URI (``: `scy` uses `vmess_cipher`, never `"tls"`, `aid` from `settings`).
pub fn vmess_uri(e: &SubEntry) -> String {
    let scy = e
        .vmess_cipher
        .as_deref()
        .filter(|c| matches!(*c, "auto" | "aes-128-gcm" | "chacha20-poly1305" | "none" | "zero"))
        .unwrap_or("auto");
    let host = e
        .ws_host
        .clone()
        .or_else(|| e.httpupgrade_host.clone())
        .or_else(|| e.h2_host.clone())
        .or_else(|| e.xhttp_host.clone())
        .unwrap_or_default();
    let path = e
        .ws_path
        .clone()
        .or_else(|| e.httpupgrade_path.clone())
        .or_else(|| e.h2_path.clone())
        .or_else(|| e.xhttp_path.clone())
        .or_else(|| e.grpc_service_name.clone())
        .or_else(|| e.stream_path.clone())
        .unwrap_or_default();

    let mut obj = Map::new();
    obj.insert("v".into(), json!("2"));
    obj.insert("ps".into(), json!(e.tag));
    obj.insert("add".into(), json!(bracket_host(&e.host)));
    obj.insert("port".into(), json!(e.port.to_string()));
    obj.insert("id".into(), json!(e.uuid));
    obj.insert("aid".into(), json!(e.alter_id.to_string()));
    obj.insert("scy".into(), json!(scy));
    obj.insert("net".into(), json!(e.net()));
    obj.insert(
        "type".into(),
        json!(e.kcp_header_type.clone().unwrap_or_else(|| "none".into())),
    );
    obj.insert("host".into(), json!(host));
    obj.insert("path".into(), json!(path));
    obj.insert("tls".into(), json!(if e.is_tls() { "tls" } else { "" }));
    obj.insert("sni".into(), json!(e.sni.clone().unwrap_or_default()));
    obj.insert("fp".into(), json!(e.fingerprint.clone().unwrap_or_default()));
    obj.insert("alpn".into(), json!(e.alpn.clone().unwrap_or_default()));

    let encoded = STANDARD.encode(Value::Object(obj).to_string().as_bytes());
    format!("vmess://{encoded}")
}

/// Formats a `trojan://` URI.
pub fn trojan_uri(e: &SubEntry) -> Result<String, SubError> {
    let net = e.net();
    let sec = e.security.as_deref().unwrap_or("tls");
    let mut params: Vec<(&'static str, String)> =
        vec![("type", net.to_string()), ("security", sec.to_string())];

    if sec == "reality" {
        if let Some(ref sni) = e.sni {
            params.push(("sni", sni.clone()));
        }
        if let Some(ref fp) = e.fingerprint {
            params.push(("fp", fp.clone()));
        }
        if let Some(ref pbk) = e.public_key {
            params.push(("pbk", pbk.clone()));
        }
        if let Some(ref sid) = e.short_id {
            params.push(("sid", sid.clone()));
        }
        if let Some(ref spx) = e.spider_x {
            params.push(("spx", spx.clone()));
        }
        if let Some(ref pqv) = e.mldsa65_verify {
            params.push(("pqv", pqv.clone()));
        }
    } else {
        if let Some(ref sni) = e.sni {
            params.push(("sni", sni.clone()));
        }
        if let Some(ref fp) = e.fingerprint {
            params.push(("fp", fp.clone()));
        }
        if let Some(ref alpn) = e.alpn {
            params.push(("alpn", alpn.clone()));
        }
        if e.allow_insecure {
            params.push(("allowInsecure", "1".into()));
        }
        if let Some(ref pcs) = e.pinned_peer_sha256 {
            params.push(("pcs", pcs.clone()));
        }
        if let Some(ref vcn) = e.verify_peer_cert_by_name {
            params.push(("vcn", vcn.clone()));
        }
        if let Some(ref ech) = e.ech_config {
            params.push(("ech", ech.clone()));
        }
    }

    push_transport_params(&mut params, e);
    if let Some(ref fm) = e.final_mask {
        if let Ok(compact) = serde_json::to_string(fm) {
            params.push(("fm", compact));
        }
    }
    if let Some(ref frag) = e.fragment {
        if !frag.trim().is_empty() {
            params.push(("fragment", frag.clone()));
        }
    }

    let qs = form_urlencode(&params);
    Ok(format!(
        "trojan://{}@{}:{}?{qs}#{}",
        encode_uri_component(&e.password),
        bracket_host(&e.host),
        e.port,
        encode_uri_component(&e.tag)
    ))
}

/// Formats a `ss://` SIP002 URI (``: SS-2022 `<iPSK>:<uPSK>` + standard base64).
pub fn ss_uri(e: &SubEntry) -> String {
    let method = e.method.as_deref().unwrap_or("aes-256-gcm");
    let effective_pw = format_ss2022_key(e);
    let userinfo = STANDARD.encode(format!("{method}:{effective_pw}").as_bytes());
    let mut query = String::new();
    if let Some(ref st_pw) = e.shadowtls_password {
        let ver = e.shadowtls_version.unwrap_or(3);
        let mut plugin_parts = vec![
            "shadow-tls".to_string(),
            format!("host={}", e.sni.as_deref().unwrap_or(&e.host)),
            format!("passwd={st_pw}"),
            format!("version={ver}"),
        ];
        if let Some(ref fp) = e.fingerprint {
            plugin_parts.push(format!("fp={fp}"));
        }
        let plugin_val = plugin_parts.join(";");
        query = format!("?{}", form_urlencode(&[("plugin", plugin_val)]));
    }
    format!(
        "ss://{userinfo}@{}:{}{query}#{}",
        bracket_host(&e.host),
        e.port,
        encode_uri_component(&e.tag)
    )
}

/// Formats a `hy2://` URI with optional multi-port hopping authority (`host:start-end`), `ech`, and `obfs`.
pub fn hysteria2_uri(e: &SubEntry) -> Result<String, SubError> {
    let mut params: Vec<(&'static str, String)> = Vec::new();
    if let Some(ref sni) = e.sni {
        params.push(("sni", sni.clone()));
    }
    if e.allow_insecure {
        params.push(("insecure", "1".into()));
    }
    if let Some(ref pcs) = e.pinned_peer_sha256 {
        params.push(("pinSHA256", pcs.clone()));
    }
    if let Some(ref obfs) = e.obfs_type {
        if obfs != "none" && !obfs.is_empty() {
            params.push(("obfs", obfs.clone()));
            if let Some(ref pw) = e.obfs_password {
                params.push(("obfs-password", pw.clone()));
            }
        }
    }
    if let Some(ref ech) = e.ech_config {
        params.push(("ech", ech.clone()));
    }
    let authority_port = if let Some(ref pr) = e.port_range {
        parse_port_range(pr)?;
        let normalized_pr = pr.trim().to_string();
        params.push(("mport", normalized_pr.clone()));
        normalized_pr
    } else {
        e.port.to_string()
    };

    let qs = if params.is_empty() {
        String::new()
    } else {
        format!("?{}", form_urlencode(&params))
    };

    Ok(format!(
        "hy2://{}@{}:{authority_port}{qs}#{}",
        encode_uri_component(&e.password),
        bracket_host(&e.host),
        encode_uri_component(&e.tag)
    ))
}

/// Alias for [`hysteria2_uri`].
pub fn hy2_uri(e: &SubEntry) -> Result<String, SubError> {
    hysteria2_uri(e)
}

/// Formats a `tuic://` URI (``: includes `allow_insecure=1` and `zero_rtt_handshake=1` when set).
pub fn tuic_uri(e: &SubEntry) -> String {
    let cc = e.congestion_control.as_deref().unwrap_or("bbr");
    let alpn = e.alpn.as_deref().unwrap_or("h3");
    let udp_mode = e.tuic_udp_relay_mode.as_deref().unwrap_or("native");
    let mut params: Vec<(&'static str, String)> = vec![
        ("congestion_control", cc.to_string()),
        ("udp_relay_mode", udp_mode.to_string()),
        ("alpn", alpn.to_string()),
    ];
    if let Some(ref sni) = e.sni {
        params.push(("sni", sni.clone()));
    }
    if e.allow_insecure {
        params.push(("allow_insecure", "1".into()));
    }
    if e.zero_rtt {
        params.push(("zero_rtt_handshake", "1".into()));
    }
    params.push(("disable_sni", "0".into()));
    if let Some(ref ech) = e.ech_config {
        params.push(("ech", ech.clone()));
    }
    let qs = form_urlencode(&params);
    format!(
        "tuic://{}:{}@{}:{}?{qs}#{}",
        encode_uri_component(&e.uuid),
        encode_uri_component(&e.password),
        bracket_host(&e.host),
        e.port,
        encode_uri_component(&e.tag)
    )
}

/// Formats a raw NaiveProxy URL (`https://user:pass@host:port#tag` or `quic://...`).
pub fn naive_uri(e: &SubEntry) -> String {
    let scheme = match e.naive_proto.as_deref() {
        Some("quic") => "quic",
        _ => "https",
    };
    format!(
        "{scheme}://{}:{}@{}:{}#{}",
        encode_uri_component(&e.username),
        encode_uri_component(&e.password),
        bracket_host(&e.host),
        e.port,
        encode_uri_component(&e.tag)
    )
}

/// Formats a `naive+https://` or `naive+quic://` share link for multi-protocol clients.
pub fn naive_link(e: &SubEntry) -> String {
    format!("naive+{}", naive_uri(e))
}

/// Normalizes `settings.secretMode` for the Telegram WEB proxy (`plain` or `dd`).
pub fn tgweb_secret_mode(settings: &Value) -> String {
    match jstr(settings, &["secretMode"])
        .unwrap_or_else(|| "dd".into())
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "plain" => "plain".into(),
        _ => "dd".into(),
    }
}

/// Derives the 32-hex-char MTProxy secret from any per-user credential, deterministically.
///
/// The panel stores a random URL-safe credential per client; telemt needs exactly 16 raw bytes
/// (32 hex chars). Hashing keeps the value stable across restarts without a new DB column.
pub fn tgweb_user_secret_hex(credential: &str) -> String {
    let digest = Sha256::digest(format!("tgweb-secret-v1:{credential}").as_bytes());
    digest[..16].iter().map(|b| format!("{b:02x}")).collect()
}

/// Builds a Telegram Desktop `tg://webproxy` link.
///
/// The `server` value is `HOST` or `HOST/BASE` (percent-encoded). Without a base path the secret
/// is the legacy hex string; with one it is unpadded base64url of `0x70 || client_secret`, where
/// `client_secret` is the raw secret (`plain`) or `0xdd || secret` (`dd`).
pub fn tgweb_link(e: &SubEntry) -> Result<String, SubError> {
    if e.host.trim().is_empty() || is_ip_literal(e.host.trim()) {
        return Err(SubError::Invalid(
            "Telegram WEB proxy needs a DNS hostname (settings.domain), not an IP address".into(),
        ));
    }
    let hex = tgweb_user_secret_hex(&e.password);
    let dd = e.tgweb_secret_mode.as_deref() != Some("plain");
    let base = e
        .tgweb_base_path
        .as_deref()
        .map(|p| p.trim_matches('/'))
        .filter(|p| !p.is_empty());
    let (server, secret) = match base {
        None => (
            e.host.trim().to_string(),
            if dd { format!("dd{hex}") } else { hex },
        ),
        Some(path) => {
            let raw = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0))
                .collect::<Vec<u8>>();
            let mut payload = vec![0x70u8];
            if dd {
                payload.push(0xdd);
            }
            payload.extend_from_slice(&raw);
            (
                format!("{}/{}", e.host.trim(), path),
                URL_SAFE_NO_PAD.encode(payload),
            )
        }
    };
    Ok(format!(
        "tg://webproxy?server={}&secret={}",
        encode_uri_component(&server),
        secret
    ))
}

/// Builds a native Mieru client configuration JSON document, validating `portRange`.
pub fn mieru_config(e: &SubEntry) -> Result<Value, SubError> {
    let transport = e
        .mieru_transport
        .as_deref()
        .unwrap_or("tcp")
        .trim()
        .to_ascii_lowercase();
    let protocols: Vec<&'static str> = match transport.as_str() {
        "both" | "tcp,udp" => vec!["TCP", "UDP"],
        "udp" => vec!["UDP"],
        _ => vec!["TCP"],
    };

    let mut port_bindings = Vec::with_capacity(protocols.len());
    if let Some(ref pr) = e.mieru_port_range {
        let trimmed = pr.trim();
        let (a, b) = trimmed
            .split_once('-')
            .ok_or_else(|| SubError::Invalid(format!("Invalid Mieru portRange: {pr}")))?;
        let start: u16 = a
            .trim()
            .parse()
            .map_err(|_| SubError::Invalid(format!("Invalid Mieru portRange start: {pr}")))?;
        let end: u16 = b
            .trim()
            .parse()
            .map_err(|_| SubError::Invalid(format!("Invalid Mieru portRange end: {pr}")))?;
        if start < 1025 || start > end {
            return Err(SubError::Invalid(format!(
                "Mieru portRange must satisfy 1025 <= start <= end <= 65535: {pr}"
            )));
        }
        for proto in protocols {
            let mut m = Map::new();
            m.insert("portRange".into(), json!(trimmed));
            m.insert("protocol".into(), json!(proto));
            port_bindings.push(Value::Object(m));
        }
    } else {
        for proto in protocols {
            let mut m = Map::new();
            m.insert("port".into(), json!(e.port));
            m.insert("protocol".into(), json!(proto));
            port_bindings.push(Value::Object(m));
        }
    }

    let domain = if !e.host.is_empty() {
        e.host.clone()
    } else {
        e.mieru_domain
            .clone()
            .or_else(|| e.sni.clone())
            .unwrap_or_default()
    };

    Ok(json!({
        "profiles": [{
            "profileName": e.tag,
            "user": {
                "name": e.username,
                "password": e.password,
            },
            "servers": [{
                "domainName": domain,
                "portBindings": port_bindings,
            }]
        }],
        "activeProfile": e.tag,
        "socks5Port": 1080,
    }))
}

/// Formats an official Mieru simple share link (`mierus://user:pass@host?profile=...&port=...&protocol=...`).
pub fn mieru_link(e: &SubEntry) -> Result<String, SubError> {
    let cfg = mieru_config(e)?;
    let domain = cfg
        .pointer("/profiles/0/servers/0/domainName")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or(&e.host);
    let profile = cfg
        .pointer("/profiles/0/profileName")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or(&e.tag);

    let mut params: Vec<(&'static str, String)> = vec![
        ("profile", profile.to_string()),
        ("mtu", "1400".to_string()),
        ("multiplexing", "MULTIPLEXING_LOW".to_string()),
    ];
    let transport = e
        .mieru_transport
        .as_deref()
        .unwrap_or("tcp")
        .trim()
        .to_ascii_lowercase();
    let link_proto = if transport == "udp" { "UDP" } else { "TCP" };
    let port_spec = e
        .mieru_port_range
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| e.port.to_string());
    params.push(("port", port_spec));
    params.push(("protocol", link_proto.to_string()));
    let qs = form_urlencode(&params);

    Ok(format!(
        "mierus://{}:{}@{}?{qs}#{}",
        encode_uri_component(&e.username),
        encode_uri_component(&e.password),
        bracket_host(domain),
        encode_uri_component(profile),
    ))
}

/// Alias for [`mieru_link`].
pub fn mieru_uri(e: &SubEntry) -> Result<String, SubError> {
    mieru_link(e)
}

/// Formats an `anytls://password@host:port?...#tag` share URI.
pub fn anytls_uri(e: &SubEntry) -> String {
    let mut params: Vec<(&'static str, String)> = Vec::new();
    if let Some(ref sni) = e.sni {
        params.push(("sni", sni.clone()));
    }
    if let Some(ref fp) = e.fingerprint {
        params.push(("fp", fp.clone()));
    }
    if let Some(ref alpn) = e.alpn {
        params.push(("alpn", alpn.clone()));
    }
    if e.allow_insecure {
        params.push(("insecure", "1".into()));
    }
    if let Some(ref ech) = e.ech_config {
        params.push(("ech", ech.clone()));
    }
    let qs = if params.is_empty() {
        String::new()
    } else {
        format!("?{}", form_urlencode(&params))
    };
    format!(
        "anytls://{}@{}:{}{qs}#{}",
        encode_uri_component(&e.password),
        bracket_host(&e.host),
        e.port,
        encode_uri_component(&e.tag)
    )
}

/// Formats a `shadowtls://` (or Shadowsocks + ShadowTLS plugin) share URI.
pub fn shadowtls_uri(e: &SubEntry) -> String {
    if e.method.is_some() {
        return ss_uri(e);
    }
    let ver = e.shadowtls_version.unwrap_or(3);
    let pw = e.shadowtls_password.as_deref().unwrap_or(&e.password);
    let userinfo = STANDARD.encode(format!("{ver}:{pw}").as_bytes());
    let mut params: Vec<(&'static str, String)> = Vec::new();
    if let Some(ref sni) = e.sni {
        params.push(("sni", sni.clone()));
    }
    if let Some(ref fp) = e.fingerprint {
        params.push(("fp", fp.clone()));
    }
    let qs = if params.is_empty() {
        String::new()
    } else {
        format!("?{}", form_urlencode(&params))
    };
    format!(
        "shadowtls://{userinfo}@{}:{}{qs}#{}",
        bracket_host(&e.host),
        e.port,
        encode_uri_component(&e.tag)
    )
}

#[cfg(test)]
mod tgweb_tests {
    use super::*;

    fn entry(base: Option<&str>, mode: &str) -> SubEntry {
        SubEntry {
            protocol: Protocol::Tgweb,
            host: "tg.example.com".into(),
            port: 443,
            password: "client-credential".into(),
            tgweb_secret_mode: Some(mode.into()),
            tgweb_base_path: base.map(Into::into),
            ..SubEntry::default()
        }
    }

    #[test]
    fn secret_is_32_hex_and_stable() {
        let a = tgweb_user_secret_hex("client-credential");
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(a, tgweb_user_secret_hex("client-credential"));
        assert_ne!(a, tgweb_user_secret_hex("another"));
    }

    #[test]
    fn root_link_uses_hex_secret_with_dd_prefix() {
        let hex = tgweb_user_secret_hex("client-credential");
        assert_eq!(
            tgweb_link(&entry(None, "dd")).unwrap(),
            format!("tg://webproxy?server=tg.example.com&secret=dd{hex}")
        );
        assert_eq!(
            tgweb_link(&entry(None, "plain")).unwrap(),
            format!("tg://webproxy?server=tg.example.com&secret={hex}")
        );
    }

    #[test]
    fn path_link_encodes_server_and_base64url_secret() {
        let link = tgweb_link(&entry(Some("/telegram/web/"), "dd")).unwrap();
        assert!(link.starts_with("tg://webproxy?server=tg.example.com%2Ftelegram%2Fweb&secret="));
        let secret = link.rsplit("secret=").next().unwrap();
        let raw = URL_SAFE_NO_PAD.decode(secret).unwrap();
        // 0x70 marker, 0xdd (dd mode), then the 16 raw secret bytes.
        assert_eq!(raw.len(), 18);
        assert_eq!(&raw[..2], &[0x70, 0xdd]);
        let hex: String = raw[2..].iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, tgweb_user_secret_hex("client-credential"));

        let plain = tgweb_link(&entry(Some("telegram/web"), "plain")).unwrap();
        let raw = URL_SAFE_NO_PAD
            .decode(plain.rsplit("secret=").next().unwrap())
            .unwrap();
        assert_eq!(raw.len(), 17);
        assert_eq!(raw[0], 0x70);
    }

    #[test]
    fn ip_host_is_rejected_and_share_link_routes_to_tgweb() {
        let mut e = entry(None, "dd");
        e.host = "203.0.113.10".into();
        assert!(tgweb_link(&e).is_err());
        let ok = entry(None, "dd");
        assert!(share_link(&ok).unwrap().starts_with("tg://webproxy?"));
        assert!(display_link(&ok).unwrap().starts_with("tg://webproxy?"));
    }

    #[test]
    fn secret_mode_defaults_to_dd() {
        assert_eq!(tgweb_secret_mode(&json!({})), "dd");
        assert_eq!(tgweb_secret_mode(&json!({"secretMode": "PLAIN"})), "plain");
        assert_eq!(tgweb_secret_mode(&json!({"secretMode": "ee"})), "dd");
    }
}
