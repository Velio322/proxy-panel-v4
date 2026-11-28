use crate::api::common::{ok, valid_id, AppJson};
use crate::audit::{self, AuditEntry};
use crate::auth::extract::AuthUser;
use crate::error::{AppError, AppResult};
use crate::net::{user_agent, ClientIp};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", get(get_one).put(update).delete(remove))
        .route("/{id}/stats", get(stats))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateResellerReq {
    name: String,
    contact_email: Option<String>,
    contact_telegram: Option<String>,
    max_clients: Option<i32>,
    max_nodes: Option<i32>,
    traffic_limit: Option<i64>,
    balance: Option<Decimal>,
    active: Option<bool>,
    white_label: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateResellerReq {
    name: Option<String>,
    contact_email: Option<String>,
    contact_telegram: Option<String>,
    max_clients: Option<i32>,
    max_nodes: Option<i32>,
    traffic_limit: Option<i64>,
    balance: Option<Decimal>,
    white_label: Option<Value>,
    active: Option<bool>,
}

async fn list(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Vec<Value>>> {
    user.require_admin()?;
    let rows = sqlx::query(
        r#"SELECT r.id, r.name, r."contactEmail", r."contactTelegram", r."maxClients", r."maxNodes",
                  r."trafficLimit", r."usedTraffic", r.balance, r."whiteLabel", r.active, r."expiresAt",
                  r."createdAt", r."updatedAt",
                  (SELECT COUNT(*) FROM "User" u WHERE u."resellerId" = r.id) AS user_count,
                  (SELECT COUNT(*) FROM "Client" c WHERE c."resellerId" = r.id) AS client_count
           FROM "Reseller" r
           ORDER BY r."createdAt" DESC"#,
    )
    .fetch_all(&state.db)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(reseller_json(&r, true)?);
    }
    Ok(Json(out))
}

async fn get_one(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    user.require_reseller()?;
    valid_id(&id)?;
    if user.is_reseller() && user.reseller_id.as_deref() != Some(id.as_str()) {
        return Err(AppError::forbidden("Forbidden"));
    }

    let r = sqlx::query(
        r#"SELECT id, name, "contactEmail", "contactTelegram", "maxClients", "maxNodes",
                  "trafficLimit", "usedTraffic", balance, "whiteLabel", active, "expiresAt",
                  "createdAt", "updatedAt"
           FROM "Reseller" WHERE id = $1"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Reseller not found"))?;

    Ok(Json(reseller_json(&r, false)?))
}

async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<CreateResellerReq>,
) -> AppResult<Response> {
    user.require_admin()?;
    if body.name.trim().is_empty() {
        return Err(AppError::validation("name", "Name is required"));
    }

    let id = crate::db::new_id();
    let now = crate::db::now();
    let max_clients = body.max_clients.unwrap_or(100).max(1);
    let max_nodes = body.max_nodes.unwrap_or(5).max(1);
    let traffic_limit = body.traffic_limit.unwrap_or(107_374_182_400).max(0);
    let balance = body.balance.unwrap_or(Decimal::ZERO);
    let active = body.active.unwrap_or(true);

    let r = sqlx::query(
        r#"INSERT INTO "Reseller" (id, name, "contactEmail", "contactTelegram", "maxClients", "maxNodes", "trafficLimit", balance, active, "whiteLabel", "createdAt", "updatedAt")
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)
           RETURNING id, name, "contactEmail", "contactTelegram", "maxClients", "maxNodes", "trafficLimit", "usedTraffic", balance, "whiteLabel", active, "expiresAt", "createdAt", "updatedAt""#,
    )
    .bind(&id)
    .bind(body.name.trim())
    .bind(&body.contact_email)
    .bind(&body.contact_telegram)
    .bind(max_clients)
    .bind(max_nodes)
    .bind(traffic_limit)
    .bind(balance)
    .bind(active)
    .bind(&body.white_label)
    .bind(now)
    .fetch_one(&state.db)
    .await?;

    audit::spawn(
        &state.db,
        AuditEntry::new("CREATE", "reseller")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    Ok((StatusCode::CREATED, Json(reseller_json(&r, false)?)).into_response())
}

async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<UpdateResellerReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;

    let existing = sqlx::query(
        r#"SELECT name, "contactEmail", "contactTelegram", "maxClients", "maxNodes", "trafficLimit", balance, "whiteLabel", active
           FROM "Reseller" WHERE id = $1"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Reseller not found"))?;

    let name = body
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| existing.try_get("name").unwrap_or_default());
    let contact_email = body
        .contact_email
        .or_else(|| existing.try_get("contactEmail").ok().flatten());
    let contact_telegram = body
        .contact_telegram
        .or_else(|| existing.try_get("contactTelegram").ok().flatten());
    let max_clients = body
        .max_clients
        .unwrap_or_else(|| existing.try_get("maxClients").unwrap_or(100));
    let max_nodes = body
        .max_nodes
        .unwrap_or_else(|| existing.try_get("maxNodes").unwrap_or(5));
    let traffic_limit = body
        .traffic_limit
        .unwrap_or_else(|| existing.try_get("trafficLimit").unwrap_or(0));
    let balance = body
        .balance
        .unwrap_or_else(|| existing.try_get("balance").unwrap_or(Decimal::ZERO));
    let white_label = body
        .white_label
        .or_else(|| existing.try_get("whiteLabel").ok().flatten());
    let active = body
        .active
        .unwrap_or_else(|| existing.try_get("active").unwrap_or(true));
    let now = crate::db::now();

    let r = sqlx::query(
        r#"UPDATE "Reseller"
           SET name = $2, "contactEmail" = $3, "contactTelegram" = $4, "maxClients" = $5,
               "maxNodes" = $6, "trafficLimit" = $7, balance = $8, "whiteLabel" = $9, active = $10, "updatedAt" = $11
           WHERE id = $1
           RETURNING id, name, "contactEmail", "contactTelegram", "maxClients", "maxNodes", "trafficLimit", "usedTraffic", balance, "whiteLabel", active, "expiresAt", "createdAt", "updatedAt""#,
    )
    .bind(&id)
    .bind(name)
    .bind(contact_email)
    .bind(contact_telegram)
    .bind(max_clients)
    .bind(max_nodes)
    .bind(traffic_limit)
    .bind(balance)
    .bind(white_label)
    .bind(active)
    .bind(now)
    .fetch_one(&state.db)
    .await?;

    audit::spawn(
        &state.db,
        AuditEntry::new("UPDATE", "reseller")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    Ok(Json(reseller_json(&r, false)?))
}

