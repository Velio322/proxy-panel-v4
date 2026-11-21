use crate::api::common::{ok, valid_id, AppJson};
use crate::audit::{self, AuditEntry};
use crate::auth::extract::AuthUser;
use crate::db::enums::PlanType;
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
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreatePlanReq {
    name: String,
    description: Option<String>,
    #[serde(rename = "type")]
    plan_type: Option<PlanType>,
    price: Option<Decimal>,
    currency: Option<String>,
    duration: Option<i32>,
    traffic_limit: Option<i64>,
    max_clients: Option<i32>,
    max_speed: Option<i32>,
    protocols: Option<Value>,
    hwid_limit: Option<i32>,
    default_reset_strategy: Option<String>,
    sort_order: Option<i32>,
    active: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdatePlanReq {
    name: Option<String>,
    description: Option<String>,
    #[serde(rename = "type")]
    plan_type: Option<PlanType>,
    price: Option<Decimal>,
    currency: Option<String>,
    duration: Option<i32>,
    traffic_limit: Option<i64>,
    max_clients: Option<Option<i32>>,
    max_speed: Option<Option<i32>>,
    protocols: Option<Value>,
    hwid_limit: Option<Option<i32>>,
    default_reset_strategy: Option<String>,
    sort_order: Option<i32>,
    active: Option<bool>,
}

fn plan_json(r: &sqlx::postgres::PgRow) -> AppResult<Value> {
    let price: Decimal = r.try_get("price")?;
    let created: NaiveDateTime = r.try_get("createdAt")?;
    let updated: NaiveDateTime = r.try_get("updatedAt")?;
    Ok(json!({
        "id": r.try_get::<String, _>("id")?,
        "name": r.try_get::<String, _>("name")?,
        "description": r.try_get::<Option<String>, _>("description")?,
        "type": r.try_get::<PlanType, _>("type")?,
        "price": price.to_string(),
        "currency": r.try_get::<String, _>("currency")?,
        "duration": r.try_get::<i32, _>("duration")?,
        "trafficLimit": r.try_get::<i64, _>("trafficLimit")?,
        "maxClients": r.try_get::<Option<i32>, _>("maxClients")?,
        "maxSpeed": r.try_get::<Option<i32>, _>("maxSpeed")?,
        "protocols": r.try_get::<Value, _>("protocols")?,
        "hwidLimit": r.try_get::<Option<i32>, _>("hwidLimit")?,
        "defaultResetStrategy": r.try_get::<String, _>("defaultResetStrategy")?,
        "sortOrder": r.try_get::<i32, _>("sortOrder")?,
        "active": r.try_get::<bool, _>("active")?,
        "createdAt": crate::db::ts::format_iso(&created),
        "updatedAt": crate::db::ts::format_iso(&updated),
    }))
}

async fn list(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Vec<Value>>> {
    let rows = sqlx::query(
        r#"SELECT id, name, description, type, price, currency, duration, "trafficLimit", "maxClients",
                  "maxSpeed", protocols, "hwidLimit", "defaultResetStrategy", "sortOrder", active, "createdAt", "updatedAt"
           FROM "Plan" ORDER BY "sortOrder" ASC, "createdAt" DESC"#,
    )
    .fetch_all(&state.db)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(plan_json(&r)?);
    }
    Ok(Json(out))
}

async fn get_one(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    valid_id(&id)?;
    let r = sqlx::query(
        r#"SELECT id, name, description, type, price, currency, duration, "trafficLimit", "maxClients",
                  "maxSpeed", protocols, "hwidLimit", "defaultResetStrategy", "sortOrder", active, "createdAt", "updatedAt"
           FROM "Plan" WHERE id = $1"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Plan not found"))?;
    Ok(Json(plan_json(&r)?))
}

