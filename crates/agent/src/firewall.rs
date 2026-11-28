//! Opens public ports for panel-managed inbounds in UFW.
//!
//! Only ever *adds* `allow` rules, only when `ufw` exists and is active, and can be disabled with
//! `MANAGE_UFW=false`. Rules are idempotent (`ufw` skips existing ones). Naive gets TCP (+UDP for
//! QUIC), Mieru gets its transport bindings, every other protocol (VLESS, Trojan, ...) gets its
//! TCP listen port — but only when the inbound actually binds a public address.

use crate::cores::mieru::build_port_bindings;
use crate::cores::write_atomic_0600;
use mypanel_proto::{InboundConfig, Protocol};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

const UFW_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirewallRule {
    /// `2053` or `30000:30100` (UFW range syntax).
    pub port: String,
    /// `tcp` or `udp`.
    pub proto: &'static str,
    pub comment: String,
}

impl FirewallRule {
    fn spec(&self) -> String {
        format!("{}/{}", self.port, self.proto)
    }
}

fn safe_comment(tag: &str) -> String {
    let cleaned: String = tag
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .take(40)
        .collect();
    format!("mypanel3-{cleaned}")
}

/// True when the inbound listens on every interface (the common case) and therefore needs a
/// firewall hole. Loopback-only binds are reached through a local reverse proxy and must not
/// be opened in UFW.
fn binds_public(listen: &str) -> bool {
    matches!(listen.trim(), "" | "0.0.0.0" | "::" | "[::]" | "::/0")
}

fn parse_validated_ufw_port_token(part: &str) -> Option<String> {
    let trimmed = part.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some((a, b)) = trimmed.split_once('-') {
        let start: u16 = a.trim().parse().ok()?;
        let end: u16 = b.trim().parse().ok()?;
        if start == 0 || end == 0 || start > end {
            return None;
        }
        Some(if start == end {
            start.to_string()
        } else {
            format!("{start}:{end}")
        })
    } else {
        let p: u16 = trimmed.parse().ok()?;
        (p >= 1).then(|| p.to_string())
    }
}

/// Public ports that an enabled inbound needs reachable.
pub fn rules_for_inbound(ib: &InboundConfig) -> Vec<FirewallRule> {
    if !ib.enable || !binds_public(&ib.listen) {
        return Vec::new();
    }
    let comment = safe_comment(&ib.tag);
    match ib.protocol {
        Protocol::Tgweb => Vec::new(),
        Protocol::Tuic => vec![FirewallRule {
            port: ib.port.to_string(),
            proto: "udp",
            comment,
        }],
        Protocol::Hysteria2 => {
            let mut rules = vec![FirewallRule {
                port: ib.port.to_string(),
                proto: "udp",
                comment: comment.clone(),
            }];
            if let Some(range) = ib.settings.get("portRange").and_then(|v| v.as_str()) {
                for part in range.split(',') {
                    if let Some(ufw_port) = parse_validated_ufw_port_token(part) {
                        if ufw_port != ib.port.to_string() {
                            rules.push(FirewallRule {
                                port: ufw_port,
                                proto: "udp",
                                comment: comment.clone(),
                            });
                        }
                    }
                }
            }
            rules
        }
        Protocol::Shadowsocks => {
            let net = ib
                .settings
                .get("network")
                .and_then(|v| v.as_str())
                .unwrap_or("tcp,udp")
                .to_ascii_lowercase();
            let mut rules = Vec::new();
            if net.contains("tcp") || net.is_empty() {
                rules.push(FirewallRule {
                    port: ib.port.to_string(),
                    proto: "tcp",
                    comment: comment.clone(),
                });
            }
            if net.contains("udp") {
                rules.push(FirewallRule {
                    port: ib.port.to_string(),
                    proto: "udp",
                    comment,
                });
            }
            rules
        }
        Protocol::Naiveproxy => {
            let mut rules = vec![FirewallRule {
                port: ib.port.to_string(),
                proto: "tcp",
                comment: comment.clone(),
            }];
            let proto = ib
                .settings
                .get("proto")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if matches!(proto.as_str(), "quic" | "both" | "h3") {
                rules.push(FirewallRule {
                    port: ib.port.to_string(),
                    proto: "udp",
                    comment,
                });
            }
            rules
        }
        Protocol::Mieru => build_port_bindings(ib)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|b| {
                let port = if let Some(r) = b.get("portRange").and_then(|v| v.as_str()) {
                    r.replace('-', ":")
                } else {
                    b.get("port")?.as_u64()?.to_string()
                };
                let proto = match b.get("protocol").and_then(|v| v.as_str()) {
                    Some("UDP") => "udp",
                    Some("TCP") => "tcp",
                    _ => return None,
                };
                Some(FirewallRule {
                    port,
                    proto,
                    comment: comment.clone(),
                })
            })
            .collect(),
        _ => vec![FirewallRule {
            port: ib.port.to_string(),
            proto: "tcp",
            comment,
        }],
    }
}