async fn remove(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;

    let active_clients: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "Client" WHERE "resellerId" = $1 AND banned = false"#,
    )
    .bind(&id)
    .fetch_one(&state.db)
    .await?;
    if active_clients > 0 {
        return Err(AppError::bad_request(format!(
            "Cannot delete reseller with {active_clients} active clients"
        )));
    }

    let res = sqlx::query(r#"DELETE FROM "Reseller" WHERE id = $1"#)
        .bind(&id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("Reseller not found"));
    }

    audit::spawn(
        &state.db,
        AuditEntry::new("DELETE", "reseller")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );
    Ok(ok())
}

async fn stats(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    user.require_reseller()?;
    valid_id(&id)?;
    if user.is_reseller() && user.reseller_id.as_deref() != Some(id.as_str()) {
        return Err(AppError::forbidden("Forbidden"));
    }

    let reseller = sqlx::query(
        r#"SELECT "maxClients", "trafficLimit", "usedTraffic", balance FROM "Reseller" WHERE id = $1"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Reseller not found"))?;

    let total_clients: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Client" WHERE "resellerId" = $1"#)
            .bind(&id)
            .fetch_one(&state.db)
            .await?;
    let active_clients: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "Client" WHERE "resellerId" = $1 AND banned = false"#,
    )
    .bind(&id)
    .fetch_one(&state.db)
    .await?;

    let today = crate::db::now()
        .date()
        .and_hms_opt(0, 0, 0)
        .unwrap_or_else(crate::db::now);

    let today_row = sqlx::query(
        r#"SELECT COALESCE(SUM(t.upload), 0)::bigint AS up, COALESCE(SUM(t.download), 0)::bigint AS down
           FROM "TrafficLog" t JOIN "Client" c ON c.id = t."clientId"
           WHERE c."resellerId" = $1 AND t."recordAt" >= $2"#,
    )
    .bind(&id)
    .bind(today)
    .fetch_one(&state.db)
    .await?;

    let total_row = sqlx::query(
        r#"SELECT COALESCE(SUM(t.upload), 0)::bigint AS up, COALESCE(SUM(t.download), 0)::bigint AS down
           FROM "TrafficLog" t JOIN "Client" c ON c.id = t."clientId"
           WHERE c."resellerId" = $1"#,
    )
    .bind(&id)
    .fetch_one(&state.db)
    .await?;

    let bal: Decimal = reseller.try_get("balance")?;

    Ok(Json(json!({
        "clients": { "total": total_clients, "active": active_clients },
        "traffic": {
            "today": {
                "upload": today_row.try_get::<i64, _>("up")?,
                "download": today_row.try_get::<i64, _>("down")?
            },
            "total": {
                "upload": total_row.try_get::<i64, _>("up")?,
                "download": total_row.try_get::<i64, _>("down")?
            }
        },
        "limits": {
            "maxClients": reseller.try_get::<i32, _>("maxClients")?,
            "trafficLimit": reseller.try_get::<i64, _>("trafficLimit")?,
            "usedTraffic": reseller.try_get::<i64, _>("usedTraffic")?,
        },
        "balance": bal.to_string()
    })))
}

fn reseller_json(r: &sqlx::postgres::PgRow, include_counts: bool) -> AppResult<Value> {
    let created: NaiveDateTime = r.try_get("createdAt")?;
    let updated: NaiveDateTime = r.try_get("updatedAt")?;
    let expires: Option<NaiveDateTime> = r.try_get("expiresAt")?;
    let bal: Decimal = r.try_get("balance")?;
    let mut obj = json!({
        "id": r.try_get::<String, _>("id")?,
        "name": r.try_get::<String, _>("name")?,
        "contactEmail": r.try_get::<Option<String>, _>("contactEmail")?,
        "contactTelegram": r.try_get::<Option<String>, _>("contactTelegram")?,
        "maxClients": r.try_get::<i32, _>("maxClients")?,
        "maxNodes": r.try_get::<i32, _>("maxNodes")?,
        "trafficLimit": r.try_get::<i64, _>("trafficLimit")?,
        "usedTraffic": r.try_get::<i64, _>("usedTraffic")?,
        "balance": bal.to_string(),
        "whiteLabel": r.try_get::<Option<Value>, _>("whiteLabel")?,
        "active": r.try_get::<bool, _>("active")?,
        "expiresAt": expires.map(|d| crate::db::ts::format_iso(&d)),
        "createdAt": crate::db::ts::format_iso(&created),
        "updatedAt": crate::db::ts::format_iso(&updated),
    });
    if include_counts {
        if let Some(map) = obj.as_object_mut() {
            map.insert(
                "_count".into(),
                json!({
                    "users": r.try_get::<i64, _>("user_count")?,
                    "clients": r.try_get::<i64, _>("client_count")?
                }),
            );
        }
    }
    Ok(obj)
}
