use crate::api::common::AppJson;
use crate::api::middleware::transfer_rate_limit;
use crate::auth::extract::AuthUser;
use crate::db::enums::UserRole;
use crate::error::{AppError, AppResult};
use crate::services::config_transfer::{
    self, TransferDocumentV1, MAX_TRANSFER_BYTES,
};
use crate::state::AppState;
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware as mw;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;
use sqlx::Row;
use std::collections::HashMap;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/export", get(export_v1))
        .route("/import", post(import_v1))
        .route("/v2/export", get(export_v2))
        .route("/v2/import", post(import_v2))
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
        .layer(mw::from_fn_with_state(state, transfer_rate_limit))
}

async fn verify_super_admin_live(state: &AppState, user: &AuthUser) -> AppResult<()> {
    user.require_super_admin()?;
    let row = sqlx::query(r#"SELECT role, banned FROM "User" WHERE id = $1"#)
        .bind(&user.id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::forbidden("Forbidden"))?;

    let role: UserRole = row.try_get("role")?;
    let banned: bool = row.try_get("banned")?;
    if banned || role != UserRole::SuperAdmin {
        return Err(AppError::forbidden("Forbidden"));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportQuery {
    format: Option<String>,
    include_secrets: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ImportV1Req {
    format: Option<String>,
    content: String,
    #[serde(default)]
    apply: bool,
    #[serde(default)]
    credentials: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct ImportV2Req {
    format: Option<String>,
    content: String,
    #[serde(default)]
    apply: bool,
    #[serde(default = "default_mode")]
    mode: String,
}

fn default_mode() -> String {
    "create-only".to_string()
}

fn format_response(filename: &str, format: &str, body: String) -> AppResult<Response> {
    if body.len() > MAX_TRANSFER_BYTES {
        return Err(AppError::payload_too_large("Export exceeds size limit"));
    }
    let ct = if format == "yaml" {
        "application/yaml; charset=utf-8"
    } else {
        "application/json; charset=utf-8"
    };
    let mut res = (StatusCode::OK, body).into_response();
    res.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(ct));
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );
    if let Ok(cd) = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")) {
        res.headers_mut().insert(header::CONTENT_DISPOSITION, cd);
    }
    Ok(res)
}

async fn export_v1(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<ExportQuery>,
) -> AppResult<Response> {
    verify_super_admin_live(&state, &user).await?;
    let fmt = q.format.as_deref().unwrap_or("json").to_ascii_lowercase();
    let doc = config_transfer::export_v1(&state).await?;
    let content = match fmt.as_str() {
        "yaml" => serde_yaml::to_string(&doc)
            .map_err(|e| AppError::internal(format!("YAML encode failed: {e}")))?,
        "json" => serde_json::to_string_pretty(&doc)
            .map_err(|e| AppError::internal(format!("JSON encode failed: {e}")))?,
        _ => return Err(AppError::bad_request("Invalid format (expected json or yaml)")),
    };
    let filename = format!("proxpanel-config.{fmt}");
    format_response(&filename, &fmt, content)
}

async fn import_v1(
    State(state): State<AppState>,
    user: AuthUser,
    AppJson(body): AppJson<ImportV1Req>,
) -> AppResult<Json<Value>> {
    verify_super_admin_live(&state, &user).await?;
    if body.content.len() > MAX_TRANSFER_BYTES {
        return Err(AppError::payload_too_large("Request exceeds size limit"));
    }
    let fmt = body.format.as_deref().unwrap_or("json").to_ascii_lowercase();
    let doc: TransferDocumentV1 = match fmt.as_str() {
        "yaml" => serde_yaml::from_str(&body.content)
            .map_err(|_| AppError::bad_request("Invalid transfer document or credentials"))?,
        "json" => serde_json::from_str(&body.content)
            .map_err(|_| AppError::bad_request("Invalid transfer document or credentials"))?,
        _ => return Err(AppError::bad_request("Invalid format")),
    };
    let out = config_transfer::import_v1(&state, doc, &body.credentials, body.apply).await?;
    Ok(Json(out))
}

async fn export_v2(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<ExportQuery>,
) -> AppResult<Response> {
    verify_super_admin_live(&state, &user).await?;
    let fmt = q.format.as_deref().unwrap_or("json").to_ascii_lowercase();
    let include_secrets = q.include_secrets.as_deref() == Some("true");
    let doc = config_transfer::export_v2(&state, include_secrets).await?;
    let content = match fmt.as_str() {
        "yaml" => serde_yaml::to_string(&doc)
            .map_err(|e| AppError::internal(format!("YAML encode failed: {e}")))?,
        "json" => serde_json::to_string_pretty(&doc)
            .map_err(|e| AppError::internal(format!("JSON encode failed: {e}")))?,
        _ => return Err(AppError::bad_request("Invalid format (expected json or yaml)")),
    };
    let filename = format!("proxpanel-config-v2.{fmt}");
    format_response(&filename, &fmt, content)
}

async fn import_v2(
    State(state): State<AppState>,
    user: AuthUser,
    AppJson(body): AppJson<ImportV2Req>,
) -> AppResult<Json<Value>> {
    verify_super_admin_live(&state, &user).await?;
    if body.content.len() > MAX_TRANSFER_BYTES {
        return Err(AppError::payload_too_large("Request exceeds size limit"));
    }
    let fmt = body.format.as_deref().unwrap_or("json").to_ascii_lowercase();
    let doc: Value = match fmt.as_str() {
        "yaml" => serde_yaml::from_str(&body.content)
            .map_err(|_| AppError::bad_request("Invalid transfer document or credentials"))?,
        "json" => serde_json::from_str(&body.content)
            .map_err(|_| AppError::bad_request("Invalid transfer document or credentials"))?,
        _ => return Err(AppError::bad_request("Invalid format")),
    };
    let out = config_transfer::import_v2(&state, doc, &body.mode, body.apply).await?;
    Ok(Json(out))
}
