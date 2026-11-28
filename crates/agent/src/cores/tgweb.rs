//! Telegram WEB proxy (`tg://webproxy`) served by `telemt`.
//!
//! `telemt` terminates nothing itself: a TLS terminator (Caddy) forwards plain HTTP/1.1 to a
//! loopback WEB listener, and `telemt` answers ordinary requests with a decoy site. The panel
//! stores one random credential per client; the 16-byte MTProxy secret is derived from it with
//! [`mypanel_proto::links::tgweb_user_secret_hex`], so the node and the subscription link agree
//! without a new database column.

use crate::cores::{extract_protocol_users, safe_id, str_or_empty, validate_inbound};
use mypanel_proto::links::{tgweb_secret_mode, tgweb_user_secret_hex};
use mypanel_proto::{InboundConfig, Protocol, TrafficCounter};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

/// Loopback port of the telemt Prometheus endpoint (one TGWEB inbound per node).
pub const TGWEB_METRICS_PORT: u16 = 19092;
/// Loopback port of the telemt Control API. Upstream defaults to `0.0.0.0:9091`, which
/// exposes user management to the WAN; the generated config always binds loopback instead.
pub const TGWEB_API_PORT: u16 = 19091;

const CARRIERS: [&str; 4] = ["https", "https-lanes", "websocket", "websocket-lanes"];

/// Returns `<CONFIG_DIR>/tgweb-<safe_id>.toml`.
pub fn tgweb_config_path(config_dir: &Path, inbound_id: &str) -> Result<PathBuf, String> {
    let id = safe_id(inbound_id)?;
    Ok(config_dir.join(format!("tgweb-{id}.toml")))
}

/// Returns the telemt data directory `<CONFIG_DIR>/tgweb-<safe_id>/`.
pub fn tgweb_data_dir(config_dir: &Path, inbound_id: &str) -> Result<PathBuf, String> {
    let id = safe_id(inbound_id)?;
    Ok(config_dir.join(format!("tgweb-{id}")))
}

