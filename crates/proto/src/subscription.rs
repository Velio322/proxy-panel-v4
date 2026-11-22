use crate::links::{
    apply_vless_route, build_uri, encode_uri_component, format_ss2022_key, is_ip_literal,
    parse_port_range, share_link, SubEntry, SubError,
};
use crate::wire::Protocol;
use age::armor::{ArmoredWriter, Format as ArmorFormat};
use age::secrecy::SecretString;
use base64::{engine::general_purpose, Engine as _};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use rand::rngs::OsRng;
use rand::RngCore;
use regex::Regex;
use rsa::pkcs1::{DecodeRsaPrivateKey, DecodeRsaPublicKey};
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
use rsa::{Oaep, Pkcs1v15Encrypt, RsaPrivateKey, RsaPublicKey};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::Sha256;
use std::collections::{BTreeMap, HashMap};
use std::io::Write;

const STANDARD: general_purpose::GeneralPurpose = general_purpose::STANDARD;

/// Subscription output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SubFormat {
    Base64,
    Json,
    Clash,
    Singbox,
    Original,
    Happ,
}

impl SubFormat {
    /// Parses `?flag=` query parameter (case-insensitive).
    pub fn from_flag(flag: &str) -> Result<Self, SubError> {
        match flag.trim().to_ascii_lowercase().as_str() {
            "base64" => Ok(Self::Base64),
            "json" => Ok(Self::Json),
            "clash" => Ok(Self::Clash),
            "singbox" | "sing-box" => Ok(Self::Singbox),
            "original" => Ok(Self::Original),
            "happ" => Ok(Self::Happ),
            _ => Err(SubError::Invalid("Invalid subscription format".into())),
        }
    }

    /// Resolves subscription format from explicit `?flag=` or User-Agent header sniffing.
    pub fn detect(flag: Option<&str>, user_agent: Option<&str>) -> Result<Self, SubError> {
        if let Some(f) = flag {
            return Self::from_flag(f);
        }
        Ok(detect_client_from_ua(user_agent.unwrap_or("")).0)
    }

    /// Canonical HTTP `Content-Type` header for this format.
    pub const fn content_type(&self) -> &'static str {
        match self {
            Self::Base64 | Self::Happ => "text/plain; charset=utf-8",
            Self::Json | Self::Singbox | Self::Original => "application/json; charset=utf-8",
            Self::Clash => "application/yaml; charset=utf-8",
        }
    }
}

/// Detected client profile derived from HTTP `User-Agent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientProfile {
    Generic,
    Clash,
    Mihomo,
    Stash,
    FlClashX,
    Singbox,
    Hiddify,
    Karing,
    Happ,
    Throne,
    Streisand,
}

/// Detects both [`SubFormat`] and [`ClientProfile`] from a client `User-Agent` string.
pub fn detect_client_from_ua(ua: &str) -> (SubFormat, ClientProfile) {
    let lower = ua.to_ascii_lowercase();
    if lower.contains("happ") {
        return (SubFormat::Happ, ClientProfile::Happ);
    }
    if lower.contains("flclashx") || lower.contains("flclash") {
        return (SubFormat::Clash, ClientProfile::FlClashX);
    }
    if lower.contains("mihomo") {
        return (SubFormat::Clash, ClientProfile::Mihomo);
    }
    if lower.contains("stash") {
        return (SubFormat::Clash, ClientProfile::Stash);
    }
    if lower.contains("clash") {
        return (SubFormat::Clash, ClientProfile::Clash);
    }
    if lower.contains("karing") {
        return (SubFormat::Singbox, ClientProfile::Karing);
    }
    if lower.contains("hiddify") {
        return (SubFormat::Singbox, ClientProfile::Hiddify);
    }
    if lower.contains("sing-box")
        || lower.contains("podkop")
        || lower.contains("sfa")
        || lower.contains("sfi")
        || lower.contains("sfm")
        || lower.contains("sft")
    {
        return (SubFormat::Singbox, ClientProfile::Singbox);
    }
    if lower.contains("throne") {
        return (SubFormat::Base64, ClientProfile::Throne);
    }
    if lower.contains("streisand") {
        return (SubFormat::Base64, ClientProfile::Streisand);
    }
    (SubFormat::Base64, ClientProfile::Generic)
}

/// Subscriber traffic/quota metadata emitted in HTTP subscription headers and JSON payloads.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UserInfo {
    pub username: String,
    pub uuid: String,
    pub upload: u64,
    pub download: u64,
    pub total: u64,
    pub expire: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_interval: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_web_page_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub announce: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing_url: Option<String>,
}

/// Builds HTTP response headers including RFC 5987 `Content-Disposition` and Happ metadata.
pub fn headers(
    info: &UserInfo,
    format: SubFormat,
    profile: ClientProfile,
) -> Vec<(String, String)> {
    let interval = info.update_interval.unwrap_or(12);
    let title = info
        .profile_title
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(&info.username);

    let clean_username: String = info
        .username
        .chars()
        .filter(|c| !matches!(c, '\r' | '\n' | '\0'))
        .collect();
    let ascii_filename: String = clean_username
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let ascii_fallback = if ascii_filename.is_empty() {
        "subscription".to_string()
    } else {
        ascii_filename
    };

    let mut hdrs = vec![
        ("Profile-Update-Interval".into(), interval.to_string()),
        (
            "Subscription-Userinfo".into(),
            format!(
                "upload={}; download={}; total={}; expire={}",
                info.upload, info.download, info.total, info.expire
            ),
        ),
        (
            "Profile-Title".into(),
            format!("base64:{}", STANDARD.encode(title.as_bytes())),
        ),
        (
            "Content-Disposition".into(),
            format!(
                "attachment; filename=\"{ascii_fallback}\"; filename*=UTF-8''{}",
                encode_uri_component(&clean_username)
            ),
        ),
    ];

    if format == SubFormat::Happ || profile == ClientProfile::Happ {
        if let Some(sup) = info.support_url.as_deref().filter(|s| !s.is_empty()) {
            hdrs.push(("Support-Url".into(), sup.to_string()));
        }
        if let Some(web) = info
            .profile_web_page_url
            .as_deref()
            .filter(|s| !s.is_empty())
        {
            hdrs.push(("Profile-Web-Page-Url".into(), web.to_string()));
        }
        if let Some(ann) = info.announce.as_deref().filter(|s| !s.is_empty()) {
            hdrs.push((
                "Announce".into(),
                format!("base64:{}", STANDARD.encode(ann.as_bytes())),
            ));
        }
        if let Some(r_url) = info.routing_url.as_deref().filter(|s| !s.is_empty()) {
            hdrs.push(("Routing".into(), r_url.to_string()));
        }
    }

    hdrs
}

/// Deduplicates node tags deterministically: `"tag"`, `"tag (2)"`, `"tag (3)"`, ....
pub fn dedup_tag(seen: &mut HashMap<String, usize>, raw_tag: &str) -> String {
    let base = if raw_tag.is_empty() { "node" } else { raw_tag };
    let count = seen.entry(base.to_string()).or_insert(0);
    *count += 1;
    if *count == 1 {
        base.to_string()
    } else {
        format!("{base} ({})", *count)
    }
}

fn net(e: &SubEntry) -> &str {
    e.normalized_net()
}

fn vmess_cipher(e: &SubEntry) -> &'static str {
    match e.vmess_cipher.as_deref() {
        Some("aes-128-gcm") => "aes-128-gcm",
        Some("chacha20-poly1305") => "chacha20-poly1305",
        Some("none") => "none",
        Some("zero") => "zero",
        _ => "auto",
    }
}

/// Remote or local `.srs` rule set entry for Sing-box 1.14+ `route.rule_set`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SrsRuleSet {
    pub tag: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_detour: Option<String>,
}

/// Options controlling Sing-box 1.14+ rendering.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SingboxRenderOptions {
    #[serde(default)]
    pub srs_rule_sets: Vec<SrsRuleSet>,
    #[serde(default = "default_true")]
    pub enable_dns_114: bool,
}

impl Default for SingboxRenderOptions {
    fn default() -> Self {
        Self {
            srs_rule_sets: Vec::new(),
            enable_dns_114: true,
        }
    }
}

const fn default_true() -> bool {
    true
}

/// Renders Base64 subscription (`base64std(links.join('\n'))`) via canonical [`share_link`].
pub fn render_base64(entries: &[SubEntry]) -> Result<String, SubError> {
    let mut lines = Vec::with_capacity(entries.len());
    for e in entries {
        lines.push(share_link(e)?);
    }
    Ok(STANDARD.encode(lines.join("\n").as_bytes()))
}

/// Renders `flag=json` payload matching `02-http-api.md` §5.1 plus structured `entries` for UI rendering.
pub fn render_json(entries: &[SubEntry], info: &UserInfo) -> Result<String, SubError> {
    let mut proxies: Vec<Value> = Vec::with_capacity(entries.len());
    let mut links: Vec<String> = Vec::with_capacity(entries.len());
    let mut structured: Vec<Value> = Vec::with_capacity(entries.len());

    for e in entries {
        let share = share_link(e)?;
        let raw = if e.raw.is_empty() {
            build_uri(e)?
        } else {
            e.raw.clone()
        };
        match e.protocol {
            Protocol::Mieru => {
                let parsed: Value = serde_json::from_str(&raw).map_err(|err| {
                    SubError::Invalid(format!("Invalid Mieru JSON configuration: {err}"))
                })?;
                proxies.push(parsed.clone());
                links.push(share.clone());
                structured.push(json!({
                    "protocol": e.protocol.as_str(),
                    "tag": e.tag,
                    "host": e.host,
                    "port": e.port,
                    "link": share,
                    "rawUri": share,
                    "config": parsed,
                }));
            }
            Protocol::Naiveproxy => {
                let proxy_no_frag = raw.split('#').next().unwrap_or(&raw);
                let naive_cfg = json!({
                    "listen": "socks://127.0.0.1:1080",
                    "proxy": proxy_no_frag,
                });
                proxies.push(naive_cfg.clone());
                links.push(share.clone());
                structured.push(json!({
                    "protocol": e.protocol.as_str(),
                    "tag": e.tag,
                    "host": e.host,
                    "port": e.port,
                    "link": share,
                    "rawUri": raw,
                    "config": naive_cfg,
                }));
            }
            _ => {
                proxies.push(Value::String(share.clone()));
                links.push(share.clone());
                structured.push(json!({
                    "protocol": e.protocol.as_str(),
                    "tag": e.tag,
                    "host": e.host,
                    "port": e.port,
                    "link": share,
                    "rawUri": raw,
                    "config": Value::Null,
                }));
            }
        }
    }

    let payload = json!({
        "client": {
            "username": info.username,
            "uuid": info.uuid,
            "upload": info.upload,
            "download": info.download,
            "total": info.total,
            "expire": info.expire,
        },
        "proxies": proxies,
        "links": links,
        "entries": structured,
    });

    serde_json::to_string_pretty(&payload)
        .map_err(|err| SubError::Invalid(format!("JSON serialization failed: {err}")))
}

fn split_alpn(alpn: Option<&String>) -> Vec<String> {
    alpn.map(|s| {
        s.split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(ToString::to_string)
            .collect()
    })
    .unwrap_or_default()
}

