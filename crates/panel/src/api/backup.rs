use crate::api::common::{positive_int, sanitize_file_name, valid_id, OptionalJson};
use crate::auth::extract::AuthUser;
use crate::db::enums::{BackupStatus, BackupType};
use crate::error::{AppError, AppResult};
use crate::services::backup;
use crate::state::AppState;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::NaiveDateTime;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use std::path::PathBuf;
use tokio_util::io::ReaderStream;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/logs", get(logs))
        .route("/trigger", post(trigger))
        .route("/config", get(config))
        .route("/download/{id}", get(download))
}

#[derive(Debug, Deserialize)]
struct LogsQuery {
    page: Option<String>,
    limit: Option<String>,
}

async fn logs(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<LogsQuery>,
) -> AppResult<Json<Value>> {
    user.require_super_admin()?;
    let page = positive_int(q.page.as_deref(), 1, 100_000, "page")?;
    let limit = positive_int(q.limit.as_deref(), 20, 100, "limit")?;
    let offset = (page - 1) * limit;

    let total: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "BackupLog""#)
        .fetch_one(&state.db)
        .await?;

    let rows = sqlx::query(
        r#"SELECT id, type, status, destination, "filePath", "fileSize", error, "createdAt"
           FROM "BackupLog" ORDER BY "createdAt" DESC LIMIT $1 OFFSET $2"#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let mut data = Vec::with_capacity(rows.len());
    for r in rows {
        let created_at: NaiveDateTime = r.try_get("createdAt")?;
        data.push(json!({
            "id": r.try_get::<String, _>("id")?,
            "type": r.try_get::<BackupType, _>("type")?,
            "status": r.try_get::<BackupStatus, _>("status")?,
            "destination": r.try_get::<String, _>("destination")?,
            "filePath": r.try_get::<Option<String>, _>("filePath")?,
            "fileSize": r.try_get::<Option<i64>, _>("fileSize")?,
            "error": r.try_get::<Option<String>, _>("error")?,
            "createdAt": crate::db::ts::format_iso(&created_at),
        }));
    }

    let pages = ((total + limit - 1) / limit).max(1);
    Ok(Json(json!({
        "data": data,
        "total": total,
        "page": page,
        "pages": pages
    })))
}

async fn trigger(
    State(state): State<AppState>,
    user: AuthUser,
    OptionalJson(body): OptionalJson<Value>,
) -> AppResult<Response> {
    user.require_super_admin()?;
    if let Some(t) = body.get("type").and_then(Value::as_str) {
        if !matches!(t, "DATABASE" | "CONFIG" | "FULL") {
            return Err(AppError::bad_request(
                "Unsupported backup type (expected DATABASE, CONFIG, or FULL)",
            ));
        }
    }
    if let Some(d) = body.get("destination").and_then(Value::as_str) {
        if d != "local" {
            return Err(AppError::bad_request(
                "Only local destination is currently enabled on this server",
            ));
        }
    }

    let id = backup::trigger_background_backup(state).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "id": id,
            "status": "IN_PROGRESS"
        })),
    )
        .into_response())
}

async fn config(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Value>> {
    user.require_super_admin()?;
    Ok(Json(json!({
        "enabled": state.cfg.backup_enabled,
        "interval": state.cfg.backup_interval,
        "destinations": {
            "telegram": state.cfg.backup_telegram_token.is_some() && state.cfg.backup_telegram_chat_id.is_some(),
            "s3": state.cfg.backup_s3_bucket.is_some(),
            "local": true
        }
    })))
}

/// Streaming backup download using `tokio::fs::File` + `ReaderStream`.
async fn download(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Response> {
    user.require_super_admin()?;
    valid_id(&id)?;

    let file_path: Option<String> =
        sqlx::query_scalar(r#"SELECT "filePath" FROM "BackupLog" WHERE id = $1"#)
            .bind(&id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::not_found("Backup not found"))?;

    let raw_path = file_path.ok_or_else(|| AppError::not_found("Backup file not available"))?;
    let canonical = tokio::fs::canonicalize(PathBuf::from(&raw_path))
        .await
        .map_err(|_| AppError::not_found("Backup file not found on disk"))?;
    let base_dir = tokio::fs::canonicalize(&state.cfg.backup_dir)
        .await
        .unwrap_or_else(|_| state.cfg.backup_dir.clone());
    if !canonical.starts_with(&base_dir) {
        return Err(AppError::forbidden("Backup path outside backup directory"));
    }

    let name = canonical
        .file_name()
        .and_then(|n| n.to_str())
        .map(sanitize_file_name)
        .unwrap_or_else(|| "backup.sql".to_string());

    let file = tokio::fs::File::open(&canonical).await.map_err(|e| {
        tracing::error!(error = %e, "cannot read backup file");
        AppError::internal("cannot read backup file")
    })?;
    let stream = ReaderStream::new(file);
    let mut res = Response::new(Body::from_stream(stream));
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    if let Ok(cd) = HeaderValue::from_str(&format!("attachment; filename=\"{name}\"")) {
        res.headers_mut().insert(header::CONTENT_DISPOSITION, cd);
    }
    Ok(res)
}