/// Quotes a TOML basic string, rejecting control characters.
fn toml_str(value: &str) -> Result<String, String> {
    if value.chars().any(char::is_control) {
        return Err("Unsafe TOML value: contains control characters".into());
    }
    Ok(format!(
        "\"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

fn valid_hostname(host: &str) -> bool {
    let h = host.trim();
    !h.is_empty()
        && h.len() <= 253
        && h.contains('.')
        && h.parse::<std::net::IpAddr>().is_err()
        && h.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

fn valid_base_path(path: &str) -> bool {
    path.len() <= 128
        && path.split('/').all(|seg| {
            let mut chars = seg.chars();
            matches!(chars.next(), Some(c) if c.is_ascii_alphanumeric())
                && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        })
}

/// Generates the `telemt` TOML configuration for a TGWEB inbound.
pub fn generate_config(inbound: &InboundConfig) -> Result<String, String> {
    if inbound.protocol != Protocol::Tgweb {
        return Err(format!(
            "Protocol {} is not supported by telemt",
            inbound.protocol
        ));
    }
    validate_inbound(inbound)?;
    let s = &inbound.settings;

    let domain = str_or_empty(s, "domain");
    let domain = if domain.is_empty() { str_or_empty(s, "sni") } else { domain };
    if !valid_hostname(domain) {
        return Err("Telegram WEB proxy needs settings.domain (a DNS name, not an IP address)".into());
    }

    let public_addr = str_or_empty(s, "publicAddr");
    let public_addr: SocketAddr = public_addr
        .parse()
        .map_err(|_| "settings.publicAddr must be '<public IP>:443'".to_string())?;
    if public_addr.port() != 443 {
        return Err("settings.publicAddr must use port 443 (Telegram WEB links have no port)".into());
    }

    let base_path = str_or_empty(s, "basePath").trim_matches('/');
    if !base_path.is_empty() && !valid_base_path(base_path) {
        return Err("settings.basePath must be slash-separated [A-Za-z0-9_-] segments".into());
    }

    let carrier = {
        let c = str_or_empty(s, "carrier").to_ascii_lowercase();
        if c.is_empty() { "https-lanes".to_string() } else { c }
    };
    if !CARRIERS.contains(&carrier.as_str()) {
        return Err(format!("Invalid carrier '{carrier}'"));
    }
    let secret_mode = tgweb_secret_mode(s);
    let decoy_dir_buf = crate::cores::validate_fallback_root(&str_or_empty(s, "fallbackRoot"))?;
    let decoy_dir = decoy_dir_buf.to_string_lossy();

    let users = extract_protocol_users(&inbound.users, s, true)?;
    if users.is_empty() {
        return Err("Telegram WEB inbound has no authorized users".into());
    }
    for u in &users {
        if safe_id(&u.name).is_err() {
            return Err(format!("Unsafe telemt user name: {}", u.name));
        }
    }

    let listen = match inbound.listen.trim() {
        "::1" | "[::1]" => "::1",
        _ => "127.0.0.1",
    };

    let mut out = String::new();
    out.push_str("[general]\nuse_middle_proxy = false\nlog_level = \"normal\"\n\n");
    out.push_str("[general.telemetry]\nuser_enabled = true\n\n");
    // Never print `tg://` links (they carry secrets) into the agent log.
    out.push_str("[general.links]\nshow = []\n\n");
    out.push_str(&format!(
        "[server]\nport = {}\nmetrics_listen = \"127.0.0.1:{TGWEB_METRICS_PORT}\"\nmetrics_whitelist = [\"127.0.0.1/32\"]\n\n",
        inbound.port
    ));
    // Control API is unused by the agent; keep it disabled so no proxy client can reach it.
    out.push_str(&format!(
        "[server.api]\nenabled = false\nlisten = \"127.0.0.1:{TGWEB_API_PORT}\"\nwhitelist = [\"127.0.0.1/32\"]\n\n"
    ));
    out.push_str(&format!(
        "[[server.listeners]]\nip = {}\nport = {}\ntransport = \"web\"\nproxy_protocol = false\nweb_client_ip_source = \"x_forwarded_for\"\nweb_trusted_proxy_cidrs = [\"127.0.0.1/32\"]\n\n",
        toml_str(listen)?,
        inbound.port
    ));
    out.push_str(&format!(
        "[access.users]\n{}\n[web]\nenabled = true\ncarrier = {}\n\n",
        users
            .iter()
            .map(|u| Ok(format!(
                "{} = {}\n",
                toml_str(&u.name)?,
                toml_str(&tgweb_user_secret_hex(&u.password))?
            )))
            .collect::<Result<String, String>>()?,
        toml_str(&carrier)?
    ));
    out.push_str(&format!("[[web.vhosts]]\nhost = {}\n", toml_str(domain)?));
    if !base_path.is_empty() {
        out.push_str(&format!("base_path = {}\n", toml_str(base_path)?));
    }
    out.push_str(&format!(
        "public_addr = {}\n\n[web.vhosts.decoy]\nmode = \"static_directory\"\ndirectory = {}\n\n",
        toml_str(&public_addr.to_string())?,
        toml_str(&decoy_dir)?
    ));
    for u in &users {
        out.push_str(&format!(
            "[[web.vhosts.profiles]]\nuser = {}\nsecret_mode = {}\nmax_sessions = 8\nmax_streams = 512\nmax_streams_per_session = 64\n\n",
            toml_str(&u.name)?,
            toml_str(&secret_mode)?
        ));
    }
    Ok(out)
}

/// Parses telemt Prometheus text into per-user counters
/// (`telemt_user_octets_from_client_total` = upload, `..._to_client_total` = download).
pub fn parse_telemt_metrics(text: &str) -> HashMap<String, TrafficCounter> {
    let mut out: HashMap<String, TrafficCounter> = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        let (is_up, rest) = if let Some(r) = line.strip_prefix("telemt_user_octets_from_client_total{") {
            (true, r)
        } else if let Some(r) = line.strip_prefix("telemt_user_octets_to_client_total{") {
            (false, r)
        } else {
            continue;
        };
        let Some((labels, value)) = rest.split_once('}') else { continue };
        let Some(user) = labels
            .split(',')
            .find_map(|kv| kv.trim().strip_prefix("user=\""))
            .and_then(|v| v.strip_suffix('"'))
        else {
            continue;
        };
        let Ok(bytes) = value.trim().parse::<f64>() else { continue };
        if !bytes.is_finite() || bytes < 0.0 {
            continue;
        }
        let entry = out.entry(user.to_string()).or_default();
        if is_up {
            entry.upload = bytes as u64;
        } else {
            entry.download = bytes as u64;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ib(settings: serde_json::Value) -> InboundConfig {
        serde_json::from_value(json!({
            "id": "t1", "tag": "TGWEB", "protocol": "TGWEB", "port": 18080,
            "listen": "127.0.0.1",
            "settings": settings,
            "users": [{"id": "c1", "username": "alice", "uuid": "00000000-0000-0000-0000-000000000001",
                       "email": "c1", "name": "c1", "password": "pw-one"}]
        }))
        .unwrap()
    }

    fn ok_settings() -> serde_json::Value {
        json!({"domain": "tg.example.com", "publicAddr": "203.0.113.10:443"})
    }

    #[test]
    fn config_has_web_listener_vhost_and_profile() {
        let cfg = generate_config(&ib(ok_settings())).unwrap();
        assert!(cfg.contains("transport = \"web\""));
        assert!(cfg.contains("host = \"tg.example.com\""));
        assert!(cfg.contains("public_addr = \"203.0.113.10:443\""));
        assert!(cfg.contains("secret_mode = \"dd\""));
        assert!(cfg.contains("show = []"));
        assert!(cfg.contains("[server.api]\nenabled = false"));
        assert!(cfg.contains(&format!("\"c1\" = \"{}\"", tgweb_user_secret_hex("pw-one"))));
        assert!(!cfg.contains("base_path"));
    }

    #[test]
    fn forces_non_loopback_listen_to_loopback() {
        let mut inbound = ib(ok_settings());
        inbound.listen = "::".into();
        let cfg = generate_config(&inbound).unwrap();
        assert!(cfg.contains("ip = \"127.0.0.1\""));
        inbound.listen = "0.0.0.0".into();
        assert!(generate_config(&inbound).unwrap().contains("ip = \"127.0.0.1\""));
        inbound.listen = "::1".into();
        assert!(generate_config(&inbound).unwrap().contains("ip = \"::1\""));
    }

    #[test]
    fn base_path_carrier_and_mode_are_applied() {
        let cfg = generate_config(&ib(json!({
            "domain": "tg.example.com", "publicAddr": "203.0.113.10:443",
            "basePath": "/telegram/web/", "carrier": "websocket", "secretMode": "plain"
        })))
        .unwrap();
        assert!(cfg.contains("base_path = \"telegram/web\""));
        assert!(cfg.contains("carrier = \"websocket\""));
        assert!(cfg.contains("secret_mode = \"plain\""));
    }

    #[test]
    fn rejects_ip_domain_bad_public_addr_and_bad_inputs() {
        let bad = |s: serde_json::Value| generate_config(&ib(s)).is_err();
        assert!(bad(json!({"domain": "203.0.113.10", "publicAddr": "203.0.113.10:443"})));
        assert!(bad(json!({"publicAddr": "203.0.113.10:443"})));
        assert!(bad(json!({"domain": "tg.example.com"})));
        assert!(bad(json!({"domain": "tg.example.com", "publicAddr": "203.0.113.10:8443"})));
        assert!(bad(ok_with("basePath", json!("../etc"))));
        assert!(bad(ok_with("carrier", json!("quic"))));
        assert!(bad(ok_with("fallbackRoot", json!("relative/dir"))));
        assert!(bad(ok_with("domain", json!("a\"b.example.com"))));
    }

    fn ok_with(key: &str, val: serde_json::Value) -> serde_json::Value {
        let mut s = ok_settings();
        s[key] = val;
        s
    }

    #[test]
    fn user_name_must_be_safe() {
        let mut inbound = ib(ok_settings());
        inbound.users[0].email = "bad\"name".into();
        inbound.users[0].name = "bad\"name".into();
        assert!(generate_config(&inbound).is_err());
    }

    #[test]
    fn metrics_are_parsed_per_user() {
        let text = "# HELP telemt_user_octets_from_client_total x\n\
telemt_user_octets_from_client_total{user=\"c1\"} 1500\n\
telemt_user_octets_to_client_total{user=\"c1\"} 4200\n\
telemt_user_octets_to_client_total{user=\"c2\"} 7\n\
telemt_user_unique_ips_current{user=\"c1\"} 2\n";
        let m = parse_telemt_metrics(text);
        assert_eq!(m["c1"].upload, 1500);
        assert_eq!(m["c1"].download, 4200);
        assert_eq!(m["c2"].download, 7);
        assert_eq!(m["c2"].upload, 0);
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn config_path_rejects_traversal() {
        assert!(tgweb_config_path(Path::new("/tmp/x"), "../evil").is_err());
        assert!(tgweb_config_path(Path::new("/tmp/x"), "abc").is_ok());
    }
}
