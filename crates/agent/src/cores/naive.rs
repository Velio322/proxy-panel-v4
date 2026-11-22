use crate::cores::{extract_protocol_users, safe_id, str_or_empty, validate_inbound};
use mypanel_proto::{InboundConfig, Protocol};
use std::path::{Path, PathBuf};

/// Default fallback HTML served by Caddy's `file_server` for unauthenticated HTTP requests.
const DEFAULT_INDEX_HTML: &str =
    "<!DOCTYPE html><html><head><title>Welcome</title></head><body><h1>Welcome</h1></body></html>\n";

/// Returns `<CONFIG_DIR>/naive-<safe_id>.Caddyfile`.
pub fn caddyfile_path(config_dir: &Path, inbound_id: &str) -> Result<PathBuf, String> {
    let id = safe_id(inbound_id)?;
    Ok(config_dir.join(format!("naive-{id}.Caddyfile")))
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
        if d.is_empty() {
            "example.com"
        } else {
            d
        }
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
    let fallback_root = {
        let r = str_or_empty(s, "fallbackRoot");
        if r.is_empty() {
            "/var/www/html"
        } else {
            r
        }
    };
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
    let domain_with_port = if domain.contains(':') {
        domain.to_string()
    } else {
        format!("{domain}:{}", inbound.port)
    };
    let q_domain = caddy_token(&domain_with_port)?;
    let q_listen = caddy_token(listen)?;
    let q_log_path = caddy_token(&access_log_path.to_string_lossy())?;
    let q_fallback_root = caddy_token(fallback_root)?;

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
    out.push_str("  admin off\n");
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

    out.push_str(&format!(":{}, {} {{\n", inbound.port, q_domain));
    out.push_str(&format!("  bind {q_listen}\n"));
    out.push_str(&format!("{tls_line}\n\n"));
    if !enable_h3 {
        out.push_str("  header -Alt-Svc\n\n");
    }

    out.push_str("  log {\n");
    out.push_str(&format!("    output file {q_log_path} {{\n"));
    out.push_str("      roll_size 50mb\n");
    out.push_str("      roll_keep 3\n");
    out.push_str("    }\n");
    out.push_str("    format json\n");
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
    out.push_str("      deny 127.0.0.0/8 ::1/128 10.0.0.0/8 172.16.0.0/12 192.168.0.0/16 169.254.0.0/16 localhost\n");
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

/// Ensures `fallback_root` exists and contains a default `index.html` if none is present.
pub async fn ensure_fallback_index(fallback_root: &Path) -> Result<(), String> {
    tokio::fs::create_dir_all(fallback_root)
        .await
        .map_err(|e| format!("Failed to create fallback root {}: {e}", fallback_root.display()))?;
    let index_path = fallback_root.join("index.html");
    if tokio::fs::metadata(&index_path).await.is_err() {
        tokio::fs::write(&index_path, DEFAULT_INDEX_HTML)
            .await
            .map_err(|e| format!("Failed to write {}: {e}", index_path.display()))?;
    }
    Ok(())
}
