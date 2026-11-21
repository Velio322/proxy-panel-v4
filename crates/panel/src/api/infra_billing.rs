use crate::api::common::{ok, valid_id, AppJson};
use crate::auth::extract::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use axum::{Json, Router};
use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/providers", get(list_providers).post(create_provider))
        .route("/providers/{id}", put(update_provider).delete(delete_provider))
        .route("/nodes", get(list_node_billing).post(create_node_billing))
        .route("/nodes/{id}", put(update_node_billing).delete(delete_node_billing))
        .route("/summary", get(summary))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateProviderReq {
    name: String,
    website: Option<String>,
    support_url: Option<String>,
    notes: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateNodeBillingReq {
    node_id: String,
    provider_id: String,
    amount: Decimal,
    #[serde(default = "default_currency")]
    currency: String,
    #[serde(default = "default_cycle")]
    billing_cycle: String,
    next_due_date: Option<String>,
    #[serde(default = "default_true")]
    auto_renew: bool,
    notes: Option<String>,
}

fn default_currency() -> String {
    "USD".into()
}
fn default_cycle() -> String {
    "MONTHLY".into()
}
fn default_true() -> bool {
    true
}

async fn list_providers(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Vec<Value>>> {
    user.require_admin()?;
    let rows = sqlx::query(
        r#"SELECT id, name, website, "supportUrl", notes, "createdAt", "updatedAt"
           FROM "InfraProvider" ORDER BY name ASC"#,
    )
    .fetch_all(&state.db)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let ca: NaiveDateTime = r.try_get("createdAt")?;
        out.push(json!({
            "id": r.try_get::<String, _>("id")?,
            "name": r.try_get::<String, _>("name")?,
            "website": r.try_get::<Option<String>, _>("website")?,
            "supportUrl": r.try_get::<Option<String>, _>("supportUrl")?,
            "notes": r.try_get::<Option<String>, _>("notes")?,
            "createdAt": crate::db::ts::format_iso(&ca),
        }));
    }
    Ok(Json(out))
}

async fn create_provider(
    State(state): State<AppState>,
    user: AuthUser,
    AppJson(body): AppJson<CreateProviderReq>,
) -> AppResult<Response> {
    user.require_admin()?;
    let id = crate::db::new_id();
    let now = crate::db::now();
    sqlx::query(
        r#"INSERT INTO "InfraProvider" (id, name, website, "supportUrl", notes, "createdAt", "updatedAt")
           VALUES ($1, $2, $3, $4, $5, $6, $6)"#,
    )
    .bind(&id)
    .bind(body.name.trim())
    .bind(&body.website)
    .bind(&body.support_url)
    .bind(&body.notes)
    .bind(now)
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": id,
            "name": body.name.trim(),
            "website": body.website,
            "supportUrl": body.support_url,
            "notes": body.notes,
            "createdAt": crate::db::ts::format_iso(&now)
        })),
    )
        .into_response())
}

async fn delete_provider(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let res = sqlx::query(r#"DELETE FROM "InfraProvider" WHERE id = $1"#)
        .bind(&id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("Provider not found"));
    }
    Ok(ok())
}

async fn update_provider(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    AppJson(body): AppJson<CreateProviderReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let now = crate::db::now();
    let r = sqlx::query(
        r#"UPDATE "InfraProvider"
           SET name = $2, website = $3, "supportUrl" = $4, notes = $5, "updatedAt" = $6
           WHERE id = $1
           RETURNING id, name, website, "supportUrl", notes, "createdAt""#,
    )
    .bind(&id)
    .bind(body.name.trim())
    .bind(&body.website)
    .bind(&body.support_url)
    .bind(&body.notes)
    .bind(now)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Provider not found"))?;

    let ca: NaiveDateTime = r.try_get("createdAt")?;
    Ok(Json(json!({
        "id": r.try_get::<String, _>("id")?,
        "name": r.try_get::<String, _>("name")?,
        "website": r.try_get::<Option<String>, _>("website")?,
        "supportUrl": r.try_get::<Option<String>, _>("supportUrl")?,
        "notes": r.try_get::<Option<String>, _>("notes")?,
        "createdAt": crate::db::ts::format_iso(&ca),
    })))
}

