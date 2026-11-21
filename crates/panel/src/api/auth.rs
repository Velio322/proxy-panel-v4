use crate::api::common::{ok, AppJson};
use crate::audit::{self, AuditEntry};
use crate::auth::crypto::sha256_hex;
use crate::auth::extract::{bearer_from_headers, cookie_token_from_headers, AuthUser};
use crate::auth::password::{hash_password, verify_password};
use crate::db::enums::UserRole;
use crate::error::{AppError, AppResult};
use crate::net::{user_agent, ClientIp};
use crate::state::AppState;
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::NaiveDateTime;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use totp_rs::{Algorithm, Secret, TOTP};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/login/2fa", post(login_2fa))
        .route("/me", get(me))
        .route("/logout", post(logout))
        .route("/change-password", post(change_password))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangePasswordReq {
    current_password: String,
    new_password: String,
}

#[derive(Debug, Deserialize)]
struct LoginReq {
    username: String,
    password: String,
    #[serde(default)]
    totp_code: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TwoFaVerifyReq {
    challenge_token: String,
    code: String,
}

fn auth_cookie(token: &str, max_age_secs: i64, secure: bool) -> AppResult<HeaderValue> {
    let sec_flag = if secure { "; Secure" } else { "" };
    let val = format!(
        "auth_token={token}; Path=/; Max-Age={max_age_secs}; HttpOnly; SameSite=Lax{sec_flag}"
    );
    HeaderValue::from_str(&val).map_err(|e| AppError::internal(format!("invalid cookie value: {e}")))
}

/// Clears `auth_token` cookie (``: includes `; Secure` when `production` is true).
fn cleared_cookie(secure: bool) -> HeaderValue {
    if secure {
        HeaderValue::from_static(
            "auth_token=; Path=/; Expires=Thu, 01 Jan 1970 00:00:00 GMT; HttpOnly; SameSite=Lax; Secure",
        )
    } else {
        HeaderValue::from_static(
            "auth_token=; Path=/; Expires=Thu, 01 Jan 1970 00:00:00 GMT; HttpOnly; SameSite=Lax",
        )
    }
}

pub fn verify_totp_code(secret_b32: &str, code: &str) -> bool {
    let Ok(bytes) = Secret::Encoded(secret_b32.to_string()).to_bytes() else {
        return false;
    };
    let Ok(totp) = TOTP::new(Algorithm::SHA1, 6, 1, 30, bytes, None, String::new()) else {
        return false;
    };
    totp.check_current(code.trim()).unwrap_or(false)
}

pub(crate) async fn complete_login(
    state: &AppState,
    user_id: &str,
    username: &str,
    email: Option<String>,
    role: UserRole,
    reseller_id: Option<String>,
    language: String,
    ip: &str,
    ua: Option<String>,
) -> AppResult<Response> {
    let now = crate::db::now();
    sqlx::query(
        r#"UPDATE "User" SET "lastLoginAt" = $2, "lastLoginIp" = $3, "updatedAt" = $2 WHERE id = $1"#,
    )
    .bind(user_id)
    .bind(now)
    .bind(ip)
    .execute(&state.db)
    .await?;

    let (token, expires_at) = state.jwt.sign(user_id, username, role, reseller_id.clone())?;
    let token_hash = sha256_hex(&token);

    sqlx::query(
        r#"INSERT INTO "UserSession" (id, "userId", token, ip, "userAgent", "expiresAt", "createdAt")
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
    )
    .bind(crate::db::new_id())
    .bind(user_id)
    .bind(&token_hash)
    .bind(ip)
    .bind(&ua)
    .bind(expires_at)
    .bind(now)
    .execute(&state.db)
    .await?;

    audit::spawn(
        &state.db,
        AuditEntry::new("LOGIN", "user")
            .user(user_id)
            .resource_id(user_id)
            .ip(ip)
            .user_agent(ua),
    );

    let max_age = state.jwt.ttl_secs().max(0);
    let cookie = auth_cookie(&token, max_age, state.cfg.production)?;

    // HttpOnly cookie-first; also include user object (and token only for API clients)
    let mut res = (
        StatusCode::OK,
        Json(json!({
            "token": token,
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
    res.headers_mut().insert(header::SET_COOKIE, cookie);
    Ok(res)
}

async fn login(
    State(state): State<AppState>,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<LoginReq>,
) -> AppResult<Response> {
    if body.username.trim().is_empty() || body.password.is_empty() {
        return Err(AppError::validation("username", "Username and password are required"));
    }

    let row = sqlx::query(
        r#"SELECT id, username, email, password, role, "resellerId", language, banned, "totpEnabled", "totpSecret"
           FROM "User" WHERE username = $1"#,
    )
    .bind(body.username.trim())
    .fetch_optional(&state.db)
    .await?;

    let Some(row) = row else {
        return Err(AppError::unauthorized("Invalid credentials"));
    };

    let banned: bool = row.try_get("banned")?;
    if banned {
        return Err(AppError::forbidden("Account is banned"));
    }

    let pw_hash: String = row.try_get("password")?;
    if !verify_password(&body.password, &pw_hash).await? {
        return Err(AppError::unauthorized("Invalid credentials"));
    }

    let user_id: String = row.try_get("id")?;
    let username: String = row.try_get("username")?;
    let email: Option<String> = row.try_get("email")?;
    let role: UserRole = row.try_get("role")?;
    let reseller_id: Option<String> = row.try_get("resellerId")?;
    let language: String = row.try_get("language")?;
    let totp_enabled: bool = row.try_get::<Option<bool>, _>("totpEnabled")?.unwrap_or(false);
    let totp_secret: Option<String> = row.try_get("totpSecret")?;

    if totp_enabled {
        if let Some(sec) = totp_secret.as_deref().filter(|s| !s.is_empty()) {
            let valid = body
                .totp_code
                .as_deref()
                .is_some_and(|c| verify_totp_code(sec, c));
            if !valid {
                let challenge = state
                    .jwt
                    .sign_2fa_challenge(&user_id, &username, role, reseller_id)?;
                return Ok((
                    StatusCode::OK,
                    Json(json!({
                        "requires2fa": true,
                        "challengeToken": challenge
                    })),
                )
                    .into_response());
            }
        }
    }

    complete_login(
        &state,
        &user_id,
        &username,
        email,
        role,
        reseller_id,
        language,
        &ip,
        user_agent(&headers),
    )
    .await
}

async fn login_2fa(
    State(state): State<AppState>,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<TwoFaVerifyReq>,
) -> AppResult<Response> {
    let claims = state.jwt.verify_2fa_challenge(&body.challenge_token)?;
    let row = sqlx::query(
        r#"SELECT id, username, email, role, "resellerId", language, banned, "totpEnabled", "totpSecret"
           FROM "User" WHERE id = $1"#,
    )
    .bind(&claims.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::unauthorized("User not found"))?;

    if row.try_get::<bool, _>("banned")? {
        return Err(AppError::forbidden("Account is banned"));
    }
    let totp_secret: Option<String> = row.try_get("totpSecret")?;
    let Some(sec) = totp_secret.as_deref().filter(|s| !s.is_empty()) else {
        return Err(AppError::bad_request("2FA is not configured"));
    };
    if !verify_totp_code(sec, &body.code) {
        return Err(AppError::unauthorized("Invalid 2FA code"));
    }

    complete_login(
        &state,
        &row.try_get::<String, _>("id")?,
        &row.try_get::<String, _>("username")?,
        row.try_get("email")?,
        row.try_get("role")?,
        row.try_get("resellerId")?,
        row.try_get("language")?,
        &ip,
        user_agent(&headers),
    )
    .await
}

async fn me(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Value>> {
    let row = sqlx::query(
        r#"SELECT id, username, email, role, "resellerId", language, "totpEnabled", "telegramId", "createdAt"
           FROM "User" WHERE id = $1"#,
    )
    .bind(&user.id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::service_unavailable("Unable to load user"))?
    .ok_or_else(|| AppError::unauthorized("User not found"))?;

    let created_at: NaiveDateTime = row.try_get("createdAt")?;
    Ok(Json(json!({
        "id": row.try_get::<String, _>("id")?,
        "username": row.try_get::<String, _>("username")?,
        "email": row.try_get::<Option<String>, _>("email")?,
        "role": row.try_get::<UserRole, _>("role")?,
        "resellerId": row.try_get::<Option<String>, _>("resellerId")?,
        "language": row.try_get::<String, _>("language")?,
        "totpEnabled": row.try_get::<Option<bool>, _>("totpEnabled")?.unwrap_or(false),
        "telegramId": row.try_get::<Option<i64>, _>("telegramId")?,
        "createdAt": crate::db::ts::format_iso(&created_at)
    })))
}

/// Clears `auth_token` cookie even if expired/missing; revokes session by Bearer or Cookie token.
async fn logout(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    let token = bearer_from_headers(&headers).or_else(|| cookie_token_from_headers(&headers));
    if let Some(tok) = token {
        let hash = sha256_hex(&tok);
        sqlx::query(r#"DELETE FROM "UserSession" WHERE token = $1 OR token = $2"#)
            .bind(&tok)
            .bind(&hash)
            .execute(&state.db)
            .await
            .map_err(|_| AppError::service_unavailable("Unable to revoke session"))?;
    }

    let mut res = (StatusCode::OK, Json(json!({ "success": true }))).into_response();
    res.headers_mut()
        .insert(header::SET_COOKIE, cleared_cookie(state.cfg.production));
    Ok(res)
}

async fn change_password(
    State(state): State<AppState>,
    user: AuthUser,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<ChangePasswordReq>,
) -> AppResult<Json<Value>> {
    if body.new_password.len() < 8 {
        return Err(AppError::validation(
            "newPassword",
            "New password must be at least 8 characters",
        ));
    }

    let pw_hash: String = sqlx::query_scalar(r#"SELECT password FROM "User" WHERE id = $1"#)
        .bind(&user.id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("User not found"))?;

    if !verify_password(&body.current_password, &pw_hash).await? {
        return Err(AppError::unauthorized("Current password is incorrect"));
    }

    let next_hash = hash_password(&body.new_password, 12).await?;
    sqlx::query(r#"UPDATE "User" SET password = $2, "updatedAt" = $3 WHERE id = $1"#)
        .bind(&user.id)
        .bind(&next_hash)
        .bind(crate::db::now())
        .execute(&state.db)
        .await?;

    audit::spawn(
        &state.db,
        AuditEntry::new("CHANGE_PASSWORD", "user")
            .user(&user.id)
            .resource_id(&user.id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    Ok(ok())
}