fn insert_clash_transport(p: &mut Map<String, Value>, e: &SubEntry) {
    let network = net(e);
    match network {
        "ws" => {
            p.insert("network".into(), json!("ws"));
            let mut ws_opts = Map::new();
            ws_opts.insert(
                "path".into(),
                json!(e
                    .ws_path
                    .clone()
                    .or_else(|| e.stream_path.clone())
                    .unwrap_or_else(|| "/".into())),
            );
            if let Some(ref h) = e.ws_host {
                ws_opts.insert("headers".into(), json!({ "Host": h }));
            }
            p.insert("ws-opts".into(), Value::Object(ws_opts));
        }
        "httpupgrade" => {
            // Mihomo represents HTTPUpgrade via ws-opts + v2ray-http-upgrade: true
            p.insert("network".into(), json!("ws"));
            let mut ws_opts = Map::new();
            ws_opts.insert(
                "path".into(),
                json!(e
                    .httpupgrade_path
                    .clone()
                    .or_else(|| e.stream_path.clone())
                    .unwrap_or_else(|| "/".into())),
            );
            ws_opts.insert("v2ray-http-upgrade".into(), json!(true));
            if let Some(ref h) = e.httpupgrade_host {
                ws_opts.insert("headers".into(), json!({ "Host": h }));
            }
            p.insert("ws-opts".into(), Value::Object(ws_opts));
        }
        "grpc" => {
            p.insert("network".into(), json!("grpc"));
            p.insert(
                "grpc-opts".into(),
                json!({
                    "grpc-service-name": e.grpc_service_name.clone().unwrap_or_default(),
                }),
            );
        }
        "h2" => {
            p.insert("network".into(), json!("h2"));
            let mut h2_opts = Map::new();
            h2_opts.insert(
                "path".into(),
                json!(e
                    .h2_path
                    .clone()
                    .or_else(|| e.stream_path.clone())
                    .unwrap_or_else(|| "/".into())),
            );
            if let Some(ref h) = e.h2_host {
                h2_opts.insert("host".into(), json!([h]));
            }
            p.insert("h2-opts".into(), Value::Object(h2_opts));
        }
        "xhttp" => {
            p.insert("network".into(), json!("xhttp"));
            let mut xh_opts = Map::new();
            xh_opts.insert(
                "path".into(),
                json!(e
                    .xhttp_path
                    .clone()
                    .or_else(|| e.stream_path.clone())
                    .unwrap_or_else(|| "/".into())),
            );
            if let Some(ref h) = e.xhttp_host {
                xh_opts.insert("host".into(), json!(h));
            }
            if let Some(ref m) = e.xhttp_mode {
                xh_opts.insert("mode".into(), json!(m));
            }
            if let Some(Value::Object(extra)) = e.xhttp_extra.as_ref() {
                if let Some(hdrs) = extra.get("headers").filter(|v| v.is_object()) {
                    xh_opts.insert("headers".into(), hdrs.clone());
                }
                if let Some(v) = extra.get("noGRPCHeader").filter(|v| !v.is_null()) {
                    xh_opts.insert("no-grpc-header".into(), v.clone());
                }
                if let Some(v) = extra.get("xPaddingBytes").filter(|v| !v.is_null()) {
                    xh_opts.insert("x-padding-bytes".into(), v.clone());
                }
                if let Some(v) = extra.get("scMaxEachPostBytes").filter(|v| !v.is_null()) {
                    xh_opts.insert("sc-max-each-post-bytes".into(), v.clone());
                }
                if let Some(v) = extra.get("scMinPostsIntervalMs").filter(|v| !v.is_null()) {
                    xh_opts.insert("sc-min-posts-interval-ms".into(), v.clone());
                }
                if let Some(xmux) = extra.get("xmux").filter(|v| v.is_object()) {
                    xh_opts.insert("xmux".into(), xmux.clone());
                }
            }
            p.insert("xhttp-opts".into(), Value::Object(xh_opts));
        }
        "tcp" | "raw" if e.tcp_header_type.as_deref() == Some("http") => {
            p.insert("network".into(), json!("http"));
            let mut http_opts = Map::new();
            http_opts.insert(
                "path".into(),
                json!([e.tcp_header_path.as_deref().unwrap_or("/")]),
            );
            if let Some(ref h) = e.tcp_header_host {
                http_opts.insert("headers".into(), json!({ "Host": [h] }));
            }
            p.insert("http-opts".into(), Value::Object(http_opts));
        }
        other => {
            p.insert("network".into(), json!(other));
        }
    }
}

fn clash_apply_tls_extras(p: &mut Map<String, Value>, e: &SubEntry) {
    if let Some(ref ip_ver) = e.mihomo_ip_version {
        if !ip_ver.trim().is_empty() {
            p.insert("ip-version".into(), json!(ip_ver.trim()));
        }
    }
    if let Some(ref pcs) = e.pinned_peer_sha256 {
        if !pcs.trim().is_empty() {
            p.insert("fingerprint".into(), json!(pcs.trim()));
        }
    }
    if let Some(ref ech) = e.ech_config {
        if !ech.trim().is_empty() {
            p.insert(
                "ech-opts".into(),
                json!({ "enable": true, "config": ech.trim() }),
            );
        }
    }
}

fn clash_proxy(e: &SubEntry, uname: &str) -> Result<Option<Value>, SubError> {
    let mut p = Map::new();
    match e.protocol {
        Protocol::Vless => {
            let uuid = if let Some(ref rid) = e.vless_route_id {
                apply_vless_route(&e.uuid, rid)?
            } else {
                e.uuid.clone()
            };
            p.insert("name".into(), json!(uname));
            p.insert("type".into(), json!("vless"));
            p.insert("server".into(), json!(e.host));
            p.insert("port".into(), json!(e.port));
            p.insert("uuid".into(), json!(uuid));
            p.insert("udp".into(), json!(true));
            p.insert("packet-encoding".into(), json!("xudp"));
            if let Some(ref flow) = e.flow {
                if !flow.is_empty() {
                    p.insert("flow".into(), json!(flow));
                }
            }
            p.insert("tls".into(), json!(e.is_tls()));
            p.insert("skip-cert-verify".into(), json!(e.allow_insecure));
            if let Some(ref sni) = e.sni {
                p.insert("servername".into(), json!(sni));
            }
            if let Some(ref fp) = e.fingerprint {
                p.insert("client-fingerprint".into(), json!(fp));
            }
            let alpn = split_alpn(e.alpn.as_ref());
            if !alpn.is_empty() {
                p.insert("alpn".into(), json!(alpn));
            }
            if e.security.as_deref() == Some("reality") {
                p.insert(
                    "reality-opts".into(),
                    json!({
                        "public-key": e.public_key.clone().unwrap_or_default(),
                        "short-id": e.short_id.clone().unwrap_or_default(),
                    }),
                );
            }
            clash_apply_tls_extras(&mut p, e);
            insert_clash_transport(&mut p, e);
            Ok(Some(Value::Object(p)))
        }
        Protocol::Vmess => {
            p.insert("name".into(), json!(uname));
            p.insert("type".into(), json!("vmess"));
            p.insert("server".into(), json!(e.host));
            p.insert("port".into(), json!(e.port));
            p.insert("uuid".into(), json!(e.uuid));
            p.insert("alterId".into(), json!(e.alter_id));
            p.insert("cipher".into(), json!(vmess_cipher(e)));
            p.insert("udp".into(), json!(true));
            p.insert("packet-encoding".into(), json!("xudp"));
            p.insert("tls".into(), json!(e.is_tls()));
            p.insert("skip-cert-verify".into(), json!(e.allow_insecure));
            if let Some(ref sni) = e.sni {
                p.insert("servername".into(), json!(sni));
            }
            if let Some(ref fp) = e.fingerprint {
                p.insert("client-fingerprint".into(), json!(fp));
            }
            let alpn = split_alpn(e.alpn.as_ref());
            if !alpn.is_empty() {
                p.insert("alpn".into(), json!(alpn));
            }
            clash_apply_tls_extras(&mut p, e);
            insert_clash_transport(&mut p, e);
            Ok(Some(Value::Object(p)))
        }
        Protocol::Trojan => {
            p.insert("name".into(), json!(uname));
            p.insert("type".into(), json!("trojan"));
            p.insert("server".into(), json!(e.host));
            p.insert("port".into(), json!(e.port));
            p.insert("password".into(), json!(e.password));
            p.insert("udp".into(), json!(true));
            p.insert("skip-cert-verify".into(), json!(e.allow_insecure));
            if let Some(ref sni) = e.sni {
                p.insert("sni".into(), json!(sni));
            }
            if let Some(ref fp) = e.fingerprint {
                p.insert("client-fingerprint".into(), json!(fp));
            }
            let alpn = split_alpn(e.alpn.as_ref());
            if !alpn.is_empty() {
                p.insert("alpn".into(), json!(alpn));
            }
            if e.security.as_deref() == Some("reality") {
                p.insert(
                    "reality-opts".into(),
                    json!({
                        "public-key": e.public_key.clone().unwrap_or_default(),
                        "short-id": e.short_id.clone().unwrap_or_default(),
                    }),
                );
            }
            clash_apply_tls_extras(&mut p, e);
            insert_clash_transport(&mut p, e);
            Ok(Some(Value::Object(p)))
        }
        Protocol::Shadowsocks => {
            p.insert("name".into(), json!(uname));
            p.insert("type".into(), json!("ss"));
            p.insert("server".into(), json!(e.host));
            p.insert("port".into(), json!(e.port));
            p.insert(
                "cipher".into(),
                json!(e.method.as_deref().unwrap_or("aes-256-gcm")),
            );
            p.insert("password".into(), json!(format_ss2022_key(e)));
            p.insert("udp".into(), json!(true));
            if let Some(ref st_pw) = e.shadowtls_password {
                p.insert("plugin".into(), json!("shadow-tls"));
                p.insert(
                    "plugin-opts".into(),
                    json!({
                        "host": e.sni.as_deref().unwrap_or(&e.host),
                        "password": st_pw,
                        "version": e.shadowtls_version.unwrap_or(3),
                    }),
                );
            }
            clash_apply_tls_extras(&mut p, e);
            Ok(Some(Value::Object(p)))
        }
        Protocol::Hysteria2 => {
            p.insert("name".into(), json!(uname));
            p.insert("type".into(), json!("hysteria2"));
            p.insert("server".into(), json!(e.host));
            p.insert("port".into(), json!(e.port));
            if let Some(ref pr) = e.port_range {
                parse_port_range(pr)?;
                p.insert("ports".into(), json!(pr));
                p.insert("hop-interval".into(), json!(30));
            }
            p.insert("password".into(), json!(e.password));
            if let Some(ref sni) = e.sni {
                p.insert("sni".into(), json!(sni));
            }
            p.insert("skip-cert-verify".into(), json!(e.allow_insecure));
            let alpn = split_alpn(e.alpn.as_ref());
            if !alpn.is_empty() {
                p.insert("alpn".into(), json!(alpn));
            }
            if let Some(ref obfs) = e.obfs_type {
                if obfs != "none" && !obfs.is_empty() {
                    p.insert("obfs".into(), json!(obfs));
                    p.insert(
                        "obfs-password".into(),
                        json!(e.obfs_password.clone().unwrap_or_default()),
                    );
                }
            }
            if let Some(up) = e.upload_mbps {
                p.insert("up".into(), json!(format!("{up} Mbps")));
            }
            if let Some(down) = e.download_mbps {
                p.insert("down".into(), json!(format!("{down} Mbps")));
            }
            clash_apply_tls_extras(&mut p, e);
            Ok(Some(Value::Object(p)))
        }
        Protocol::Tuic => {
            p.insert("name".into(), json!(uname));
            p.insert("type".into(), json!("tuic"));
            p.insert("server".into(), json!(e.host));
            p.insert("port".into(), json!(e.port));
            p.insert("uuid".into(), json!(e.uuid));
            p.insert("password".into(), json!(e.password));
            p.insert(
                "congestion-controller".into(),
                json!(e.congestion_control.as_deref().unwrap_or("bbr")),
            );
            p.insert(
                "udp-relay-mode".into(),
                json!(e.tuic_udp_relay_mode.as_deref().unwrap_or("native")),
            );
            p.insert("disable-sni".into(), json!(false));
            p.insert("skip-cert-verify".into(), json!(e.allow_insecure));
            if e.zero_rtt {
                p.insert("reduce-rtt".into(), json!(true));
            }
            if let Some(ref hb) = e.tuic_heartbeat {
                if !hb.trim().is_empty() {
                    p.insert("heartbeat-interval".into(), json!(hb));
                }
            }
            if let Some(ref sni) = e.sni {
                p.insert("sni".into(), json!(sni));
            }
            let alpn = split_alpn(e.alpn.as_ref());
            if !alpn.is_empty() {
                p.insert("alpn".into(), json!(alpn));
            }
            clash_apply_tls_extras(&mut p, e);
            Ok(Some(Value::Object(p)))
        }
        Protocol::Anytls => {
            p.insert("name".into(), json!(uname));
            p.insert("type".into(), json!("anytls"));
            p.insert("server".into(), json!(e.host));
            p.insert("port".into(), json!(e.port));
            p.insert("password".into(), json!(e.password));
            p.insert("udp".into(), json!(true));
            p.insert("skip-cert-verify".into(), json!(e.allow_insecure));
            if let Some(ref sni) = e.sni {
                p.insert("sni".into(), json!(sni));
            }
            if let Some(ref fp) = e.fingerprint {
                p.insert("client-fingerprint".into(), json!(fp));
            }
            let alpn = split_alpn(e.alpn.as_ref());
            if !alpn.is_empty() {
                p.insert("alpn".into(), json!(alpn));
            }
            if let Some(ref chk) = e.anytls_idle_check {
                p.insert("idle-session-check-interval".into(), json!(chk));
            }
            if let Some(ref to) = e.anytls_idle_timeout {
                p.insert("idle-session-timeout".into(), json!(to));
            }
            if let Some(min) = e.anytls_min_idle {
                p.insert("min-idle-session".into(), json!(min));
            }
            clash_apply_tls_extras(&mut p, e);
            Ok(Some(Value::Object(p)))
        }
        Protocol::Mieru => {
            p.insert("name".into(), json!(uname));
            p.insert("type".into(), json!("mieru"));
            p.insert("server".into(), json!(e.host));
            if let Some(ref pr) = e.mieru_port_range {
                let trimmed = pr.trim();
                if !trimmed.is_empty() {
                    p.insert("port-range".into(), json!(trimmed));
                } else {
                    p.insert("port".into(), json!(e.port));
                }
            } else {
                p.insert("port".into(), json!(e.port));
            }
            let transport = match e
                .mieru_transport
                .as_deref()
                .unwrap_or("tcp")
                .trim()
                .to_ascii_lowercase()
                .as_str()
            {
                "udp" => "UDP",
                _ => "TCP",
            };
            p.insert("transport".into(), json!(transport));
            p.insert("username".into(), json!(e.username));
            p.insert("password".into(), json!(e.password));
            p.insert("multiplexing".into(), json!("MULTIPLEXING_LOW"));
            Ok(Some(Value::Object(p)))
        }
        Protocol::Naiveproxy | Protocol::Shadowtls => Ok(None),
    }
}

