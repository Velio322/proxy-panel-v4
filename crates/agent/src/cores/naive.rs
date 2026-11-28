use crate::cores::{extract_protocol_users, safe_id, str_or_empty, validate_inbound};
use mypanel_proto::{InboundConfig, Protocol};
use std::path::{Path, PathBuf};

/// Default fallback HTML served by Caddy's `file_server` when no decoy site is configured.
/// Deliberately looks like an ordinary parked page instead of a bare "Welcome" stub.
const DEFAULT_INDEX_HTML: &str = "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>Under maintenance</title>\n<style>body{font-family:system-ui,sans-serif;background:#f6f7f9;color:#333;display:flex;min-height:100vh;align-items:center;justify-content:center;margin:0}main{text-align:center;padding:2rem}h1{font-weight:500;font-size:1.6rem}p{color:#777}</style>\n</head>\n<body>\n<main>\n<h1>We'll be back soon</h1>\n<p>This site is undergoing scheduled maintenance.</p>\n</main>\n</body>\n</html>\n";

/// Returns `<CONFIG_DIR>/naive-<safe_id>.Caddyfile`.
pub fn caddyfile_path(config_dir: &Path, inbound_id: &str) -> Result<PathBuf, String> {
    let id = safe_id(inbound_id)?;
    Ok(config_dir.join(format!("naive-{id}.Caddyfile")))
}

/// Longest unix socket path accepted by the kernel is 107 bytes; stay below it.
const MAX_SOCKET_PATH: usize = 100;

/// Admin API socket of one naive (Caddy) process: `<CONFIG_DIR>/naive-<safe_id>.admin.sock`.
/// Used to apply user/config changes with `caddy reload` instead of killing the process.
pub fn admin_socket_path(config_dir: &Path, inbound_id: &str) -> Result<PathBuf, String> {
    let id = safe_id(inbound_id)?;
    Ok(config_dir.join(format!("naive-{id}.admin.sock")))
}

/// Validates and quotes a Caddyfile token, defending against control-character and `{...}` placeholder injection.
pub fn caddy_token(value: &str) -> Result<String, String> {
    if value.chars().any(|c| c.is_control()) {
        return Err("Unsafe Caddyfile token: contains control characters".into());
    }
    if value.contains("{$") || value.contains('{') || value.contains('}') {
        return Err("Unsafe Caddyfile token: placeholder braces are not allowed".into());
    }
    serde_json::to_string(value).map_err(|e| format!("Failed to quote Caddyfile token: {e}"))
}

