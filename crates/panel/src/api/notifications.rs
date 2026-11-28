use crate::api::common::{ok, valid_id};
use crate::auth::extract::AuthUser;
use crate::db::enums::NotificationType;
use crate::error::AppResult;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::NaiveDateTime;
use serde_json::{json, Value};
use sqlx::Row;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/read-all", post(mark_all_read))
        .route("/{id}/read", post(mark_read))
}

async fn list(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Vec<Value>>> {
    let rows = sqlx::query(
        r#"SELECT id, type, title, message, read, metadata, "createdAt"
           FROM "Notification"
           WHERE ("targetUserId" IS NULL OR "targetUserId" = $1)
             AND ("targetResellerId" IS NULL OR "targetResellerId" = $2)
           ORDER BY "createdAt" DESC LIMIT 50"#,
    )
    .bind(&user.id)
    .bind(&user.reseller_id)
    .fetch_all(&state.db)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let ca: NaiveDateTime = r.try_get("createdAt")?;
        out.push(json!({
            "id": r.try_get::<String, _>("id")?,
            "type": r.try_get::<NotificationType, _>("type")?,
            "title": r.try_get::<String, _>("title")?,
            "message": r.try_get::<String, _>("message")?,
            "read": r.try_get::<bool, _>("read")?,
            "metadata": r.try_get::<Option<Value>, _>("metadata")?,
            "createdAt": crate::db::ts::format_iso(&ca),
        }));
    }
    Ok(Json(out))
}

async fn mark_read(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    valid_id(&id)?;
    if user.is_admin() {
        sqlx::query(
            r#"UPDATE "Notification" SET read = true
               WHERE id = $1
                 AND ("targetUserId" IS NULL OR "targetUserId" = $2)
                 AND ("targetResellerId" IS NULL OR "targetResellerId" = $3)"#,
        )
        .bind(&id)
        .bind(&user.id)
        .bind(&user.reseller_id)
        .execute(&state.db)
        .await?;
    } else {
        sqlx::query(
            r#"UPDATE "Notification" SET read = true
               WHERE id = $1
                 AND ("targetUserId" = $2 OR ($3::text IS NOT NULL AND "targetResellerId" = $3))"#,
        )
        .bind(&id)
        .bind(&user.id)
        .bind(&user.reseller_id)
        .execute(&state.db)
        .await?;
    }
    Ok(ok())
}

async fn mark_all_read(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Value>> {
    if user.is_admin() {
        sqlx::query(
            r#"UPDATE "Notification" SET read = true
               WHERE ("targetUserId" IS NULL OR "targetUserId" = $1)
                 AND ("targetResellerId" IS NULL OR "targetResellerId" = $2)
                 AND read = false"#,
        )
        .bind(&user.id)
        .bind(&user.reseller_id)
        .execute(&state.db)
        .await?;
    } else {
        sqlx::query(
            r#"UPDATE "Notification" SET read = true
               WHERE ("targetUserId" = $1 OR ($2::text IS NOT NULL AND "targetResellerId" = $2))
                 AND read = false"#,
        )
        .bind(&user.id)
        .bind(&user.reseller_id)
        .execute(&state.db)
        .await?;
    }
    Ok(ok())
}