async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<CreatePlanReq>,
) -> AppResult<Response> {
    user.require_admin()?;
    if body.name.trim().is_empty() {
        return Err(AppError::validation("name", "Plan name is required"));
    }
    let id = crate::db::new_id();
    let now = crate::db::now();
    let r = sqlx::query(
        r#"INSERT INTO "Plan" (id, name, description, type, price, currency, duration, "trafficLimit", "maxClients", "maxSpeed", protocols, "hwidLimit", "defaultResetStrategy", "sortOrder", active, "createdAt", "updatedAt")
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $16)
           RETURNING id, name, description, type, price, currency, duration, "trafficLimit", "maxClients", "maxSpeed", protocols, "hwidLimit", "defaultResetStrategy", "sortOrder", active, "createdAt", "updatedAt""#,
    )
    .bind(&id)
    .bind(body.name.trim())
    .bind(&body.description)
    .bind(body.plan_type.unwrap_or(PlanType::User))
    .bind(body.price.unwrap_or_default())
    .bind(body.currency.unwrap_or_else(|| "USD".into()))
    .bind(body.duration.unwrap_or(30).max(1))
    .bind(body.traffic_limit.unwrap_or(0).max(0))
    .bind(body.max_clients)
    .bind(body.max_speed)
    .bind(body.protocols.unwrap_or_else(|| json!(["VLESS", "HYSTERIA2"])))
    .bind(body.hwid_limit)
    .bind(body.default_reset_strategy.unwrap_or_else(|| "NO_RESET".into()))
    .bind(body.sort_order.unwrap_or(0))
    .bind(body.active.unwrap_or(true))
    .bind(now)
    .fetch_one(&state.db)
    .await?;

    audit::spawn(
        &state.db,
        AuditEntry::new("CREATE", "plan")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    Ok((StatusCode::CREATED, Json(plan_json(&r)?)).into_response())
}

async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<UpdatePlanReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;

    let ex = sqlx::query(
        r#"SELECT name, description, type, price, currency, duration, "trafficLimit", "maxClients", "maxSpeed", protocols, "hwidLimit", "defaultResetStrategy", "sortOrder", active
           FROM "Plan" WHERE id = $1"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Plan not found"))?;

    let now = crate::db::now();
    let r = sqlx::query(
        r#"UPDATE "Plan"
           SET name = $2, description = $3, type = $4, price = $5, currency = $6, duration = $7,
               "trafficLimit" = $8, "maxClients" = $9, "maxSpeed" = $10, protocols = $11,
               "hwidLimit" = $12, "defaultResetStrategy" = $13, "sortOrder" = $14, active = $15, "updatedAt" = $16
           WHERE id = $1
           RETURNING id, name, description, type, price, currency, duration, "trafficLimit", "maxClients", "maxSpeed", protocols, "hwidLimit", "defaultResetStrategy", "sortOrder", active, "createdAt", "updatedAt""#,
    )
    .bind(&id)
    .bind(body.name.unwrap_or_else(|| ex.try_get("name").unwrap_or_default()))
    .bind(body.description.or_else(|| ex.try_get("description").ok().flatten()))
    .bind(body.plan_type.unwrap_or_else(|| ex.try_get("type").unwrap_or(PlanType::User)))
    .bind(body.price.unwrap_or_else(|| ex.try_get("price").unwrap_or_default()))
    .bind(body.currency.unwrap_or_else(|| ex.try_get("currency").unwrap_or_else(|_| "USD".into())))
    .bind(body.duration.unwrap_or_else(|| ex.try_get("duration").unwrap_or(30)))
    .bind(body.traffic_limit.unwrap_or_else(|| ex.try_get("trafficLimit").unwrap_or(0)))
    .bind(body.max_clients.unwrap_or_else(|| ex.try_get("maxClients").ok().flatten()))
    .bind(body.max_speed.unwrap_or_else(|| ex.try_get("maxSpeed").ok().flatten()))
    .bind(body.protocols.unwrap_or_else(|| ex.try_get("protocols").unwrap_or_else(|_| json!([]))))
    .bind(body.hwid_limit.unwrap_or_else(|| ex.try_get("hwidLimit").ok().flatten()))
    .bind(body.default_reset_strategy.unwrap_or_else(|| ex.try_get("defaultResetStrategy").unwrap_or_else(|_| "NO_RESET".into())))
    .bind(body.sort_order.unwrap_or_else(|| ex.try_get("sortOrder").unwrap_or(0)))
    .bind(body.active.unwrap_or_else(|| ex.try_get("active").unwrap_or(true)))
    .bind(now)
    .fetch_one(&state.db)
    .await?;

    audit::spawn(
        &state.db,
        AuditEntry::new("UPDATE", "plan")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );
    Ok(Json(plan_json(&r)?))
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
    let res = sqlx::query(r#"DELETE FROM "Plan" WHERE id = $1"#)
        .bind(&id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("Plan not found"));
    }
    audit::spawn(
        &state.db,
        AuditEntry::new("DELETE", "plan")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );
    Ok(ok())
}