/// Renders Mihomo / Clash Meta YAML (with tag dedup and `["DIRECT"]` fallback).
pub fn render_clash(entries: &[SubEntry]) -> Result<String, SubError> {
    let mut proxies: Vec<Value> = Vec::new();
    let mut names: Vec<Value> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();

    for e in entries {
        let candidate = dedup_tag(&mut seen, &e.tag);
        if let Some(proxy) = clash_proxy(e, &candidate)? {
            names.push(json!(candidate));
            proxies.push(proxy);
        }
    }

    if !entries.is_empty() && proxies.is_empty() {
        return Err(SubError::Unsupported(
            "This Clash export does not support Mieru/Naive; request native JSON instead".into(),
        ));
    }

    let auto_proxies = if names.is_empty() {
        vec![json!("DIRECT")]
    } else {
        names.clone()
    };
    let select_proxies = if names.is_empty() {
        vec![json!("DIRECT")]
    } else {
        let mut s = vec![json!("AUTO")];
        s.extend(names);
        s.push(json!("DIRECT"));
        s
    };

    let doc = json!({
        "mixed-port": 7890,
        "allow-lan": false,
        "mode": "rule",
        "log-level": "info",
        "proxies": proxies,
        "proxy-groups": [
            {
                "name": "PROXY",
                "type": "select",
                "proxies": select_proxies,
            },
            {
                "name": "AUTO",
                "type": "url-test",
                "proxies": auto_proxies,
                "url": "https://www.gstatic.com/generate_204",
                "interval": 300,
            }
        ],
        "rules": [
            "GEOIP,CN,DIRECT",
            "MATCH,PROXY"
        ]
    });

    serde_yaml::to_string(&doc)
        .map_err(|err| SubError::Invalid(format!("Clash YAML serialization failed: {err}")))
}

fn is_xhttp_net(e: &SubEntry) -> bool {
    matches!(net(e), "xhttp" | "splithttp")
}

fn singbox_server(e: &SubEntry) -> &str {
    if let Some(ip) = e
        .resolved_ip
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if !e.is_cdn || e.security.as_deref() == Some("reality") {
            return ip;
        }
    }
    &e.host
}

fn is_valid_sni_host(candidate: &str) -> bool {
    !candidate.is_empty() && !is_ip_literal(candidate)
}

fn singbox_sni(e: &SubEntry) -> Option<String> {
    let raw = e
        .sni
        .as_deref()
        .map(str::trim)
        .filter(|s| is_valid_sni_host(s));
    if e.security.as_deref() == Some("reality") {
        return Some(raw.unwrap_or("www.google.com").to_string());
    }
    raw.map(ToString::to_string)
}

fn singbox_utls_fingerprint(e: &SubEntry) -> Option<String> {
    let raw = e
        .fingerprint
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let is_reality_or_xhttp = e.security.as_deref() == Some("reality") || is_xhttp_net(e);
    if is_reality_or_xhttp {
        // Keep the fingerprint configured on the inbound (`fp` in share links) verbatim
        // so every format carries the same TLS/Reality identity; fall back to Xray's
        // default `chrome` only when the inbound stores no fingerprint.
        return Some(raw.unwrap_or("chrome").to_string());
    }
    raw.map(ToString::to_string)
}

fn extract_xhttp_padding(e: &SubEntry) -> String {
    if let Some(Value::Object(extra)) = e.xhttp_extra.as_ref() {
        if let Some(val) = extra
            .get("xPaddingBytes")
            .or_else(|| extra.get("x_padding_bytes"))
        {
            if let Some(s) = val.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                return s.to_string();
            }
            if let Some(n) = val.as_u64() {
                return n.to_string();
            }
        }
    }
    "100-1000".to_string()
}

fn singbox_tls(e: &SubEntry) -> Value {
    let mut tls = Map::new();
    tls.insert("enabled".into(), json!(true));
    if let Some(sni) = singbox_sni(e) {
        tls.insert("server_name".into(), json!(sni));
    }
    if e.allow_insecure && e.security.as_deref() != Some("reality") {
        tls.insert("insecure".into(), json!(true));
    }
    let alpn = split_alpn(e.alpn.as_ref());
    if !alpn.is_empty() {
        tls.insert("alpn".into(), json!(alpn));
    } else if is_xhttp_net(e) {
        tls.insert("alpn".into(), json!(["h2", "http/1.1"]));
    }
    if e.security.as_deref() == Some("reality") {
        tls.insert(
            "reality".into(),
            json!({
                "enabled": true,
                "public_key": e.public_key.clone().unwrap_or_default(),
                "short_id": e.short_id.clone().unwrap_or_default(),
            }),
        );
    }
    if let Some(fp) = singbox_utls_fingerprint(e) {
        tls.insert(
            "utls".into(),
            json!({
                "enabled": true,
                "fingerprint": fp,
            }),
        );
    }
    if let Some(ref ech) = e.ech_config {
        tls.insert(
            "ech".into(),
            json!({
                "enabled": true,
                "config": [ech],
            }),
        );
    }
    Value::Object(tls)
}

fn singbox_transport(e: &SubEntry) -> Option<Value> {
    match net(e) {
        "ws" => {
            let mut t = Map::new();
            t.insert("type".into(), json!("ws"));
            t.insert(
                "path".into(),
                json!(e
                    .ws_path
                    .clone()
                    .or_else(|| e.stream_path.clone())
                    .unwrap_or_else(|| "/".into())),
            );
            if let Some(ref h) = e.ws_host {
                t.insert("headers".into(), json!({ "Host": h }));
            }
            Some(Value::Object(t))
        }
        "grpc" => Some(json!({
            "type": "grpc",
            "service_name": e.grpc_service_name.clone().unwrap_or_default(),
        })),
        "httpupgrade" => {
            let mut t = Map::new();
            t.insert("type".into(), json!("httpupgrade"));
            t.insert(
                "path".into(),
                json!(e
                    .httpupgrade_path
                    .clone()
                    .or_else(|| e.stream_path.clone())
                    .unwrap_or_else(|| "/".into())),
            );
            if let Some(ref h) = e.httpupgrade_host {
                t.insert("host".into(), json!(h));
            }
            Some(Value::Object(t))
        }
        "xhttp" | "splithttp" => {
            let mut t = Map::new();
            t.insert("type".into(), json!("xhttp"));
            let path = e
                .xhttp_path
                .as_deref()
                .or(e.stream_path.as_deref())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("/");
            t.insert("path".into(), json!(path));
            let mode = match e.xhttp_mode.as_deref().map(str::trim) {
                Some("packet-up") => "packet-up",
                Some("stream-up") => "stream-up",
                Some("stream-one") => "stream-one",
                _ => "packet-up",
            };
            t.insert("mode".into(), json!(mode));
            t.insert("x_padding_bytes".into(), json!(extract_xhttp_padding(e)));
            let host = if e.security.as_deref() == Some("reality") {
                e.xhttp_host
                    .as_deref()
                    .map(str::trim)
                    .filter(|h| is_valid_sni_host(h))
                    .map(ToString::to_string)
                    .or_else(|| singbox_sni(e))
            } else {
                e.xhttp_host
                    .as_deref()
                    .map(str::trim)
                    .filter(|h| !h.is_empty())
                    .map(ToString::to_string)
            };
            if let Some(h) = host {
                t.insert("host".into(), json!(h));
            }
            if let Some(Value::Object(extra)) = e.xhttp_extra.as_ref() {
                if let Some(hdrs) = extra.get("headers").and_then(Value::as_object) {
                    if !hdrs.is_empty() {
                        t.insert("headers".into(), Value::Object(hdrs.clone()));
                    }
                }
                if extra
                    .get("noGRPCHeader")
                    .or_else(|| extra.get("no_grpc_header"))
                    .and_then(Value::as_bool)
                    == Some(true)
                {
                    t.insert("no_grpc_header".into(), json!(true));
                }
            }
            Some(Value::Object(t))
        }
        "h2" => {
            let mut t = Map::new();
            t.insert("type".into(), json!("http"));
            t.insert(
                "path".into(),
                json!(e
                    .h2_path
                    .clone()
                    .or_else(|| e.stream_path.clone())
                    .unwrap_or_else(|| "/".into())),
            );
            if let Some(ref h) = e.h2_host {
                t.insert("host".into(), json!([h]));
            }
            Some(Value::Object(t))
        }
        _ => None,
    }
}

