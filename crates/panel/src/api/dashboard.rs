use crate::api::common::positive_int;
use crate::auth::extract::AuthUser;
use crate::db::enums::UserRole;
use crate::error::AppResult;
use crate::state::AppState;
use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{Datelike, Duration as ChronoDuration, NaiveDate, NaiveDateTime};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{QueryBuilder, Row};
use std::collections::HashMap;
use std::time::Duration;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/overview", get(overview))
        .route("/traffic-chart", get(traffic_chart))
        .route("/top-clients", get(top_clients))
        .route("/recent-audit", get(recent_audit))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChartQuery {
    days: Option<String>,
    node_id: Option<String>,
    client_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TopQuery {
    days: Option<String>,
    limit: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RecentAuditQuery {
    limit: Option<String>,
}

async fn overview(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Value>> {
    let tenant = user.tenant_scope()?;
    let cache_key = format!(
        "dashboard:overview:{}:{}",
        user.role.as_str(),
        tenant.as_deref().unwrap_or("all")
    );
    if let Some(cached) = state.cache.get_json(&cache_key) {
        return Ok(Json(cached));
    }

    let now = crate::db::now();
    let today = now.date().and_hms_opt(0, 0, 0).unwrap_or(now);
    let tomorrow = today + ChronoDuration::days(1);
    let this_month = NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .unwrap_or(today);

    let (total_clients, active_clients, banned_clients, expiring_today, today_up, today_down, month_up, month_down) =
        if let Some(ref rid) = tenant {
            let total: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Client" WHERE "resellerId" = $1"#)
                .bind(rid)
                .fetch_one(&state.db)
                .await?;
            let active: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "Client" WHERE "resellerId" = $1 AND banned = false AND ("expireAt" IS NULL OR "expireAt" > $2)"#,
            )
            .bind(rid)
            .bind(now)
            .fetch_one(&state.db)
            .await?;
            let banned: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "Client" WHERE "resellerId" = $1 AND banned = true"#,
            )
            .bind(rid)
            .fetch_one(&state.db)
            .await?;
            let exp: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "Client" WHERE "resellerId" = $1 AND "expireAt" >= $2 AND "expireAt" < $3"#,
            )
            .bind(rid)
            .bind(today)
            .bind(tomorrow)
            .fetch_one(&state.db)
            .await?;
            let td = sqlx::query(
                r#"SELECT COALESCE(SUM(t.upload), 0)::bigint AS up, COALESCE(SUM(t.download), 0)::bigint AS down
                   FROM "TrafficLog" t JOIN "Client" c ON c.id = t."clientId"
                   WHERE c."resellerId" = $1 AND t."recordAt" >= $2"#,
            )
            .bind(rid)
            .bind(today)
            .fetch_one(&state.db)
            .await?;
            let mo = sqlx::query(
                r#"SELECT COALESCE(SUM(t.upload), 0)::bigint AS up, COALESCE(SUM(t.download), 0)::bigint AS down
                   FROM "TrafficLog" t JOIN "Client" c ON c.id = t."clientId"
                   WHERE c."resellerId" = $1 AND t."recordAt" >= $2"#,
            )
            .bind(rid)
            .bind(this_month)
            .fetch_one(&state.db)
            .await?;
            (
                total,
                active,
                banned,
                exp,
                td.try_get::<i64, _>("up")?,
                td.try_get::<i64, _>("down")?,
                mo.try_get::<i64, _>("up")?,
                mo.try_get::<i64, _>("down")?,
            )
        } else {
            let total: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Client""#)
                .fetch_one(&state.db)
                .await?;
            let active: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "Client" WHERE banned = false AND ("expireAt" IS NULL OR "expireAt" > $1)"#,
            )
            .bind(now)
            .fetch_one(&state.db)
            .await?;
            let banned: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Client" WHERE banned = true"#)
                .fetch_one(&state.db)
                .await?;
            let exp: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "Client" WHERE "expireAt" >= $1 AND "expireAt" < $2"#,
            )
            .bind(today)
            .bind(tomorrow)
            .fetch_one(&state.db)
            .await?;
            let td = sqlx::query(
                r#"SELECT COALESCE(SUM(upload), 0)::bigint AS up, COALESCE(SUM(download), 0)::bigint AS down
                   FROM "TrafficLog" WHERE "recordAt" >= $1"#,
            )
            .bind(today)
            .fetch_one(&state.db)
            .await?;
            let mo = sqlx::query(
                r#"SELECT COALESCE(SUM(upload), 0)::bigint AS up, COALESCE(SUM(download), 0)::bigint AS down
                   FROM "TrafficLog" WHERE "recordAt" >= $1"#,
            )
            .bind(this_month)
            .fetch_one(&state.db)
            .await?;
            (
                total,
                active,
                banned,
                exp,
                td.try_get::<i64, _>("up")?,
                td.try_get::<i64, _>("down")?,
                mo.try_get::<i64, _>("up")?,
                mo.try_get::<i64, _>("down")?,
            )
        };

    let nodes_total: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Node" WHERE active = true"#)
        .fetch_one(&state.db)
        .await?;
    let nodes_online: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "Node" WHERE active = true AND status = 'ONLINE'"#,
    )
    .fetch_one(&state.db)
    .await?;
    let inbounds_total: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Inbound" WHERE enable = true"#)
            .fetch_one(&state.db)
            .await?;

    let payload = json!({
        "clients": {
            "total": total_clients,
            "active": active_clients,
            "banned": banned_clients
        },
        "nodes": {
            "total": nodes_total,
            "online": nodes_online,
            "offline": (nodes_total - nodes_online).max(0)
        },
        "inbounds": { "total": inbounds_total },
        "expiringToday": expiring_today,
        "traffic": {
            "today": { "upload": today_up, "download": today_down },
            "month": { "upload": month_up, "download": month_down }
        }
    });

    state
        .cache
        .set_json(cache_key, payload.clone(), Duration::from_secs(60));
    Ok(Json(payload))
}

