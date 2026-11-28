use crate::cores::{extract_protocol_users, safe_id, str_or_empty, validate_inbound, CoreUser};
use mypanel_proto::{InboundConfig, Protocol};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

/// Computes Mieru's `hashedPassword` (`hex(SHA-256(password || 0x00 || username))`),
/// matching `github.com/enfein/mieru/v3/pkg/cipher.HashPassword`.
pub fn hash_mieru_password(username: &str, password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hasher.update([0u8]);
    hasher.update(username.as_bytes());
    hex::encode(hasher.finalize())
}

/// Returns `<CONFIG_DIR>/mieru-<safe_id>.json`.
pub fn mieru_config_path(config_dir: &Path, inbound_id: &str) -> Result<PathBuf, String> {
    let id = safe_id(inbound_id)?;
    Ok(config_dir.join(format!("mieru-{id}.json")))
}

/// Parses and validates Mieru transport (`tcp`, `udp`, `both`, `tcp,udp`) into uppercase protocol names.
pub fn parse_mieru_protocols(settings: &Value) -> Result<Vec<&'static str>, String> {
    let raw = settings
        .get("transport")
        .or_else(|| settings.get("protocol"))
        .and_then(Value::as_str)
        .unwrap_or("tcp")
        .trim()
        .to_ascii_lowercase();

    match raw.as_str() {
        "tcp" | "" => Ok(vec!["TCP"]),
        "udp" => Ok(vec!["UDP"]),
        "both" | "tcp,udp" => Ok(vec!["TCP", "UDP"]),
        _ => Err(format!("Invalid Mieru transport: {raw}")),
    }
}

/// Validates and builds Mieru `portBindings` array (`port` or `portRange` `"start-end"` with `1025..=65535`).
pub fn build_port_bindings(inbound: &InboundConfig) -> Result<Vec<Value>, String> {
    let protocols = parse_mieru_protocols(&inbound.settings)?;
    let port_range = inbound
        .settings
        .get("portRange")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());

    if let Some(range) = port_range {
        let (a, b) = range
            .split_once('-')
            .ok_or_else(|| format!("Invalid Mieru port range: {range}"))?;
        let start: u16 = a
            .trim()
            .parse()
            .map_err(|_| format!("Invalid Mieru port range: {range}"))?;
        let end: u16 = b
            .trim()
            .parse()
            .map_err(|_| format!("Invalid Mieru port range: {range}"))?;
        if start < 1025 || start > end {
            return Err(format!("Invalid Mieru port range: {range}"));
        }
        let normalized = format!("{start}-{end}");
        Ok(protocols
            .into_iter()
            .map(|p| {
                json!({
                    "portRange": normalized,
                    "protocol": p
                })
            })
            .collect())
    } else {
        if inbound.port == 0 {
            return Err("Inbound port must be between 1 and 65535".into());
        }
        Ok(protocols
            .into_iter()
            .map(|p| {
                json!({
                    "port": inbound.port,
                    "protocol": p
                })
            })
            .collect())
    }
}

/// Generates a Mieru (`mita`) server JSON configuration (`<CONFIG_DIR>/mieru-<id>.json`).
pub fn generate_config(inbound: &InboundConfig) -> Result<Value, String> {
    if inbound.protocol != Protocol::Mieru {
        return Err(format!(
            "Protocol {} is not supported by Mieru",
            inbound.protocol
        ));
    }
    validate_inbound(inbound)?;

    let bindings = build_port_bindings(inbound)?;
    let users = extract_protocol_users(&inbound.users, &inbound.settings, true)?;
    let user_vals: Vec<Value> = users
        .into_iter()
        .map(|u| {
            let hashed = hash_mieru_password(&u.name, &u.password);
            json!({
                "name": u.name,
                "password": u.password,
                "hashedPassword": hashed
            })
        })
        .collect();

    let logging_level = {
        let lvl = str_or_empty(&inbound.settings, "loggingLevel");
        if lvl.is_empty() {
            "WARN"
        } else {
            lvl
        }
    };

    let mtu = inbound
        .settings
        .get("mtu")
        .and_then(Value::as_u64)
        .filter(|&m| (1280..=1500).contains(&m))
        .unwrap_or(1400);

    Ok(json!({
        "portBindings": bindings,
        "users": user_vals,
        "loggingLevel": logging_level,
        "mtu": mtu,
        "egress": {
            "rules": [
                {
                    "ipRanges": [
                        "127.0.0.0/8",
                        "10.0.0.0/8",
                        "172.16.0.0/12",
                        "192.168.0.0/16",
                        "169.254.0.0/16",
                        "100.64.0.0/10",
                        "0.0.0.0/8",
                        "::1/128",
                        "::/128",
                        "fc00::/7",
                        "fe80::/10"
                    ],
                    "action": "REJECT"
                },
                {
                    "domainNames": ["localhost", "localhost.localdomain"],
                    "action": "REJECT"
                }
            ]
        }
    }))
}

