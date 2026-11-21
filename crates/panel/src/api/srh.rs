use crate::api::common::{like_pattern, positive_int};
use crate::auth::extract::AuthUser;
use crate::error::AppResult;
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
        .route("/srh", get(list_srh))
        .route("/hwid", get(list_hwid))
        .route("/torrent-reports", get(list_torrent_reports))
}

#[derive(Debug, Deserialize)]
struct PageQuery {
    page: Option<String>,
    limit: Option<String>,
    search: Option<String>,
}

async fn list_srh(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<PageQuery>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    let page = positive_int(q.page.as_deref(), 1, 100_000, "page")?;
    let limit = positive_int(q.limit.as_deref(), 50, 200, "limit")?;
    let offset = (page - 1) * limit;

    let mut count_qb = QueryBuilder::new(r#"SELECT COUNT(*) FROM "SubRequestHistory" WHERE 1=1"#);
    let mut data_qb = QueryBuilder::new(
        r#"SELECT id, "clientId", username, ip, "userAgent", "detectedApp", format, "srrRuleName", status, hwid, "createdAt"
           FROM "SubRequestHistory" WHERE 1=1"#,
    );

    if let Some(ref s) = q.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let pat = like_pattern(s);
        for qb in [&mut count_qb, &mut data_qb] {
            qb.push(r#" AND (COALESCE(username,'') ILIKE "#)
                .push_bind(pat.clone())
                .push(r#" OR ip ILIKE "#)
                .push_bind(pat.clone())
                .push(r#" OR "userAgent" ILIKE "#)
                .push_bind(pat.clone())
                .push(r#" OR COALESCE("detectedApp",'') ILIKE "#)
                .push_bind(pat.clone())
                .push(r#" OR COALESCE("srrRuleName",'') ILIKE "#)
                .push_bind(pat.clone())
                .push(r#" OR COALESCE(hwid,'') ILIKE "#)
                .push_bind(pat.clone())
                .push(")");
        }
    }

    let total: i64 = count_qb.build_query_scalar().fetch_one(&state.db).await?;
    data_qb
        .push(r#" ORDER BY "createdAt" DESC LIMIT "#)
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);

    let rows = data_qb.build().fetch_all(&state.db).await?;

    let mut data = Vec::with_capacity(rows.len());
    for r in rows {
        let ca: NaiveDateTime = r.try_get("createdAt")?;
        data.push(json!({
            "id": r.try_get::<String, _>("id")?,
            "clientId": r.try_get::<Option<String>, _>("clientId")?,
            "username": r.try_get::<Option<String>, _>("username")?,
            "ip": r.try_get::<String, _>("ip")?,
            "userAgent": r.try_get::<String, _>("userAgent")?,
            "detectedApp": r.try_get::<Option<String>, _>("detectedApp")?,
            "format": r.try_get::<Option<String>, _>("format")?,
            "srrRuleName": r.try_get::<Option<String>, _>("srrRuleName")?,
            "status": r.try_get::<i32, _>("status")?,
            "hwid": r.try_get::<Option<String>, _>("hwid")?,
            "createdAt": crate::db::ts::format_iso(&ca),
        }));
    }

    Ok(Json(json!({
        "data": data,
        "total": total,
        "page": page,
        "limit": limit
    })))
}

async fn list_hwid(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<PageQuery>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    let page = positive_int(q.page.as_deref(), 1, 100_000, "page")?;
    let limit = positive_int(q.limit.as_deref(), 50, 200, "limit")?;
    let offset = (page - 1) * limit;

    let mut count_qb = QueryBuilder::new(
        r#"SELECT COUNT(*) FROM "Device" d JOIN "Client" c ON c.id = d."clientId" WHERE 1=1"#,
    );
    let mut data_qb = QueryBuilder::new(
        r#"SELECT d.id, d."clientId", c.username, c."deviceLimit", d.hwid, d.os, d."osVersion", d.model, d."userAgent", d.ip, d."lastSeenAt", d."createdAt"
           FROM "Device" d
           JOIN "Client" c ON c.id = d."clientId"
           WHERE 1=1"#,
    );

    if let Some(ref s) = q.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let pat = like_pattern(s);
        for qb in [&mut count_qb, &mut data_qb] {
            qb.push(r#" AND (d.hwid ILIKE "#)
                .push_bind(pat.clone())
                .push(r#" OR c.username ILIKE "#)
                .push_bind(pat.clone())
                .push(r#" OR COALESCE(d.os,'') ILIKE "#)
                .push_bind(pat.clone())
                .push(r#" OR COALESCE(d.model,'') ILIKE "#)
                .push_bind(pat.clone())
                .push(r#" OR COALESCE(d.ip,'') ILIKE "#)
                .push_bind(pat.clone())
                .push(")");
        }
    }

    let total: i64 = count_qb.build_query_scalar().fetch_one(&state.db).await?;
    data_qb
        .push(r#" ORDER BY d."lastSeenAt" DESC LIMIT "#)
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);

    let rows = data_qb.build().fetch_all(&state.db).await?;

    let mut data = Vec::with_capacity(rows.len());
    for r in rows {
        let ls: NaiveDateTime = r.try_get("lastSeenAt")?;
        let ca: NaiveDateTime = r.try_get("createdAt")?;
        data.push(json!({
            "id": r.try_get::<String, _>("id")?,
            "clientId": r.try_get::<String, _>("clientId")?,
            "username": r.try_get::<String, _>("username")?,
            "deviceLimit": r.try_get::<Option<i32>, _>("deviceLimit")?,
            "hwid": r.try_get::<String, _>("hwid")?,
            "os": r.try_get::<Option<String>, _>("os")?,
            "osVersion": r.try_get::<Option<String>, _>("osVersion")?,
            "model": r.try_get::<Option<String>, _>("model")?,
            "userAgent": r.try_get::<Option<String>, _>("userAgent")?,
            "ip": r.try_get::<Option<String>, _>("ip")?,
            "lastSeenAt": crate::db::ts::format_iso(&ls),
            "createdAt": crate::db::ts::format_iso(&ca),
        }));
    }

    Ok(Json(json!({
        "data": data,
        "total": total,
        "page": page,
        "limit": limit
    })))
}

async fn list_torrent_reports(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    let rows = sqlx::query(
        r#"SELECT a.id, a."resourceId", n.name AS node_name, a.details, a.ip, a."createdAt"
           FROM "AuditLog" a
           LEFT JOIN "Node" n ON n.id = a."resourceId"
           WHERE a.action = 'ALERT' AND (a.details::text ILIKE '%torrent%' OR a.details::text ILIKE '%bittorrent%')
           ORDER BY a."createdAt" DESC LIMIT 100"#,
    )
    .fetch_all(&state.db)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let ca: NaiveDateTime = r.try_get("createdAt")?;
        let details: Option<Value> = r.try_get("details")?;
        let d_obj = details.as_ref().and_then(Value::as_object);
        let fallback_ip: Option<String> = r.try_get("ip")?;
        out.push(json!({
            "id": r.try_get::<String, _>("id")?,
            "nodeId": r.try_get::<Option<String>, _>("resourceId")?,
            "nodeName": r.try_get::<Option<String>, _>("node_name")?,
            "clientId": d_obj.and_then(|m| m.get("clientId")).and_then(Value::as_str),
            "username": d_obj.and_then(|m| m.get("username")).and_then(Value::as_str),
            "sourceIp": d_obj.and_then(|m| m.get("sourceIp").or_else(|| m.get("ip"))).and_then(Value::as_str).or(fallback_ip.as_deref()).unwrap_or("—"),
            "protocol": d_obj.and_then(|m| m.get("protocol")).and_then(Value::as_str).unwrap_or("BitTorrent"),
            "rule": d_obj.and_then(|m| m.get("rule")).and_then(Value::as_str),
            "action": d_obj.and_then(|m| m.get("action")).and_then(Value::as_str).unwrap_or("BLOCKED"),
            "details": details,
            "createdAt": crate::db::ts::format_iso(&ca),
        }));
    }
    let total = out.len();
    Ok(Json(json!({ "data": out, "total": total })))
}
