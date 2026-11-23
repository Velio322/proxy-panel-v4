//! Opens public ports for NaiveProxy / Mieru inbounds in UFW.
//!
//! Only ever *adds* `allow` rules, only when `ufw` exists and is active, and can be disabled with
//! `MANAGE_UFW=false`. Rules are idempotent (`ufw` skips existing ones).

use crate::cores::mieru::build_port_bindings;
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

/// Public ports that an enabled Naive/Mieru inbound needs reachable.
pub fn rules_for_inbound(ib: &InboundConfig) -> Vec<FirewallRule> {
    if !ib.enable {
        return Vec::new();
    }
    let comment = safe_comment(&ib.tag);
    match ib.protocol {
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
        _ => Vec::new(),
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
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let out = tokio::time::timeout(UFW_TIMEOUT, cmd.output())
        .await
        .map_err(|_| "ufw timed out".to_string())?
        .map_err(|e| e.to_string())?;
    Ok((
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
    ))
}

/// Adds the required allow rules; returns human-readable warnings (never fails the config apply).
pub async fn open_ports_for_inbounds(inbounds: &[InboundConfig]) -> Vec<String> {
    if !management_enabled() {
        return Vec::new();
    }
    let mut rules: Vec<FirewallRule> = inbounds.iter().flat_map(rules_for_inbound).collect();
    rules.sort_by(|a, b| (&a.port, a.proto).cmp(&(&b.port, b.proto)));
    rules.dedup_by(|a, b| a.port == b.port && a.proto == b.proto);
    if rules.is_empty() {
        return Vec::new();
    }

    // `ufw` missing or inactive: nothing to do (the host may use another firewall).
    match run_ufw(&["status"]).await {
        Ok((true, out)) if out.trim_start().starts_with("Status: active") => {}
        _ => return Vec::new(),
    }

    let mut warnings = Vec::new();
    for rule in rules {
        let spec = rule.spec();
        match run_ufw(&["allow", &spec, "comment", &rule.comment]).await {
            Ok((true, _)) => tracing::info!(rule = %spec, "ufw allow ensured"),
            Ok((false, out)) => warnings.push(format!("ufw allow {spec} failed: {}", out.trim())),
            Err(e) => warnings.push(format!("ufw allow {spec} failed: {e}")),
        }
    }
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
    fn disabled_and_other_protocols_yield_no_rules() {
        let mut ib = inbound("MIERU", 2053, json!({}));
        ib.enable = false;
        assert!(rules_for_inbound(&ib).is_empty());
        assert!(rules_for_inbound(&inbound("VLESS", 443, json!({}))).is_empty());
    }

    #[test]
    fn comment_is_sanitised() {
        let r = rules_for_inbound(&inbound("MIERU", 2053, json!({})));
        assert_eq!(r[0].comment, "mypanel3-mytagx");
    }
}
