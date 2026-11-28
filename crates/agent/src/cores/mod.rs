pub mod mieru;
pub mod naive;
pub mod singbox;
pub mod tgweb;
pub mod xray;
pub mod xray_grpc;

use mypanel_proto::{InboundConfig, NodeUser, Protocol};
use serde_json::{Map, Value};
use std::path::Path;
use tokio::io::AsyncWriteExt;

/// Extracted per-user credential entry used by proxy core generators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreUser {
    pub id: String,
    pub name: String,
    pub email: String,
    pub username: String,
    pub password: String,
    pub flow: String,
    pub level: u32,
}

/// Validates that an identifier (`inbound.id`, `tag`, etc.) is safe for filenames and tags (`^[A-Za-z0-9_.-]{1,64}$`).
pub fn safe_id(id: &str) -> Result<&str, String> {
    let trimmed = id.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err(format!("Invalid identifier length: {id}"));
    }
    if trimmed == "." || trimmed == ".." {
        return Err(format!("Unsafe relative identifier: {id}"));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(format!("Unsafe characters in identifier: {id}"));
    }
    Ok(trimmed)
}

/// Validates that a tag contains no path separators or control characters.
pub fn safe_tag(tag: &str) -> Result<&str, String> {
    let trimmed = tag.trim();
    if trimmed.is_empty() || trimmed.len() > 128 {
        return Err("Tag must be between 1 and 128 characters".into());
    }
    if trimmed
        .chars()
        .any(|c| c == '/' || c == '\\' || c.is_control())
    {
        return Err(format!("Invalid characters in tag: {tag}"));
    }
    Ok(trimmed)
}

/// Validates an SNI / hostname pattern (`^[A-Za-z0-9*.-]{1,253}$`).
pub fn safe_sni(sni: &str) -> Result<&str, String> {
    let trimmed = sni.trim();
    if trimmed.is_empty() || trimmed.len() > 253 {
        return Err(format!("Invalid SNI length: {sni}"));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '*' | '.' | '-'))
    {
        return Err(format!("Unsafe characters in SNI: {sni}"));
    }
    Ok(trimmed)
}

/// Validates an InboundConfig against core protocol/transport invariants (`03-worker-and-cores.md §4.2`).
pub fn validate_inbound(inbound: &InboundConfig) -> Result<(), String> {
    safe_tag(&inbound.tag)?;
    if inbound.port == 0 {
        return Err("Inbound port must be between 1 and 65535".into());
    }
    if !inbound.settings.is_object() {
        return Err("Inbound settings must be a JSON object".into());
    }
    if !inbound.stream.is_object() && !inbound.stream.is_null() {
        return Err("Inbound stream must be a JSON object".into());
    }

    let network = inbound
        .stream
        .get("network")
        .and_then(Value::as_str)
        .unwrap_or("tcp")
        .to_ascii_lowercase();
    let valid_net = matches!(
        network.as_str(),
        "tcp" | "raw" | "ws" | "grpc" | "httpupgrade" | "xhttp" | "h2" | "http" | "kcp" | "quic"
    );
    if !valid_net {
        return Err(format!("Unsupported stream network: {network}"));
    }

    let security = inbound
        .stream
        .get("security")
        .and_then(Value::as_str)
        .unwrap_or("none")
        .to_ascii_lowercase();
    if !matches!(security.as_str(), "none" | "tls" | "reality" | "") {
        return Err(format!("Unsupported stream security: {security}"));
    }

    let flow = inbound
        .settings
        .get("flow")
        .and_then(Value::as_str)
        .or_else(|| inbound.stream.get("flow").and_then(Value::as_str))
        .unwrap_or("")
        .trim();

    if inbound.protocol == Protocol::Vless && !flow.is_empty() {
        let valid_flow = matches!(flow, "xtls-rprx-vision" | "xtls-rprx-vision-udp443");
        let valid_flow_net = matches!(network.as_str(), "tcp" | "raw");
        let valid_flow_sec = matches!(security.as_str(), "tls" | "reality");
        if !valid_flow || !valid_flow_net || !valid_flow_sec {
            return Err(format!(
                "VLESS flow '{flow}' requires tcp/raw network with tls or reality security"
            ));
        }
    }

    if security == "reality" && !matches!(network.as_str(), "tcp" | "raw" | "grpc" | "xhttp" | "h2")
    {
        return Err(format!(
            "Reality security is not supported on network '{network}'"
        ));
    }

    if let Some(obfs) = inbound.settings.get("obfs").and_then(Value::as_object) {
        if obfs.get("type").and_then(Value::as_str) == Some("salamander") {
            let pw = obfs
                .get("password")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            if pw.is_empty() {
                return Err("Salamander obfuscation requires a non-empty password".into());
            }
        }
    }

    if inbound.protocol == Protocol::Mieru {
        if let Some(t) = inbound
            .settings
            .get("transport")
            .or_else(|| inbound.settings.get("protocol"))
            .and_then(Value::as_str)
        {
            let lower = t.trim().to_ascii_lowercase();
            if !matches!(lower.as_str(), "tcp" | "udp" | "both" | "tcp,udp") {
                return Err(format!("Invalid Mieru transport: {t}"));
            }
        }
    }

    Ok(())
}

/// Returns `true` if `pw` looks like a bcrypt password hash (`$2a$`, `$2b$`, `$2y$`).
pub fn is_bcrypt_hash(pw: &str) -> bool {
    let b = pw.as_bytes();
    b.len() >= 4 && b[0] == b'$' && b[1] == b'2' && matches!(b[2], b'a' | b'b' | b'y') && b[3] == b'$'
}

