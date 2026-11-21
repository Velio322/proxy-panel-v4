use crate::auth::crypto::{
    decrypt_str, encrypt_str, random_base64_16, random_base64_32, random_protocol_secret,
    AAD_PROTOCOL_CREDENTIAL,
};
use crate::db::enums::{DbProtocol, Protocol};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use sqlx::Row;
use std::collections::HashMap;

/// Retrieves or lazily provisions an encrypted per-client protocol credential.
///
/// For Shadowsocks-2022 (`2022-blake3-*`), generates 16-byte (AES-128) or 32-byte (AES-256 / ChaCha20)
/// standard base64 keys when `ss_method` is provided.
pub async fn get_protocol_secret(
    state: &AppState,
    client_id: &str,
    protocol: Protocol,
    ss_method: Option<&str>,
) -> AppResult<String> {
    let enc_key = state.encryption_key().ok_or_else(|| {
        AppError::bad_request("ENCRYPTION_KEY must be configured for protocol credentials")
    })?;

    if let Some(row) = sqlx::query(
        r#"SELECT "encryptedSecret" FROM "ProtocolCredential"
           WHERE "clientId" = $1 AND protocol = $2"#,
    )
    .bind(client_id)
    .bind(DbProtocol::from(protocol))
    .fetch_optional(&state.db)
    .await?
    {
        let enc: String = row.try_get("encryptedSecret")?;
        if let Ok(plain) = decrypt_str(Some(enc_key), &enc, AAD_PROTOCOL_CREDENTIAL) {
            return Ok(normalize_ss_secret(&plain, protocol, ss_method));
        }
    }

    let raw_secret = generate_secret_for(protocol, ss_method);
    let encrypted = encrypt_str(Some(enc_key), &raw_secret, AAD_PROTOCOL_CREDENTIAL)?;
    let id = crate::db::new_id();
    let now = crate::db::now();

    let row = sqlx::query(
        r#"INSERT INTO "ProtocolCredential" (id, "clientId", protocol, "encryptedSecret", "createdAt", "updatedAt")
           VALUES ($1, $2, $3, $4, $5, $5)
           ON CONFLICT ("clientId", protocol) DO UPDATE SET "updatedAt" = EXCLUDED."updatedAt"
           RETURNING "encryptedSecret""#,
    )
    .bind(id)
    .bind(client_id)
    .bind(DbProtocol::from(protocol))
    .bind(&encrypted)
    .bind(now)
    .fetch_one(&state.db)
    .await?;

    let winning_enc: String = row.try_get("encryptedSecret")?;
    let plain = decrypt_str(Some(enc_key), &winning_enc, AAD_PROTOCOL_CREDENTIAL)?;
    Ok(normalize_ss_secret(&plain, protocol, ss_method))
}

/// Bulk-loads decrypted protocol credentials for a set of client IDs.
pub async fn get_protocol_secrets_bulk(
    state: &AppState,
    client_ids: &[String],
) -> AppResult<HashMap<(String, Protocol), String>> {
    let mut map = HashMap::new();
    if client_ids.is_empty() {
        return Ok(map);
    }
    let Some(enc_key) = state.encryption_key() else {
        return Ok(map);
    };

    let rows = sqlx::query(
        r#"SELECT "clientId", protocol, "encryptedSecret"
           FROM "ProtocolCredential"
           WHERE "clientId" = ANY($1::text[])"#,
    )
    .bind(client_ids)
    .fetch_all(&state.db)
    .await?;

    for row in rows {
        let client_id: String = row.try_get("clientId")?;
        let protocol: DbProtocol = row.try_get("protocol")?;
        let enc: String = row.try_get("encryptedSecret")?;
        if let Ok(plain) = decrypt_str(Some(enc_key), &enc, AAD_PROTOCOL_CREDENTIAL) {
            map.insert((client_id, protocol.into()), plain);
        }
    }
    Ok(map)
}

pub fn generate_secret_for(protocol: Protocol, ss_method: Option<&str>) -> String {
    if protocol == Protocol::Shadowsocks {
        if let Some(m) = ss_method {
            let lower = m.to_ascii_lowercase();
            if lower.contains("2022") {
                if lower.contains("128") {
                    return random_base64_16();
                }
                return random_base64_32();
            }
        }
    }
    random_protocol_secret()
}

/// Ensures a stored Shadowsocks-2022 secret has valid 16B or 32B standard base64 length.
pub fn normalize_ss_secret(secret: &str, protocol: Protocol, ss_method: Option<&str>) -> String {
    if protocol != Protocol::Shadowsocks {
        return secret.to_string();
    }
    let Some(method) = ss_method else {
        return secret.to_string();
    };
    let lower = method.to_ascii_lowercase();
    if !lower.contains("2022") {
        return secret.to_string();
    }
    let target_len = if lower.contains("128") { 16 } else { 32 };
    if let Ok(decoded) = STANDARD.decode(secret.trim()) {
        if decoded.len() == target_len {
            return secret.trim().to_string();
        }
        if decoded.len() >= target_len {
            return STANDARD.encode(&decoded[..target_len]);
        }
    }
    // Derive deterministic 16B/32B standard base64 key from existing secret so it remains stable across restarts
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(secret.as_bytes());
    STANDARD.encode(&digest[..target_len])
}

/// Ensures multi-user SS-2022 inbound settings have a valid server `iPSK`.
pub fn ensure_ss2022_ipsk(settings: &serde_json::Value) -> Option<String> {
    let method = settings
        .get("method")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if !method.to_ascii_lowercase().contains("2022") {
        return None;
    }
    let existing = settings
        .get("password")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(ipsk) = existing {
        return Some(normalize_ss_secret(ipsk, Protocol::Shadowsocks, Some(method)));
    }
    None
}
