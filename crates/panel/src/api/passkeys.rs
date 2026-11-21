use crate::api::auth::complete_login;
use crate::api::common::{ok, valid_id, AppJson, OptionalJson};
use crate::auth::extract::AuthUser;
use crate::db::enums::UserRole;
use crate::error::{AppError, AppResult};
use crate::net::{user_agent, ClientIp};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::Response;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::NaiveDateTime;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use webauthn_rs::prelude::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/register/start", post(register_start))
        .route("/register/finish", post(register_finish))
        .route("/login/start", post(login_start))
        .route("/login/finish", post(login_finish))
        .route("/{id}", delete(remove))
}

fn build_webauthn(state: &AppState) -> AppResult<Webauthn> {
    let rp_origin = Url::parse(&state.cfg.frontend_url)
        .or_else(|_| Url::parse(&state.cfg.api_url))
        .map_err(|e| AppError::internal(format!("Invalid RP URL: {e}")))?;
    let rp_id = rp_origin.host_str().unwrap_or("localhost");
    WebauthnBuilder::new(rp_id, &rp_origin)
        .map_err(|e| AppError::internal(format!("WebAuthn init failed: {e}")))?
        .rp_name("ProxyPanel v4")
        .build()
        .map_err(|e| AppError::internal(format!("WebAuthn build failed: {e}")))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterFinishReq {
    #[allow(dead_code)]
    challenge_id: Option<String>,
    name: Option<String>,
    credential: RegisterPublicKeyCredential,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PasskeyLoginStartReq {
    username: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PasskeyLoginFinishReq {
    challenge_id: String,
    credential: PublicKeyCredential,
}

async fn list(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Vec<Value>>> {
    let rows = sqlx::query(
        r#"SELECT id, "credentialId", name, "lastUsedAt", "createdAt"
           FROM "WebAuthnCredential" WHERE "userId" = $1 ORDER BY "createdAt" DESC"#,
    )
    .bind(&user.id)
    .fetch_all(&state.db)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let lu: Option<NaiveDateTime> = r.try_get("lastUsedAt")?;
        let ca: NaiveDateTime = r.try_get("createdAt")?;
        out.push(json!({
            "id": r.try_get::<String, _>("id")?,
            "credentialId": r.try_get::<String, _>("credentialId")?,
            "name": r.try_get::<Option<String>, _>("name")?,
            "lastUsedAt": lu.map(|d| crate::db::ts::format_iso(&d)),
            "createdAt": crate::db::ts::format_iso(&ca),
        }));
    }
    Ok(Json(out))
}

async fn register_start(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Value>> {
    let webauthn = build_webauthn(&state)?;
    let uid = Uuid::parse_str(&user.id).unwrap_or_else(|_| Uuid::new_v4());
    let (ccr, reg_state) = webauthn
        .start_passkey_registration(uid, &user.username, &user.username, None)
        .map_err(|e| AppError::internal(format!("Passkey registration start failed: {e}")))?;

    state
        .passkey_reg_states
        .insert(user.id.clone(), reg_state);

    let val = serde_json::to_value(&ccr)
        .map_err(|e| AppError::internal(format!("Serialize challenge: {e}")))?;
    let options = val.get("publicKey").cloned().unwrap_or(val);
    Ok(Json(json!({
        "challengeId": user.id,
        "options": options.clone(),
        "publicKey": options
    })))
}

async fn register_finish(
    State(state): State<AppState>,
    user: AuthUser,
    AppJson(body): AppJson<RegisterFinishReq>,
) -> AppResult<Json<Value>> {
    let webauthn = build_webauthn(&state)?;
    let (_, reg_state) = state
        .passkey_reg_states
        .remove(&user.id)
        .ok_or_else(|| AppError::bad_request("Passkey registration session expired"))?;

    let passkey = webauthn
        .finish_passkey_registration(&body.credential, &reg_state)
        .map_err(|e| AppError::bad_request(format!("Passkey verification failed: {e}")))?;

    let cred_id = hex::encode(passkey.cred_id().as_slice());
    let passkey_json = serde_json::to_value(&passkey)
        .map_err(|e| AppError::internal(format!("Serialize passkey: {e}")))?;

    let id = crate::db::new_id();
    let now = crate::db::now();
    sqlx::query(
        r#"INSERT INTO "WebAuthnCredential" (id, "userId", "credentialId", name, "passkeyJson", "createdAt")
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(&id)
    .bind(&user.id)
    .bind(&cred_id)
    .bind(&body.name)
    .bind(&passkey_json)
    .bind(now)
    .execute(&state.db)
    .await?;

    Ok(Json(json!({
        "id": id,
        "credentialId": cred_id,
        "name": body.name,
        "createdAt": crate::db::ts::format_iso(&now)
    })))
}

async fn login_start(
    State(state): State<AppState>,
    OptionalJson(body): OptionalJson<PasskeyLoginStartReq>,
) -> AppResult<Json<Value>> {
    let webauthn = build_webauthn(&state)?;
    let uname = body
        .username
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);

    let rows = if let Some(ref username) = uname {
        sqlx::query(
            r#"SELECT w."userId", w."passkeyJson"
               FROM "WebAuthnCredential" w
               JOIN "User" u ON u.id = w."userId"
               WHERE u.username = $1 AND u.banned = false"#,
        )
        .bind(username)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query(
            r#"SELECT w."userId", w."passkeyJson"
               FROM "WebAuthnCredential" w
               JOIN "User" u ON u.id = w."userId"
               WHERE u.banned = false
               ORDER BY w."createdAt" DESC
               LIMIT 50"#,
        )
        .fetch_all(&state.db)
        .await?
    };

    let mut passkeys: Vec<Passkey> = Vec::with_capacity(rows.len());
    let mut user_id_hint: Option<String> = None;
    for r in rows {
        if user_id_hint.is_none() && uname.is_some() {
            user_id_hint = r.try_get::<String, _>("userId").ok();
        }
        if let Ok(pj) = r.try_get::<Value, _>("passkeyJson") {
            if let Ok(pk) = serde_json::from_value::<Passkey>(pj) {
                passkeys.push(pk);
            }
        }
    }

    if passkeys.is_empty() {
        return Err(AppError::bad_request(
            "No passkeys registered for this account",
        ));
    }

    let (rcr, auth_state) = webauthn
        .start_passkey_authentication(&passkeys)
        .map_err(|e| AppError::internal(format!("Passkey authentication start failed: {e}")))?;

    let challenge_id = crate::db::new_id();
    state
        .passkey_auth_states
        .insert(challenge_id.clone(), (user_id_hint, auth_state));

    let val = serde_json::to_value(&rcr)
        .map_err(|e| AppError::internal(format!("Serialize auth challenge: {e}")))?;
    let options = val.get("publicKey").cloned().unwrap_or(val);
    Ok(Json(json!({
        "challengeId": challenge_id,
        "options": options.clone(),
        "publicKey": options
    })))
}

async fn login_finish(
    State(state): State<AppState>,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<PasskeyLoginFinishReq>,
) -> AppResult<Response> {
    let webauthn = build_webauthn(&state)?;
    let (_, (_, auth_state)) = state
        .passkey_auth_states
        .remove(&body.challenge_id)
        .ok_or_else(|| AppError::bad_request("Passkey authentication challenge expired"))?;

    let auth_result = webauthn
        .finish_passkey_authentication(&body.credential, &auth_state)
        .map_err(|e| AppError::unauthorized(format!("Passkey verification failed: {e}")))?;

    let cred_id = hex::encode(auth_result.cred_id().as_slice());
    let cred_row = sqlx::query(
        r#"SELECT w.id, u.id AS user_id, u.username, u.email, u.role, u."resellerId", u.language, u.banned
           FROM "WebAuthnCredential" w
           JOIN "User" u ON u.id = w."userId"
           WHERE w."credentialId" = $1"#,
    )
    .bind(&cred_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::unauthorized("Passkey credential not registered"))?;

    if cred_row.try_get::<bool, _>("banned")? {
        return Err(AppError::forbidden("Account is banned"));
    }

    let cred_row_id: String = cred_row.try_get("id")?;
    let _ = sqlx::query(
        r#"UPDATE "WebAuthnCredential" SET "lastUsedAt" = $2 WHERE id = $1"#,
    )
    .bind(&cred_row_id)
    .bind(crate::db::now())
    .execute(&state.db)
    .await;

    complete_login(
        &state,
        &cred_row.try_get::<String, _>("user_id")?,
        &cred_row.try_get::<String, _>("username")?,
        cred_row.try_get("email")?,
        cred_row.try_get::<UserRole, _>("role")?,
        cred_row.try_get("resellerId")?,
        cred_row.try_get("language")?,
        &ip,
        user_agent(&headers),
    )
    .await
}

async fn remove(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    valid_id(&id)?;
    let res = sqlx::query(
        r#"DELETE FROM "WebAuthnCredential" WHERE id = $1 AND "userId" = $2"#,
    )
    .bind(&id)
    .bind(&user.id)
    .execute(&state.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("Passkey not found"));
    }
    Ok(ok())
}