/// Generates a Mieru client profile JSON helper for subscription / diagnostics.
pub fn generate_client_config(
    inbound: &InboundConfig,
    host: &str,
    user: &CoreUser,
) -> Result<Value, String> {
    let bindings = build_port_bindings(inbound)?;
    let domain_name = if !host.trim().is_empty() {
        host.trim()
    } else {
        let d = str_or_empty(&inbound.settings, "domain");
        if !d.is_empty() {
            d
        } else {
            str_or_empty(&inbound.settings, "sni")
        }
    };

    Ok(json!({
        "profiles": [{
            "profileName": inbound.tag,
            "user": {
                "name": user.name,
                "password": user.password
            },
            "servers": [{
                "domainName": domain_name,
                "portBindings": bindings
            }]
        }],
        "activeProfile": inbound.tag,
        "socks5Port": 1080
    }))
}

const MITA_USER: &str = "mita";
/// Directories `mita` expects to own (same as the official package `postinst`).
const MITA_DIRS: [&str; 3] = ["/etc/mita", "/var/lib/mita", "/var/run/mita"];

async fn run_quiet(program: &str, args: &[&str]) -> Result<bool, String> {
    let fut = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let status = tokio::time::timeout(Duration::from_secs(15), fut)
        .await
        .map_err(|_| format!("{program} timed out"))?
        .map_err(|e| format!("{program}: {e}"))?;
    Ok(status.success())
}

/// `mita` refuses to start (`getUid("mita") failed: unknown user mita`) unless a `mita` system user
/// exists. The official `.deb` creates it in `postinst`, but a bare binary install (install.sh or
/// panel auto-provisioning) does not, so create the user and runtime directories here (idempotent).
pub async fn ensure_mita_runtime() -> Result<(), String> {
    if !run_quiet("id", &[MITA_USER]).await.unwrap_or(false)
        && !run_quiet(
            "useradd",
            &["--system", "--no-create-home", "--user-group", MITA_USER],
        )
        .await?
    {
        return Err("cannot create system user 'mita' (useradd failed)".into());
    }
    for dir in MITA_DIRS {
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(|e| format!("cannot create {dir}: {e}"))?;
        let owner = format!("{MITA_USER}:{MITA_USER}");
        if !run_quiet("chown", &["-R", &owner, dir]).await? {
            return Err(format!("cannot chown {dir} to {owner}"));
        }
        run_quiet("chmod", &["775", dir]).await?;
    }
    Ok(())
}

#[cfg(test)]
mod runtime_tests {
    use super::*;

    #[test]
    fn mita_dirs_match_official_package() {
        assert_eq!(MITA_DIRS, ["/etc/mita", "/var/lib/mita", "/var/run/mita"]);
    }

    #[tokio::test]
    async fn run_quiet_reports_exit_status() {
        assert!(run_quiet("true", &[]).await.unwrap());
        assert!(!run_quiet("false", &[]).await.unwrap());
        assert!(run_quiet("definitely-not-a-real-binary-xyz", &[]).await.is_err());
    }

    #[test]
    fn generate_config_includes_egress_loopback_and_private_blocks() {
        let inbound: InboundConfig = serde_json::from_value(json!({
            "id": "m1",
            "tag": "mieru-2053",
            "protocol": "MIERU",
            "port": 2053,
            "settings": {"transport": "tcp"},
            "users": [{"id": "u1", "username": "alice", "uuid": "00000000-0000-0000-0000-000000000001", "password": "pw"}]
        }))
        .unwrap();
        let cfg = generate_config(&inbound).unwrap();
        let rules = cfg["egress"]["rules"].as_array().unwrap();
        assert!(!rules.is_empty());
        let ips: Vec<&str> = rules
            .iter()
            .filter_map(|r| r["ipRanges"].as_array())
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        for blocked in ["127.0.0.0/8", "10.0.0.0/8", "169.254.0.0/16", "::1/128"] {
            assert!(ips.contains(&blocked), "missing egress block for {blocked}");
        }
        assert!(!ips.contains(&"::ffff:0:0/96"));
        assert!(rules.iter().all(|r| r["action"] == "REJECT"));
    }
}