pub(crate) fn singbox_outbound(e: &SubEntry, uname: &str) -> Result<Option<Value>, SubError> {
    let mut out = Map::new();
    let server = singbox_server(e);
    match e.protocol {
        Protocol::Vless => {
            let uuid = if let Some(ref rid) = e.vless_route_id {
                apply_vless_route(&e.uuid, rid)?
            } else {
                e.uuid.clone()
            };
            out.insert("type".into(), json!("vless"));
            out.insert("tag".into(), json!(uname));
            out.insert("server".into(), json!(server));
            out.insert("server_port".into(), json!(e.port));
            out.insert("uuid".into(), json!(uuid));
            if !is_xhttp_net(e) {
                out.insert("packet_encoding".into(), json!("xudp"));
                if let Some(ref flow) = e.flow {
                    let trimmed = flow.trim();
                    if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("none") {
                        out.insert("flow".into(), json!(trimmed));
                    }
                }
            }
            if e.is_tls() {
                out.insert("tls".into(), singbox_tls(e));
            }
            if let Some(tr) = singbox_transport(e) {
                out.insert("transport".into(), tr);
            }
            Ok(Some(Value::Object(out)))
        }
        Protocol::Vmess => {
            out.insert("type".into(), json!("vmess"));
            out.insert("tag".into(), json!(uname));
            out.insert("server".into(), json!(server));
            out.insert("server_port".into(), json!(e.port));
            out.insert("uuid".into(), json!(e.uuid));
            out.insert("security".into(), json!(vmess_cipher(e)));
            out.insert("alter_id".into(), json!(e.alter_id));
            out.insert("packet_encoding".into(), json!("xudp"));
            if e.is_tls() {
                out.insert("tls".into(), singbox_tls(e));
            }
            if let Some(tr) = singbox_transport(e) {
                out.insert("transport".into(), tr);
            }
            Ok(Some(Value::Object(out)))
        }
        Protocol::Trojan => {
            out.insert("type".into(), json!("trojan"));
            out.insert("tag".into(), json!(uname));
            out.insert("server".into(), json!(server));
            out.insert("server_port".into(), json!(e.port));
            out.insert("password".into(), json!(e.password));
            out.insert("tls".into(), singbox_tls(e));
            if let Some(tr) = singbox_transport(e) {
                out.insert("transport".into(), tr);
            }
            Ok(Some(Value::Object(out)))
        }
        Protocol::Shadowsocks => {
            out.insert("type".into(), json!("shadowsocks"));
            out.insert("tag".into(), json!(uname));
            out.insert("server".into(), json!(server));
            out.insert("server_port".into(), json!(e.port));
            out.insert(
                "method".into(),
                json!(e.method.as_deref().unwrap_or("aes-256-gcm")),
            );
            out.insert("password".into(), json!(format_ss2022_key(e)));
            Ok(Some(Value::Object(out)))
        }
        Protocol::Hysteria2 => {
            out.insert("type".into(), json!("hysteria2"));
            out.insert("tag".into(), json!(uname));
            out.insert("server".into(), json!(server));
            if let Some(ref pr) = e.port_range {
                let ports = parse_port_range(pr)?;
                if let (Some(&first), Some(&last)) = (ports.first(), ports.last()) {
                    out.insert("server_port".into(), json!(first));
                    out.insert("server_ports".into(), json!([format!("{first}:{last}")]));
                    out.insert("hop_interval".into(), json!("30s"));
                } else {
                    out.insert("server_port".into(), json!(e.port));
                }
            } else {
                out.insert("server_port".into(), json!(e.port));
            }
            out.insert("password".into(), json!(e.password));
            if let Some(ref bbr) = e.bbr_profile {
                out.insert("bbr_profile".into(), json!(bbr));
            }
            if e.disable_chrome_parrot {
                out.insert("disable_chrome_parrot".into(), json!(true));
            }
            if let Some(up) = e.upload_mbps {
                out.insert("up_mbps".into(), json!(up));
            }
            if let Some(down) = e.download_mbps {
                out.insert("down_mbps".into(), json!(down));
            }
            if let Some(ref obfs) = e.obfs_type {
                if obfs != "none" && !obfs.is_empty() {
                    out.insert(
                        "obfs".into(),
                        json!({
                            "type": obfs,
                            "password": e.obfs_password.clone().unwrap_or_default(),
                        }),
                    );
                }
            }
            out.insert("tls".into(), singbox_tls(e));
            Ok(Some(Value::Object(out)))
        }
        Protocol::Tuic => {
            out.insert("type".into(), json!("tuic"));
            out.insert("tag".into(), json!(uname));
            out.insert("server".into(), json!(server));
            out.insert("server_port".into(), json!(e.port));
            out.insert("uuid".into(), json!(e.uuid));
            out.insert("password".into(), json!(e.password));
            out.insert(
                "congestion_control".into(),
                json!(e.congestion_control.as_deref().unwrap_or("bbr")),
            );
            out.insert(
                "udp_relay_mode".into(),
                json!(e.tuic_udp_relay_mode.as_deref().unwrap_or("native")),
            );
            out.insert("zero_rtt_handshake".into(), json!(e.zero_rtt));
            if let Some(ref at) = e.tuic_auth_timeout {
                if !at.trim().is_empty() {
                    out.insert("auth_timeout".into(), json!(at));
                }
            }
            if let Some(ref hb) = e.tuic_heartbeat {
                if !hb.trim().is_empty() {
                    out.insert("heartbeat".into(), json!(hb));
                }
            }
            out.insert("tls".into(), singbox_tls(e));
            Ok(Some(Value::Object(out)))
        }
        Protocol::Anytls => {
            out.insert("type".into(), json!("anytls"));
            out.insert("tag".into(), json!(uname));
            out.insert("server".into(), json!(server));
            out.insert("server_port".into(), json!(e.port));
            out.insert("password".into(), json!(e.password));
            out.insert("udp_over_tcp".into(), json!(true));
            if let Some(ref chk) = e.anytls_idle_check {
                if !chk.trim().is_empty() {
                    out.insert("idle_session_check_interval".into(), json!(chk));
                }
            }
            if let Some(ref to) = e.anytls_idle_timeout {
                if !to.trim().is_empty() {
                    out.insert("idle_session_timeout".into(), json!(to));
                }
            }
            if let Some(min) = e.anytls_min_idle {
                out.insert("min_idle_session".into(), json!(min));
            }
            out.insert("tls".into(), singbox_tls(e));
            Ok(Some(Value::Object(out)))
        }
        Protocol::Shadowtls => {
            out.insert("type".into(), json!("shadowtls"));
            out.insert("tag".into(), json!(uname));
            out.insert("server".into(), json!(server));
            out.insert("server_port".into(), json!(e.port));
            out.insert("version".into(), json!(e.shadowtls_version.unwrap_or(3)));
            out.insert(
                "password".into(),
                json!(e.shadowtls_password.as_deref().unwrap_or(&e.password)),
            );
            if let Some(ref detour) = e.shadowtls_detour {
                if !detour.trim().is_empty() {
                    out.insert("detour".into(), json!(detour));
                }
            }
            out.insert("tls".into(), singbox_tls(e));
            Ok(Some(Value::Object(out)))
        }
        Protocol::Mieru | Protocol::Naiveproxy => Ok(None),
    }
}

/// Renders an outbounds-only Sing-box JSON document (`{"outbounds": [...]}`) for OpenWrt/Podkop.
pub fn render_singbox_outbounds(entries: &[SubEntry]) -> Result<String, SubError> {
    let mut outbounds = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for e in entries {
        let uname = dedup_tag(&mut seen, &e.tag);
        if let Some(ob) = singbox_outbound(e, &uname)? {
            outbounds.push(ob);
        }
    }
    serde_json::to_string_pretty(&json!({ "outbounds": outbounds }))
        .map_err(|err| SubError::Invalid(format!("Sing-box JSON serialization failed: {err}")))
}

/// Renders Sing-box 1.14+ JSON with default options.
pub fn render_singbox(entries: &[SubEntry]) -> Result<String, SubError> {
    render_singbox_with_options(entries, &SingboxRenderOptions::default())
}

/// Renders Sing-box 1.14+ JSON with optional `.srs` rule sets and DNS `evaluate`/`match_response`.
pub fn render_singbox_with_options(
    entries: &[SubEntry],
    opts: &SingboxRenderOptions,
) -> Result<String, SubError> {
    let mut proxy_outbounds: Vec<Value> = Vec::new();
    let mut tags: Vec<Value> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();

    for e in entries {
        let candidate = dedup_tag(&mut seen, &e.tag);
        if let Some(ob) = singbox_outbound(e, &candidate)? {
            tags.push(json!(candidate));
            proxy_outbounds.push(ob);
        }
    }

    if !entries.is_empty() && proxy_outbounds.is_empty() {
        return Err(SubError::Unsupported(
            "Naive/Mieru require version-specific clients; request native JSON instead".into(),
        ));
    }

    let auto_outbounds = if tags.is_empty() {
        vec![json!("direct")]
    } else {
        tags.clone()
    };
    let selector_outbounds = if tags.is_empty() {
        vec![json!("direct")]
    } else {
        let mut s = vec![json!("auto")];
        s.extend(tags);
        s.push(json!("direct"));
        s
    };

    let mut outbounds = vec![
        json!({
            "type": "selector",
            "tag": "proxy",
            "outbounds": selector_outbounds,
            "default": if proxy_outbounds.is_empty() { "direct" } else { "auto" },
        }),
        json!({
            "type": "urltest",
            "tag": "auto",
            "outbounds": auto_outbounds,
            "url": "https://www.gstatic.com/generate_204",
            "interval": "3m",
        }),
    ];
    outbounds.extend(proxy_outbounds);
    outbounds.push(json!({ "type": "direct", "tag": "direct" }));

    let rule_sets: Vec<Value> = opts
        .srs_rule_sets
        .iter()
        .map(|rs| {
            json!({
                "tag": rs.tag,
                "type": "remote",
                "format": "binary",
                "url": rs.url,
                "download_detour": rs.download_detour.as_deref().unwrap_or("proxy"),
            })
        })
        .collect();

    let mut route_rules = vec![
        json!({ "action": "sniff" }),
        json!({ "protocol": "dns", "action": "hijack-dns" }),
        json!({ "ip_is_private": true, "outbound": "direct" }),
    ];
    let geoip_tags: Vec<String> = opts
        .srs_rule_sets
        .iter()
        .filter(|rs| rs.tag.contains("geoip"))
        .map(|rs| rs.tag.clone())
        .collect();
    let geosite_tags: Vec<String> = opts
        .srs_rule_sets
        .iter()
        .filter(|rs| rs.tag.contains("geosite"))
        .map(|rs| rs.tag.clone())
        .collect();
    if !geoip_tags.is_empty() || !geosite_tags.is_empty() {
        let mut combined = geosite_tags.clone();
        combined.extend(geoip_tags.clone());
        route_rules.push(json!({
            "rule_set": combined,
            "outbound": "direct",
        }));
    }

    let dns_block = if opts.enable_dns_114 {
        let mut dns_rules: Vec<Value> = Vec::new();
        if !geoip_tags.is_empty() {
            dns_rules.push(json!({
                "rule_set": geoip_tags,
                "action": "evaluate",
                "server": "remote-dns",
            }));
            dns_rules.push(json!({
                "match_response": true,
                "rule_set": geoip_tags,
                "action": "route",
                "server": "local-dns",
            }));
        }
        if !geosite_tags.is_empty() {
            dns_rules.push(json!({
                "rule_set": geosite_tags,
                "action": "route",
                "server": "local-dns",
            }));
        }
        if dns_rules.is_empty() {
            dns_rules.push(json!({
                "domain_suffix": [".cn", ".ir"],
                "action": "route",
                "server": "local-dns",
            }));
        }
        json!({
            "servers": [
                {
                    "tag": "remote-dns",
                    "type": "https",
                    "server": "1.1.1.1",
                    "detour": "proxy",
                },
                {
                    "tag": "local-dns",
                    "type": "local",
                }
            ],
            "rules": dns_rules,
        })
    } else {
        json!({})
    };

    let mut route_obj = Map::new();
    if !rule_sets.is_empty() {
        route_obj.insert("rule_set".into(), Value::Array(rule_sets));
    }
    route_obj.insert("rules".into(), Value::Array(route_rules));
    route_obj.insert("auto_detect_interface".into(), json!(true));
    if opts.enable_dns_114 {
        route_obj.insert("default_domain_resolver".into(), json!("local-dns"));
    }

    let mut root = Map::new();
    if opts.enable_dns_114 {
        root.insert("dns".into(), dns_block);
    }
    root.insert("outbounds".into(), Value::Array(outbounds));
    root.insert("route".into(), Value::Object(route_obj));

    serde_json::to_string_pretty(&Value::Object(root))
        .map_err(|err| SubError::Invalid(format!("Sing-box JSON serialization failed: {err}")))
}