fn management_enabled() -> bool {
    !matches!(
        std::env::var("MANAGE_UFW")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "false" | "0" | "no" | "off"
    )
}

async fn run_ufw(args: &[&str]) -> Result<(bool, String), String> {
    let mut cmd = Command::new("ufw");
    cmd.args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let out = tokio::time::timeout(UFW_TIMEOUT, cmd.output())
        .await
        .map_err(|_| "ufw timed out".to_string())?
        .map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    let combined = if stderr.is_empty() {
        stdout
    } else if stdout.trim().is_empty() {
        stderr
    } else {
        format!("{stdout}\n{stderr}")
    };
    Ok((out.status.success(), combined))
}

const STATE_FILE: &str = "firewall-managed.json";

/// Rules this agent added itself, persisted so stale ones can be removed later.
async fn load_managed(path: &std::path::Path) -> Vec<(String, String)> {
    tokio::fs::read_to_string(path)
        .await
        .ok()
        .and_then(|raw| serde_json::from_str::<Vec<(String, String)>>(&raw).ok())
        .unwrap_or_default()
}

async fn save_managed(path: &std::path::Path, rules: &[(String, String)]) {
    if let Ok(raw) = serde_json::to_vec(rules) {
        let _ = write_atomic_0600(path, &raw).await;
    }
}

fn is_protected_rule(port: &str, proto: &str) -> bool {
    proto == "tcp" && matches!(port, "22" | "80" | "443")
}