/// Extracts normalized user credentials from `InboundConfig.users` or `settings.{users,clients}`.
pub fn extract_protocol_users(
    inbound_users: &[NodeUser],
    settings: &Value,
    require_password: bool,
) -> Result<Vec<CoreUser>, String> {
    if !inbound_users.is_empty() {
        let mut out = Vec::with_capacity(inbound_users.len());
        for u in inbound_users {
            let pw = u
                .password
                .as_deref()
                .or(u.auth.as_deref())
                .unwrap_or("")
                .trim()
                .to_string();
            if require_password {
                if pw.is_empty() || is_bcrypt_hash(&pw) {
                    return Err(
                        "A persisted plaintext protocol credential is required (not an account password hash)"
                            .into(),
                    );
                }
            }
            let id = if !u.uuid.trim().is_empty() {
                u.uuid.trim().to_string()
            } else {
                u.id.trim().to_string()
            };
            let email = if !u.email.trim().is_empty() {
                u.email.trim().to_string()
            } else if !u.name.trim().is_empty() {
                u.name.trim().to_string()
            } else if !u.id.trim().is_empty() {
                u.id.trim().to_string()
            } else {
                u.username.trim().to_string()
            };
            let name = if !email.is_empty() {
                email.clone()
            } else if !u.username.trim().is_empty() {
                u.username.trim().to_string()
            } else {
                "user".to_string()
            };
            out.push(CoreUser {
                id,
                name,
                email,
                username: u.username.trim().to_string(),
                password: pw,
                flow: u.flow.clone().unwrap_or_default(),
                level: u.level,
            });
        }
        return Ok(out);
    }

    let explicit_arr = settings
        .get("users")
        .and_then(Value::as_array)
        .or_else(|| settings.get("clients").and_then(Value::as_array));

    if let Some(arr) = explicit_arr {
        let mut out = Vec::with_capacity(arr.len());
        for item in arr {
            out.push(parse_user_value(item, require_password)?);
        }
        return Ok(out);
    }

    let has_inline = settings
        .get("password")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.trim().is_empty())
        || settings
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.trim().is_empty())
        || settings
            .get("uuid")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.trim().is_empty());

    if has_inline {
        Ok(vec![parse_user_value(settings, require_password)?])
    } else {
        Ok(Vec::new())
    }
}

fn parse_user_value(v: &Value, require_password: bool) -> Result<CoreUser, String> {
    let pw = v
        .get("password")
        .or_else(|| v.get("auth"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if require_password {
        if pw.is_empty() || is_bcrypt_hash(&pw) {
            return Err(
                "A persisted plaintext protocol credential is required (not an account password hash)"
                    .into(),
            );
        }
    }
    let id = v
        .get("id")
        .or_else(|| v.get("uuid"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let email = v
        .get("email")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            v.get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
        })
        .unwrap_or("")
        .to_string();
    let username = v
        .get("username")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("")
        .to_string();
    let name = if !email.is_empty() {
        email.clone()
    } else if !username.is_empty() {
        username.clone()
    } else {
        "user".to_string()
    };
    let flow = v
        .get("flow")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let level = v
        .get("level")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(0);

    Ok(CoreUser {
        id,
        name,
        email,
        username,
        password: pw,
        flow,
        level,
    })
}

/// Atomically writes `contents` to `path` with mode `0600` (and parent directory `0700` on Unix).
pub async fn write_atomic_0600(path: &Path, contents: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create directory {}: {e}", parent.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = tokio::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).await;
        }
    }

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("config.tmp");
    let tmp_path = path.with_file_name(format!(".{file_name}.{}.tmp", uuid::Uuid::new_v4()));

    let mut opts = tokio::fs::OpenOptions::new();
    opts.create_new(true).write(true);
    #[cfg(unix)]
    {
        opts.mode(0o600);
    }
    let mut file = opts
        .open(&tmp_path)
        .await
        .map_err(|e| format!("Failed to open temp file {}: {e}", tmp_path.display()))?;

    file.write_all(contents)
        .await
        .map_err(|e| format!("Failed to write temp file {}: {e}", tmp_path.display()))?;
    file.sync_all()
        .await
        .map_err(|e| format!("Failed to sync temp file {}: {e}", tmp_path.display()))?;
    drop(file);

    tokio::fs::rename(&tmp_path, path)
        .await
        .map_err(|e| format!("Failed to rename {} to {}: {e}", tmp_path.display(), path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).await;
    }

    Ok(())
}

/// Confines decoy/fallback web roots under `/var/www` without `..` traversal or symlink escapes.
pub fn validate_fallback_root(raw: &str) -> Result<std::path::PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(std::path::PathBuf::from("/var/www/html"));
    }
    let p = std::path::Path::new(trimmed);
    if !p.is_absolute() {
        return Err("settings.fallbackRoot must be an absolute path under /var/www".into());
    }
    for comp in p.components() {
        if matches!(comp, std::path::Component::ParentDir) {
            return Err("settings.fallbackRoot must not contain '..'".into());
        }
    }
    let web_base = std::path::Path::new("/var/www");
    if !p.starts_with(web_base) || p == web_base {
        return Err("settings.fallbackRoot must point to a subdirectory under /var/www".into());
    }
    if p.exists() {
        let canon = std::fs::canonicalize(p)
            .map_err(|e| format!("Invalid fallbackRoot {}: {e}", p.display()))?;
        let canon_base = std::fs::canonicalize(web_base).unwrap_or_else(|_| web_base.to_path_buf());
        if !canon.starts_with(&canon_base) {
            return Err("settings.fallbackRoot symlink escapes /var/www".into());
        }
    }
    Ok(p.to_path_buf())
}

/// Helper to read a string property or return `""`.
pub(crate) fn str_or_empty<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
}

/// Helper to convert a Value into a cloned JSON object Map or empty Map.
pub(crate) fn as_obj_or_empty(v: &Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}
