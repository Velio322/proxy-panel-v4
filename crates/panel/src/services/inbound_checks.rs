//! Cross-inbound validation performed on the panel when an inbound is created or updated.
//!
//! The per-inbound checks in `api::inbounds::validate_inbound_config` cannot see sibling inbounds
//! on the same node, so port conflicts and reserved ports used to surface only when the agent
//! failed to start a core. These checks run before anything is saved.

use crate::db::enums::Protocol;
use crate::error::{AppError, AppResult};
use serde_json::Value;
use std::collections::BTreeSet;

/// Ports the node itself needs: agent API, Xray gRPC, sing-box Clash API, Caddy ACME/fallback.
pub const RESERVED_PORTS: [u16; 5] = [2222, 10085, 9091, 80, 9443];

/// Transport an inbound occupies on its public port(s).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Net {
    Tcp,
    Udp,
}

/// `(port, transport)` pairs an inbound listens on.
pub fn bindings(protocol: Protocol, port: u16, settings: &Value, stream: &Value) -> Vec<(u16, Net)> {
    let tcp = |p| (p, Net::Tcp);
    let udp = |p| (p, Net::Udp);
    match protocol {
        Protocol::Hysteria2 | Protocol::Tuic => vec![udp(port)],
        Protocol::Naiveproxy => {
            let quic = matches!(
                settings.get("proto").and_then(Value::as_str).map(str::to_ascii_lowercase).as_deref(),
                Some("quic" | "both" | "h3")
            );
            if quic {
                vec![tcp(port), udp(port)]
            } else {
                vec![tcp(port)]
            }
        }
        Protocol::Mieru => {
            let transport = settings
                .get("transport")
                .or_else(|| settings.get("protocol"))
                .and_then(Value::as_str)
                .unwrap_or("tcp")
                .to_ascii_lowercase();
            let nets: &[Net] = match transport.as_str() {
                "udp" => &[Net::Udp],
                "both" | "tcp,udp" => &[Net::Tcp, Net::Udp],
                _ => &[Net::Tcp],
            };
            let ports: Vec<u16> = match parse_range(settings.get("portRange").and_then(Value::as_str)) {
                Some((a, b)) => (a..=b).collect(),
                None => vec![port],
            };
            ports
                .into_iter()
                .flat_map(|p| nets.iter().map(move |n| (p, *n)))
                .collect()
        }
        _ => {
            // Xray/sing-box stream protocols: kcp and quic transports use UDP.
            let net = stream
                .get("network")
                .and_then(Value::as_str)
                .unwrap_or("tcp")
                .to_ascii_lowercase();
            if matches!(net.as_str(), "kcp" | "quic") {
                vec![udp(port)]
            } else {
                vec![tcp(port)]
            }
        }
    }
}

/// Parses `"30000-30100"`; `None` when absent or malformed (malformed is rejected elsewhere).
pub fn parse_range(raw: Option<&str>) -> Option<(u16, u16)> {
    let (a, b) = raw?.trim().split_once('-')?;
    let (a, b): (u16, u16) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
    (a <= b).then_some((a, b))
}