/// Makes UFW match the enabled inbounds: adds missing `allow` rules and deletes rules that this
/// agent previously added for inbounds that no longer exist. Pre-existing rules not created by
/// the agent are never tracked or deleted. Returns human-readable warnings (never fails apply).
pub async fn sync_ports_for_inbounds(
    state_dir: &std::path::Path,
    inbounds: &[InboundConfig],
) -> Vec<String> {
    if !management_enabled() {
        return Vec::new();
    }
    let wanted: Vec<FirewallRule> = {
        let mut r: Vec<FirewallRule> = inbounds.iter().flat_map(rules_for_inbound).collect();
        r.sort_by(|a, b| (&a.port, a.proto).cmp(&(&b.port, b.proto)));
        r.dedup_by(|a, b| a.port == b.port && a.proto == b.proto);
        r
    };
    let state_path = state_dir.join(STATE_FILE);
    let previous = load_managed(&state_path).await;
    if wanted.is_empty() && previous.is_empty() {
        return Vec::new();
    }

    // `ufw` missing or inactive: nothing to do (the host may use another firewall).
    match run_ufw(&["status"]).await {
        Ok((true, out)) if out.trim_start().starts_with("Status: active") => {}
        _ => return Vec::new(),
    }

    let mut warnings = Vec::new();
    let mut managed: Vec<(String, String)> = Vec::new();

    for rule in &wanted {
        let spec = rule.spec();
        let was_managed = previous
            .iter()
            .any(|(p, pr)| p == &rule.port && pr == rule.proto);
        match run_ufw(&["allow", &spec, "comment", &rule.comment]).await {
            Ok((true, out)) => {
                let pre_existing = out
                    .to_ascii_lowercase()
                    .contains("skipping adding existing rule");
                if was_managed || !pre_existing {
                    tracing::info!(rule = %spec, "ufw allow ensured");
                    managed.push((rule.port.clone(), rule.proto.to_string()));
                } else {
                    tracing::debug!(rule = %spec, "ufw rule already existed outside agent; leaving untracked");
                }
            }
            Ok((false, out)) => warnings.push(format!("ufw allow {spec} failed: {}", out.trim())),
            Err(e) => warnings.push(format!("ufw allow {spec} failed: {e}")),
        }
    }

    for (port, proto) in previous {
        if managed.iter().any(|(p, pr)| *p == port && *pr == proto) {
            continue;
        }
        if is_protected_rule(&port, &proto) {
            continue;
        }
        let spec = format!("{port}/{proto}");
        match run_ufw(&["--force", "delete", "allow", &spec]).await {
            Ok((true, _)) => tracing::info!(rule = %spec, "stale ufw rule removed"),
            Ok((false, out)) => {
                warnings.push(format!("ufw delete allow {spec} failed: {}", out.trim()));
                managed.push((port, proto));
            }
            Err(e) => {
                warnings.push(format!("ufw delete allow {spec} failed: {e}"));
                managed.push((port, proto));
            }
        }
    }

    managed.sort();
    managed.dedup();
    save_managed(&state_path, &managed).await;
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn inbound(protocol: &str, port: u16, settings: serde_json::Value) -> InboundConfig {
        serde_json::from_value(json!({
            "id": "i1",
            "tag": "my tag;$(x)",
            "protocol": protocol,
            "port": port,
            "settings": settings,
        }))
        .expect("valid inbound")
    }

    #[test]
    fn naive_https_opens_tcp_only_and_quic_adds_udp() {
        let tcp = rules_for_inbound(&inbound("NAIVEPROXY", 8443, json!({"proto": "https"})));
        assert_eq!(tcp.len(), 1);
        assert_eq!((tcp[0].port.as_str(), tcp[0].proto), ("8443", "tcp"));

        let quic = rules_for_inbound(&inbound("NAIVEPROXY", 8443, json!({"proto": "quic"})));
        assert_eq!(quic.iter().map(|r| r.proto).collect::<Vec<_>>(), ["tcp", "udp"]);
    }

    #[test]
    fn mieru_both_and_port_range() {
        let both = rules_for_inbound(&inbound("MIERU", 2053, json!({"transport": "both"})));
        assert_eq!(both.len(), 2);
        assert!(both.iter().all(|r| r.port == "2053"));

        let range = rules_for_inbound(&inbound(
            "MIERU",
            30000,
            json!({"transport": "udp", "portRange": "30000-30100"}),
        ));
        assert_eq!(range.len(), 1);
        assert_eq!((range[0].port.as_str(), range[0].proto), ("30000:30100", "udp"));
    }

    #[test]
    fn disabled_yields_no_rules_and_vless_opens_tcp() {
        let mut ib = inbound("MIERU", 2053, json!({}));
        ib.enable = false;
        assert!(rules_for_inbound(&ib).is_empty());
        let vless = rules_for_inbound(&inbound("VLESS", 993, json!({})));
        assert_eq!(vless.len(), 1);
        assert_eq!((vless[0].port.as_str(), vless[0].proto), ("993", "tcp"));
    }

    #[test]
    fn loopback_bind_yields_no_rules() {
        let mut ib = inbound("VLESS", 443, json!({}));
        ib.listen = "127.0.0.1".to_string();
        assert!(rules_for_inbound(&ib).is_empty());
    }

    #[test]
    fn comment_is_sanitised() {
        let r = rules_for_inbound(&inbound("MIERU", 2053, json!({})));
        assert_eq!(r[0].comment, "mypanel3-mytagx");
    }

    #[test]
    fn udp_and_loopback_protocols_mapped_correctly() {
        let hy2 = rules_for_inbound(&inbound(
            "HYSTERIA2",
            443,
            json!({"portRange": "20000-20010"}),
        ));
        assert_eq!(hy2.len(), 2);
        assert_eq!((hy2[0].port.as_str(), hy2[0].proto), ("443", "udp"));
        assert_eq!((hy2[1].port.as_str(), hy2[1].proto), ("20000:20010", "udp"));

        let tuic = rules_for_inbound(&inbound("TUIC", 8443, json!({})));
        assert_eq!(tuic.len(), 1);
        assert_eq!((tuic[0].port.as_str(), tuic[0].proto), ("8443", "udp"));

        let ss = rules_for_inbound(&inbound("SHADOWSOCKS", 8388, json!({})));
        assert_eq!(ss.iter().map(|r| r.proto).collect::<Vec<_>>(), ["tcp", "udp"]);

        let tgweb = rules_for_inbound(&inbound("TGWEB", 18080, json!({})));
        assert!(tgweb.is_empty());
    }
}