fn xray_stream_settings(e: &SubEntry) -> Value {
    let network = net(e);
    let security = e.security.as_deref().unwrap_or("none");
    let mut stream = Map::new();
    stream.insert("network".into(), json!(network));
    stream.insert("security".into(), json!(security));

    if security == "tls" {
        let mut tls = Map::new();
        if let Some(ref sni) = e.sni {
            tls.insert("serverName".into(), json!(sni));
        }
        if let Some(ref fp) = e.fingerprint {
            tls.insert("fingerprint".into(), json!(fp));
        }
        let alpn = split_alpn(e.alpn.as_ref());
        if !alpn.is_empty() {
            tls.insert("alpn".into(), json!(alpn));
        }
        if e.allow_insecure {
            tls.insert("allowInsecure".into(), json!(true));
        }
        if let Some(ref pcs) = e.pinned_peer_sha256 {
            tls.insert("pinnedPeerCertSha256".into(), json!(pcs));
        }
        if let Some(ref vcn) = e.verify_peer_cert_by_name {
            tls.insert("verifyPeerCertByName".into(), json!(vcn));
        }
        if let Some(ref ech) = e.ech_config {
            tls.insert("echConfigList".into(), json!(ech));
        }
        stream.insert("tlsSettings".into(), Value::Object(tls));
    } else if security == "reality" {
        let mut reality = Map::new();
        if let Some(ref sni) = e.sni {
            reality.insert("serverName".into(), json!(sni));
        }
        if let Some(ref fp) = e.fingerprint {
            reality.insert("fingerprint".into(), json!(fp));
        }
        reality.insert(
            "publicKey".into(),
            json!(e.public_key.clone().unwrap_or_default()),
        );
        reality.insert(
            "shortId".into(),
            json!(e.short_id.clone().unwrap_or_default()),
        );
        reality.insert(
            "spiderX".into(),
            json!(e.spider_x.clone().unwrap_or_default()),
        );
        reality.insert(
            "mldsa65Verify".into(),
            json!(e.mldsa65_verify.clone().unwrap_or_default()),
        );
        stream.insert("realitySettings".into(), Value::Object(reality));
    }

    match network {
        "ws" => {
            let mut ws = Map::new();
            ws.insert(
                "path".into(),
                json!(e
                    .ws_path
                    .clone()
                    .or_else(|| e.stream_path.clone())
                    .unwrap_or_else(|| "/".into())),
            );
            if let Some(ref h) = e.ws_host {
                ws.insert("host".into(), json!(h));
            }
            stream.insert("wsSettings".into(), Value::Object(ws));
        }
        "grpc" => {
            stream.insert(
                "grpcSettings".into(),
                json!({
                    "serviceName": e.grpc_service_name.clone().unwrap_or_default(),
                    "multiMode": e.grpc_multi_mode,
                }),
            );
        }
        "httpupgrade" => {
            let mut hu = Map::new();
            hu.insert(
                "path".into(),
                json!(e
                    .httpupgrade_path
                    .clone()
                    .or_else(|| e.stream_path.clone())
                    .unwrap_or_else(|| "/".into())),
            );
            if let Some(ref h) = e.httpupgrade_host {
                hu.insert("host".into(), json!(h));
            }
            stream.insert("httpupgradeSettings".into(), Value::Object(hu));
        }
        "xhttp" => {
            let mut xh = Map::new();
            xh.insert(
                "path".into(),
                json!(e
                    .xhttp_path
                    .clone()
                    .or_else(|| e.stream_path.clone())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "/".into())),
            );
            if let Some(ref h) = e.xhttp_host {
                xh.insert("host".into(), json!(h));
            }
            if let Some(ref m) = e.xhttp_mode {
                xh.insert("mode".into(), json!(m));
            }
            if let Some(ref extra) = e.xhttp_extra {
                xh.insert("extra".into(), extra.clone());
            }
            stream.insert("xhttpSettings".into(), Value::Object(xh));
        }
        "h2" => {
            let mut h2 = Map::new();
            h2.insert(
                "path".into(),
                json!(e
                    .h2_path
                    .clone()
                    .or_else(|| e.stream_path.clone())
                    .unwrap_or_else(|| "/".into())),
            );
            if let Some(ref h) = e.h2_host {
                h2.insert("host".into(), json!([h]));
            }
            stream.insert("httpSettings".into(), Value::Object(h2));
        }
        "kcp" => {
            let mut kcp = Map::new();
            kcp.insert(
                "header".into(),
                json!({ "type": e.kcp_header_type.as_deref().unwrap_or("none") }),
            );
            if let Some(ref seed) = e.kcp_seed {
                if !seed.is_empty() {
                    kcp.insert("seed".into(), json!(seed));
                }
            }
            stream.insert("kcpSettings".into(), Value::Object(kcp));
        }
        "tcp" | "raw" if e.tcp_header_type.as_deref() == Some("http") => {
            let mut headers = Map::new();
            if let Some(ref h) = e.tcp_header_host {
                headers.insert("Host".into(), json!([h]));
            }
            stream.insert(
                "tcpSettings".into(),
                json!({
                    "header": {
                        "type": "http",
                        "request": {
                            "path": [e.tcp_header_path.as_deref().unwrap_or("/")],
                            "headers": headers
                        }
                    }
                }),
            );
        }
        _ => {}
    }

    if e.fragment.as_deref().is_some_and(|s| !s.trim().is_empty()) {
        stream.insert("sockopt".into(), json!({ "dialerProxy": "fragment" }));
    }

    if let Some(ref fm) = e.final_mask {
        stream.insert("finalmask".into(), fm.clone());
    }
    Value::Object(stream)
}

fn xray_outbounds_list(entries: &[SubEntry]) -> Result<Vec<Value>, SubError> {
    let mut outbounds = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();

    for e in entries {
        let tag = dedup_tag(&mut seen, &e.tag);
        match e.protocol {
            Protocol::Vless => {
                let uuid = if let Some(ref rid) = e.vless_route_id {
                    apply_vless_route(&e.uuid, rid)?
                } else {
                    e.uuid.clone()
                };
                outbounds.push(json!({
                    "tag": tag,
                    "protocol": "vless",
                    "settings": {
                        "vnext": [{
                            "address": e.host,
                            "port": e.port,
                            "users": [{
                                "id": uuid,
                                "encryption": e.vless_enc.as_deref().unwrap_or("none"),
                                "flow": e.flow.clone().unwrap_or_default(),
                            }]
                        }]
                    },
                    "streamSettings": xray_stream_settings(e),
                }));
            }
            Protocol::Vmess => {
                outbounds.push(json!({
                    "tag": tag,
                    "protocol": "vmess",
                    "settings": {
                        "vnext": [{
                            "address": e.host,
                            "port": e.port,
                            "users": [{
                                "id": e.uuid,
                                "alterId": e.alter_id,
                                "security": vmess_cipher(e),
                            }]
                        }]
                    },
                    "streamSettings": xray_stream_settings(e),
                }));
            }
            Protocol::Trojan => {
                outbounds.push(json!({
                    "tag": tag,
                    "protocol": "trojan",
                    "settings": {
                        "servers": [{
                            "address": e.host,
                            "port": e.port,
                            "password": e.password,
                        }]
                    },
                    "streamSettings": xray_stream_settings(e),
                }));
            }
            Protocol::Shadowsocks => {
                outbounds.push(json!({
                    "tag": tag,
                    "protocol": "shadowsocks",
                    "settings": {
                        "servers": [{
                            "address": e.host,
                            "port": e.port,
                            "method": e.method.as_deref().unwrap_or("aes-256-gcm"),
                            "password": format_ss2022_key(e),
                        }]
                    },
                }));
            }
            Protocol::Hysteria2 => {
                outbounds.push(json!({
                    "tag": tag,
                    "protocol": "hysteria",
                    "settings": {
                        "version": 2,
                        "address": e.host,
                        "port": e.port,
                    },
                    "streamSettings": {
                        "network": "hysteria",
                        "hysteriaSettings": {
                            "version": 2,
                            "auth": e.password,
                        },
                        "security": "tls",
                        "tlsSettings": {
                            "serverName": e.sni.clone().unwrap_or_default(),
                            "allowInsecure": e.allow_insecure,
                        }
                    }
                }));
            }
            _ => {}
        }
    }
    Ok(outbounds)
}

/// Renders standard V2Ray/Xray JSON configuration (`SubFormat::Original`).
pub fn render_original(entries: &[SubEntry]) -> Result<String, SubError> {
    let mut outbounds = xray_outbounds_list(entries)?;
    if !entries.is_empty() && outbounds.is_empty() {
        return Err(SubError::Unsupported(
            "No Xray-compatible entries available for original JSON export".into(),
        ));
    }
    outbounds.push(json!({ "protocol": "freedom", "tag": "direct" }));
    outbounds.push(json!({ "protocol": "blackhole", "tag": "block" }));

    if let Some(frag_spec) = entries
        .iter()
        .find_map(|e| e.fragment.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    {
        let mut parts = frag_spec.split(',').map(str::trim);
        let packets = parts.next().filter(|s| !s.is_empty()).unwrap_or("tlshello");
        let length = parts.next().filter(|s| !s.is_empty()).unwrap_or("10-20");
        let interval = parts.next().filter(|s| !s.is_empty()).unwrap_or("10-20");
        outbounds.push(json!({
            "tag": "fragment",
            "protocol": "freedom",
            "settings": {
                "fragment": {
                    "packets": packets,
                    "length": length,
                    "interval": interval
                }
            }
        }));
    }

    let doc = json!({
        "log": { "loglevel": "warning" },
        "inbounds": [{
            "port": 10808,
            "listen": "127.0.0.1",
            "protocol": "socks",
            "settings": { "udp": true },
        }],
        "outbounds": outbounds,
        "routing": {
            "domainStrategy": "AsIs",
            "rules": [],
        }
    });

    serde_json::to_string_pretty(&doc)
        .map_err(|err| SubError::Invalid(format!("Original Xray JSON serialization failed: {err}")))
}

/// Renders a Happ-compatible base64 URI subscription.
pub fn render_happ(entries: &[SubEntry]) -> Result<String, SubError> {
    render_base64(entries)
}

fn parse_rsa_public_key(pem: &str) -> Result<RsaPublicKey, SubError> {
    RsaPublicKey::from_public_key_pem(pem)
        .or_else(|_| RsaPublicKey::from_pkcs1_pem(pem))
        .map_err(|e| SubError::Invalid(format!("Invalid RSA public key PEM: {e}")))
}

fn parse_rsa_private_key(pem: &str) -> Result<RsaPrivateKey, SubError> {
    RsaPrivateKey::from_pkcs8_pem(pem)
        .or_else(|_| RsaPrivateKey::from_pkcs1_pem(pem))
        .map_err(|e| SubError::Invalid(format!("Invalid RSA private key PEM: {e}")))
}

/// Encrypts a subscription URL into `happ://crypt4/<base64>` using RSA PKCS#1 v1.5.
pub fn build_happ_crypt4(sub_url: &str, rsa_pub_pem: &str) -> Result<String, SubError> {
    let pub_key = parse_rsa_public_key(rsa_pub_pem)?;
    let mut rng = OsRng;
    let encrypted = pub_key
        .encrypt(&mut rng, Pkcs1v15Encrypt, sub_url.as_bytes())
        .map_err(|e| SubError::Invalid(format!("Happ crypt4 encryption failed: {e}")))?;
    Ok(format!("happ://crypt4/{}", STANDARD.encode(&encrypted)))
}

/// Decrypts a `happ://crypt4/<base64>` link using the corresponding RSA private key PEM.
pub fn decrypt_happ_crypt4(link: &str, rsa_priv_pem: &str) -> Result<String, SubError> {
    let b64 = link
        .strip_prefix("happ://crypt4/")
        .ok_or_else(|| SubError::Invalid("Not a happ://crypt4 link".into()))?;
    let ct = STANDARD
        .decode(b64.trim())
        .map_err(|e| SubError::Invalid(format!("Invalid crypt4 base64: {e}")))?;
    let priv_key = parse_rsa_private_key(rsa_priv_pem)?;
    let pt = priv_key
        .decrypt(Pkcs1v15Encrypt, &ct)
        .map_err(|e| SubError::Invalid(format!("Happ crypt4 decryption failed: {e}")))?;
    String::from_utf8(pt)
        .map_err(|e| SubError::Invalid(format!("Decrypted crypt4 payload is not UTF-8: {e}")))
}

/// Encrypts a subscription URL into `happ://crypt5/<base64>` using hybrid RSA-OAEP-SHA256 + ChaCha20-Poly1305.
///
/// Binary layout before base64: `[u16 BE wrapped_key_len][rsa_oaep(32B_key)][12B_nonce][aead_ciphertext]`.
pub fn build_happ_crypt5(sub_url: &str, rsa_pub_pem: &str) -> Result<String, SubError> {
    let pub_key = parse_rsa_public_key(rsa_pub_pem)?;
    let mut rng = OsRng;
    let mut raw_key = [0u8; 32];
    let mut raw_nonce = [0u8; 12];
    rng.fill_bytes(&mut raw_key);
    rng.fill_bytes(&mut raw_nonce);

    let cipher = ChaCha20Poly1305::new(Key::from_slice(&raw_key));
    let ct = cipher
        .encrypt(Nonce::from_slice(&raw_nonce), sub_url.as_bytes())
        .map_err(|e| SubError::Invalid(format!("Happ crypt5 AEAD failed: {e}")))?;

    let wrapped_key = pub_key
        .encrypt(&mut rng, Oaep::new::<Sha256>(), &raw_key)
        .map_err(|e| SubError::Invalid(format!("Happ crypt5 RSA wrap failed: {e}")))?;
    let key_len = u16::try_from(wrapped_key.len())
        .map_err(|_| SubError::Invalid("Wrapped RSA key exceeds u16 length".into()))?;

    let mut blob = Vec::with_capacity(2 + wrapped_key.len() + 12 + ct.len());
    blob.extend_from_slice(&key_len.to_be_bytes());
    blob.extend_from_slice(&wrapped_key);
    blob.extend_from_slice(&raw_nonce);
    blob.extend_from_slice(&ct);

    Ok(format!("happ://crypt5/{}", STANDARD.encode(&blob)))
}

/// Decrypts a `happ://crypt5/<base64>` link using the corresponding RSA private key PEM.
pub fn decrypt_happ_crypt5(link: &str, rsa_priv_pem: &str) -> Result<String, SubError> {
    let b64 = link
        .strip_prefix("happ://crypt5/")
        .ok_or_else(|| SubError::Invalid("Not a happ://crypt5 link".into()))?;
    let blob = STANDARD
        .decode(b64.trim())
        .map_err(|e| SubError::Invalid(format!("Invalid crypt5 base64: {e}")))?;
    if blob.len() < 2 + 12 {
        return Err(SubError::Invalid("Truncated crypt5 payload".into()));
    }
    let key_len = u16::from_be_bytes([blob[0], blob[1]]) as usize;
    if blob.len() < 2 + key_len + 12 {
        return Err(SubError::Invalid("Malformed crypt5 key length".into()));
    }
    let wrapped_key = &blob[2..2 + key_len];
    let nonce_bytes = &blob[2 + key_len..2 + key_len + 12];
    let ct = &blob[2 + key_len + 12..];

    let priv_key = parse_rsa_private_key(rsa_priv_pem)?;
    let raw_key = priv_key
        .decrypt(Oaep::new::<Sha256>(), wrapped_key)
        .map_err(|e| SubError::Invalid(format!("Happ crypt5 RSA unwrap failed: {e}")))?;
    if raw_key.len() != 32 {
        return Err(SubError::Invalid("Invalid unwrapped ChaCha20 key size".into()));
    }
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&raw_key));
    let pt = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ct)
        .map_err(|e| SubError::Invalid(format!("Happ crypt5 AEAD decrypt failed: {e}")))?;
    String::from_utf8(pt)
        .map_err(|e| SubError::Invalid(format!("Decrypted crypt5 payload is not UTF-8: {e}")))
}