async fn list_node_billing(
    State(state): State<AppState>,
    user: AuthUser,
) -> AppResult<Json<Vec<Value>>> {
    user.require_admin()?;
    let rows = sqlx::query(
        r#"SELECT b.id, b."nodeId", b."providerId", b.amount, b.currency, b."billingCycle",
                  b."nextDueDate", b."autoRenew", b.notes, n.name AS node_name, p.name AS provider_name
           FROM "InfraNodeBilling" b
           JOIN "Node" n ON n.id = b."nodeId"
           JOIN "InfraProvider" p ON p.id = b."providerId"
           ORDER BY b."nextDueDate" ASC"#,
    )
    .fetch_all(&state.db)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let amt: Decimal = r.try_get("amount")?;
        let due: NaiveDateTime = r.try_get("nextDueDate")?;
        out.push(json!({
            "id": r.try_get::<String, _>("id")?,
            "nodeId": r.try_get::<String, _>("nodeId")?,
            "nodeName": r.try_get::<String, _>("node_name")?,
            "providerId": r.try_get::<String, _>("providerId")?,
            "providerName": r.try_get::<String, _>("provider_name")?,
            "amount": amt.to_string(),
            "currency": r.try_get::<String, _>("currency")?,
            "billingCycle": r.try_get::<String, _>("billingCycle")?,
            "nextDueDate": crate::db::ts::format_iso(&due),
            "autoRenew": r.try_get::<bool, _>("autoRenew")?,
            "notes": r.try_get::<Option<String>, _>("notes")?,
        }));
    }
    Ok(Json(out))
}

async fn create_node_billing(
    State(state): State<AppState>,
    user: AuthUser,
    AppJson(body): AppJson<CreateNodeBillingReq>,
) -> AppResult<Response> {
    user.require_admin()?;
    let id = crate::db::new_id();
    let now = crate::db::now();
    let due = body
        .next_due_date
        .as_deref()
        .map(crate::db::ts::parse_iso)
        .transpose()
        .map_err(AppError::bad_request)?
        .unwrap_or(now);

    sqlx::query(
        r#"INSERT INTO "InfraNodeBilling" (id, "nodeId", "providerId", amount, currency, "billingCycle", "nextDueDate", "autoRenew", notes, "createdAt", "updatedAt")
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)"#,
    )
    .bind(&id)
    .bind(&body.node_id)
    .bind(&body.provider_id)
    .bind(body.amount)
    .bind(&body.currency)
    .bind(&body.billing_cycle)
    .bind(due)
    .bind(body.auto_renew)
    .bind(&body.notes)
    .bind(now)
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": id,
            "nodeId": body.node_id,
            "providerId": body.provider_id,
            "amount": body.amount.to_string(),
            "currency": body.currency,
            "billingCycle": body.billing_cycle,
            "nextDueDate": crate::db::ts::format_iso(&due),
            "autoRenew": body.auto_renew
        })),
    )
        .into_response())
}

async fn delete_node_billing(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let res = sqlx::query(r#"DELETE FROM "InfraNodeBilling" WHERE id = $1"#)
        .bind(&id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("Billing record not found"));
    }
    Ok(ok())
}

async fn update_node_billing(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    AppJson(body): AppJson<CreateNodeBillingReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let now = crate::db::now();
    let due = body
        .next_due_date
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(crate::db::ts::parse_iso)
        .transpose()
        .map_err(AppError::bad_request)?
        .unwrap_or(now);

    let r = sqlx::query(
        r#"UPDATE "InfraNodeBilling"
           SET "nodeId" = $2, "providerId" = $3, amount = $4, currency = $5,
               "billingCycle" = $6, "nextDueDate" = $7, "autoRenew" = $8, notes = $9, "updatedAt" = $10
           WHERE id = $1
           RETURNING id, "nodeId", "providerId", amount, currency, "billingCycle", "nextDueDate", "autoRenew", notes"#,
    )
    .bind(&id)
    .bind(&body.node_id)
    .bind(&body.provider_id)
    .bind(body.amount)
    .bind(&body.currency)
    .bind(&body.billing_cycle)
    .bind(due)
    .bind(body.auto_renew)
    .bind(&body.notes)
    .bind(now)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Billing record not found"))?;

    let amt: Decimal = r.try_get("amount")?;
    let due_dt: NaiveDateTime = r.try_get("nextDueDate")?;
    Ok(Json(json!({
        "id": r.try_get::<String, _>("id")?,
        "nodeId": r.try_get::<String, _>("nodeId")?,
        "providerId": r.try_get::<String, _>("providerId")?,
        "amount": amt.to_string(),
        "currency": r.try_get::<String, _>("currency")?,
        "billingCycle": r.try_get::<String, _>("billingCycle")?,
        "nextDueDate": crate::db::ts::format_iso(&due_dt),
        "autoRenew": r.try_get::<bool, _>("autoRenew")?,
        "notes": r.try_get::<Option<String>, _>("notes")?,
    })))
}

async fn summary(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Value>> {
    user.require_admin()?;
    let rows = sqlx::query(
        r#"SELECT currency, SUM(amount)::text AS total, COUNT(*) AS cnt
           FROM "InfraNodeBilling" GROUP BY currency"#,
    )
    .fetch_all(&state.db)
    .await?;

    let mut by_currency = Vec::with_capacity(rows.len());
    for r in rows {
        by_currency.push(json!({
            "currency": r.try_get::<String, _>("currency")?,
            "total": r.try_get::<Option<String>, _>("total")?.unwrap_or_else(|| "0".into()),
            "count": r.try_get::<i64, _>("cnt")?,
        }));
    }
    Ok(Json(json!({ "totals": by_currency })))
}
