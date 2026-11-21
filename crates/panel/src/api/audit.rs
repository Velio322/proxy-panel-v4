use crate::auth::extract::AuthUser;
use crate::db::enums::UserRole;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::NaiveDateTime;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{QueryBuilder, Row};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/resources", get(resources))
        .route("/actions", get(actions))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuditQuery {
    user_id: Option<String>,
    resource: Option<String>,
    action: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
    page: Option<String>,
    limit: Option<String>,
}

async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<AuditQuery>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    let page = q
        .page
        .as_deref()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(1)
        .max(1);
    let limit = q
        .limit
        .as_deref()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(50)
        .clamp(1, 500);
    let offset = (page - 1).saturating_mul(limit);

    let start_dt = q
        .start_date
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(crate::db::ts::parse_iso)
        .transpose()
        .map_err(AppError::bad_request)?;
    let end_dt = q
        .end_date
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(crate::db::ts::parse_iso)
        .transpose()
        .map_err(AppError::bad_request)?;

    let mut count_qb = QueryBuilder::new(r#"SELECT COUNT(*) FROM "AuditLog" a WHERE 1=1"#);
    let mut data_qb = QueryBuilder::new(
        r#"SELECT a.id, a."userId", a.action, a.resource, a."resourceId", a.details, a.ip, a."userAgent", a."createdAt",
                  u.username AS u_username, u.role AS u_role
           FROM "AuditLog" a
           LEFT JOIN "User" u ON u.id = a."userId"
           WHERE 1=1"#,
    );

    if let Some(ref uid) = q.user_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        for qb in [&mut count_qb, &mut data_qb] {
            qb.push(r#" AND a."userId" = "#).push_bind(uid.to_string());
        }
    }
    if let Some(ref res) = q.resource.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        for qb in [&mut count_qb, &mut data_qb] {
            qb.push(r#" AND a.resource = "#).push_bind(res.to_string());
        }
    }
    if let Some(ref act) = q.action.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        for qb in [&mut count_qb, &mut data_qb] {
            qb.push(r#" AND a.action = "#).push_bind(act.to_string());
        }
    }
    if let Some(sd) = start_dt {
        for qb in [&mut count_qb, &mut data_qb] {
            qb.push(r#" AND a."createdAt" >= "#).push_bind(sd);
        }
    }
    if let Some(ed) = end_dt {
        for qb in [&mut count_qb, &mut data_qb] {
            qb.push(r#" AND a."createdAt" <= "#).push_bind(ed);
        }
    }

    let total: i64 = count_qb.build_query_scalar().fetch_one(&state.db).await?;
    data_qb
        .push(r#" ORDER BY a."createdAt" DESC LIMIT "#)
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);

    let rows = data_qb.build().fetch_all(&state.db).await?;
    let mut logs = Vec::with_capacity(rows.len());
    for r in rows {
        let created_at: NaiveDateTime = r.try_get("createdAt")?;
        let user_id: Option<String> = r.try_get("userId")?;
        let u_username: Option<String> = r.try_get("u_username")?;
        let u_role: Option<UserRole> = r.try_get("u_role")?;
        let user_obj = match (user_id.as_ref(), u_username, u_role) {
            (Some(uid), Some(uname), Some(role)) => json!({
                "id": uid,
                "username": uname,
                "role": role
            }),
            _ => Value::Null,
        };
        logs.push(json!({
            "id": r.try_get::<String, _>("id")?,
            "userId": user_id,
            "action": r.try_get::<String, _>("action")?,
            "resource": r.try_get::<String, _>("resource")?,
            "resourceId": r.try_get::<Option<String>, _>("resourceId")?,
            "details": r.try_get::<Option<Value>, _>("details")?,
            "ip": r.try_get::<Option<String>, _>("ip")?,
            "userAgent": r.try_get::<Option<String>, _>("userAgent")?,
            "createdAt": crate::db::ts::format_iso(&created_at),
            "user": user_obj
        }));
    }

    let pages = ((total + limit - 1) / limit).max(1);
    Ok(Json(json!({
        "logs": logs,
        "total": total,
        "page": page,
        "pages": pages
    })))
}

async fn resources(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Vec<Value>>> {
    user.require_admin()?;
    let rows = sqlx::query(
        r#"SELECT resource, COUNT(*) AS cnt FROM "AuditLog" GROUP BY resource ORDER BY cnt DESC"#,
    )
    .fetch_all(&state.db)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(json!({
            "resource": r.try_get::<String, _>("resource")?,
            "_count": { "id": r.try_get::<i64, _>("cnt")? }
        }));
    }
    Ok(Json(out))
}

async fn actions(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Vec<Value>>> {
    user.require_admin()?;
    let rows = sqlx::query(
        r#"SELECT action, COUNT(*) AS cnt FROM "AuditLog" GROUP BY action ORDER BY cnt DESC"#,
    )
    .fetch_all(&state.db)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(json!({
            "action": r.try_get::<String, _>("action")?,
            "_count": { "id": r.try_get::<i64, _>("cnt")? }
        }));
    }
    Ok(Json(out))
}