/// Supported subscription template formats.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TemplateType {
    XrayJson,
    XrayBase64,
    Mihomo,
    Clash,
    Stash,
    Singbox,
}

impl TemplateType {
    pub const fn content_type(&self) -> &'static str {
        match self {
            Self::XrayJson | Self::Singbox => "application/json; charset=utf-8",
            Self::XrayBase64 => "text/plain; charset=utf-8",
            Self::Mihomo | Self::Clash | Self::Stash => "application/yaml; charset=utf-8",
        }
    }
}

/// User-defined subscription template with placeholder tokens.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionTemplate {
    pub id: String,
    pub name: String,
    pub template_type: TemplateType,
    pub content: String,
}

/// Renders a [`SubscriptionTemplate`], substituting placeholders and validating structured JSON/YAML output.
pub fn apply_template(
    tpl: &SubscriptionTemplate,
    entries: &[SubEntry],
    info: &UserInfo,
) -> Result<RenderedSubscription, SubError> {
    let mut links = Vec::with_capacity(entries.len());
    for e in entries {
        links.push(share_link(e)?);
    }
    let links_joined = links.join("\n");
    let links_b64 = STANDARD.encode(links_joined.as_bytes());

    let mut clash_proxies = Vec::new();
    let mut sb_outbounds = Vec::new();
    let mut names = Vec::new();
    let mut seen = HashMap::new();
    for e in entries {
        let uname = dedup_tag(&mut seen, &e.tag);
        if let Some(p) = clash_proxy(e, &uname)? {
            clash_proxies.push(p);
            names.push(uname.clone());
        }
        if let Some(sb) = singbox_outbound(e, &uname)? {
            sb_outbounds.push(sb);
        }
    }
    let xray_outbounds = xray_outbounds_list(entries)?;

    let proxies_str = match tpl.template_type {
        TemplateType::Mihomo | TemplateType::Clash | TemplateType::Stash => {
            serde_yaml::to_string(&clash_proxies)
                .map_err(|e| SubError::Invalid(format!("Template YAML proxy error: {e}")))?
        }
        TemplateType::Singbox => serde_json::to_string(&sb_outbounds)
            .map_err(|e| SubError::Invalid(format!("Template Singbox proxy error: {e}")))?,
        TemplateType::XrayJson => serde_json::to_string(&xray_outbounds)
            .map_err(|e| SubError::Invalid(format!("Template Xray proxy error: {e}")))?,
        TemplateType::XrayBase64 => links_joined.clone(),
    };

    let names_str = match tpl.template_type {
        TemplateType::Mihomo | TemplateType::Clash | TemplateType::Stash => {
            serde_yaml::to_string(&names)
                .map_err(|e| SubError::Invalid(format!("Template YAML names error: {e}")))?
        }
        _ => serde_json::to_string(&names)
            .map_err(|e| SubError::Invalid(format!("Template JSON names error: {e}")))?,
    };

    let sb_outbounds_str = serde_json::to_string(&sb_outbounds)
        .map_err(|e| SubError::Invalid(format!("Template Singbox outbounds error: {e}")))?;
    let xray_outbounds_str = serde_json::to_string(&xray_outbounds)
        .map_err(|e| SubError::Invalid(format!("Template Xray outbounds error: {e}")))?;

    let rendered_body = tpl
        .content
        .replace("{{PROXIES}}", proxies_str.trim_end())
        .replace("{{PROXY_NAMES}}", names_str.trim_end())
        .replace("{{LINKS}}", &links_joined)
        .replace("{{LINKS_BASE64}}", &links_b64)
        .replace("{{SINGBOX_OUTBOUNDS}}", &sb_outbounds_str)
        .replace("{{XRAY_OUTBOUNDS}}", &xray_outbounds_str)
        .replace("{{USERNAME}}", &info.username)
        .replace("{{UUID}}", &info.uuid)
        .replace("{{UPLOAD}}", &info.upload.to_string())
        .replace("{{DOWNLOAD}}", &info.download.to_string())
        .replace("{{TOTAL}}", &info.total.to_string())
        .replace("{{EXPIRE}}", &info.expire.to_string());

    // Validate structured outputs so broken templates fail cleanly
    match tpl.template_type {
        TemplateType::XrayJson | TemplateType::Singbox => {
            serde_json::from_str::<Value>(&rendered_body).map_err(|e| {
                SubError::Invalid(format!("Rendered JSON template is invalid: {e}"))
            })?;
        }
        TemplateType::Mihomo | TemplateType::Clash | TemplateType::Stash => {
            serde_yaml::from_str::<serde_yaml::Value>(&rendered_body).map_err(|e| {
                SubError::Invalid(format!("Rendered YAML template is invalid: {e}"))
            })?;
        }
        TemplateType::XrayBase64 => {}
    }

    let format = match tpl.template_type {
        TemplateType::XrayJson => SubFormat::Original,
        TemplateType::XrayBase64 => SubFormat::Base64,
        TemplateType::Mihomo | TemplateType::Clash | TemplateType::Stash => SubFormat::Clash,
        TemplateType::Singbox => SubFormat::Singbox,
    };

    Ok(RenderedSubscription {
        body_bytes: rendered_body.as_bytes().to_vec(),
        body: rendered_body,
        content_type: tpl.template_type.content_type(),
        headers: headers(info, format, ClientProfile::Generic),
        format,
        early_status: None,
    })
}

/// Request context evaluated by Subscription Request Rules (SRR).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SrrContext {
    pub user_agent: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_ip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hwid: Option<String>,
}

/// Matching operator for a single [`SrrCondition`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SrrOperator {
    Contains,
    Equals,
    Regex,
    Exists,
}

/// Condition evaluated against request headers or metadata (`user-agent`, `x-hwid`, `client-ip`, etc.).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SrrCondition {
    pub header: String,
    #[serde(default)]
    pub pattern: String,
    #[serde(default)]
    pub negate: bool,
    pub operator: SrrOperator,
}

/// Age encryption target for SRR responses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AgeEncryptionConfig {
    Recipients(Vec<String>),
    Passphrase(String),
}

/// Action taken when an [`SrrRule`] matches.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SrrAction {
    RespondWithRemarks(Vec<String>),
    RejectStatus(u16),
    SocketDrop,
    SelectTemplate(String),
    OverrideFormat(SubFormat),
    AgeEncrypt,
}

/// Subscription Request Rule (SRR) evaluated in priority order.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SrrRule {
    pub id: String,
    pub name: String,
    pub priority: i32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub conditions: Vec<SrrCondition>,
    pub action: SrrAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub age_config: Option<AgeEncryptionConfig>,
}

/// Aggregated decision produced by [`evaluate_srr`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SrrDecision {
    pub matched_rule_ids: Vec<String>,
    pub early_status: Option<u16>,
    pub socket_drop: bool,
    pub remarks: Option<Vec<String>>,
    pub template_id: Option<String>,
    pub override_format: Option<SubFormat>,
    pub age_config: Option<AgeEncryptionConfig>,
}

fn eval_condition(ctx: &SrrContext, cond: &SrrCondition) -> bool {
    let key = cond.header.trim().to_ascii_lowercase();
    let val_opt: Option<&str> = match key.as_str() {
        "user-agent" => (!ctx.user_agent.is_empty()).then_some(ctx.user_agent.as_str()),
        "client-ip" | "x-client-ip" => ctx.client_ip.as_deref(),
        "x-hwid" | "hwid" => ctx
            .hwid
            .as_deref()
            .or_else(|| ctx.headers.get("x-hwid").map(String::as_str)),
        other => ctx.headers.get(other).map(String::as_str),
    };

    let matched = match cond.operator {
        SrrOperator::Exists => val_opt.is_some_and(|v| !v.is_empty()),
        SrrOperator::Contains => val_opt
            .is_some_and(|v| v.to_ascii_lowercase().contains(&cond.pattern.to_ascii_lowercase())),
        SrrOperator::Equals => val_opt.is_some_and(|v| v == cond.pattern),
        SrrOperator::Regex => match (val_opt, Regex::new(&cond.pattern)) {
            (Some(v), Ok(re)) => re.is_match(v),
            _ => false,
        },
    };

    if cond.negate {
        !matched
    } else {
        matched
    }
}

/// Pure, deterministic priority-ordered SRR evaluator.
pub fn evaluate_srr(ctx: &SrrContext, rules: &[SrrRule]) -> SrrDecision {
    let mut ordered: Vec<&SrrRule> = rules.iter().filter(|r| r.enabled).collect();
    ordered.sort_by_key(|r| r.priority);

    let mut decision = SrrDecision::default();
    for rule in ordered {
        let all_match = rule.conditions.iter().all(|c| eval_condition(ctx, c));
        if !all_match {
            continue;
        }
        decision.matched_rule_ids.push(rule.id.clone());
        match &rule.action {
            SrrAction::SocketDrop => {
                decision.socket_drop = true;
                return decision;
            }
            SrrAction::RejectStatus(code) => {
                decision.early_status = Some(*code);
                return decision;
            }
            SrrAction::RespondWithRemarks(lines) => {
                decision.remarks = Some(lines.clone());
                return decision;
            }
            SrrAction::SelectTemplate(tpl_id) => {
                decision.template_id = Some(tpl_id.clone());
            }
            SrrAction::OverrideFormat(fmt) => {
                decision.override_format = Some(*fmt);
            }
            SrrAction::AgeEncrypt => {
                if let Some(ref cfg) = rule.age_config {
                    decision.age_config = Some(cfg.clone());
                }
            }
        }
    }
    decision
}