/// Static, node-independent rules (no database needed).
pub fn check_static(protocol: Protocol, port: i32, settings: &Value, stream: &Value) -> AppResult<()> {
    let port_u16 = u16::try_from(port)
        .ok()
        .filter(|p| *p != 0)
        .ok_or_else(|| AppError::validation("port", "Port must be between 1 and 65535"))?;

    match protocol {
        Protocol::Vless | Protocol::Vmess | Protocol::Trojan => {
            let security = stream
                .get("security")
                .and_then(Value::as_str)
                .unwrap_or("none")
                .to_ascii_lowercase();
            if security == "reality" {
                let r = stream.get("realitySettings").unwrap_or(&Value::Null);
                let pick = |keys: &[&str]| {
                    keys.iter()
                        .find_map(|k| {
                            r.get(*k)
                                .or_else(|| stream.get(*k))
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                        })
                        .is_some()
                };
                if !pick(&["privateKey"]) {
                    return Err(AppError::validation(
                        "stream.realitySettings.privateKey",
                        "Reality requires a private key",
                    ));
                }
                if !pick(&["target", "dest"]) {
                    return Err(AppError::validation(
                        "stream.realitySettings.target",
                        "Reality requires a target (dest), e.g. www.microsoft.com:443",
                    ));
                }
                let has_names = r
                    .get("serverNames")
                    .or_else(|| stream.get("serverNames"))
                    .and_then(Value::as_array)
                    .is_some_and(|a| a.iter().any(|v| v.as_str().is_some_and(|s| !s.trim().is_empty())));
                if !has_names {
                    return Err(AppError::validation(
                        "stream.realitySettings.serverNames",
                        "Reality requires at least one server name",
                    ));
                }
            }
        }
        Protocol::Mieru => {
            if let Some(raw) = settings
                .get("portRange")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                let (a, b) = parse_range(Some(raw)).ok_or_else(|| {
                    AppError::validation("settings.portRange", "Port range must look like 30000-30100")
                })?;
                if a < 1025 {
                    return Err(AppError::validation(
                        "settings.portRange",
                        "Mieru port range must start at 1025 or above",
                    ));
                }
                if u32::from(b) - u32::from(a) > 2000 {
                    return Err(AppError::validation(
                        "settings.portRange",
                        "Mieru port range is limited to 2000 ports",
                    ));
                }
            } else if port_u16 < 1025 {
                return Err(AppError::validation("port", "Mieru ports must be 1025 or above"));
            }
        }
        Protocol::Naiveproxy => {
            let s = |k: &str| settings.get(k).and_then(Value::as_str).map(str::trim).unwrap_or("");
            let domain = if s("domain").is_empty() { s("sni") } else { s("domain") };
            if domain.is_empty() {
                return Err(AppError::validation(
                    "settings.domain",
                    "NaiveProxy requires a domain (used for TLS and probe resistance)",
                ));
            }
            let custom = s("tlsMode").eq_ignore_ascii_case("custom");
            let cert = |k: &str| {
                let v = s(k);
                if v.is_empty() {
                    stream.get(k).and_then(Value::as_str).map(str::trim).unwrap_or("")
                } else {
                    v
                }
            };
            if custom {
                if cert("certFile").is_empty() || cert("keyFile").is_empty() {
                    return Err(AppError::validation(
                        "settings.certFile",
                        "tlsMode=custom requires certFile and keyFile",
                    ));
                }
            } else if s("email").is_empty() && !domain.contains('.') {
                return Err(AppError::validation(
                    "settings.email",
                    "Let's Encrypt mode requires a real domain name or an email",
                ));
            }
        }
        Protocol::Tgweb => {
            let s = |k: &str| settings.get(k).and_then(Value::as_str).map(str::trim).unwrap_or("");
            let domain = if s("domain").is_empty() { s("sni") } else { s("domain") };
            let looks_like_dns = domain.contains('.')
                && domain.parse::<std::net::IpAddr>().is_err()
                && domain
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'));
            if !looks_like_dns {
                return Err(AppError::validation(
                    "settings.domain",
                    "Telegram WEB proxy needs a DNS name with a TLS certificate (not an IP address)",
                ));
            }
            let public: Option<std::net::SocketAddr> = s("publicAddr").parse().ok();
            if public.map(|a| a.port()) != Some(443) {
                return Err(AppError::validation(
                    "settings.publicAddr",
                    "publicAddr must be '<node public IP>:443'",
                ));
            }
            let base = s("basePath").trim_matches('/');
            if !base.is_empty()
                && !base.split('/').all(|seg| {
                    !seg.is_empty()
                        && seg.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
                        && seg.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
                })
            {
                return Err(AppError::validation(
                    "settings.basePath",
                    "basePath must be slash-separated [A-Za-z0-9_-] segments",
                ));
            }
            if !matches!(s("secretMode"), "" | "dd" | "plain") {
                return Err(AppError::validation("settings.secretMode", "secretMode must be dd or plain"));
            }
            if !matches!(s("carrier"), "" | "https" | "https-lanes" | "websocket" | "websocket-lanes") {
                return Err(AppError::validation("settings.carrier", "Unknown carrier"));
            }
            // telemt listens on loopback only; the public port belongs to the TLS terminator.
            if port_u16 < 1025 {
                return Err(AppError::validation(
                    "port",
                    "The Telegram WEB listener is a private loopback port (1025 or above, e.g. 18080)",
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

/// A sibling inbound on the same node, as loaded from the database.
#[derive(Debug, Clone)]
pub struct Sibling {
    pub id: String,
    pub tag: String,
    pub protocol: Protocol,
    pub port: u16,
    pub settings: Value,
    pub stream: Value,
}

/// Port-conflict and reserved-port checks against the other inbounds of the same node.
///
/// Two inbounds on the same `(port, transport)` are rejected unless the pair is a legitimate
/// SNI/HTTP share, which uses PortShare rather than a second inbound.
pub fn check_conflicts(
    self_id: Option<&str>,
    protocol: Protocol,
    port: i32,
    settings: &Value,
    stream: &Value,
    siblings: &[Sibling],
) -> AppResult<()> {
    let port_u16 = u16::try_from(port).unwrap_or(0);
    let mine: BTreeSet<(u16, Net)> = bindings(protocol, port_u16, settings, stream).into_iter().collect();

    for (p, _) in &mine {
        if RESERVED_PORTS.contains(p) {
            return Err(AppError::validation(
                "port",
                format!("Port {p} is reserved for the node (agent API, core APIs, ACME)"),
            ));
        }
    }

    for sib in siblings.iter().filter(|s| Some(s.id.as_str()) != self_id) {
        let theirs: BTreeSet<(u16, Net)> =
            bindings(sib.protocol, sib.port, &sib.settings, &sib.stream).into_iter().collect();
        if let Some((p, n)) = mine.intersection(&theirs).next() {
            let proto = match n {
                Net::Tcp => "tcp",
                Net::Udp => "udp",
            };
            return Err(AppError::validation(
                "port",
                format!(
                    "Port {p}/{proto} is already used by inbound '{}' on this node; use a different port or a PortShare",
                    sib.tag
                ),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sib(id: &str, protocol: Protocol, port: u16, settings: Value) -> Sibling {
        Sibling { id: id.into(), tag: id.into(), protocol, port, settings, stream: json!({}) }
    }

    #[test]
    fn bindings_cover_transports() {
        assert_eq!(bindings(Protocol::Hysteria2, 443, &json!({}), &json!({})), vec![(443, Net::Udp)]);
        assert_eq!(bindings(Protocol::Vless, 443, &json!({}), &json!({})), vec![(443, Net::Tcp)]);
        assert_eq!(
            bindings(Protocol::Vless, 443, &json!({}), &json!({"network":"kcp"})),
            vec![(443, Net::Udp)]
        );
        let both = bindings(Protocol::Mieru, 2053, &json!({"transport":"both"}), &json!({}));
        assert!(both.contains(&(2053, Net::Tcp)) && both.contains(&(2053, Net::Udp)));
        let range = bindings(Protocol::Mieru, 30000, &json!({"transport":"udp","portRange":"30000-30002"}), &json!({}));
        assert_eq!(range.len(), 3);
    }

    #[test]
    fn tcp_and_udp_on_same_port_do_not_conflict() {
        let siblings = vec![sib("vless", Protocol::Vless, 443, json!({}))];
        // Hysteria2 is UDP/443 next to VLESS TCP/443: allowed.
        assert!(check_conflicts(None, Protocol::Hysteria2, 443, &json!({}), &json!({}), &siblings).is_ok());
        // Naive TCP/443 collides with VLESS TCP/443.
        let err = check_conflicts(None, Protocol::Naiveproxy, 443, &json!({"proto":"https"}), &json!({}), &siblings);
        assert!(err.is_err());
    }

    #[test]
    fn mieru_range_overlap_and_self_exclusion() {
        let siblings = vec![sib("m1", Protocol::Mieru, 30000, json!({"transport":"tcp","portRange":"30000-30100"}))];
        let clash = check_conflicts(None, Protocol::Mieru, 30050, &json!({"transport":"tcp"}), &json!({}), &siblings);
        assert!(clash.is_err());
        // Updating m1 itself must not conflict with itself.
        assert!(check_conflicts(
            Some("m1"), Protocol::Mieru, 30000,
            &json!({"transport":"tcp","portRange":"30000-30100"}), &json!({}), &siblings
        ).is_ok());
    }

    #[test]
    fn reserved_ports_are_rejected() {
        for p in [2222, 10085, 80, 9443] {
            assert!(check_conflicts(None, Protocol::Vless, p, &json!({}), &json!({}), &[]).is_err(), "{p}");
        }
        assert!(check_conflicts(None, Protocol::Vless, 8443, &json!({}), &json!({}), &[]).is_ok());
    }

    #[test]
    fn reality_requires_key_target_and_names() {
        let ok = json!({"network":"tcp","security":"reality","realitySettings":{
            "privateKey":"k","target":"www.microsoft.com:443","serverNames":["www.microsoft.com"]}});
        assert!(check_static(Protocol::Vless, 443, &json!({}), &ok).is_ok());
        for broken in [
            json!({"security":"reality","realitySettings":{"target":"a:443","serverNames":["a"]}}),
            json!({"security":"reality","realitySettings":{"privateKey":"k","serverNames":["a"]}}),
            json!({"security":"reality","realitySettings":{"privateKey":"k","target":"a:443"}}),
        ] {
            assert!(check_static(Protocol::Vless, 443, &json!({}), &broken).is_err());
        }
    }

    #[test]
    fn mieru_static_rules() {
        assert!(check_static(Protocol::Mieru, 2053, &json!({"transport":"tcp"}), &json!({})).is_ok());
        assert!(check_static(Protocol::Mieru, 443, &json!({}), &json!({})).is_err());
        assert!(check_static(Protocol::Mieru, 30000, &json!({"portRange":"abc"}), &json!({})).is_err());
        assert!(check_static(Protocol::Mieru, 30000, &json!({"portRange":"100-200"}), &json!({})).is_err());
        assert!(check_static(Protocol::Mieru, 30000, &json!({"portRange":"30000-39000"}), &json!({})).is_err());
        assert!(check_static(Protocol::Mieru, 30000, &json!({"portRange":"30000-30100"}), &json!({})).is_ok());
        let tg = json!({"domain":"tg.example.com","publicAddr":"203.0.113.10:443"});
        assert!(check_static(Protocol::Tgweb, 18080, &tg, &json!({})).is_ok());
        assert!(check_static(Protocol::Tgweb, 443, &tg, &json!({})).is_err());
        assert!(check_static(Protocol::Tgweb, 18080, &json!({"domain":"203.0.113.10","publicAddr":"203.0.113.10:443"}), &json!({})).is_err());
        assert!(check_static(Protocol::Tgweb, 18080, &json!({"domain":"tg.example.com"}), &json!({})).is_err());
        assert!(check_static(Protocol::Tgweb, 18080, &json!({"domain":"tg.example.com","publicAddr":"203.0.113.10:8443"}), &json!({})).is_err());
        assert!(check_static(Protocol::Tgweb, 18080, &json!({"domain":"tg.example.com","publicAddr":"203.0.113.10:443","basePath":"../x"}), &json!({})).is_err());
    }

    #[test]
    fn naive_static_rules() {
        let acme = json!({"domain":"naive.example.com","tlsMode":"letsencrypt","email":"a@b.c"});
        assert!(check_static(Protocol::Naiveproxy, 8443, &acme, &json!({})).is_ok());
        assert!(check_static(Protocol::Naiveproxy, 8443, &json!({"tlsMode":"letsencrypt"}), &json!({})).is_err());
        let custom_missing = json!({"domain":"n.example.com","tlsMode":"custom"});
        assert!(check_static(Protocol::Naiveproxy, 8443, &custom_missing, &json!({})).is_err());
        let custom_ok = json!({"domain":"n.example.com","tlsMode":"custom","certFile":"/c","keyFile":"/k"});
        assert!(check_static(Protocol::Naiveproxy, 8443, &custom_ok, &json!({})).is_ok());
    }
}
