use crate::api::common::{ok, AppJson};
use crate::auth::crypto::{ct_eq, sha256_hex};
use crate::auth::extract::AuthUser;
use crate::db::enums::UserRole;
use crate::error::{AppError, AppResult};
use crate::net::{user_agent, ClientIp};
use crate::state::AppState;
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::collections::BTreeMap;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login_telegram))
        .route("/link", post(link_telegram))
        .route("/unlink", post(unlink_telegram))
}

#[derive(Debug, Deserialize)]
struct TgAuthData {
    id: i64,
    first_name: Option<String>,
    last_name: Option<String>,
    username: Option<String>,
    photo_url: Option<String>,
    auth_date: i64,
    hash: String,
}

fn verify_telegram_widget(bot_token: &str, data: &TgAuthData) -> bool {
    let now_ts = chrono::Utc::now().timestamp();
    if (now_ts - data.auth_date).abs() > 86_400 {
        return false;
    }

    let mut fields = BTreeMap::new();
    fields.insert("auth_date", data.auth_date.to_string());
    if let Some(ref f) = data.first_name {
        fields.insert("first_name", f.clone());
    }
    fields.insert("id", data.id.to_string());
    if let Some(ref l) = data.last_name {
        fields.insert("last_name", l.clone());
    }
    if let Some(ref p) = data.photo_url {
        fields.insert("photo_url", p.clone());
    }
    if let Some(ref u) = data.username {
        fields.insert("username", u.clone());
    }

    let check_string = fields
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("\n");

    let secret_key = Sha256::digest(bot_token.as_bytes());
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(&secret_key) else {
        return false;
    };
    mac.update(check_string.as_bytes());
    let computed = hex::encode(mac.finalize().into_bytes());
    ct_eq(&computed, &data.hash.to_ascii_lowercase())
}

async fn login_telegram(
    State(state): State<AppState>,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<TgAuthData>,
) -> AppResult<Response> {
    let token = state
        .cfg
        .telegram_bot_token
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::bad_request("Telegram OAuth is not configured"))?;

    if !verify_telegram_widget(token, &body) {
        return Err(AppError::unauthorized("Invalid Telegram auth signature"));
    }

    let row = sqlx::query(
        r#"SELECT id, username, email, role, "resellerId", language, banned
           FROM "User" WHERE "telegramId" = $1"#,
    )
    .bind(body.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::unauthorized("No panel user linked to this Telegram account"))?;

    if row.try_get::<bool, _>("banned")? {
        return Err(AppError::forbidden("Account is banned"));
    }

    let user_id: String = row.try_get("id")?;
    let username: String = row.try_get("username")?;
    let email: Option<String> = row.try_get("email")?;
    let role: UserRole = row.try_get("role")?;
    let reseller_id: Option<String> = row.try_get("resellerId")?;
    let language: String = row.try_get("language")?;

    let (jwt_tok, expires_at) = state.jwt.sign(&user_id, &username, role, reseller_id.clone())?;
    let now = crate::db::now();
    sqlx::query(
        r#"INSERT INTO "UserSession" (id, "userId", token, ip, "userAgent", "expiresAt", "createdAt")
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
    )
    .bind(crate::db::new_id())
    .bind(&user_id)
    .bind(sha256_hex(&jwt_tok))
    .bind(&*ip)
    .bind(user_agent(&headers))
    .bind(expires_at)
    .bind(now)
    .execute(&state.db)
    .await?;

    let sec_flag = if state.cfg.production { "; Secure" } else { "" };
    let cookie_val = format!(
        "auth_token={jwt_tok}; Path=/; Max-Age={}; HttpOnly; SameSite=Lax{sec_flag}",
        state.jwt.ttl_secs().max(0)
    );
    let mut res = (
        StatusCode::OK,
        Json(json!({
            "token": jwt_tok,
            "user": {
                "id": user_id,
                "username": username,
                "email": email,
                "role": role,
                "resellerId": reseller_id,
                "language": language
            }
        })),
    )
        .into_response();
    if let Ok(cv) = HeaderValue::from_str(&cookie_val) {
        res.headers_mut().insert(header::SET_COOKIE, cv);
    }
    Ok(res)
}

async fn link_telegram(
    State(state): State<AppState>,
    user: AuthUser,
    AppJson(body): AppJson<TgAuthData>,
) -> AppResult<Json<Value>> {
    let token = state
        .cfg
        .telegram_bot_token
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::bad_request("Telegram OAuth is not configured"))?;

    if !verify_telegram_widget(token, &body) {
        return Err(AppError::unauthorized("Invalid Telegram auth signature"));
    }

    sqlx::query(r#"UPDATE "User" SET "telegramId" = $2, "updatedAt" = $3 WHERE id = $1"#)
        .bind(&user.id)
        .bind(body.id)
        .bind(crate::db::now())
        .execute(&state.db)
        .await?;
    Ok(ok())
}

async fn unlink_telegram(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Value>> {
    sqlx::query(r#"UPDATE "User" SET "telegramId" = NULL, "updatedAt" = $2 WHERE id = $1"#)
        .bind(&user.id)
        .bind(crate::db::now())
        .execute(&state.db)
        .await?;
    Ok(ok())
}
