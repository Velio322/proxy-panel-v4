use crate::api::common::{ok, valid_id, AppJson};
use crate::auth::extract::AuthUser;
use crate::error::{AppError, AppResult};
use crate::services::webhook;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::NaiveDateTime;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", get(get_one).put(update).delete(remove))
        .route("/{id}/test", post(test_webhook))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpsertWebhookReq {
    name: String,
    url: String,
    #[serde(default)]
    secret: String,
    #[serde(default)]
    scopes: Vec<String>,
    #[serde(default)]
    events: Vec<String>,
    #[serde(default = "default_true")]
    active: bool,
}

fn default_true() -> bool {
    true
}

fn webhook_json(r: &sqlx::postgres::PgRow) -> AppResult<Value> {
    let last_del: Option<NaiveDateTime> = r.try_get("lastDeliveredAt")?;
    let ca: NaiveDateTime = r.try_get("createdAt")?;
    let ua: NaiveDateTime = r.try_get("updatedAt")?;
    Ok(json!({
        "id": r.try_get::<String, _>("id")?,
        "name": r.try_get::<String, _>("name")?,
        "url": r.try_get::<String, _>("url")?,
        "scopes": r.try_get::<Vec<String>, _>("scopes")?,
        "events": r.try_get::<Vec<String>, _>("events")?,
        "active": r.try_get::<bool, _>("active")?,
        "lastStatus": r.try_get::<Option<i32>, _>("lastStatus")?,
        "lastError": r.try_get::<Option<String>, _>("lastError")?,
        "lastDeliveredAt": last_del.map(|d| crate::db::ts::format_iso(&d)),
        "createdAt": crate::db::ts::format_iso(&ca),
        "updatedAt": crate::db::ts::format_iso(&ua),
    }))
}

async fn list(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Vec<Value>>> {
    user.require_admin()?;
    let rows = sqlx::query(
        r#"SELECT id, name, url, scopes, events, active, "lastStatus", "lastError", "lastDeliveredAt", "createdAt", "updatedAt"
           FROM "Webhook" ORDER BY "createdAt" DESC"#,
    )
    .fetch_all(&state.db)
    .await?;
    rows.iter().map(webhook_json).collect::<AppResult<Vec<_>>>().map(Json)
}

async fn get_one(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let r = sqlx::query(
        r#"SELECT id, name, url, scopes, events, active, "lastStatus", "lastError", "lastDeliveredAt", "createdAt", "updatedAt"
           FROM "Webhook" WHERE id = $1"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Webhook not found"))?;
    Ok(Json(webhook_json(&r)?))
}

async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    AppJson(body): AppJson<UpsertWebhookReq>,
) -> AppResult<Response> {
    user.require_admin()?;
    let parsed = url::Url::parse(body.url.trim())
        .map_err(|_| AppError::validation("url", "Invalid webhook URL"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(AppError::validation("url", "Webhook URL must be http or https"));
    }
    let id = crate::db::new_id();
    let now = crate::db::now();
    let secret = if body.secret.trim().is_empty() {
        crate::auth::crypto::random_hex(24)
    } else {
        body.secret
    };
    let r = sqlx::query(
        r#"INSERT INTO "Webhook" (id, name, url, secret, scopes, events, active, "createdAt", "updatedAt")
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
           RETURNING id, name, url, scopes, events, active, "lastStatus", "lastError", "lastDeliveredAt", "createdAt", "updatedAt""#,
    )
    .bind(&id)
    .bind(body.name.trim())
    .bind(body.url.trim())
    .bind(&secret)
    .bind(&body.scopes)
    .bind(&body.events)
    .bind(body.active)
    .bind(now)
    .fetch_one(&state.db)
    .await?;
    let mut val = webhook_json(&r)?;
    if let Some(m) = val.as_object_mut() {
        m.insert("secret".into(), json!(secret));
    }
    Ok((StatusCode::CREATED, Json(val)).into_response())
}

async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    AppJson(body): AppJson<UpsertWebhookReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let r = sqlx::query(
        r#"UPDATE "Webhook"
           SET name = $2, url = $3, secret = CASE WHEN $4 = '' THEN secret ELSE $4 END,
               scopes = $5, events = $6, active = $7, "updatedAt" = $8
           WHERE id = $1
           RETURNING id, name, url, scopes, events, active, "lastStatus", "lastError", "lastDeliveredAt", "createdAt", "updatedAt""#,
    )
    .bind(&id)
    .bind(body.name.trim())
    .bind(body.url.trim())
    .bind(body.secret.trim())
    .bind(&body.scopes)
    .bind(&body.events)
    .bind(body.active)
    .bind(crate::db::now())
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Webhook not found"))?;
    Ok(Json(webhook_json(&r)?))
}

async fn remove(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let res = sqlx::query(r#"DELETE FROM "Webhook" WHERE id = $1"#)
        .bind(&id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("Webhook not found"));
    }
    Ok(ok())
}

async fn test_webhook(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    webhook::emit(&state, "system", "webhook.test", json!({ "webhookId": id }));
    Ok(ok())
}
