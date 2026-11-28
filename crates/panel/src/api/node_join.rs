//! One-command node enrollment.
//!
//! An admin creates a short-lived, single-use join token (`POST /nodes/join-tokens`). The node
//! installer presents it once (`POST /nodes/self/join`); the panel then creates the node, generates
//! its secret server-side and returns it exactly once. Only the SHA-256 of the token is stored.

use crate::api::common::AppJson;
use crate::audit::{self, AuditEntry};
use crate::auth::crypto::{random_hex, sha256_hex};
use crate::auth::extract::AuthUser;
use crate::error::{AppError, AppResult};
use crate::net::{user_agent, ClientIp};
use crate::state::AppState;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use chrono::Duration as ChronoDuration;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

const TOKEN_TTL_MINUTES: i64 = 60;
const MAX_NAME_LEN: usize = 100;

fn token_hash(token: &str) -> String {
    sha256_hex(&format!("node-join:{token}"))
}

fn clean_name(raw: Option<&str>) -> AppResult<Option<String>> {
    let Some(n) = raw.map(str::trim).filter(|n| !n.is_empty()) else {
        return Ok(None);
    };
    if n.len() > MAX_NAME_LEN || n.chars().any(char::is_control) {
        return Err(AppError::validation("name", "Invalid node name"));
    }
    Ok(Some(n.to_string()))
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CreateJoinTokenReq {
    #[serde(default)]
    name: Option<String>,
}

/// `POST /api/v1/nodes/join-tokens` (admin): mints a one-time token and the install command.
pub async fn create_join_token(
    State(state): State<AppState>,
    user: AuthUser,
    ip: ClientIp,
    headers: HeaderMap,
    crate::api::common::OptionalJson(body): crate::api::common::OptionalJson<CreateJoinTokenReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    let name = clean_name(body.name.as_deref())?;
    let token = random_hex(24);
    let now = crate::db::now();
    let expires = now + ChronoDuration::minutes(TOKEN_TTL_MINUTES);

    sqlx::query(
        r#"INSERT INTO "NodeJoinToken" (id, "tokenHash", name, "createdBy", "createdAt", "expiresAt")
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(crate::db::new_id())
    .bind(token_hash(&token))
    .bind(&name)
    .bind(&user.id)
    .bind(now)
    .bind(expires)
    .execute(&state.db)
    .await?;

    let (panel_url, _) = crate::api::nodes::resolve_panel_urls(&state, &headers);
    let command = format!("curl -fsSL '{panel_url}/install-node.sh' | sudo bash -s -- --join '{token}'");

    audit::spawn(
        &state.db,
        AuditEntry::new("CREATE", "node_join_token")
            .user(&user.id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    Ok(Json(json!({
        "token": token,
        "expiresAt": crate::db::ts::format_iso(&expires),
        "ttlMinutes": TOKEN_TTL_MINUTES,
        "panelUrl": panel_url,
        "command": command,
    })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinReq {
    token: String,
    #[serde(default)]
    host: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    api_port: Option<i32>,
    #[serde(default)]
    arch: Option<String>,
}

/// `POST /api/v1/nodes/self/join` (public, token-gated): consumes the token and creates the node.
pub async fn join(
    State(state): State<AppState>,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<JoinReq>,
) -> AppResult<Json<Value>> {
    if !state.join_limiter.check(&ip).allowed {
        return Err(AppError::too_many_requests("Too many join attempts"));
    }
    let token = body.token.trim();
    if token.len() < 32 || token.len() > 128 || !token.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::unauthorized("Invalid or expired join token"));
    }

    let host = body.host.as_deref().map(str::trim).unwrap_or("");
    if host.is_empty() {
        return Err(AppError::validation("host", "Node host is required"));
    }
    // Always reject private/loopback/link-local targets here, whatever ALLOW_PRIVATE_NODES says:
    // a join token must never be able to point the panel at an internal address.
    crate::net::validate_node_host_async(host, false).await?;
    let api_port = body.api_port.unwrap_or(2222);
    if !(1..=65535).contains(&api_port) {
        return Err(AppError::validation("apiPort", "Port must be 1..=65535"));
    }
    let arch = match body.arch.as_deref().map(str::trim) {
        Some("arm64") | Some("aarch64") => "arm64",
        _ => "amd64",
    };

    let now = crate::db::now();
    let mut tx = state.db.begin().await?;
    // Single statement: the token is valid exactly once, even under concurrent requests.
    let claimed = sqlx::query(
        r#"UPDATE "NodeJoinToken" SET "usedAt" = $2
           WHERE "tokenHash" = $1 AND "usedAt" IS NULL AND "expiresAt" > $2
           RETURNING id, name"#,
    )
    .bind(token_hash(token))
    .bind(now)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::unauthorized("Invalid or expired join token"))?;

    let token_id: String = claimed.try_get("id")?;
    let preset: Option<String> = claimed.try_get("name")?;
    let name = clean_name(body.name.as_deref())?
        .or(preset)
        .unwrap_or_else(|| host.to_string());

    let node_id = crate::db::new_id();
    let secret = random_hex(32);
    sqlx::query(
        r#"INSERT INTO "Node" (id, name, host, port, "apiPort", secret, "secretHash", arch, tags, active, "createdAt", "updatedAt")
           VALUES ($1, $2, $3, 443, $4, $5, $6, $7, '{}', true, $8, $8)"#,
    )
    .bind(&node_id)
    .bind(&name)
    .bind(host)
    .bind(api_port)
    .bind(&secret)
    .bind(sha256_hex(&secret))
    .bind(arch)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(r#"UPDATE "NodeJoinToken" SET "nodeId" = $2 WHERE id = $1"#)
        .bind(&token_id)
        .bind(&node_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    state.invalidate_derived();
    audit::spawn(
        &state.db,
        AuditEntry::new("JOIN", "node")
            .resource_id(&node_id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    Ok(Json(json!({ "nodeId": node_id, "name": name, "secret": secret })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_hash_is_stable_and_domain_separated() {
        assert_eq!(token_hash("abc"), token_hash("abc"));
        assert_ne!(token_hash("abc"), sha256_hex("abc"));
    }

    #[test]
    fn names_are_validated() {
        assert_eq!(clean_name(Some("  EU-1 ")).unwrap().as_deref(), Some("EU-1"));
        assert!(clean_name(Some("   ")).unwrap().is_none());
        assert!(clean_name(Some("bad\nname")).is_err());
        assert!(clean_name(Some(&"x".repeat(101))).is_err());
    }
}