async fn traffic_chart(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<ChartQuery>,
) -> AppResult<Json<Vec<Value>>> {
    let tenant = user.tenant_scope()?;
    let days = positive_int(q.days.as_deref(), 7, 366, "days")?;
    let start = crate::db::now() - ChronoDuration::days(days);

    let mut qb = QueryBuilder::new(
        r#"SELECT date_trunc('day', t."recordAt") AS day,
                  COALESCE(SUM(t.upload), 0)::bigint AS upload,
                  COALESCE(SUM(t.download), 0)::bigint AS download
           FROM "TrafficLog" t
           JOIN "Client" c ON c.id = t."clientId"
           WHERE t."recordAt" >= "#,
    );
    qb.push_bind(start);

    if let Some(ref rid) = tenant {
        qb.push(r#" AND c."resellerId" = "#).push_bind(rid.clone());
    }
    if let Some(ref nid) = q.node_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        qb.push(r#" AND t."nodeId" = "#).push_bind(nid.to_string());
    }
    if let Some(ref cid) = q.client_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        qb.push(r#" AND t."clientId" = "#).push_bind(cid.to_string());
    }
    qb.push(r#" GROUP BY 1 ORDER BY day ASC"#);

    let rows = qb.build().fetch_all(&state.db).await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let day: NaiveDateTime = r.try_get("day")?;
        let up: i64 = r.try_get("upload")?;
        let down: i64 = r.try_get("download")?;
        out.push(json!({
            "day": crate::db::ts::format_iso(&day),
            "upload": up,
            "download": down
        }));
    }
    Ok(Json(out))
}

/// Cast `SUM(t.upload)` and `SUM(t.download)` to `::bigint` so SQLx `i64` decoding never fails on `NUMERIC`.
async fn top_clients(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<TopQuery>,
) -> AppResult<Json<Vec<Value>>> {
    let tenant = user.tenant_scope()?;
    let days = positive_int(q.days.as_deref(), 7, 366, "days")?;
    let limit = positive_int(q.limit.as_deref(), 10, 100, "limit")?;
    let start = crate::db::now() - ChronoDuration::days(days);

    let mut qb = QueryBuilder::new(
        r#"SELECT t."clientId" AS client_id,
                  COALESCE(SUM(t.upload), 0)::bigint AS upload,
                  COALESCE(SUM(t.download), 0)::bigint AS download
           FROM "TrafficLog" t
           JOIN "Client" c ON c.id = t."clientId"
           WHERE t."recordAt" >= "#,
    );
    qb.push_bind(start);
    if let Some(ref rid) = tenant {
        qb.push(r#" AND c."resellerId" = "#).push_bind(rid.clone());
    }
    qb.push(r#" GROUP BY t."clientId" ORDER BY download DESC LIMIT "#)
        .push_bind(limit);

    let rows = qb.build().fetch_all(&state.db).await?;
    if rows.is_empty() {
        return Ok(Json(Vec::new()));
    }

    let client_ids: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String, _>("client_id").ok())
        .collect();

    let client_rows = sqlx::query(
        r#"SELECT id, username, uuid FROM "Client" WHERE id = ANY($1::text[])"#,
    )
    .bind(&client_ids)
    .fetch_all(&state.db)
    .await?;

    let mut clients_map = HashMap::new();
    for cr in client_rows {
        let id: String = cr.try_get("id")?;
        clients_map.insert(
            id.clone(),
            json!({
                "id": id,
                "username": cr.try_get::<String, _>("username")?,
                "uuid": cr.try_get::<String, _>("uuid")?
            }),
        );
    }

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let cid: String = r.try_get("client_id")?;
        let up: i64 = r.try_get("upload")?;
        let down: i64 = r.try_get("download")?;
        out.push(json!({
            "clientId": cid,
            "_sum": { "upload": up, "download": down },
            "client": clients_map.get(&cid).cloned()
        }));
    }
    Ok(Json(out))
}

async fn recent_audit(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<RecentAuditQuery>,
) -> AppResult<Json<Vec<Value>>> {
    let limit = positive_int(q.limit.as_deref(), 20, 100, "limit")?;
    user.require_admin()?;

    let rows = sqlx::query(
        r#"SELECT a.id, a."userId", a.action, a.resource, a."resourceId", a.details, a.ip, a."userAgent", a."createdAt",
                  u.username AS u_username, u.role AS u_role
           FROM "AuditLog" a
           LEFT JOIN "User" u ON u.id = a."userId"
           ORDER BY a."createdAt" DESC LIMIT $1"#,
    )
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let created_at: NaiveDateTime = r.try_get("createdAt")?;
        let uid: Option<String> = r.try_get("userId")?;
        let uname: Option<String> = r.try_get("u_username")?;
        let urole: Option<UserRole> = r.try_get("u_role")?;
        let uobj = match (uid.as_ref(), uname, urole) {
            (Some(id), Some(un), Some(rl)) => json!({ "id": id, "username": un, "role": rl }),
            _ => Value::Null,
        };
        out.push(json!({
            "id": r.try_get::<String, _>("id")?,
            "userId": uid,
            "action": r.try_get::<String, _>("action")?,
            "resource": r.try_get::<String, _>("resource")?,
            "resourceId": r.try_get::<Option<String>, _>("resourceId")?,
            "details": r.try_get::<Option<Value>, _>("details")?,
            "ip": r.try_get::<Option<String>, _>("ip")?,
            "userAgent": r.try_get::<Option<String>, _>("userAgent")?,
            "createdAt": crate::db::ts::format_iso(&created_at),
            "user": uobj
        }));
    }
    Ok(Json(out))
}