/// Encrypts subscription bytes using `age` (`age1...` X25519, `age1pq1...` MLKEM768+X25519, or scrypt passphrase).
pub fn encrypt_age(
    plaintext: &[u8],
    cfg: &AgeEncryptionConfig,
    armor: bool,
) -> Result<Vec<u8>, SubError> {
    let encryptor = match cfg {
        AgeEncryptionConfig::Passphrase(pass) => {
            if pass.is_empty() {
                return Err(SubError::Invalid("Empty age passphrase".into()));
            }
            age::Encryptor::with_user_passphrase(SecretString::from(pass.clone()))
        }
        AgeEncryptionConfig::Recipients(list) => {
            if list.is_empty() {
                return Err(SubError::Invalid("No age recipients specified".into()));
            }
            let mut parsed: Vec<Box<dyn age::Recipient>> = Vec::with_capacity(list.len());
            for item in list {
                let r = item.trim();
                if r.starts_with("age1pq1") {
                    let pq: age::tagpq::Recipient = r.parse().map_err(|e| {
                        SubError::Invalid(format!("Invalid age PQ recipient '{r}': {e:?}"))
                    })?;
                    parsed.push(Box::new(pq));
                } else {
                    let x: age::x25519::Recipient = r.parse().map_err(|e| {
                        SubError::Invalid(format!("Invalid age X25519 recipient '{r}': {e}"))
                    })?;
                    parsed.push(Box::new(x));
                }
            }
            age::Encryptor::with_recipients(parsed.iter().map(|r| &**r))
                .map_err(|e| SubError::Invalid(format!("Failed to initialize age encryptor: {e}")))?
        }
    };

    let mut output = Vec::new();
    if armor {
        let armor_writer = ArmoredWriter::wrap_output(&mut output, ArmorFormat::AsciiArmor)
            .map_err(|e| SubError::Invalid(format!("Age armor init failed: {e}")))?;
        let mut writer = encryptor
            .wrap_output(armor_writer)
            .map_err(|e| SubError::Invalid(format!("Age wrap_output failed: {e}")))?;
        writer
            .write_all(plaintext)
            .map_err(|e| SubError::Invalid(format!("Age write failed: {e}")))?;
        let armor_finish = writer
            .finish()
            .map_err(|e| SubError::Invalid(format!("Age finish failed: {e}")))?;
        armor_finish
            .finish()
            .map_err(|e| SubError::Invalid(format!("Age armor finish failed: {e}")))?;
    } else {
        let mut writer = encryptor
            .wrap_output(&mut output)
            .map_err(|e| SubError::Invalid(format!("Age wrap_output failed: {e}")))?;
        writer
            .write_all(plaintext)
            .map_err(|e| SubError::Invalid(format!("Age write failed: {e}")))?;
        writer
            .finish()
            .map_err(|e| SubError::Invalid(format!("Age finish failed: {e}")))?;
    }
    Ok(output)
}

/// Complete rendered HTTP subscription response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedSubscription {
    pub body: String,
    pub body_bytes: Vec<u8>,
    pub content_type: &'static str,
    pub headers: Vec<(String, String)>,
    pub format: SubFormat,
    pub early_status: Option<u16>,
}

/// Top-level subscription renderer coordinating SRR rules, templates, format renderers, headers, and `age` encryption.
pub struct SubscriptionRenderer;

impl SubscriptionRenderer {
    /// Renders entries into a complete [`RenderedSubscription`].
    pub fn render(
        entries: &[SubEntry],
        info: &UserInfo,
        format: SubFormat,
        template_opt: Option<&SubscriptionTemplate>,
        srr_ctx: Option<(&SrrContext, &[SrrRule])>,
    ) -> Result<RenderedSubscription, SubError> {
        Self::render_with_options(
            entries,
            info,
            format,
            template_opt,
            srr_ctx,
            &SingboxRenderOptions::default(),
        )
    }

