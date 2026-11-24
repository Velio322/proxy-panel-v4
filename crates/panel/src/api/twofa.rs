use crate::api::auth::{open_totp_secret, seal_totp_secret, verify_totp_code};
use crate::api::common::{ok, AppJson};
use crate::auth::extract::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use rand::RngCore;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use totp_rs::{Algorithm, Secret, TOTP};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(status))
        .route("/setup", post(setup))
        .route("/enable", post(enable))
        .route("/disable", post(disable))
}

#[derive(Debug, Deserialize)]
struct CodeReq {
    code: String,
}

async fn status(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Value>> {
    let enabled: bool = sqlx::query_scalar(r#"SELECT "totpEnabled" FROM "User" WHERE id = $1"#)
        .bind(&user.id)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(json!({ "enabled": enabled })))
}

async fn setup(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Value>> {
    let mut raw = [0u8; 20];
    rand::rngs::OsRng.fill_bytes(&mut raw);
    let secret = Secret::Raw(raw.to_vec());
    let b32 = secret.to_encoded().to_string();

    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        raw.to_vec(),
        Some("ProxyPanel".to_string()),
        user.username.clone(),
    )
    .map_err(|e| AppError::internal(format!("TOTP init failed: {e}")))?;

    let otpauth_url = format!(
        "otpauth://totp/ProxyPanel:{}?secret={}&issuer=ProxyPanel",
        url::form_urlencoded::byte_serialize(user.username.as_bytes()).collect::<String>(),
        b32
    );
    let _ = totp;

    sqlx::query(r#"UPDATE "User" SET "totpSecret" = $2, "updatedAt" = $3 WHERE id = $1"#)
        .bind(&user.id)
        .bind(seal_totp_secret(&state, &b32)?)
        .bind(crate::db::now())
        .execute(&state.db)
        .await?;

    Ok(Json(json!({
        "secret": b32,
        "otpauthUrl": otpauth_url
    })))
}

async fn enable(
    State(state): State<AppState>,
    user: AuthUser,
    AppJson(body): AppJson<CodeReq>,
) -> AppResult<Json<Value>> {
    let sec: Option<String> =
        sqlx::query_scalar(r#"SELECT "totpSecret" FROM "User" WHERE id = $1"#)
            .bind(&user.id)
            .fetch_one(&state.db)
            .await?;
    let secret = sec
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::bad_request("Call /2fa/setup first"))?;
    if !verify_totp_code(&open_totp_secret(&state, &secret)?, &body.code) {
        return Err(AppError::bad_request("Invalid TOTP code"));
    }

    sqlx::query(r#"UPDATE "User" SET "totpEnabled" = true, "updatedAt" = $2 WHERE id = $1"#)
        .bind(&user.id)
        .bind(crate::db::now())
        .execute(&state.db)
        .await?;
    Ok(ok())
}

async fn disable(
    State(state): State<AppState>,
    user: AuthUser,
    AppJson(body): AppJson<CodeReq>,
) -> AppResult<Json<Value>> {
    let row = sqlx::query(r#"SELECT "totpEnabled", "totpSecret" FROM "User" WHERE id = $1"#)
        .bind(&user.id)
        .fetch_one(&state.db)
        .await?;
    let enabled: bool = row.try_get("totpEnabled")?;
    let sec: Option<String> = row.try_get("totpSecret")?;
    if enabled {
        if let Some(s) = sec.as_deref().filter(|s| !s.is_empty()) {
            if !verify_totp_code(&open_totp_secret(&state, s)?, &body.code) {
                return Err(AppError::bad_request("Invalid TOTP code"));
            }
        }
    }

    sqlx::query(
        r#"UPDATE "User" SET "totpEnabled" = false, "totpSecret" = NULL, "updatedAt" = $2 WHERE id = $1"#,
    )
    .bind(&user.id)
    .bind(crate::db::now())
    .execute(&state.db)
    .await?;
    Ok(ok())
}
