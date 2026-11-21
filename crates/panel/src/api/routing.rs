use crate::api::common::{ok, valid_id, AppJson};
use crate::audit::{self, AuditEntry};
use crate::auth::extract::AuthUser;
use crate::error::{AppError, AppResult};
use crate::net::{user_agent, ClientIp};
use crate::services::outbox;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::NaiveDateTime;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{QueryBuilder, Row};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/presets/list", get(presets_list))
        .route("/reorder", post(reorder))
        .route("/{id}", get(get_one).put(update).delete(remove))
        .route("/{id}/toggle", post(toggle))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    node_scope: Option<String>,
    enabled: Option<String>,
    outbound_tag: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateRuleReq {
    name: String,
    description: Option<String>,
    enabled: Option<bool>,
    priority: Option<i32>,
    #[serde(rename = "type")]
    rule_type: Option<String>,
    domain: Option<Vec<String>>,
    ip: Option<Vec<String>>,
    port: Option<String>,
    source_port: Option<String>,
    source: Option<Vec<String>>,
    protocol: Option<Vec<String>>,
    inbound_tag: Option<Vec<String>>,
    outbound_tag: Option<String>,
    balancer_tag: Option<String>,
    sub_rules: Option<Value>,
    node_scope: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateRuleReq {
    name: Option<String>,
    description: Option<String>,
    enabled: Option<bool>,
    priority: Option<i32>,
    #[serde(rename = "type")]
    rule_type: Option<String>,
    domain: Option<Vec<String>>,
    ip: Option<Vec<String>>,
    port: Option<Option<String>>,
    source_port: Option<Option<String>>,
    source: Option<Vec<String>>,
    protocol: Option<Vec<String>>,
    inbound_tag: Option<Vec<String>>,
    outbound_tag: Option<String>,
    balancer_tag: Option<Option<String>>,
    sub_rules: Option<Option<Value>>,
    node_scope: Option<Option<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReorderReq {
    rule_ids: Vec<String>,
}

fn rule_json(r: &sqlx::postgres::PgRow) -> AppResult<Value> {
    let created: NaiveDateTime = r.try_get("createdAt")?;
    let updated: NaiveDateTime = r.try_get("updatedAt")?;
    Ok(json!({
        "id": r.try_get::<String, _>("id")?,
        "name": r.try_get::<String, _>("name")?,
        "description": r.try_get::<Option<String>, _>("description")?,
        "enabled": r.try_get::<bool, _>("enabled")?,
        "priority": r.try_get::<i32, _>("priority")?,
        "type": r.try_get::<String, _>("type")?,
        "domain": r.try_get::<Option<Vec<String>>, _>("domain")?.unwrap_or_default(),
        "ip": r.try_get::<Option<Vec<String>>, _>("ip")?.unwrap_or_default(),
        "port": r.try_get::<Option<String>, _>("port")?,
        "sourcePort": r.try_get::<Option<String>, _>("sourcePort")?,
        "source": r.try_get::<Option<Vec<String>>, _>("source")?.unwrap_or_default(),
        "protocol": r.try_get::<Option<Vec<String>>, _>("protocol")?.unwrap_or_default(),
        "inboundTag": r.try_get::<Option<Vec<String>>, _>("inboundTag")?.unwrap_or_default(),
        "outboundTag": r.try_get::<String, _>("outboundTag")?,
        "balancerTag": r.try_get::<Option<String>, _>("balancerTag")?,
        "subRules": r.try_get::<Option<Value>, _>("subRules")?,
        "nodeScope": r.try_get::<Option<String>, _>("nodeScope")?,
        "createdAt": crate::db::ts::format_iso(&created),
        "updatedAt": crate::db::ts::format_iso(&updated),
    }))
}

async fn list(
    State(state): State<AppState>,
    _user: AuthUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<Value>>> {
    let mut qb = QueryBuilder::new(
        r#"SELECT id, name, description, enabled, priority, "type", domain, ip, port, "sourcePort",
                  source, protocol, "inboundTag", "outboundTag", "balancerTag", "subRules", "nodeScope",
                  "createdAt", "updatedAt"
           FROM "RoutingRule" WHERE 1=1"#,
    );
    if let Some(ref ns) = q.node_scope {
        if ns == "null" {
            qb.push(r#" AND "nodeScope" IS NULL"#);
        } else {
            qb.push(r#" AND "nodeScope" = "#).push_bind(ns.clone());
        }
    }
    if let Some(ref en) = q.enabled {
        qb.push(r#" AND enabled = "#).push_bind(en == "true");
    }
    if let Some(ref ot) = q.outbound_tag.as_deref().filter(|s| !s.is_empty()) {
        qb.push(r#" AND "outboundTag" = "#).push_bind(ot.to_string());
    }
    qb.push(r#" ORDER BY priority DESC, "createdAt" ASC"#);

    let rows = qb.build().fetch_all(&state.db).await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(rule_json(&r)?);
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
        r#"SELECT id, name, description, enabled, priority, "type", domain, ip, port, "sourcePort",
                  source, protocol, "inboundTag", "outboundTag", "balancerTag", "subRules", "nodeScope",
                  "createdAt", "updatedAt"
           FROM "RoutingRule" WHERE id = $1"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Rule not found"))?;
    Ok(Json(rule_json(&r)?))
}

async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<CreateRuleReq>,
) -> AppResult<Response> {
    user.require_admin()?;
    if body.name.trim().is_empty() || body.name.len() > 100 {
        return Err(AppError::validation("name", "Rule name must be 1..=100 chars"));
    }
    let id = crate::db::new_id();
    let now = crate::db::now();
    let r = sqlx::query(
        r#"INSERT INTO "RoutingRule" (id, name, description, enabled, priority, "type", domain, ip, port, "sourcePort", source, protocol, "inboundTag", "outboundTag", "balancerTag", "subRules", "nodeScope", "createdAt", "updatedAt")
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $18)
           RETURNING id, name, description, enabled, priority, "type", domain, ip, port, "sourcePort", source, protocol, "inboundTag", "outboundTag", "balancerTag", "subRules", "nodeScope", "createdAt", "updatedAt""#,
    )
    .bind(&id)
    .bind(body.name.trim())
    .bind(&body.description)
    .bind(body.enabled.unwrap_or(true))
    .bind(body.priority.unwrap_or(0).max(0))
    .bind(body.rule_type.unwrap_or_else(|| "field".into()))
    .bind(body.domain.unwrap_or_default())
    .bind(body.ip.unwrap_or_default())
    .bind(&body.port)
    .bind(&body.source_port)
    .bind(body.source.unwrap_or_default())
    .bind(body.protocol.unwrap_or_default())
    .bind(body.inbound_tag.unwrap_or_default())
    .bind(body.outbound_tag.unwrap_or_else(|| "direct".into()))
    .bind(&body.balancer_tag)
    .bind(&body.sub_rules)
    .bind(&body.node_scope)
    .bind(now)
    .fetch_one(&state.db)
    .await?;

    outbox::broadcast_config_update(&state).await;
    audit::spawn(
        &state.db,
        AuditEntry::new("CREATE", "routing_rule")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    Ok((StatusCode::CREATED, Json(rule_json(&r)?)).into_response())
}

async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<UpdateRuleReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let ex = sqlx::query(
        r#"SELECT name, description, enabled, priority, "type", domain, ip, port, "sourcePort", source, protocol, "inboundTag", "outboundTag", "balancerTag", "subRules", "nodeScope"
           FROM "RoutingRule" WHERE id = $1"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Rule not found"))?;

    let now = crate::db::now();
    let r = sqlx::query(
        r#"UPDATE "RoutingRule"
           SET name = $2, description = $3, enabled = $4, priority = $5, "type" = $6,
               domain = $7, ip = $8, port = $9, "sourcePort" = $10, source = $11,
               protocol = $12, "inboundTag" = $13, "outboundTag" = $14, "balancerTag" = $15,
               "subRules" = $16, "nodeScope" = $17, "updatedAt" = $18
           WHERE id = $1
           RETURNING id, name, description, enabled, priority, "type", domain, ip, port, "sourcePort", source, protocol, "inboundTag", "outboundTag", "balancerTag", "subRules", "nodeScope", "createdAt", "updatedAt""#,
    )
    .bind(&id)
    .bind(body.name.unwrap_or_else(|| ex.try_get("name").unwrap_or_default()))
    .bind(body.description.or_else(|| ex.try_get("description").ok().flatten()))
    .bind(body.enabled.unwrap_or_else(|| ex.try_get("enabled").unwrap_or(true)))
    .bind(body.priority.unwrap_or_else(|| ex.try_get("priority").unwrap_or(0)))
    .bind(body.rule_type.unwrap_or_else(|| ex.try_get("type").unwrap_or_else(|_| "field".into())))
    .bind(body.domain.unwrap_or_else(|| ex.try_get("domain").unwrap_or_default()))
    .bind(body.ip.unwrap_or_else(|| ex.try_get("ip").unwrap_or_default()))
    .bind(body.port.unwrap_or_else(|| ex.try_get("port").ok().flatten()))
    .bind(body.source_port.unwrap_or_else(|| ex.try_get("sourcePort").ok().flatten()))
    .bind(body.source.unwrap_or_else(|| ex.try_get("source").unwrap_or_default()))
    .bind(body.protocol.unwrap_or_else(|| ex.try_get("protocol").unwrap_or_default()))
    .bind(body.inbound_tag.unwrap_or_else(|| ex.try_get("inboundTag").unwrap_or_default()))
    .bind(body.outbound_tag.unwrap_or_else(|| ex.try_get("outboundTag").unwrap_or_else(|_| "direct".into())))
    .bind(body.balancer_tag.unwrap_or_else(|| ex.try_get("balancerTag").ok().flatten()))
    .bind(body.sub_rules.unwrap_or_else(|| ex.try_get("subRules").ok().flatten()))
    .bind(body.node_scope.unwrap_or_else(|| ex.try_get("nodeScope").ok().flatten()))
    .bind(now)
    .fetch_one(&state.db)
    .await?;

    outbox::broadcast_config_update(&state).await;
    audit::spawn(
        &state.db,
        AuditEntry::new("UPDATE", "routing_rule")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );
    Ok(Json(rule_json(&r)?))
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
    let res = sqlx::query(r#"DELETE FROM "RoutingRule" WHERE id = $1"#)
        .bind(&id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("Rule not found"));
    }
    outbox::broadcast_config_update(&state).await;
    audit::spawn(
        &state.db,
        AuditEntry::new("DELETE", "routing_rule")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );
    Ok(ok())
}

async fn toggle(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let r = sqlx::query(
        r#"UPDATE "RoutingRule" SET enabled = NOT enabled, "updatedAt" = $2
           WHERE id = $1
           RETURNING id, name, description, enabled, priority, "type", domain, ip, port, "sourcePort", source, protocol, "inboundTag", "outboundTag", "balancerTag", "subRules", "nodeScope", "createdAt", "updatedAt""#,
    )
    .bind(&id)
    .bind(crate::db::now())
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Rule not found"))?;

    outbox::broadcast_config_update(&state).await;
    audit::spawn(
        &state.db,
        AuditEntry::new("UPDATE", "routing_rule")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );
    Ok(Json(rule_json(&r)?))
}

async fn reorder(
    State(state): State<AppState>,
    user: AuthUser,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<ReorderReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    let len = i32::try_from(body.rule_ids.len()).unwrap_or(0);
    let now = crate::db::now();
    let mut tx = state.db.begin().await?;
    for (idx, rid) in body.rule_ids.iter().enumerate() {
        let prio = len - i32::try_from(idx).unwrap_or(0);
        sqlx::query(r#"UPDATE "RoutingRule" SET priority = $2, "updatedAt" = $3 WHERE id = $1"#)
            .bind(rid)
            .bind(prio)
            .bind(now)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    outbox::broadcast_config_update(&state).await;
    audit::spawn(
        &state.db,
        AuditEntry::new("UPDATE", "routing_rule")
            .user(&user.id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );
    Ok(ok())
}

async fn presets_list(_user: AuthUser) -> Json<Value> {
    Json(json!([
        {"name":"Block Ads","description":"Block common ad domains using geosite lists","domain":["geosite:category-ads-all"],"outboundTag":"block"},
        {"name":"Block Bittorrent","description":"Block BitTorrent protocol traffic","protocol":["bittorrent"],"outboundTag":"block"},
        {"name":"Bypass Private IPs","description":"Route private/local IPs directly","ip":["geoip:private"],"outboundTag":"direct"},
        {"name":"Proxy All","description":"Route all traffic through proxy (catch-all)","domain":["geosite:geolocation-!cn"],"outboundTag":"proxy"},
        {"name":"Direct CN","description":"Route Chinese IPs directly","ip":["geoip:cn"],"outboundTag":"direct"},
        {"name":"Block Telegram","description":"Block Telegram domains","domain":["geosite:telegram"],"outboundTag":"block"}
    ]))
}