    /// Renders entries with explicit Sing-box 1.14+ `.srs`/DNS options.
    pub fn render_with_options(
        entries: &[SubEntry],
        info: &UserInfo,
        format: SubFormat,
        template_opt: Option<&SubscriptionTemplate>,
        srr_ctx: Option<(&SrrContext, &[SrrRule])>,
        singbox_opts: &SingboxRenderOptions,
    ) -> Result<RenderedSubscription, SubError> {
        let mut effective_format = format;
        let mut profile = ClientProfile::Generic;
        let mut age_cfg: Option<AgeEncryptionConfig> = None;

        if let Some((ctx, rules)) = srr_ctx {
            profile = detect_client_from_ua(&ctx.user_agent).1;
            let decision = evaluate_srr(ctx, rules);
            if decision.socket_drop {
                return Err(SubError::Unsupported("Subscription request dropped by SRR".into()));
            }
            if let Some(code) = decision.early_status {
                return Ok(RenderedSubscription {
                    body: String::new(),
                    body_bytes: Vec::new(),
                    content_type: "text/plain; charset=utf-8",
                    headers: Vec::new(),
                    format: effective_format,
                    early_status: Some(code),
                });
            }
            if let Some(remarks) = decision.remarks {
                let text = remarks.join("\n");
                let body = STANDARD.encode(text.as_bytes());
                return Ok(RenderedSubscription {
                    body_bytes: body.as_bytes().to_vec(),
                    body,
                    content_type: "text/plain; charset=utf-8",
                    headers: headers(info, SubFormat::Base64, profile),
                    format: SubFormat::Base64,
                    early_status: None,
                });
            }
            if let Some(fmt) = decision.override_format {
                effective_format = fmt;
            }
            age_cfg = decision.age_config;
        }

        let mut rendered = if let Some(tpl) = template_opt {
            apply_template(tpl, entries, info)?
        } else {
            let body = match effective_format {
                SubFormat::Base64 => render_base64(entries)?,
                SubFormat::Json => render_json(entries, info)?,
                SubFormat::Clash => render_clash(entries)?,
                SubFormat::Singbox => render_singbox_with_options(entries, singbox_opts)?,
                SubFormat::Original => render_original(entries)?,
                SubFormat::Happ => render_happ(entries)?,
            };
            RenderedSubscription {
                body_bytes: body.as_bytes().to_vec(),
                body,
                content_type: effective_format.content_type(),
                headers: headers(info, effective_format, profile),
                format: effective_format,
                early_status: None,
            }
        };

        if let Some(ref cfg) = age_cfg {
            let encrypted = encrypt_age(&rendered.body_bytes, cfg, true)?;
            let armored_str = String::from_utf8(encrypted)
                .map_err(|e| SubError::Invalid(format!("Invalid UTF-8 in age armor: {e}")))?;
            rendered.body_bytes = armored_str.as_bytes().to_vec();
            rendered.body = armored_str;
            rendered.content_type = "text/plain; charset=utf-8";
        }

        Ok(rendered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::links::{build_entry, EntryContext};

    #[test]
    fn podkop_ua_detects_singbox_format() {
        assert_eq!(
            detect_client_from_ua("podkop/1.0"),
            (SubFormat::Singbox, ClientProfile::Singbox)
        );
    }

    /// Exact `VLESS + XHTTP + Reality` inbound from the reference Remnawave node config.
    fn remnawave_reference_stream() -> Value {
        json!({
            "network": "xhttp",
            "sockopt": {
                "mark": 0,
                "V6Only": false,
                "tproxy": "off",
                "tcpMptcp": false,
                "interface": "",
                "penetrate": false,
                "tcpMaxSeg": 1440,
                "dialerProxy": "",
                "tcpFastOpen": true,
                "tcpcongestion": "bbr",
                "domainStrategy": "UseIP",
                "tcpUserTimeout": 10000,
                "tcpWindowClamp": 600,
                "tcpKeepAliveIdle": 300,
                "acceptProxyProtocol": false,
                "tcpKeepAliveInterval": 0
            },
            "security": "reality",
            "xhttpSettings": {
                "host": "www.twitch.tv",
                "mode": "packet-up",
                "path": "/"
            },
            "realitySettings": {
                "show": false,
                "xver": 0,
                "target": "www.twitch.tv:443",
                "shortIds": [""],
                "privateKey": "L-y1ZTWHuRgg09Bv0JJOYqQkly9jMcsmD7D97O2093o",
                "publicKey": "YSia7kbg9eXUatxAQ5e2nbmKkFNgormAPs27SsWeJXg",
                "fingerprint": "firefox",
                "serverNames": ["www.twitch.tv"]
            }
        })
    }

    /// Builds the reference Remnawave `Finland` entry (client UUID/address from the
    /// reference outbound) and proves every network & TLS/Reality parameter of the
    /// generated subscription matches the reference client config.
    #[test]
    fn remnawave_reference_vless_xhttp_reality_matches_all_formats() {
        let settings = json!({ "clients": [], "decryption": "none" });
        let ctx = EntryContext {
            node_host: "95.85.251.177".into(),
            port: 443,
            client_uuid: "3a8a4cc5-c756-41a3-9961-3789079074a7".into(),
            client_id: "finland-user".into(),
            secret: String::new(),
        };

        let entry = build_entry(
            Protocol::Vless,
            "Finland",
            &settings,
            &remnawave_reference_stream(),
            &ctx,
        )
        .unwrap()
        .unwrap();

        let expected_link = "vless://3a8a4cc5-c756-41a3-9961-3789079074a7@95.85.251.177:443\
?type=xhttp&encryption=none&security=reality\
&sni=www.twitch.tv&fp=firefox&pbk=YSia7kbg9eXUatxAQ5e2nbmKkFNgormAPs27SsWeJXg&sid=\
&path=%2F&host=www.twitch.tv&mode=packet-up#Finland";
        assert_eq!(entry.raw, expected_link);

        // Base64 (v2rayNG / Happ) decode to the same canonical URI.
        let b64_decoded = String::from_utf8(
            STANDARD
                .decode(render_base64(std::slice::from_ref(&entry)).unwrap())
                .unwrap(),
        )
        .unwrap();
        let happ_decoded = String::from_utf8(
            STANDARD
                .decode(render_happ(std::slice::from_ref(&entry)).unwrap())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(b64_decoded, expected_link);
        assert_eq!(happ_decoded, expected_link);

        // Sing-box JSON (podkop / routers) — matches the reference client outbound.
        let sb: Value =
            serde_json::from_str(&render_singbox(std::slice::from_ref(&entry)).unwrap()).unwrap();
        let vless = sb["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["type"] == "vless")
            .expect("vless outbound present")
            .clone();
        assert_eq!(
            vless,
            json!({
                "type": "vless",
                "tag": "Finland",
                "server": "95.85.251.177",
                "server_port": 443,
                "uuid": "3a8a4cc5-c756-41a3-9961-3789079074a7",
                "tls": {
                    "enabled": true,
                    "server_name": "www.twitch.tv",
                    "alpn": ["h2", "http/1.1"],
                    "reality": {
                        "enabled": true,
                        "public_key": "YSia7kbg9eXUatxAQ5e2nbmKkFNgormAPs27SsWeJXg",
                        "short_id": ""
                    },
                    "utls": {
                        "enabled": true,
                        "fingerprint": "firefox"
                    }
                },
                "transport": {
                    "type": "xhttp",
                    "path": "/",
                    "mode": "packet-up",
                    "x_padding_bytes": "100-1000",
                    "host": "www.twitch.tv"
                }
            })
        );
        assert!(vless.get("flow").is_none());
        assert!(vless.get("packet_encoding").is_none());

        // Xray JSON (`flag=original`) — matches the reference `Finland` client outbound.
        let orig: Value =
            serde_json::from_str(&render_original(std::slice::from_ref(&entry)).unwrap()).unwrap();
        assert_eq!(
            orig["outbounds"][0],
            json!({
                "tag": "Finland",
                "protocol": "vless",
                "settings": {
                    "vnext": [{
                        "address": "95.85.251.177",
                        "port": 443,
                        "users": [{
                            "id": "3a8a4cc5-c756-41a3-9961-3789079074a7",
                            "encryption": "none",
                            "flow": ""
                        }]
                    }]
                },
                "streamSettings": {
                    "network": "xhttp",
                    "security": "reality",
                    "realitySettings": {
                        "serverName": "www.twitch.tv",
                        "fingerprint": "firefox",
                        "publicKey": "YSia7kbg9eXUatxAQ5e2nbmKkFNgormAPs27SsWeJXg",
                        "shortId": "",
                        "spiderX": "",
                        "mldsa65Verify": ""
                    },
                    "xhttpSettings": {
                        "path": "/",
                        "host": "www.twitch.tv",
                        "mode": "packet-up"
                    }
                }
            })
        );

        // Clash / Mihomo keeps the same TLS/Reality identity.
        let clash_yaml: serde_yaml::Value =
            serde_yaml::from_str(&render_clash(std::slice::from_ref(&entry)).unwrap()).unwrap();
        let proxy = &clash_yaml["proxies"][0];
        assert_eq!(proxy["server"].as_str(), Some("95.85.251.177"));
        assert_eq!(proxy["port"].as_i64(), Some(443));
        assert_eq!(proxy["servername"].as_str(), Some("www.twitch.tv"));
        assert_eq!(proxy["client-fingerprint"].as_str(), Some("firefox"));
        assert_eq!(
            proxy["reality-opts"]["public-key"].as_str(),
            Some("YSia7kbg9eXUatxAQ5e2nbmKkFNgormAPs27SsWeJXg")
        );
        assert_eq!(proxy["reality-opts"]["short-id"].as_str(), Some(""));
        assert_eq!(proxy["xhttp-opts"]["mode"].as_str(), Some("packet-up"));
        assert_eq!(proxy["xhttp-opts"]["path"].as_str(), Some("/"));
        assert_eq!(proxy["xhttp-opts"]["host"].as_str(), Some("www.twitch.tv"));

        println!("=== /sub/:token (Base64 / Happ — v2rayNG, Happ) ===\n{b64_decoded}\n");
        println!(
            "=== /sub/:token (Sing-box JSON) ===\n{}\n",
            render_singbox(std::slice::from_ref(&entry)).unwrap()
        );
        println!(
            "=== /sub/:token (Xray JSON) ===\n{}\n",
            render_original(std::slice::from_ref(&entry)).unwrap()
        );
    }

    /// When the inbound stores only `privateKey` (raw imports), `pbk` is derived
    /// from the X25519 scalar so subscriptions stay complete.
    #[test]
    fn reality_public_key_is_derived_from_private_key_when_missing() {
        let settings = json!({ "clients": [], "decryption": "none" });
        let mut stream = remnawave_reference_stream();
        stream["realitySettings"]
            .as_object_mut()
            .unwrap()
            .remove("publicKey");
        let ctx = EntryContext {
            node_host: "95.85.251.177".into(),
            port: 443,
            client_uuid: "3a8a4cc5-c756-41a3-9961-3789079074a7".into(),
            client_id: "finland-user".into(),
            secret: String::new(),
        };

        let entry = build_entry(Protocol::Vless, "Finland", &settings, &stream, &ctx)
            .unwrap()
            .unwrap();
        assert_eq!(
            entry.public_key.as_deref(),
            Some("Zc3_kRpdUZYYx9xCtRtV7GhlS3V4JF6RGdTE1CXLdis")
        );
        assert!(entry
            .raw
            .contains("pbk=Zc3_kRpdUZYYx9xCtRtV7GhlS3V4JF6RGdTE1CXLdis"));
    }

    #[test]
    fn vless_xhttp_reality_matches_podkop_reference_config() {
        let settings = json!({ "decryption": "none", "flow": "none" });
        let stream = json!({
            "network": "xhttp",
            "security": "reality",
            "sni": "finland.example.com",
            "fingerprint": "firefox",
            "realitySettings": {
                "dest": "www.twitch.tv:443",
                "serverNames": ["www.twitch.tv"],
                "publicKey": "YSia7kbg9eXUatxAQ5e2nbmKkFNgormAPs27SsWeJXg",
                "shortIds": [""],
                "fingerprint": "firefox"
            },
            "xhttpSettings": {
                "path": "/",
                "mode": "auto",
                "host": "finland.example.com"
            }
        });
        let ctx = EntryContext {
            node_host: "finland.example.com".into(),
            port: 443,
            client_uuid: "682e693a-33b4-4563-9ca4-f11f54fae2da".into(),
            client_id: "rostislav".into(),
            secret: String::new(),
        };

        let mut entry = build_entry(Protocol::Vless, "Finland Rostislav", &settings, &stream, &ctx)
            .unwrap()
            .unwrap();
        entry.resolved_ip = Some("178.104.166.22".into());

        for rendered in [
            render_singbox(&[entry.clone()]).unwrap(),
            render_singbox_outbounds(&[entry]).unwrap(),
        ] {
            let parsed: Value = serde_json::from_str(&rendered).unwrap();
            let vless = parsed["outbounds"]
                .as_array()
                .unwrap()
                .iter()
                .find(|o| o["type"] == "vless")
                .expect("vless outbound present")
                .clone();
            assert_eq!(
                vless,
                json!({
                    "type": "vless",
                    "tag": "Finland Rostislav",
                    "server": "178.104.166.22",
                    "server_port": 443,
                    "uuid": "682e693a-33b4-4563-9ca4-f11f54fae2da",
                    "tls": {
                        "enabled": true,
                        "server_name": "www.twitch.tv",
                        "alpn": ["h2", "http/1.1"],
                        "reality": {
                            "enabled": true,
                            "public_key": "YSia7kbg9eXUatxAQ5e2nbmKkFNgormAPs27SsWeJXg",
                            "short_id": ""
                        },
                        "utls": {
                            "enabled": true,
                            "fingerprint": "firefox"
                        }
                    },
                    "transport": {
                        "type": "xhttp",
                        "path": "/",
                        "mode": "packet-up",
                        "x_padding_bytes": "100-1000",
                        "host": "www.twitch.tv"
                    }
                })
            );
            assert!(vless.get("flow").is_none());
            assert!(vless.get("packet_encoding").is_none());
        }
    }

    #[test]
    fn vless_xhttp_reality_falls_back_to_dest_and_respects_explicit_values() {
        let settings = json!({ "decryption": "none" });
        let stream = json!({
            "network": "xhttp",
            "security": "reality",
            "fingerprint": "chrome",
            "realitySettings": {
                "dest": "www.twitch.tv:443",
                "publicKey": "PUBKEY",
                "shortIds": ["ab12"]
            },
            "xhttpSettings": {
                "path": "/xh",
                "mode": "stream-up",
                "xPaddingBytes": "200-500"
            }
        });
        let ctx = EntryContext {
            node_host: "edge.example.com".into(),
            port: 443,
            client_uuid: "682e693a-33b4-4563-9ca4-f11f54fae2da".into(),
            client_id: "u1".into(),
            secret: String::new(),
        };

        let entry = build_entry(Protocol::Vless, "Explicit", &settings, &stream, &ctx)
            .unwrap()
            .unwrap();
        let out = singbox_outbound(&entry, "Explicit").unwrap().unwrap();
        assert_eq!(out["server"], json!("edge.example.com"));
        assert_eq!(out["tls"]["server_name"], json!("www.twitch.tv"));
        assert_eq!(out["tls"]["utls"]["fingerprint"], json!("chrome"));
        assert_eq!(out["transport"]["path"], json!("/xh"));
        assert_eq!(out["transport"]["mode"], json!("stream-up"));
        assert_eq!(out["transport"]["x_padding_bytes"], json!("200-500"));
        assert_eq!(out["transport"]["host"], json!("www.twitch.tv"));
    }

    #[test]
    fn vless_tcp_vision_reality_keeps_flow_and_packet_encoding() {
        let settings = json!({ "decryption": "none", "flow": "xtls-rprx-vision" });
        let stream = json!({
            "network": "tcp",
            "security": "reality",
            "realitySettings": {
                "dest": "www.apple.com:443",
                "serverNames": ["www.apple.com"],
                "publicKey": "PUBKEY",
                "shortId": "1122"
            }
        });
        let ctx = EntryContext {
            node_host: "node.example.com".into(),
            port: 443,
            client_uuid: "682e693a-33b4-4563-9ca4-f11f54fae2da".into(),
            client_id: "u1".into(),
            secret: String::new(),
        };
        let mut entry = build_entry(Protocol::Vless, "Vision", &settings, &stream, &ctx)
            .unwrap()
            .unwrap();
        entry.resolved_ip = Some("198.51.100.10".into());

        let out = singbox_outbound(&entry, "Vision").unwrap().unwrap();
        assert_eq!(out["server"], json!("198.51.100.10"));
        assert_eq!(out["flow"], json!("xtls-rprx-vision"));
        assert_eq!(out["packet_encoding"], json!("xudp"));
        assert_eq!(out["tls"]["server_name"], json!("www.apple.com"));
        assert_eq!(out["tls"]["utls"]["fingerprint"], json!("chrome"));
        assert!(out.get("transport").is_none());
    }

    #[test]
    fn vless_xhttp_self_steal_reality_preserves_server_name_across_all_formats() {
        let settings = json!({ "decryption": "none" });
        let stream = json!({
            "network": "xhttp",
            "security": "reality",
            "sni": "pisos.fenrir322.pro",
            "fingerprint": "firefox",
            "dest": "127.0.0.1:9443",
            "serverNames": ["pisos.fenrir322.pro"],
            "publicKey": "JMTb7MWN1VTRpoZsUKWyai47TyRy3Qq1KtT9TK5pA1c",
            "shortId": "6228e0d3caec8e12",
            "spiderX": "/",
            "realitySettings": {
                "dest": "127.0.0.1:9443",
                "target": "127.0.0.1:9443",
                "serverNames": ["pisos.fenrir322.pro"],
                "publicKey": "JMTb7MWN1VTRpoZsUKWyai47TyRy3Qq1KtT9TK5pA1c",
                "shortIds": ["6228e0d3caec8e12"],
                "spiderX": "/",
                "fingerprint": "chrome"
            },
            "xhttpSettings": {
                "host": "",
                "mode": "auto",
                "path": "/xh"
            }
        });
        let ctx = EntryContext {
            node_host: "pisos.fenrir322.pro".into(),
            port: 443,
            client_uuid: "ec19a5ce-f140-47d4-891b-903354fbb52c".into(),
            client_id: "b5120507".into(),
            secret: String::new(),
        };

        let mut entry = build_entry(
            Protocol::Vless,
            "vless-xhttp-reality-443",
            &settings,
            &stream,
            &ctx,
        )
        .unwrap()
        .unwrap();
        entry.resolved_ip = Some("13.143.132.68".into());

        assert_eq!(entry.sni.as_deref(), Some("pisos.fenrir322.pro"));
        assert_eq!(entry.xhttp_host, None);
        assert!(entry.raw.contains("sni=pisos.fenrir322.pro"));
        assert!(entry.raw.contains("pbk=JMTb7MWN1VTRpoZsUKWyai47TyRy3Qq1KtT9TK5pA1c"));
        assert!(entry.raw.contains("sid=6228e0d3caec8e12"));
        assert!(entry.raw.contains("path=%2Fxh"));
        assert!(entry.raw.contains("mode=auto"));
        assert!(entry.raw.contains("encryption=none"));
        assert!(!entry.raw.contains("www.google.com"));
        assert!(!entry.raw.contains("&host="));

        let info = UserInfo {
            username: "Velio".into(),
            uuid: "ec19a5ce-f140-47d4-891b-903354fbb52c".into(),
            ..Default::default()
        };

        let b64_decoded = String::from_utf8(
            STANDARD
                .decode(render_base64(std::slice::from_ref(&entry)).unwrap())
                .unwrap(),
        )
        .unwrap();
        let happ_decoded = String::from_utf8(
            STANDARD
                .decode(render_happ(std::slice::from_ref(&entry)).unwrap())
                .unwrap(),
        )
        .unwrap();
        let json_val: Value =
            serde_json::from_str(&render_json(std::slice::from_ref(&entry), &info).unwrap())
                .unwrap();
        let json_link = json_val["entries"][0]["link"].as_str().unwrap();

        assert_eq!(b64_decoded, entry.raw);
        assert_eq!(happ_decoded, entry.raw);
        assert_eq!(json_link, entry.raw);

        let sb: Value =
            serde_json::from_str(&render_singbox(std::slice::from_ref(&entry)).unwrap()).unwrap();
        let sb_vless = sb["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["type"] == "vless")
            .unwrap();
        assert_eq!(sb_vless["server"], json!("13.143.132.68"));
        assert_eq!(sb_vless["tls"]["server_name"], json!("pisos.fenrir322.pro"));
        assert_eq!(sb_vless["transport"]["host"], json!("pisos.fenrir322.pro"));
        assert_eq!(sb_vless["transport"]["mode"], json!("packet-up"));

        let clash_yaml: serde_yaml::Value =
            serde_yaml::from_str(&render_clash(std::slice::from_ref(&entry)).unwrap()).unwrap();
        assert_eq!(
            clash_yaml["proxies"][0]["servername"].as_str(),
            Some("pisos.fenrir322.pro")
        );

        let orig: Value =
            serde_json::from_str(&render_original(std::slice::from_ref(&entry)).unwrap()).unwrap();
        assert_eq!(
            orig["outbounds"][0]["streamSettings"]["realitySettings"]["serverName"],
            json!("pisos.fenrir322.pro")
        );
    }
}