/// Generates a Caddyfile for a NaiveProxy inbound (`Protocol::Naiveproxy`), configuring
/// `forward_proxy` and structured JSON access logging to `access_log_path`.
pub fn generate_caddyfile(
    inbound: &InboundConfig,
    access_log_path: &Path,
    admin_socket: &Path,
) -> Result<String, String> {
    if inbound.protocol != Protocol::Naiveproxy {
        return Err(format!(
            "Protocol {} is not supported by NaiveProxy",
            inbound.protocol
        ));
    }
    validate_inbound(inbound)?;

    let s = &inbound.settings;
    let users = extract_protocol_users(&inbound.users, s, true)?;
    if users.is_empty() {
        return Err("Naive inbound has no authorized users".into());
    }

    let domain = {
        let d = str_or_empty(s, "domain");
        let d = if d.is_empty() {
            str_or_empty(s, "sni")
        } else {
            d
        };
        let d = if d.is_empty() {
            str_or_empty(&inbound.stream, "sni")
        } else {
            d
        };
        d
    };
    let email = {
        let e = str_or_empty(s, "email");
        if e.is_empty() {
            format!("admin@{domain}")
        } else {
            e.to_string()
        }
    };
    let listen = if inbound.listen.trim().is_empty() {
        "0.0.0.0"
    } else {
        inbound.listen.trim()
    };

    let hide_ip = s.get("hideIp").and_then(|v| v.as_bool()).unwrap_or(true);
    let hide_via = s.get("hideVia").and_then(|v| v.as_bool()).unwrap_or(true);
    let probe_resistance = s
        .get("probeResistance")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let fallback_root_buf = crate::cores::validate_fallback_root(&str_or_empty(s, "fallbackRoot"))?;
    let fallback_root = fallback_root_buf.to_string_lossy();
    let warp_upstream = str_or_empty(s, "warpUpstream");
    let tls_mode = {
        let m = str_or_empty(s, "tlsMode");
        if m.is_empty() {
            "letsencrypt"
        } else {
            m
        }
    };
    let cert_file = {
        let c = str_or_empty(s, "certFile");
        if c.is_empty() {
            str_or_empty(&inbound.stream, "certFile")
        } else {
            c
        }
    };
    let key_file = {
        let k = str_or_empty(s, "keyFile");
        if k.is_empty() {
            str_or_empty(&inbound.stream, "keyFile")
        } else {
            k
        }
    };

    let use_custom_tls = tls_mode.eq_ignore_ascii_case("custom")
        && !cert_file.is_empty()
        && !key_file.is_empty();
    if domain.is_empty() && !use_custom_tls {
        return Err(
            "Naive inbound needs a domain (settings.domain or sni) to obtain a certificate".into(),
        );
    }
    let domain_with_port = if domain.is_empty() {
        String::new()
    } else if domain.contains(':') {
        domain.to_string()
    } else {
        format!("{domain}:{}", inbound.port)
    };
    let site_address = if domain_with_port.is_empty() {
        format!(":{}", inbound.port)
    } else {
        format!(":{}, {}", inbound.port, caddy_token(&domain_with_port)?)
    };
    let q_listen = caddy_token(listen)?;
    let q_log_path = caddy_token(&access_log_path.to_string_lossy())?;
    let q_fallback_root = caddy_token(&fallback_root)?;

    let tls_line = if use_custom_tls {
        format!(
            "  tls {} {}",
            caddy_token(cert_file)?,
            caddy_token(key_file)?
        )
    } else {
        format!("  tls {}", caddy_token(&email)?)
    };

    let proto = str_or_empty(&inbound.settings, "proto").to_ascii_lowercase();
    let enable_h3 = matches!(proto.as_str(), "quic" | "both" | "h3");

    let mut out = String::new();
    out.push_str("{\n");
    // Admin API on a unix socket (root-only file) lets the agent `caddy reload` on user changes
    // without dropping live tunnels. Fall back to `admin off` if the path is too long.
    let sock = admin_socket.to_string_lossy();
    if sock.len() <= MAX_SOCKET_PATH {
        out.push_str(&format!("  admin {}\n", caddy_token(&format!("unix/{sock}"))?));
    } else {
        out.push_str("  admin off\n");
    }
    if use_custom_tls {
        out.push_str("  auto_https off\n");
    }
    out.push_str("  order forward_proxy before file_server\n");
    out.push_str("  servers {\n");
    if enable_h3 {
        out.push_str("    protocols h1 h2 h3\n");
    } else {
        out.push_str("    protocols h1 h2\n");
    }
    out.push_str("    timeouts {\n");
    out.push_str("      read_body   0s\n");
    out.push_str("      read_header 30s\n");
    out.push_str("      write       0s\n");
    out.push_str("      idle        5m\n");
    out.push_str("    }\n");
    out.push_str("  }\n");
    out.push_str("}\n\n");

    out.push_str(&format!("{site_address} {{\n"));
    out.push_str(&format!("  bind {q_listen}\n"));
    out.push_str(&format!("{tls_line}\n\n"));
    if !enable_h3 {
        out.push_str("  header -Alt-Svc\n\n");
    }

    out.push_str("  log {\n");
    out.push_str(&format!("    output file {q_log_path} {{\n"));
    out.push_str("      roll_size 50mb\n");
    out.push_str("      roll_keep 3\n");
    // Keep rotated files uncompressed so the agent can still read the unread tail by inode.
    out.push_str("      roll_compression none\n");
    out.push_str("    }\n");
    // Request headers (Proxy-Authorization, cookies, ...) never belong in the log; the
    // traffic reader only needs `user_id`, `bytes_read` and `size`.
    out.push_str("    format filter {\n");
    out.push_str("      wrap json\n");
    out.push_str("      fields {\n");
    out.push_str("        request>headers delete\n");
    out.push_str("        request>tls delete\n");
    out.push_str("      }\n");
    out.push_str("    }\n");
    out.push_str("  }\n\n");

    out.push_str("  forward_proxy {\n");
    for u in &users {
        let q_user = caddy_token(&u.name)?;
        let q_pass = caddy_token(&u.password)?;
        out.push_str(&format!("    basic_auth {q_user} {q_pass}\n"));
    }
    if hide_ip {
        out.push_str("    hide_ip\n");
    }
    if hide_via {
        out.push_str("    hide_via\n");
    }
    if probe_resistance {
        out.push_str("    probe_resistance\n");
    }
    out.push_str("    acl {\n");
    out.push_str("      deny 127.0.0.0/8 ::1/128 0.0.0.0/8 ::/128 ::ffff:0:0/96 10.0.0.0/8 172.16.0.0/12 192.168.0.0/16 169.254.0.0/16 100.64.0.0/10 fc00::/7 fe80::/10 localhost\n");
    out.push_str("      allow all\n");
    out.push_str("    }\n");
    if !warp_upstream.is_empty() {
        out.push_str(&format!("    upstream {}\n", caddy_token(warp_upstream)?));
    }
    out.push_str("  }\n\n");

    out.push_str(&format!("  root * {q_fallback_root}\n"));
    out.push_str("  file_server\n");
    out.push_str("}\n");

    Ok(out)
}

