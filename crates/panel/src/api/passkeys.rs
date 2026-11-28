use crate::api::auth::complete_login;
use crate::api::common::{ok, valid_id, AppJson, OptionalJson};
use crate::auth::extract::AuthUser;
use crate::db::enums::UserRole;
use crate::error::{AppError, AppResult};
use crate::net::{user_agent, ClientIp};
use crate::state::{AppState, PasskeyAuthState};
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::Response;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::NaiveDateTime;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use std::time::{Duration, Instant};
use webauthn_rs::prelude::*;

const PASSKEY_STATE_TTL: Duration = Duration::from_secs(300);
const MAX_PASSKEY_STATES: usize = 4096;

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
    if user.is_api_key {
        return Err(AppError::forbidden("API keys cannot manage passkeys"));
    }
    let webauthn = build_webauthn(&state)?;
    let uid = Uuid::parse_str(&user.id).unwrap_or_else(|_| Uuid::new_v4());
    let (ccr, reg_state) = webauthn
        .start_passkey_registration(uid, &user.username, &user.username, None)
        .map_err(|e| AppError::internal(format!("Passkey registration start failed: {e}")))?;

    let now = Instant::now();
    state.passkey_reg_states.remove(&user.id);
    state
        .passkey_reg_states
        .retain(|_, (ts, _)| now.saturating_duration_since(*ts) <= PASSKEY_STATE_TTL);
    if state.passkey_reg_states.len() >= MAX_PASSKEY_STATES {
        return Err(AppError::too_many_requests(
            "Too many active passkey registration challenges",
        ));
    }
    state
        .passkey_reg_states
        .insert(user.id.clone(), (now, reg_state));

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
    if user.is_api_key {
        return Err(AppError::forbidden("API keys cannot manage passkeys"));
    }
    let webauthn = build_webauthn(&state)?;
    let (_, (created_at, reg_state)) = state
        .passkey_reg_states
        .remove(&user.id)
        .ok_or_else(|| AppError::bad_request("Passkey registration session expired"))?;
    if created_at.elapsed() > PASSKEY_STATE_TTL {
        return Err(AppError::bad_request("Passkey registration session expired"));
    }

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

    let (rcr, auth_state) = if let Some(ref username) = uname {
        let rows = sqlx::query(
            r#"SELECT w."userId", w."passkeyJson"
               FROM "WebAuthnCredential" w
               JOIN "User" u ON u.id = w."userId"
               WHERE u.username = $1 AND u.banned = false"#,
        )
        .bind(username)
        .fetch_all(&state.db)
        .await?;

        let mut passkeys: Vec<Passkey> = Vec::with_capacity(rows.len());
        let mut expected_user_id: Option<String> = None;
        for r in rows {
            if expected_user_id.is_none() {
                expected_user_id = r.try_get::<String, _>("userId").ok();
            }
            if let Ok(pj) = r.try_get::<Value, _>("passkeyJson") {
                if let Ok(pk) = serde_json::from_value::<Passkey>(pj) {
                    passkeys.push(pk);
                }
            }
        }
        let Some(uid) = expected_user_id.filter(|_| !passkeys.is_empty()) else {
            return Err(AppError::bad_request(
                "No passkeys registered for this account",
            ));
        };
        let (rcr, st) = webauthn
            .start_passkey_authentication(&passkeys)
            .map_err(|e| AppError::internal(format!("Passkey authentication start failed: {e}")))?;
        (
            rcr,
            PasskeyAuthState::Named {
                user_id: uid,
                state: st,
            },
        )
    } else {
        // Discoverable (resident key) authentication avoids enumerating other users' credentialIds.
        let (rcr, st) = webauthn
            .start_discoverable_authentication()
            .map_err(|e| AppError::internal(format!("Passkey authentication start failed: {e}")))?;
        (rcr, PasskeyAuthState::Discoverable(st))
    };

    let now = Instant::now();
    state
        .passkey_auth_states
        .retain(|_, (ts, _)| now.saturating_duration_since(*ts) <= PASSKEY_STATE_TTL);
    if state.passkey_auth_states.len() >= MAX_PASSKEY_STATES {
        return Err(AppError::too_many_requests(
            "Too many active passkey authentication challenges",
        ));
    }

    let challenge_id = crate::db::new_id();
    state
        .passkey_auth_states
        .insert(challenge_id.clone(), (now, auth_state));

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
    let (_, (created_at, auth_state)) = state
        .passkey_auth_states
        .remove(&body.challenge_id)
        .ok_or_else(|| AppError::bad_request("Passkey authentication challenge expired"))?;
    if created_at.elapsed() > PASSKEY_STATE_TTL {
        return Err(AppError::bad_request(
            "Passkey authentication challenge expired",
        ));
    }

    let (cred_row, auth_result) = match auth_state {
        PasskeyAuthState::Named { user_id, state: st } => {
            let auth_result = webauthn
                .finish_passkey_authentication(&body.credential, &st)
                .map_err(|e| AppError::unauthorized(format!("Passkey verification failed: {e}")))?;
            let cred_id = hex::encode(auth_result.cred_id().as_slice());
            let cred_row = sqlx::query(
                r#"SELECT w.id, w."passkeyJson", u.id AS user_id, u.username, u.email, u.role, u."resellerId", u.language, u.banned
                   FROM "WebAuthnCredential" w
                   JOIN "User" u ON u.id = w."userId"
                   WHERE w."credentialId" = $1"#,
            )
            .bind(&cred_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::unauthorized("Passkey credential not registered"))?;

            let row_uid: String = cred_row.try_get("user_id")?;
            if row_uid != user_id {
                return Err(AppError::unauthorized(
                    "Passkey credential does not belong to requested account",
                ));
            }
            (cred_row, auth_result)
        }
        PasskeyAuthState::Discoverable(st) => {
            let (_user_uid, cred_id_bytes) = webauthn
                .identify_discoverable_authentication(&body.credential)
                .map_err(|e| AppError::unauthorized(format!("Passkey verification failed: {e}")))?;
            let cred_id = hex::encode(cred_id_bytes);
            let cred_row = sqlx::query(
                r#"SELECT w.id, w."passkeyJson", u.id AS user_id, u.username, u.email, u.role, u."resellerId", u.language, u.banned
                   FROM "WebAuthnCredential" w
                   JOIN "User" u ON u.id = w."userId"
                   WHERE w."credentialId" = $1"#,
            )
            .bind(&cred_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::unauthorized("Passkey credential not registered"))?;

            let pk_val: Value = cred_row.try_get("passkeyJson")?;
            let passkey: Passkey = serde_json::from_value(pk_val)
                .map_err(|_| AppError::unauthorized("Stored passkey credential is invalid"))?;
            let auth_result = webauthn
                .finish_discoverable_authentication_v2(&body.credential, st, &passkey)
                .map_err(|e| AppError::unauthorized(format!("Passkey verification failed: {e}")))?;
            (cred_row, auth_result)
        }
    };

    if cred_row.try_get::<bool, _>("banned")? {
        return Err(AppError::forbidden("Account is banned"));
    }

    let cred_row_id: String = cred_row.try_get("id")?;
    let mut updated_passkey_json: Option<Value> = None;
    if let Ok(pj) = cred_row.try_get::<Value, _>("passkeyJson") {
        if let Ok(mut pk) = serde_json::from_value::<Passkey>(pj) {
            if pk.update_credential(&auth_result) == Some(true) {
                updated_passkey_json = serde_json::to_value(&pk).ok();
            }
        }
    }

    let now = crate::db::now();
    if let Some(new_pj) = updated_passkey_json {
        let _ = sqlx::query(
            r#"UPDATE "WebAuthnCredential" SET "lastUsedAt" = $2, "passkeyJson" = $3 WHERE id = $1"#,
        )
        .bind(&cred_row_id)
        .bind(now)
        .bind(new_pj)
        .execute(&state.db)
        .await;
    } else {
        let _ = sqlx::query(
            r#"UPDATE "WebAuthnCredential" SET "lastUsedAt" = $2 WHERE id = $1"#,
        )
        .bind(&cred_row_id)
        .bind(now)
        .execute(&state.db)
        .await;
    }

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
    if user.is_api_key {
        return Err(AppError::forbidden("API keys cannot manage passkeys"));
    }
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