/// Ensures `fallback_root` (confined to `/var/www/*`) exists and contains a default `index.html`.
pub async fn ensure_fallback_index(fallback_root: &Path) -> Result<(), String> {
    let safe_root = crate::cores::validate_fallback_root(&fallback_root.to_string_lossy())?;
    tokio::fs::create_dir_all(&safe_root)
        .await
        .map_err(|e| format!("Failed to create fallback root {}: {e}", safe_root.display()))?;
    let index_path = safe_root.join("index.html");
    if tokio::fs::metadata(&index_path).await.is_err() {
        tokio::fs::write(&index_path, DEFAULT_INDEX_HTML)
            .await
            .map_err(|e| format!("Failed to write {}: {e}", index_path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn naive_ib(settings: serde_json::Value) -> InboundConfig {
        serde_json::from_value(json!({
            "id": "n1", "tag": "n1", "protocol": "NAIVEPROXY", "port": 8443,
            "settings": settings,
            "users": [{"id": "u1", "username": "alice", "uuid": "00000000-0000-0000-0000-000000000001", "password": "pw"}]
        }))
        .unwrap()
    }

    #[test]
    fn caddyfile_uses_admin_socket_and_header_free_log() {
        let ib = naive_ib(json!({"domain": "n.example.org"}));
        let cf = generate_caddyfile(
            &ib,
            Path::new("/var/lib/x/naive-access.log"),
            Path::new("/var/lib/x/naive-n1.admin.sock"),
        )
        .unwrap();
        assert!(cf.contains("admin \"unix//var/lib/x/naive-n1.admin.sock\""));
        assert!(cf.contains("request>headers delete"));
        assert!(cf.contains(":8443, \"n.example.org:8443\""));
        assert!(!cf.contains("admin off"));
        assert!(cf.contains("0.0.0.0/8"));
        assert!(cf.contains("::ffff:0:0/96"));
        assert!(cf.contains("100.64.0.0/10"));
    }

    #[test]
    fn overlong_socket_path_falls_back_to_admin_off() {
        let ib = naive_ib(json!({"domain": "n.example.org"}));
        let long = format!("/{}/a.sock", "d".repeat(120));
        let cf = generate_caddyfile(&ib, Path::new("/l"), Path::new(&long)).unwrap();
        assert!(cf.contains("admin off"));
    }

    #[test]
    fn empty_domain_is_rejected_for_acme_but_allowed_with_custom_tls() {
        let acme = naive_ib(json!({}));
        assert!(generate_caddyfile(&acme, Path::new("/l"), Path::new("/s")).is_err());
        let custom = naive_ib(json!({"tlsMode": "custom", "certFile": "/c.pem", "keyFile": "/k.pem"}));
        let cf = generate_caddyfile(&custom, Path::new("/l"), Path::new("/s")).unwrap();
        assert!(cf.contains("\n:8443 {"));
        assert!(!cf.contains("example.com"));
    }
}
