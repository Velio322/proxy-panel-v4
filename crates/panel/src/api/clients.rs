use crate::api::common::{like_pattern, ok, positive_int, valid_id, AppJson};
use crate::audit::{self, AuditEntry};
use crate::auth::extract::AuthUser;
use crate::auth::password::hash_password;
use crate::db::enums::{DbProtocol, Protocol, TrafficResetStrategy};
use crate::error::{AppError, AppResult};
use crate::net::{user_agent, ClientIp};
use crate::services::{enforcement, outbox, protocol_credentials};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{Duration as ChronoDuration, NaiveDateTime};
use mypanel_proto::links::{build_entry, EntryContext};
use mypanel_proto::openwrt::{
    generate_openclash_uci, generate_openclash_yaml_with_ts, generate_passwall_subscription,
    generate_singbox_subscription, generate_ssr_subscription, InboundExport,
};
use mypanel_proto::subscription::render_json as render_sub_json;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Postgres, QueryBuilder, Row, Transaction};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/bulk", post(bulk_action))
        .route("/bulk/delete", post(bulk_delete))
        .route("/bulk/ban", post(bulk_ban))
        .route("/import", post(import_clients))
        .route("/import/csv", post(import_csv))
        .route("/migrate", post(migrate_clients))
        .route("/{id}", get(get_one).put(update).delete(remove))
        .route("/{id}/reset-traffic", post(reset_traffic))
        .route("/{id}/toggle-ban", post(toggle_ban))
        .route("/{id}/stats", get(stats))
        .route("/{id}/traffic-daily", get(traffic_daily))
        .route(
            "/{id}/inbounds",
            get(get_client_inbounds).post(bind_inbounds).put(bind_inbounds),
        )
        .route(
            "/{id}/nodes",
            get(get_client_nodes).post(set_client_nodes).put(set_client_nodes),
        )
        .route("/{id}/devices", get(list_devices).delete(reset_devices))
        .route("/{id}/devices/{hwid}", delete(delete_device))
        .route("/{id}/export/openclash", get(export_openclash))
        .route("/{id}/export/passwall", get(export_passwall))
        .route("/{id}/export/uci", get(export_uci))
        .route("/{id}/export/singbox", get(export_singbox))
        .route("/{id}/export/ssr", get(export_ssr))
        .route("/{id}/links", get(get_links))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    search: Option<String>,
    page: Option<String>,
    limit: Option<String>,
    reseller_id: Option<String>,
    banned: Option<String>,
    protocol: Option<String>,
    node_id: Option<String>,
    pinned_only: Option<String>,
    sort: Option<String>,
    order: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateClientReq {
    username: String,
    email: Option<String>,
    password: Option<String>,
    traffic_limit: Option<i64>,
    expire_at: Option<String>,
    note: Option<String>,
    protocols: Option<Vec<String>>,
    all_nodes: Option<bool>,
    node_ids: Option<Vec<String>>,
    reseller_id: Option<String>,
    restriction_mode: Option<String>,
    traffic_reset_day: Option<i32>,
    reset_strategy: Option<String>,
    plan_id: Option<String>,
    device_limit: Option<i32>,
    on_hold: Option<bool>,
    on_hold_duration_days: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateClientReq {
    email: Option<Option<String>>,
    password: Option<String>,
    traffic_limit: Option<i64>,
    expire_at: Option<Option<String>>,
    banned: Option<bool>,
    restriction_mode: Option<String>,
    traffic_reset_day: Option<Option<i32>>,
    reset_strategy: Option<String>,
    note: Option<Option<String>>,
    protocols: Option<Vec<String>>,
    all_nodes: Option<bool>,
    node_ids: Option<Vec<String>>,
    plan_id: Option<Option<String>>,
    device_limit: Option<Option<i32>>,
    on_hold: Option<bool>,
    on_hold_duration_days: Option<Option<i32>>,
}

#[derive(Debug, Deserialize)]
struct DaysQuery {
    days: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BulkIdsReq {
    ids: Vec<String>,
    #[serde(default)]
    banned: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct CsvImportReq {
    csv: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BindInboundsReq {
    inbound_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetClientNodesReq {
    all_nodes: Option<bool>,
    node_ids: Option<Vec<String>>,
    sync_inbounds: Option<bool>,
}

fn extract_protocols_from_json(val: &Value) -> Vec<String> {
    val.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

async fn normalize_and_validate_node_ids(
    tx: &mut Transaction<'_, Postgres>,
    raw: &[String],
) -> AppResult<Vec<String>> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for r in raw {
        let trimmed = r.trim();
        if trimmed.is_empty() {
            continue;
        }
        valid_id(trimmed)?;
        if seen.insert(trimmed.to_string()) {
            out.push(trimmed.to_string());
        }
    }
    if !out.is_empty() {
        let count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Node" WHERE id = ANY($1::text[])"#)
                .bind(&out)
                .fetch_one(&mut **tx)
                .await?;
        if count != i64::try_from(out.len()).unwrap_or(i64::MAX) {
            return Err(AppError::bad_request("One or more nodeIds do not exist"));
        }
    }
    Ok(out)
}

fn resolve_node_scope(all_nodes: Option<bool>, node_ids: Option<&[String]>) -> (bool, Vec<String>) {
    match (all_nodes, node_ids) {
        (Some(true), _) => (true, Vec::new()),
        (_, Some(ids)) if !ids.is_empty() => (false, ids.to_vec()),
        (Some(false), _) => (false, Vec::new()),
        _ => (true, Vec::new()),
    }
}

async fn collect_client_affected_nodes_tx(
    tx: &mut Transaction<'_, Postgres>,
    client_id: &str,
) -> AppResult<Vec<String>> {
    let rows: Vec<String> = sqlx::query_scalar(
        r#"SELECT DISTINCT nid FROM (
               SELECT i."nodeId" AS nid
               FROM "ClientInbound" ci
               JOIN "Inbound" i ON i.id = ci."inboundId"
               WHERE ci."clientId" = $1
               UNION
               SELECT cn."nodeId" AS nid
               FROM "ClientNode" cn
               WHERE cn."clientId" = $1
           ) sub"#,
    )
    .bind(client_id)
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows)
}

#[allow(clippy::too_many_arguments)]
async fn sync_client_nodes_and_inbounds_tx(
    tx: &mut Transaction<'_, Postgres>,
    client_id: &str,
    client_uuid: &str,
    protocols: &[String],
    all_nodes: bool,
    node_ids: &[String],
    sync_inbounds: bool,
    now: NaiveDateTime,
) -> AppResult<Vec<String>> {
    let mut affected = collect_client_affected_nodes_tx(tx, client_id).await?;

    sqlx::query(r#"UPDATE "Client" SET "allNodes" = $2, "updatedAt" = $3 WHERE id = $1"#)
        .bind(client_id)
        .bind(all_nodes)
        .bind(now)
        .execute(&mut **tx)
        .await?;

    sqlx::query(r#"DELETE FROM "ClientNode" WHERE "clientId" = $1"#)
        .bind(client_id)
        .execute(&mut **tx)
        .await?;

    if !all_nodes && !node_ids.is_empty() {
        for nid in node_ids {
            sqlx::query(
                r#"INSERT INTO "ClientNode" (id, "clientId", "nodeId", "createdAt")
                   VALUES ($1, $2, $3, $4)
                   ON CONFLICT ("clientId", "nodeId") DO NOTHING"#,
            )
            .bind(crate::db::new_id())
            .bind(client_id)
            .bind(nid)
            .bind(now)
            .execute(&mut **tx)
            .await?;
        }
    }

    if sync_inbounds {
        if !all_nodes {
            if node_ids.is_empty() {
                sqlx::query(r#"DELETE FROM "ClientInbound" WHERE "clientId" = $1"#)
                    .bind(client_id)
                    .execute(&mut **tx)
                    .await?;
            } else {
                sqlx::query(
                    r#"DELETE FROM "ClientInbound" ci
                       USING "Inbound" i
                       WHERE ci."inboundId" = i.id
                         AND ci."clientId" = $1
                         AND NOT (i."nodeId" = ANY($2::text[]))"#,
                )
                .bind(client_id)
                .bind(node_ids)
                .execute(&mut **tx)
                .await?;

                if !protocols.is_empty() {
                    sqlx::query(
                        r#"INSERT INTO "ClientInbound" (id, "clientId", "inboundId", uuid, enable, "createdAt")
                           SELECT gen_random_uuid()::text, $1, i.id, $2, true, $3
                           FROM "Inbound" i
                           WHERE i.enable = true
                             AND i."nodeId" = ANY($4::text[])
                             AND i.protocol::text = ANY($5::text[])
                           ON CONFLICT ("clientId", "inboundId") DO NOTHING"#,
                    )
                    .bind(client_id)
                    .bind(client_uuid)
                    .bind(now)
                    .bind(node_ids)
                    .bind(protocols)
                    .execute(&mut **tx)
                    .await?;
                }
            }
        } else if !protocols.is_empty() {
            sqlx::query(
                r#"INSERT INTO "ClientInbound" (id, "clientId", "inboundId", uuid, enable, "createdAt")
                   SELECT gen_random_uuid()::text, $1, i.id, $2, true, $3
                   FROM "Inbound" i
                   WHERE i.enable = true
                     AND i.protocol::text = ANY($4::text[])
                   ON CONFLICT ("clientId", "inboundId") DO NOTHING"#,
            )
            .bind(client_id)
            .bind(client_uuid)
            .bind(now)
            .bind(protocols)
            .execute(&mut **tx)
            .await?;
        }
    }

    let after = collect_client_affected_nodes_tx(tx, client_id).await?;
    for nid in after {
        if !affected.contains(&nid) {
            affected.push(nid);
        }
    }
    Ok(affected)
}

async fn attach_client_nodes(pool: &sqlx::PgPool, items: &mut [Value]) -> AppResult<()> {
    if items.is_empty() {
        return Ok(());
    }
    let client_ids: Vec<String> = items
        .iter()
        .filter_map(|v| v.get("id").and_then(Value::as_str).map(ToString::to_string))
        .collect();
    if client_ids.is_empty() {
        return Ok(());
    }

    let rows = sqlx::query(
        r#"SELECT cn."clientId" AS client_id, n.id AS node_id, n.name, n.host,
                  n.status::text AS status, n.country
           FROM "ClientNode" cn
           JOIN "Node" n ON n.id = cn."nodeId"
           WHERE cn."clientId" = ANY($1::text[])
           ORDER BY n.name ASC"#,
    )
    .bind(&client_ids)
    .fetch_all(pool)
    .await?;

    let mut map_ids: HashMap<String, Vec<String>> = HashMap::new();
    let mut map_nodes: HashMap<String, Vec<Value>> = HashMap::new();
    for r in rows {
        let cid: String = r.try_get("client_id")?;
        let nid: String = r.try_get("node_id")?;
        let name: String = r.try_get("name")?;
        let host: String = r.try_get("host")?;
        let status: Option<String> = r.try_get("status").ok();
        let country: Option<String> = r.try_get("country").ok().flatten();
        map_ids.entry(cid.clone()).or_default().push(nid.clone());
        map_nodes.entry(cid).or_default().push(json!({
            "id": nid,
            "name": name,
            "host": host,
            "status": status,
            "country": country,
        }));
    }

    for item in items.iter_mut() {
        if let Some(m) = item.as_object_mut() {
            let cid = m
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let ids = map_ids.remove(&cid).unwrap_or_default();
            let nodes = map_nodes.remove(&cid).unwrap_or_default();
            m.insert("nodeIds".into(), json!(ids));
            m.insert("nodes".into(), json!(nodes));
        }
    }
    Ok(())
}

/// Validates `Client.protocols` against the 10-variant `Protocol` enum.
fn validate_protocols(raw: &[String]) -> AppResult<Vec<String>> {
    let mut out = Vec::with_capacity(raw.len());
    for p in raw {
        let parsed = Protocol::from_str(p)
            .map_err(|_| AppError::validation("protocols", format!("Unsupported protocol: {p}")))?;
        let s = parsed.as_str().to_string();
        if !out.contains(&s) {
            out.push(s);
        }
    }
    Ok(out)
}

/// Resyncs `ClientInbound` bindings when `Client.protocols` is updated (scoped by node binding).
async fn resync_bindings(
    tx: &mut Transaction<'_, Postgres>,
    client_id: &str,
    client_uuid: &str,
    old_protocols: &[String],
    new_protocols: &[String],
    now: NaiveDateTime,
) -> AppResult<()> {
    if new_protocols.is_empty() {
        sqlx::query(r#"DELETE FROM "ClientInbound" WHERE "clientId" = $1"#)
            .bind(client_id)
            .execute(&mut **tx)
            .await?;
        return Ok(());
    }
    sqlx::query(
        r#"DELETE FROM "ClientInbound" ci
           USING "Inbound" i
           WHERE ci."inboundId" = i.id
             AND ci."clientId" = $1
             AND NOT (i.protocol::text = ANY($2::text[]))"#,
    )
    .bind(client_id)
    .bind(new_protocols)
    .execute(&mut **tx)
    .await?;

    let added: Vec<String> = new_protocols
        .iter()
        .filter(|p| !old_protocols.iter().any(|o| o.eq_ignore_ascii_case(p)))
        .cloned()
        .collect();
    if !added.is_empty() {
        sqlx::query(
            r#"INSERT INTO "ClientInbound" (id, "clientId", "inboundId", uuid, enable, "createdAt")
               SELECT gen_random_uuid()::text, c.id, i.id, $2, true, $3
               FROM "Inbound" i
               JOIN "Client" c ON c.id = $1
               WHERE i.enable = true
                 AND i.protocol::text = ANY($4::text[])
                 AND (
                   c."allNodes" = true
                   OR EXISTS (
                     SELECT 1 FROM "ClientNode" cn
                     WHERE cn."clientId" = c.id AND cn."nodeId" = i."nodeId"
                   )
                 )
               ON CONFLICT ("clientId", "inboundId") DO NOTHING"#,
        )
        .bind(client_id)
        .bind(client_uuid)
        .bind(now)
        .bind(&added)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// Transactional reseller client-count + traffic-limit check with `FOR UPDATE`.
async fn check_reseller_limits_tx(
    tx: &mut Transaction<'_, Postgres>,
    reseller_id: &str,
) -> AppResult<()> {
    let row = sqlx::query(
        r#"SELECT "maxClients", "trafficLimit", "usedTraffic", active
           FROM "Reseller" WHERE id = $1 FOR UPDATE"#,
    )
    .bind(reseller_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| AppError::bad_request("Reseller not found"))?;

    let active: bool = row.try_get("active")?;
    if !active {
        return Err(AppError::forbidden("Reseller account is inactive"));
    }
    let max_clients: i32 = row.try_get("maxClients")?;
    let traffic_limit: i64 = row.try_get("trafficLimit")?;
    let used_traffic: i64 = row.try_get("usedTraffic")?;

    let current_clients: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Client" WHERE "resellerId" = $1"#)
            .bind(reseller_id)
            .fetch_one(&mut **tx)
            .await?;
    if current_clients >= i64::from(max_clients) {
        return Err(AppError::forbidden("Reseller client limit reached"));
    }
    if traffic_limit > 0 && used_traffic >= traffic_limit {
        return Err(AppError::forbidden("Reseller traffic limit exceeded"));
    }
    Ok(())
}

async fn ensure_client_access(
    state: &AppState,
    user: &AuthUser,
    client_id: &str,
) -> AppResult<()> {
    if let Some(rid) = user.tenant_scope()? {
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "Client" WHERE id = $1 AND "resellerId" = $2"#,
        )
        .bind(client_id)
        .bind(&rid)
        .fetch_one(&state.db)
        .await?;
        if count == 0 {
            return Err(AppError::not_found("Client not found"));
        }
    }
    Ok(())
}

fn client_row_to_json(r: &sqlx::postgres::PgRow) -> AppResult<Value> {
    let expire_at: Option<NaiveDateTime> = r.try_get("expireAt")?;
    let restricted_at: Option<NaiveDateTime> = r.try_get("restrictedAt")?;
    let next_reset: Option<NaiveDateTime> = r.try_get("nextTrafficResetAt")?;
    let last_reset: Option<NaiveDateTime> = r.try_get("lastTrafficResetAt")?;
    let last_active: Option<NaiveDateTime> = r.try_get("lastActiveAt")?;
    let activated_at: Option<NaiveDateTime> = r.try_get("activatedAt").ok().flatten();
    let created_at: NaiveDateTime = r.try_get("createdAt")?;
    let updated_at: NaiveDateTime = r.try_get("updatedAt")?;

    Ok(json!({
        "id": r.try_get::<String, _>("id")?,
        "uuid": r.try_get::<String, _>("uuid")?,
        "username": r.try_get::<String, _>("username")?,
        "email": r.try_get::<Option<String>, _>("email")?,
        "trafficLimit": r.try_get::<i64, _>("trafficLimit")?,
        "usedTraffic": r.try_get::<i64, _>("usedTraffic")?,
        "uploadTraffic": r.try_get::<i64, _>("uploadTraffic")?,
        "downloadTraffic": r.try_get::<i64, _>("downloadTraffic")?,
        "expireAt": expire_at.map(|d| crate::db::ts::format_iso(&d)),
        "banned": r.try_get::<bool, _>("banned")?,
        "restrictionMode": r.try_get::<String, _>("restrictionMode")?,
        "restrictionReason": r.try_get::<Option<String>, _>("restrictionReason")?,
        "restrictedAt": restricted_at.map(|d| crate::db::ts::format_iso(&d)),
        "trafficResetDay": r.try_get::<Option<i32>, _>("trafficResetDay")?,
        "nextTrafficResetAt": next_reset.map(|d| crate::db::ts::format_iso(&d)),
        "lastTrafficResetAt": last_reset.map(|d| crate::db::ts::format_iso(&d)),
        "resetStrategy": r.try_get::<Option<String>, _>("resetStrategy").ok().flatten().unwrap_or_else(|| "NO_RESET".into()),
        "accessRevision": r.try_get::<i32, _>("accessRevision")?,
        "note": r.try_get::<Option<String>, _>("note")?,
        "telegramId": r.try_get::<Option<i64>, _>("telegramId")?,
        "subId": r.try_get::<String, _>("subId")?,
        "subToken": r.try_get::<String, _>("subToken")?,
        "protocols": r.try_get::<Value, _>("protocols")?,
        "allNodes": r.try_get::<bool, _>("allNodes").unwrap_or(true),
        "planId": r.try_get::<Option<String>, _>("planId").ok().flatten(),
        "deviceLimit": r.try_get::<Option<i32>, _>("deviceLimit").ok().flatten(),
        "onHold": r.try_get::<Option<bool>, _>("onHold").ok().flatten().unwrap_or(false),
        "onHoldDurationDays": r.try_get::<Option<i32>, _>("onHoldDurationDays").ok().flatten(),
        "activatedAt": activated_at.map(|d| crate::db::ts::format_iso(&d)),
        "lastActiveAt": last_active.map(|d| crate::db::ts::format_iso(&d)),
        "lastLoginIp": r.try_get::<Option<String>, _>("lastLoginIp")?,
        "createdAt": crate::db::ts::format_iso(&created_at),
        "updatedAt": crate::db::ts::format_iso(&updated_at),
        "resellerId": r.try_get::<Option<String>, _>("resellerId")?,
    }))
}

async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Value>> {
    let tenant = user.tenant_scope()?;
    let page = positive_int(q.page.as_deref(), 1, 100_000, "page")?;
    let limit = positive_int(q.limit.as_deref(), 20, 100, "limit")?;
    let offset = (page - 1) * limit;

    let sort_col = match q.sort.as_deref().unwrap_or("createdAt") {
        "updatedAt" => r#"c."updatedAt""#,
        "username" => "c.username",
        "expireAt" => r#"c."expireAt""#,
        "usedTraffic" => r#"c."usedTraffic""#,
        "trafficLimit" => r#"c."trafficLimit""#,
        "lastActiveAt" => r#"c."lastActiveAt""#,
        _ => r#"c."createdAt""#,
    };
    let sort_dir = if q.order.as_deref().is_some_and(|o| o.eq_ignore_ascii_case("asc")) {
        "ASC"
    } else {
        "DESC"
    };

    let mut count_qb = QueryBuilder::new(r#"SELECT COUNT(*) FROM "Client" c WHERE 1=1"#);
    let mut data_qb = QueryBuilder::new(
        r#"SELECT c.id, c.uuid, c.username, c.email, c."trafficLimit", c."usedTraffic",
                  c."uploadTraffic", c."downloadTraffic", c."expireAt", c.banned,
                  c."restrictionMode", c."restrictionReason", c."restrictedAt",
                  c."trafficResetDay", c."nextTrafficResetAt", c."lastTrafficResetAt",
                  c."resetStrategy", c."accessRevision", c.note, c."telegramId",
                  c."subId", c."subToken", c.protocols, c."allNodes", c."planId", c."deviceLimit",
                  c."onHold", c."onHoldDurationDays", c."activatedAt",
                  c."lastActiveAt", c."lastLoginIp", c."createdAt", c."updatedAt", c."resellerId",
                  r.name AS reseller_name
           FROM "Client" c
           LEFT JOIN "Reseller" r ON r.id = c."resellerId"
           WHERE 1=1"#,
    );

    if let Some(ref rid) = tenant {
        for qb in [&mut count_qb, &mut data_qb] {
            qb.push(r#" AND c."resellerId" = "#).push_bind(rid.clone());
        }
    } else if let Some(ref rid) = q
        .reseller_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        for qb in [&mut count_qb, &mut data_qb] {
            qb.push(r#" AND c."resellerId" = "#)
                .push_bind(rid.to_string());
        }
    }

    if let Some(ref s) = q.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let pat = like_pattern(s);
        for qb in [&mut count_qb, &mut data_qb] {
            qb.push(r#" AND (c.username ILIKE "#)
                .push_bind(pat.clone())
                .push(r#" OR c.email ILIKE "#)
                .push_bind(pat.clone())
                .push(r#" OR c.uuid ILIKE "#)
                .push_bind(pat.clone())
                .push(")");
        }
    }

    if let Some(ref b) = q.banned {
        let bv = b == "true";
        for qb in [&mut count_qb, &mut data_qb] {
            qb.push(r#" AND c.banned = "#).push_bind(bv);
        }
    }

    if let Some(ref proto) = q.protocol.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let upper = proto.to_ascii_uppercase();
        let parsed = Protocol::from_str(&upper).map_err(|_| {
            AppError::validation("protocol", format!("Unsupported protocol: {proto}"))
        })?;
        let proto_str = parsed.as_str().to_string();
        for qb in [&mut count_qb, &mut data_qb] {
            qb.push(r#" AND c.protocols::jsonb @> jsonb_build_array("#)
                .push_bind(proto_str.clone())
                .push("::text)");
        }
    }

    if let Some(ref raw_nid) = q.node_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        match *raw_nid {
            "__unpinned__" => {
                for qb in [&mut count_qb, &mut data_qb] {
                    qb.push(r#" AND c."allNodes" = true"#);
                }
            }
            "__pinned__" => {
                for qb in [&mut count_qb, &mut data_qb] {
                    qb.push(r#" AND c."allNodes" = false"#);
                }
            }
            "__no_nodes__" => {
                for qb in [&mut count_qb, &mut data_qb] {
                    qb.push(
                        r#" AND c."allNodes" = false AND NOT EXISTS (SELECT 1 FROM "ClientNode" cn WHERE cn."clientId" = c.id)"#,
                    );
                }
            }
            nid => {
                valid_id(nid)?;
                let pinned_only = q.pinned_only.as_deref() == Some("true");
                for qb in [&mut count_qb, &mut data_qb] {
                    if pinned_only {
                        qb.push(
                            r#" AND c."allNodes" = false AND EXISTS (SELECT 1 FROM "ClientNode" cn WHERE cn."clientId" = c.id AND cn."nodeId" = "#,
                        )
                        .push_bind(nid.to_string())
                        .push(")");
                    } else {
                        qb.push(
                            r#" AND (EXISTS (SELECT 1 FROM "ClientNode" cn WHERE cn."clientId" = c.id AND cn."nodeId" = "#,
                        )
                        .push_bind(nid.to_string())
                        .push(
                            r#") OR (c."allNodes" = true AND EXISTS (SELECT 1 FROM "ClientInbound" ci JOIN "Inbound" i ON i.id = ci."inboundId" WHERE ci."clientId" = c.id AND ci.enable = true AND i."nodeId" = "#,
                        )
                        .push_bind(nid.to_string())
                        .push(")))");
                    }
                }
            }
        }
    }

    let total: i64 = count_qb.build_query_scalar().fetch_one(&state.db).await?;
    data_qb
        .push(format!(" ORDER BY {sort_col} {sort_dir} LIMIT "))
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);

    let rows = data_qb.build().fetch_all(&state.db).await?;
    let mut data = Vec::with_capacity(rows.len());
    for r in rows {
        let mut obj = client_row_to_json(&r)?;
        let rname: Option<String> = r.try_get("reseller_name")?;
        let rid: Option<String> = r.try_get("resellerId")?;
        if let Some(m) = obj.as_object_mut() {
            m.insert(
                "reseller".into(),
                match (rid, rname) {
                    (Some(id), Some(name)) => json!({ "id": id, "name": name }),
                    _ => Value::Null,
                },
            );
        }
        data.push(obj);
    }
    attach_client_nodes(&state.db, &mut data).await?;

    let pages = ((total + limit - 1) / limit).max(1);
    Ok(Json(json!({
        "data": data,
        "total": total,
        "page": page,
        "pages": pages,
        "limit": limit
    })))
}

async fn get_one(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    valid_id(&id)?;
    ensure_client_access(&state, &user, &id).await?;

    let r = sqlx::query(
        r#"SELECT c.id, c.uuid, c.username, c.email, c."trafficLimit", c."usedTraffic",
                  c."uploadTraffic", c."downloadTraffic", c."expireAt", c.banned,
                  c."restrictionMode", c."restrictionReason", c."restrictedAt",
                  c."trafficResetDay", c."nextTrafficResetAt", c."lastTrafficResetAt",
                  c."resetStrategy", c."accessRevision", c.note, c."telegramId",
                  c."subId", c."subToken", c.protocols, c."allNodes", c."planId", c."deviceLimit",
                  c."onHold", c."onHoldDurationDays", c."activatedAt",
                  c."lastActiveAt", c."lastLoginIp", c."createdAt", c."updatedAt", c."resellerId"
           FROM "Client" c WHERE c.id = $1"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Client not found"))?;

    let mut obj = client_row_to_json(&r)?;
    attach_client_nodes(&state.db, std::slice::from_mut(&mut obj)).await?;
    Ok(Json(obj))
}

async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<CreateClientReq>,
) -> AppResult<Response> {
    user.require_admin()?;
    let uname = body.username.trim();
    if uname.len() < 3 || uname.len() > 50 {
        return Err(AppError::validation(
            "username",
            "Username must be 3 to 50 characters",
        ));
    }

    let protocols = match body.protocols {
        Some(ref p) => validate_protocols(p)?,
        None => vec!["VLESS".to_string(), "HYSTERIA2".to_string()],
    };

    let expire_at = body
        .expire_at
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(crate::db::ts::parse_iso)
        .transpose()
        .map_err(|e| AppError::validation("expireAt", e))?;

    let strategy_str = body
        .reset_strategy
        .as_deref()
        .unwrap_or(if body.traffic_reset_day.is_some() {
            "MONTH"
        } else {
            "NO_RESET"
        });
    let strategy = TrafficResetStrategy::from_str(strategy_str)
        .map_err(|e| AppError::validation("resetStrategy", e))?;
    let now = crate::db::now();
    let next_reset = enforcement::calculate_next_reset(strategy, body.traffic_reset_day, now);

    let uuid = crate::db::new_id();
    let plain_pw = body
        .password
        .as_deref()
        .filter(|s| s.len() >= 6)
        .map(ToString::to_string)
        .unwrap_or_else(|| uuid.replace('-', "").chars().take(16).collect());
    let pw_hash = hash_password(&plain_pw, 10).await?;

    let mut tx = state.db.begin().await?;
    if let Some(ref rid) = body.reseller_id {
        check_reseller_limits_tx(&mut tx, rid).await?;
    }

    let dup: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "Client" WHERE username = $1 AND "resellerId" IS NOT DISTINCT FROM $2"#,
    )
    .bind(uname)
    .bind(&body.reseller_id)
    .fetch_one(&mut *tx)
    .await?;
    if dup > 0 {
        return Err(AppError::bad_request(
            "Username already exists for this reseller",
        ));
    }

    let id = crate::db::new_id();
    let sub_id = crate::db::new_id();
    let sub_token = crate::db::new_id();
    let protocols_json = serde_json::to_value(&protocols)
        .map_err(|e| AppError::internal(format!("serialize protocols: {e}")))?;

    let (all_nodes, raw_node_ids) = resolve_node_scope(body.all_nodes, body.node_ids.as_deref());
    let node_ids = normalize_and_validate_node_ids(&mut tx, &raw_node_ids).await?;

    let r = sqlx::query(
        r#"INSERT INTO "Client" (id, uuid, username, email, password, "trafficLimit", "expireAt",
                                 "restrictionMode", "trafficResetDay", "nextTrafficResetAt", "resetStrategy",
                                 note, "subId", "subToken", protocols, "allNodes", "resellerId", "planId", "deviceLimit",
                                 "onHold", "onHoldDurationDays", "createdAt", "updatedAt")
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $22)
           RETURNING id, uuid, username, email, "trafficLimit", "usedTraffic", "uploadTraffic", "downloadTraffic",
                     "expireAt", banned, "restrictionMode", "restrictionReason", "restrictedAt",
                     "trafficResetDay", "nextTrafficResetAt", "lastTrafficResetAt", "resetStrategy",
                     "accessRevision", note, "telegramId", "subId", "subToken", protocols, "allNodes", "planId",
                     "deviceLimit", "onHold", "onHoldDurationDays", "activatedAt", "lastActiveAt",
                     "lastLoginIp", "createdAt", "updatedAt", "resellerId""#,
    )
    .bind(&id)
    .bind(&uuid)
    .bind(uname)
    .bind(&body.email)
    .bind(&pw_hash)
    .bind(body.traffic_limit.unwrap_or(0).max(0))
    .bind(expire_at)
    .bind(body.restriction_mode.unwrap_or_else(|| "SOFT".into()))
    .bind(body.traffic_reset_day)
    .bind(next_reset)
    .bind(strategy.as_str())
    .bind(&body.note)
    .bind(&sub_id)
    .bind(&sub_token)
    .bind(&protocols_json)
    .bind(all_nodes)
    .bind(&body.reseller_id)
    .bind(&body.plan_id)
    .bind(body.device_limit)
    .bind(body.on_hold.unwrap_or(false))
    .bind(body.on_hold_duration_days)
    .bind(now)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        r#"INSERT INTO "ClientSetting" (id, "clientId", "subFormat", "autoUpdate", "createdAt", "updatedAt")
           VALUES ($1, $2, $3, true, $4, $4)"#,
    )
    .bind(crate::db::new_id())
    .bind(&id)
    .bind(crate::db::enums::DbSubFormat::Base64)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    if !all_nodes && !node_ids.is_empty() {
        for nid in &node_ids {
            sqlx::query(
                r#"INSERT INTO "ClientNode" (id, "clientId", "nodeId", "createdAt")
                   VALUES ($1, $2, $3, $4)
                   ON CONFLICT ("clientId", "nodeId") DO NOTHING"#,
            )
            .bind(crate::db::new_id())
            .bind(&id)
            .bind(nid)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
    }

    // Auto-bind to enabled inbounds matching client's protocols and node scope
    if !protocols.is_empty() {
        if all_nodes {
            sqlx::query(
                r#"INSERT INTO "ClientInbound" (id, "clientId", "inboundId", uuid, enable, "createdAt")
                   SELECT gen_random_uuid()::text, $1, i.id, $2, true, $3
                   FROM "Inbound" i
                   WHERE i.enable = true AND i.protocol::text = ANY($4::text[])
                   ON CONFLICT ("clientId", "inboundId") DO NOTHING"#,
            )
            .bind(&id)
            .bind(&uuid)
            .bind(now)
            .bind(&protocols)
            .execute(&mut *tx)
            .await?;
        } else if !node_ids.is_empty() {
            sqlx::query(
                r#"INSERT INTO "ClientInbound" (id, "clientId", "inboundId", uuid, enable, "createdAt")
                   SELECT gen_random_uuid()::text, $1, i.id, $2, true, $3
                   FROM "Inbound" i
                   WHERE i.enable = true
                     AND i.protocol::text = ANY($4::text[])
                     AND i."nodeId" = ANY($5::text[])
                   ON CONFLICT ("clientId", "inboundId") DO NOTHING"#,
            )
            .bind(&id)
            .bind(&uuid)
            .bind(now)
            .bind(&protocols)
            .bind(&node_ids)
            .execute(&mut *tx)
            .await?;
        }
    }

    let affected_nodes = collect_client_affected_nodes_tx(&mut tx, &id).await?;
    for nid in &affected_nodes {
        outbox::enqueue_config_sync_tx(&mut tx, nid, json!({ "clientId": id }), 0).await?;
    }

    tx.commit().await?;
    state.invalidate_derived();
    if all_nodes {
        outbox::broadcast_config_update(&state).await;
    } else {
        outbox::push_config_many(&state, &affected_nodes).await;
    }

    audit::spawn(
        &state.db,
        AuditEntry::new("CREATE", "client")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    let mut obj = client_row_to_json(&r)?;
    attach_client_nodes(&state.db, std::slice::from_mut(&mut obj)).await?;
    if let Some(m) = obj.as_object_mut() {
        m.insert("password".into(), json!(plain_pw));
    }
    Ok((StatusCode::CREATED, Json(obj)).into_response())
}

async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<UpdateClientReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;

    let mut tx = state.db.begin().await?;
    let ex = sqlx::query(
        r#"SELECT uuid, email, "trafficLimit", "expireAt", banned, "restrictionMode",
                  "trafficResetDay", "nextTrafficResetAt", "resetStrategy", note, protocols,
                  "allNodes", "planId", "deviceLimit", "onHold", "onHoldDurationDays",
                  "subToken", "accessRevision"
           FROM "Client" WHERE id = $1 FOR UPDATE"#,
    )
    .bind(&id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::not_found("Client not found"))?;

    let client_uuid: String = ex.try_get("uuid")?;
    let sub_token: String = ex.try_get("subToken")?;
    let access_rev: i32 = ex.try_get("accessRevision")?;
    let old_protocols_val: Value = ex.try_get("protocols")?;
    let old_protocols = extract_protocols_from_json(&old_protocols_val);
    let now = crate::db::now();

    let mut affected_set: HashSet<String> = collect_client_affected_nodes_tx(&mut tx, &id)
        .await?
        .into_iter()
        .collect();

    let protocols_vec: Option<Vec<String>> = match body.protocols {
        Some(ref p) => Some(validate_protocols(p)?),
        None => None,
    };
    let (protocols_val, effective_protocols) = match protocols_vec {
        Some(ref p) => {
            resync_bindings(&mut tx, &id, &client_uuid, &old_protocols, p, now).await?;
            (serde_json::to_value(p).unwrap_or_else(|_| json!([])), p.clone())
        }
        None => (old_protocols_val, old_protocols),
    };

    if body.all_nodes.is_some() || body.node_ids.is_some() {
        let (all_nodes, raw_node_ids) =
            resolve_node_scope(body.all_nodes, body.node_ids.as_deref());
        let node_ids = normalize_and_validate_node_ids(&mut tx, &raw_node_ids).await?;
        let sync_affected = sync_client_nodes_and_inbounds_tx(
            &mut tx,
            &id,
            &client_uuid,
            &effective_protocols,
            all_nodes,
            &node_ids,
            true,
            now,
        )
        .await?;
        for nid in sync_affected {
            affected_set.insert(nid);
        }
    }

    let expire_at: Option<NaiveDateTime> = match body.expire_at {
        Some(Some(s)) if !s.trim().is_empty() => {
            Some(crate::db::ts::parse_iso(&s).map_err(|e| AppError::validation("expireAt", e))?)
        }
        Some(_) => None,
        None => ex.try_get("expireAt")?,
    };

    let reset_day: Option<i32> = body
        .traffic_reset_day
        .unwrap_or_else(|| ex.try_get("trafficResetDay").ok().flatten());
    let strategy_str = body
        .reset_strategy
        .as_deref()
        .map(str::to_string)
        .unwrap_or_else(|| ex.try_get("resetStrategy").unwrap_or_else(|_| "NO_RESET".into()));
    let strategy = TrafficResetStrategy::from_str(&strategy_str)
        .map_err(|e| AppError::validation("resetStrategy", e))?;
    let next_reset = if body.reset_strategy.is_some() || body.traffic_reset_day.is_some() {
        enforcement::calculate_next_reset(strategy, reset_day, now)
    } else {
        ex.try_get("nextTrafficResetAt")?
    };

    let new_pw_hash = match body.password.as_deref() {
        Some(p) if !p.is_empty() => {
            if p.len() < 6 {
                return Err(AppError::validation("password", "Password min 6 chars"));
            }
            Some(hash_password(p, 10).await?)
        }
        _ => None,
    };

    let r = if let Some(ref hash) = new_pw_hash {
        sqlx::query(
            r#"UPDATE "Client"
               SET email = $2, password = $3, "trafficLimit" = $4, "expireAt" = $5, banned = $6,
                   "restrictionMode" = $7, "trafficResetDay" = $8, "nextTrafficResetAt" = $9,
                   "resetStrategy" = $10, note = $11, protocols = $12, "planId" = $13,
                   "deviceLimit" = $14, "onHold" = $15, "onHoldDurationDays" = $16, "updatedAt" = $17
               WHERE id = $1
               RETURNING id, uuid, username, email, "trafficLimit", "usedTraffic", "uploadTraffic", "downloadTraffic",
                         "expireAt", banned, "restrictionMode", "restrictionReason", "restrictedAt",
                         "trafficResetDay", "nextTrafficResetAt", "lastTrafficResetAt", "resetStrategy",
                         "accessRevision", note, "telegramId", "subId", "subToken", protocols, "allNodes", "planId",
                         "deviceLimit", "onHold", "onHoldDurationDays", "activatedAt", "lastActiveAt",
                         "lastLoginIp", "createdAt", "updatedAt", "resellerId""#,
        )
        .bind(&id)
        .bind(body.email.unwrap_or_else(|| ex.try_get("email").ok().flatten()))
        .bind(hash)
        .bind(body.traffic_limit.unwrap_or_else(|| ex.try_get("trafficLimit").unwrap_or(0)))
        .bind(expire_at)
        .bind(body.banned.unwrap_or_else(|| ex.try_get("banned").unwrap_or(false)))
        .bind(body.restriction_mode.unwrap_or_else(|| ex.try_get("restrictionMode").unwrap_or_else(|_| "SOFT".into())))
        .bind(reset_day)
        .bind(next_reset)
        .bind(strategy.as_str())
        .bind(body.note.unwrap_or_else(|| ex.try_get("note").ok().flatten()))
        .bind(&protocols_val)
        .bind(body.plan_id.unwrap_or_else(|| ex.try_get("planId").ok().flatten()))
        .bind(body.device_limit.unwrap_or_else(|| ex.try_get("deviceLimit").ok().flatten()))
        .bind(body.on_hold.unwrap_or_else(|| ex.try_get("onHold").unwrap_or(false)))
        .bind(body.on_hold_duration_days.unwrap_or_else(|| ex.try_get("onHoldDurationDays").ok().flatten()))
        .bind(now)
        .fetch_one(&mut *tx)
        .await?
    } else {
        sqlx::query(
            r#"UPDATE "Client"
               SET email = $2, "trafficLimit" = $3, "expireAt" = $4, banned = $5,
                   "restrictionMode" = $6, "trafficResetDay" = $7, "nextTrafficResetAt" = $8,
                   "resetStrategy" = $9, note = $10, protocols = $11, "planId" = $12,
                   "deviceLimit" = $13, "onHold" = $14, "onHoldDurationDays" = $15, "updatedAt" = $16
               WHERE id = $1
               RETURNING id, uuid, username, email, "trafficLimit", "usedTraffic", "uploadTraffic", "downloadTraffic",
                         "expireAt", banned, "restrictionMode", "restrictionReason", "restrictedAt",
                         "trafficResetDay", "nextTrafficResetAt", "lastTrafficResetAt", "resetStrategy",
                         "accessRevision", note, "telegramId", "subId", "subToken", protocols, "allNodes", "planId",
                         "deviceLimit", "onHold", "onHoldDurationDays", "activatedAt", "lastActiveAt",
                         "lastLoginIp", "createdAt", "updatedAt", "resellerId""#,
        )
        .bind(&id)
        .bind(body.email.unwrap_or_else(|| ex.try_get("email").ok().flatten()))
        .bind(body.traffic_limit.unwrap_or_else(|| ex.try_get("trafficLimit").unwrap_or(0)))
        .bind(expire_at)
        .bind(body.banned.unwrap_or_else(|| ex.try_get("banned").unwrap_or(false)))
        .bind(body.restriction_mode.unwrap_or_else(|| ex.try_get("restrictionMode").unwrap_or_else(|_| "SOFT".into())))
        .bind(reset_day)
        .bind(next_reset)
        .bind(strategy.as_str())
        .bind(body.note.unwrap_or_else(|| ex.try_get("note").ok().flatten()))
        .bind(&protocols_val)
        .bind(body.plan_id.unwrap_or_else(|| ex.try_get("planId").ok().flatten()))
        .bind(body.device_limit.unwrap_or_else(|| ex.try_get("deviceLimit").ok().flatten()))
        .bind(body.on_hold.unwrap_or_else(|| ex.try_get("onHold").unwrap_or(false)))
        .bind(body.on_hold_duration_days.unwrap_or_else(|| ex.try_get("onHoldDurationDays").ok().flatten()))
        .bind(now)
        .fetch_one(&mut *tx)
        .await?
    };

    for nid in collect_client_affected_nodes_tx(&mut tx, &id).await? {
        affected_set.insert(nid);
    }
    let affected_nodes: Vec<String> = affected_set.into_iter().collect();

    for nid in &affected_nodes {
        outbox::enqueue_config_sync_tx(&mut tx, nid, json!({ "clientId": id }), access_rev).await?;
    }

    tx.commit().await?;
    state.cache.del_sub_token(&sub_token);
    state.invalidate_derived();
    let _ = enforcement::enforce_client_policies(&state, &id).await;
    if affected_nodes.is_empty() {
        outbox::broadcast_config_update(&state).await;
    } else {
        outbox::push_config_many(&state, &affected_nodes).await;
    }

    audit::spawn(
        &state.db,
        AuditEntry::new("UPDATE", "client")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    let mut obj = client_row_to_json(&r)?;
    attach_client_nodes(&state.db, std::slice::from_mut(&mut obj)).await?;
    Ok(Json(obj))
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

    let mut tx = state.db.begin().await?;
    let node_ids: Vec<String> = sqlx::query_scalar(
        r#"SELECT DISTINCT i."nodeId"
           FROM "ClientInbound" ci JOIN "Inbound" i ON i.id = ci."inboundId"
           WHERE ci."clientId" = $1"#,
    )
    .bind(&id)
    .fetch_all(&mut *tx)
    .await?;

    let sub_token: Option<String> =
        sqlx::query_scalar(r#"DELETE FROM "Client" WHERE id = $1 RETURNING "subToken""#)
            .bind(&id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some(tok) = sub_token else {
        return Err(AppError::not_found("Client not found"));
    };

    for nid in &node_ids {
        outbox::enqueue_config_sync_tx(&mut tx, nid, json!({ "clientId": id, "deleted": true }), 0)
            .await?;
    }
    tx.commit().await?;

    state.cache.del_sub_token(&tok);
    state.invalidate_derived();
    if node_ids.is_empty() {
        outbox::broadcast_config_update(&state).await;
    } else {
        outbox::push_config_many(&state, &node_ids).await;
    }

    audit::spawn(
        &state.db,
        AuditEntry::new("DELETE", "client")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );
    Ok(ok())
}

async fn reset_traffic(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;

    let now = crate::db::now();
    let sub_token: Option<String> = sqlx::query_scalar(
        r#"UPDATE "Client"
           SET "usedTraffic" = 0, "uploadTraffic" = 0, "downloadTraffic" = 0, "lastTrafficResetAt" = $2, "updatedAt" = $2
           WHERE id = $1
           RETURNING "subToken""#,
    )
    .bind(&id)
    .bind(now)
    .fetch_optional(&state.db)
    .await?;
    let Some(tok) = sub_token else {
        return Err(AppError::not_found("Client not found"));
    };

    state.cache.del_sub_token(&tok);
    let _ = enforcement::enforce_client_policies(&state, &id).await;
    outbox::broadcast_config_update(&state).await;

    audit::spawn(
        &state.db,
        AuditEntry::new("RESET_TRAFFIC", "client")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );
    Ok(ok())
}

async fn toggle_ban(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;

    let row = sqlx::query(
        r#"UPDATE "Client" SET banned = NOT banned, "updatedAt" = $2 WHERE id = $1 RETURNING banned, "subToken""#,
    )
    .bind(&id)
    .bind(crate::db::now())
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Client not found"))?;

    let banned: bool = row.try_get("banned")?;
    let tok: String = row.try_get("subToken")?;

    state.cache.del_sub_token(&tok);
    state.invalidate_derived();
    outbox::broadcast_config_update(&state).await;

    audit::spawn(
        &state.db,
        AuditEntry::new("TOGGLE_BAN", "client")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );
    Ok(Json(json!({ "banned": banned })))
}

async fn stats(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Query(q): Query<DaysQuery>,
) -> AppResult<Json<Vec<Value>>> {
    valid_id(&id)?;
    ensure_client_access(&state, &user, &id).await?;
    let days = positive_int(q.days.as_deref(), 7, 366, "days")?;
    let start = crate::db::now() - ChronoDuration::days(days);

    let rows = sqlx::query(
        r#"SELECT t."nodeId", COALESCE(n.name, t."nodeId") AS "nodeName",
                  COALESCE(SUM(t.upload), 0)::bigint AS up,
                  COALESCE(SUM(t.download), 0)::bigint AS down
           FROM "TrafficLog" t
           LEFT JOIN "Node" n ON n.id = t."nodeId"
           WHERE t."clientId" = $1 AND t."recordAt" >= $2
           GROUP BY t."nodeId", n.name"#,
    )
    .bind(&id)
    .bind(start)
    .fetch_all(&state.db)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let up = r.try_get::<i64, _>("up")?;
        let down = r.try_get::<i64, _>("down")?;
        out.push(json!({
            "nodeId": r.try_get::<String, _>("nodeId")?,
            "nodeName": r.try_get::<String, _>("nodeName")?,
            "upload": up,
            "download": down,
            "_sum": {
                "upload": up,
                "download": down
            }
        }));
    }
    Ok(Json(out))
}

async fn traffic_daily(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Query(q): Query<DaysQuery>,
) -> AppResult<Json<Vec<Value>>> {
    valid_id(&id)?;
    ensure_client_access(&state, &user, &id).await?;
    let days = positive_int(q.days.as_deref(), 30, 366, "days")?;
    let start = crate::db::now() - ChronoDuration::days(days);

    let rows = sqlx::query(
        r#"SELECT date_trunc('day', "recordAt") AS day,
                  COALESCE(SUM(upload), 0)::bigint AS upload,
                  COALESCE(SUM(download), 0)::bigint AS download
           FROM "TrafficLog" WHERE "clientId" = $1 AND "recordAt" >= $2
           GROUP BY date_trunc('day', "recordAt") ORDER BY day ASC"#,
    )
    .bind(&id)
    .bind(start)
    .fetch_all(&state.db)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let day: NaiveDateTime = r.try_get("day")?;
        out.push(json!({
            "day": crate::db::ts::format_iso(&day),
            "upload": r.try_get::<i64, _>("upload")?,
            "download": r.try_get::<i64, _>("download")?
        }));
    }
    Ok(Json(out))
}

async fn get_client_nodes(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    valid_id(&id)?;
    ensure_client_access(&state, &user, &id).await?;

    let all_nodes: bool =
        sqlx::query_scalar(r#"SELECT "allNodes" FROM "Client" WHERE id = $1"#)
            .bind(&id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::not_found("Client not found"))?;

    let rows = sqlx::query(
        r#"SELECT n.id, n.name, n.host, n.status::text AS status, n.country
           FROM "ClientNode" cn
           JOIN "Node" n ON n.id = cn."nodeId"
           WHERE cn."clientId" = $1
           ORDER BY n.name ASC"#,
    )
    .bind(&id)
    .fetch_all(&state.db)
    .await?;

    let mut node_ids = Vec::with_capacity(rows.len());
    let mut nodes = Vec::with_capacity(rows.len());
    for r in rows {
        let nid: String = r.try_get("id")?;
        node_ids.push(nid.clone());
        nodes.push(json!({
            "id": nid,
            "name": r.try_get::<String, _>("name")?,
            "host": r.try_get::<String, _>("host")?,
            "status": r.try_get::<Option<String>, _>("status").ok().flatten(),
            "country": r.try_get::<Option<String>, _>("country").ok().flatten(),
        }));
    }

    Ok(Json(json!({
        "allNodes": all_nodes,
        "nodeIds": node_ids,
        "nodes": nodes,
    })))
}

async fn set_client_nodes(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<SetClientNodesReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;

    let mut tx = state.db.begin().await?;
    let row = sqlx::query(
        r#"SELECT uuid, protocols, "subToken", "accessRevision"
           FROM "Client" WHERE id = $1 FOR UPDATE"#,
    )
    .bind(&id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::not_found("Client not found"))?;

    let client_uuid: String = row.try_get("uuid")?;
    let protocols_val: Value = row.try_get("protocols")?;
    let protocols = extract_protocols_from_json(&protocols_val);
    let sub_token: String = row.try_get("subToken")?;
    let access_rev: i32 = row.try_get("accessRevision")?;
    let now = crate::db::now();

    let (all_nodes, raw_node_ids) = resolve_node_scope(body.all_nodes, body.node_ids.as_deref());
    let node_ids = normalize_and_validate_node_ids(&mut tx, &raw_node_ids).await?;
    let sync_inbounds = body.sync_inbounds.unwrap_or(true);

    let affected_nodes = sync_client_nodes_and_inbounds_tx(
        &mut tx,
        &id,
        &client_uuid,
        &protocols,
        all_nodes,
        &node_ids,
        sync_inbounds,
        now,
    )
    .await?;

    for nid in &affected_nodes {
        outbox::enqueue_config_sync_tx(&mut tx, nid, json!({ "clientId": id }), access_rev).await?;
    }
    tx.commit().await?;

    state.cache.del_sub_token(&sub_token);
    state.invalidate_derived();
    if affected_nodes.is_empty() {
        outbox::broadcast_config_update(&state).await;
    } else {
        outbox::push_config_many(&state, &affected_nodes).await;
    }

    audit::spawn(
        &state.db,
        AuditEntry::new("UPDATE_NODES", "client")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    let rows = sqlx::query(
        r#"SELECT n.id, n.name, n.host, n.status::text AS status, n.country
           FROM "ClientNode" cn
           JOIN "Node" n ON n.id = cn."nodeId"
           WHERE cn."clientId" = $1
           ORDER BY n.name ASC"#,
    )
    .bind(&id)
    .fetch_all(&state.db)
    .await?;

    let mut resp_ids = Vec::with_capacity(rows.len());
    let mut nodes = Vec::with_capacity(rows.len());
    for r in rows {
        let nid: String = r.try_get("id")?;
        resp_ids.push(nid.clone());
        nodes.push(json!({
            "id": nid,
            "name": r.try_get::<String, _>("name")?,
            "host": r.try_get::<String, _>("host")?,
            "status": r.try_get::<Option<String>, _>("status").ok().flatten(),
            "country": r.try_get::<Option<String>, _>("country").ok().flatten(),
        }));
    }

    Ok(Json(json!({
        "ok": true,
        "allNodes": all_nodes,
        "nodeIds": resp_ids,
        "nodes": nodes,
        "affectedNodes": affected_nodes,
    })))
}

async fn bind_inbounds(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    AppJson(body): AppJson<BindInboundsReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let mut tx = state.db.begin().await?;
    let row = sqlx::query(
        r#"SELECT uuid, "subToken", "accessRevision", "allNodes"
           FROM "Client" WHERE id = $1 FOR UPDATE"#,
    )
    .bind(&id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::not_found("Client not found"))?;

    let uuid: String = row.try_get("uuid")?;
    let sub_token: String = row.try_get("subToken")?;
    let access_rev: i32 = row.try_get("accessRevision")?;
    let all_nodes: bool = row.try_get::<bool, _>("allNodes").unwrap_or(true);

    let mut affected_set: HashSet<String> = collect_client_affected_nodes_tx(&mut tx, &id)
        .await?
        .into_iter()
        .collect();

    if !all_nodes && !body.inbound_ids.is_empty() {
        let allowed_node_ids: Vec<String> = sqlx::query_scalar(
            r#"SELECT "nodeId" FROM "ClientNode" WHERE "clientId" = $1"#,
        )
        .bind(&id)
        .fetch_all(&mut *tx)
        .await?;
        let bad_count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "Inbound"
               WHERE id = ANY($1::text[]) AND NOT ("nodeId" = ANY($2::text[]))"#,
        )
        .bind(&body.inbound_ids)
        .bind(&allowed_node_ids)
        .fetch_one(&mut *tx)
        .await?;
        if bad_count > 0 {
            return Err(AppError::bad_request(
                "Inbound belongs to a node not assigned to this client",
            ));
        }
    }

    sqlx::query(r#"DELETE FROM "ClientInbound" WHERE "clientId" = $1"#)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    let now = crate::db::now();
    for iid in &body.inbound_ids {
        valid_id(iid)?;
        sqlx::query(
            r#"INSERT INTO "ClientInbound" (id, "clientId", "inboundId", uuid, enable, "createdAt")
               VALUES ($1, $2, $3, $4, true, $5) ON CONFLICT DO NOTHING"#,
        )
        .bind(crate::db::new_id())
        .bind(&id)
        .bind(iid)
        .bind(&uuid)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    for nid in collect_client_affected_nodes_tx(&mut tx, &id).await? {
        affected_set.insert(nid);
    }
    let affected_nodes: Vec<String> = affected_set.into_iter().collect();
    for nid in &affected_nodes {
        outbox::enqueue_config_sync_tx(&mut tx, nid, json!({ "clientId": id }), access_rev).await?;
    }

    tx.commit().await?;
    state.cache.del_sub_token(&sub_token);
    state.invalidate_derived();
    if affected_nodes.is_empty() {
        outbox::broadcast_config_update(&state).await;
    } else {
        outbox::push_config_many(&state, &affected_nodes).await;
    }
    Ok(ok())
}

async fn bulk_delete(
    State(state): State<AppState>,
    user: AuthUser,
    AppJson(body): AppJson<BulkIdsReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    if body.ids.is_empty() {
        return Ok(json!({ "deleted": 0 }).into());
    }
    let res = sqlx::query(r#"DELETE FROM "Client" WHERE id = ANY($1::text[])"#)
        .bind(&body.ids)
        .execute(&state.db)
        .await?;
    state.invalidate_derived();
    outbox::broadcast_config_update(&state).await;
    Ok(Json(json!({ "deleted": res.rows_affected() })))
}

async fn bulk_ban(
    State(state): State<AppState>,
    user: AuthUser,
    AppJson(body): AppJson<BulkIdsReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    if body.ids.is_empty() {
        return Ok(json!({ "updated": 0 }).into());
    }
    let banned = body.banned.unwrap_or(true);
    let res = sqlx::query(
        r#"UPDATE "Client" SET banned = $2, "updatedAt" = $3 WHERE id = ANY($1::text[])"#,
    )
    .bind(&body.ids)
    .bind(banned)
    .bind(crate::db::now())
    .execute(&state.db)
    .await?;
    state.invalidate_derived();
    outbox::broadcast_config_update(&state).await;
    Ok(Json(json!({ "updated": res.rows_affected(), "banned": banned })))
}

async fn import_csv(
    State(state): State<AppState>,
    user: AuthUser,
    AppJson(body): AppJson<CsvImportReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    let mut created = 0usize;
    let now = crate::db::now();
    for line in body.csv.lines().skip(1) {
        let cols: Vec<&str> = line.split(',').map(str::trim).collect();
        let Some(uname) = cols.first().filter(|s| s.len() >= 3) else {
            continue;
        };
        let email = cols.get(1).filter(|s| !s.is_empty()).map(|s| (*s).to_string());
        let limit_gb = cols
            .get(2)
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0)
            .max(0);
        let traffic_limit = limit_gb.saturating_mul(1024 * 1024 * 1024);
        let uuid = crate::db::new_id();
        let pw_hash = hash_password(&uuid.replace('-', "")[..16], 10).await?;

        let res = sqlx::query(
            r#"INSERT INTO "Client" (id, uuid, username, email, password, "trafficLimit", "subId", "subToken", "createdAt", "updatedAt")
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
               ON CONFLICT DO NOTHING"#,
        )
        .bind(crate::db::new_id())
        .bind(&uuid)
        .bind(uname)
        .bind(email)
        .bind(pw_hash)
        .bind(traffic_limit)
        .bind(crate::db::new_id())
        .bind(crate::db::new_id())
        .bind(now)
        .execute(&state.db)
        .await?;
        if res.rows_affected() > 0 {
            created += 1;
        }
    }
    state.invalidate_derived();
    Ok(Json(json!({ "imported": created })))
}

async fn list_devices(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<Value>>> {
    valid_id(&id)?;
    ensure_client_access(&state, &user, &id).await?;
    let rows = sqlx::query(
        r#"SELECT id, hwid, os, "osVersion", model, "userAgent", ip, "lastSeenAt", "createdAt"
           FROM "Device" WHERE "clientId" = $1 ORDER BY "lastSeenAt" DESC"#,
    )
    .bind(&id)
    .fetch_all(&state.db)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let ls: NaiveDateTime = r.try_get("lastSeenAt")?;
        let ca: NaiveDateTime = r.try_get("createdAt")?;
        out.push(json!({
            "id": r.try_get::<String, _>("id")?,
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
    Ok(Json(out))
}

async fn delete_device(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, hwid)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let res = sqlx::query(r#"DELETE FROM "Device" WHERE "clientId" = $1 AND (hwid = $2 OR id = $2)"#)
        .bind(&id)
        .bind(&hwid)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("Device not found"));
    }
    Ok(ok())
}

async fn collect_client_openwrt_nodes(
    state: &AppState,
    client_id: &str,
) -> AppResult<(String, Vec<InboundExport>)> {
    let c_row = sqlx::query(
        r#"SELECT uuid, username, protocols FROM "Client" WHERE id = $1"#,
    )
    .bind(client_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Client not found"))?;

    let client_uuid: String = c_row.try_get("uuid")?;
    let username: String = c_row.try_get("username")?;
    let protocols: Value = c_row.try_get("protocols")?;

    let rows = sqlx::query(
        r#"SELECT i.id, i.protocol, i.tag, i.port, i.settings, i.stream, n.host
           FROM "ClientInbound" ci
           JOIN "Client" c ON c.id = ci."clientId"
           JOIN "Inbound" i ON i.id = ci."inboundId"
           JOIN "Node" n ON n.id = i."nodeId"
           WHERE ci."clientId" = $1 AND ci.enable = true AND i.enable = true AND n.active = true
             AND (
               c."allNodes" = true
               OR EXISTS (
                 SELECT 1 FROM "ClientNode" cn
                 WHERE cn."clientId" = ci."clientId" AND cn."nodeId" = i."nodeId"
               )
             )"#,
    )
    .bind(client_id)
    .fetch_all(&state.db)
    .await?;

    let mut nodes = Vec::new();
    for r in rows {
        let db_proto: DbProtocol = r.try_get("protocol")?;
        let proto: Protocol = db_proto.into();
        let allowed = protocols
            .as_array()
            .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some(proto.as_str())));
        if !allowed {
            continue;
        }
        let mut settings: Value = r.try_get("settings")?;
        let stream: Value = r.try_get("stream")?;
        let tag: String = r.try_get("tag")?;
        let port: i32 = r.try_get("port")?;
        let host: String = r.try_get("host")?;
        let ss_method = settings.get("method").and_then(Value::as_str);

        let secret = if proto.is_uuid_auth() {
            String::new()
        } else {
            protocol_credentials::get_protocol_secret(state, client_id, proto, ss_method).await?
        };
        let port_u16 = u16::try_from(port).unwrap_or(443);
        let ctx = EntryContext {
            node_host: host.clone(),
            port: port_u16,
            client_uuid: client_uuid.clone(),
            client_id: client_id.to_string(),
            secret,
        };
        if let Ok(Some(entry)) = build_entry(proto, &tag, &settings, &stream, &ctx) {
            if let Some(obj) = settings.as_object_mut() {
                if !entry.uuid.is_empty() {
                    let has_id = obj
                        .get("id")
                        .and_then(Value::as_str)
                        .is_some_and(|s| !s.trim().is_empty());
                    if !has_id {
                        obj.insert("id".into(), json!(entry.uuid));
                    }
                }
                if !entry.password.is_empty() {
                    obj.insert("password".into(), json!(entry.password));
                }
                if !entry.username.is_empty() {
                    obj.entry("username").or_insert_with(|| json!(entry.username));
                }
            }
            let resolved_ip = crate::api::sub::resolve_host_to_ip(state, &host).await;
            nodes.push(InboundExport {
                protocol: proto,
                tag: entry.tag,
                host,
                port: port_u16,
                settings,
                stream,
                resolved_ip,
            });
        }
    }
    Ok((username, nodes))
}

async fn export_openclash(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Response> {
    valid_id(&id)?;
    ensure_client_access(&state, &user, &id).await?;
    let (_username, nodes) = collect_client_openwrt_nodes(&state, &id).await?;
    let yaml = generate_openclash_yaml_with_ts(&nodes, &crate::db::ts::format_iso(&crate::db::now()))
        .map_err(|e| AppError::internal(format!("OpenClash YAML generation failed: {e}")))?;
    let uci_cmds: Vec<_> = nodes.iter().flat_map(generate_openclash_uci).collect();
    let uci_json = serde_json::to_string_pretty(&uci_cmds)
        .map_err(|e| AppError::internal(format!("OpenClash UCI serialization failed: {e}")))?;

    let mut res = (
        StatusCode::OK,
        format!("# OpenClash UCI\n# {uci_json}\n\n# OpenClash YAML\n{yaml}"),
    )
        .into_response();
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/yaml; charset=utf-8"),
    );
    Ok(res)
}

async fn export_passwall(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Response> {
    valid_id(&id)?;
    ensure_client_access(&state, &user, &id).await?;
    let (_username, nodes) = collect_client_openwrt_nodes(&state, &id).await?;
    let cfg = generate_passwall_subscription(&nodes)
        .map_err(|e| AppError::internal(format!("Passwall config generation failed: {e}")))?;

    let mut res = (StatusCode::OK, cfg).into_response();
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    Ok(res)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BulkActionReq {
    action: String,
    #[serde(alias = "clientIds")]
    ids: Vec<String>,
    days: Option<i64>,
    plan_id: Option<String>,
    node_ids: Option<Vec<String>>,
    node_mode: Option<String>,
    all_nodes: Option<bool>,
}

async fn bulk_action(
    State(state): State<AppState>,
    user: AuthUser,
    AppJson(body): AppJson<BulkActionReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    if body.ids.is_empty() {
        return Ok(Json(json!({ "ok": true, "updated": 0, "affected": 0, "deleted": 0 })));
    }
    let now = crate::db::now();
    let sub_tokens: Vec<String> =
        sqlx::query_scalar(r#"SELECT "subToken" FROM "Client" WHERE id = ANY($1::text[])"#)
            .bind(&body.ids)
            .fetch_all(&state.db)
            .await?;

    let count = match body.action.as_str() {
        "delete" => {
            sqlx::query(r#"DELETE FROM "Client" WHERE id = ANY($1::text[])"#)
                .bind(&body.ids)
                .execute(&state.db)
                .await?
                .rows_affected()
        }
        "ban" => {
            sqlx::query(r#"UPDATE "Client" SET banned = true, "updatedAt" = $2 WHERE id = ANY($1::text[])"#)
                .bind(&body.ids)
                .bind(now)
                .execute(&state.db)
                .await?
                .rows_affected()
        }
        "unban" => {
            sqlx::query(r#"UPDATE "Client" SET banned = false, "updatedAt" = $2 WHERE id = ANY($1::text[])"#)
                .bind(&body.ids)
                .bind(now)
                .execute(&state.db)
                .await?
                .rows_affected()
        }
        "resetTraffic" | "reset_traffic" => {
            sqlx::query(
                r#"UPDATE "Client"
                   SET "usedTraffic" = 0, "uploadTraffic" = 0, "downloadTraffic" = 0,
                       "lastTrafficResetAt" = $2, "updatedAt" = $2
                   WHERE id = ANY($1::text[])"#,
            )
            .bind(&body.ids)
            .bind(now)
            .execute(&state.db)
            .await?
            .rows_affected()
        }
        "extend" => {
            let days = body.days.unwrap_or(30).clamp(1, 3650);
            let add_dur = ChronoDuration::days(days);
            let rows = sqlx::query(
                r#"SELECT id, "expireAt" FROM "Client" WHERE id = ANY($1::text[])"#,
            )
            .bind(&body.ids)
            .fetch_all(&state.db)
            .await?;
            let mut updated = 0u64;
            for r in rows {
                let cid: String = r.try_get("id")?;
                let cur: Option<NaiveDateTime> = r.try_get("expireAt")?;
                let base = cur.filter(|d| *d > now).unwrap_or(now);
                let res = sqlx::query(
                    r#"UPDATE "Client" SET "expireAt" = $2, "updatedAt" = $3 WHERE id = $1"#,
                )
                .bind(&cid)
                .bind(base + add_dur)
                .bind(now)
                .execute(&state.db)
                .await?;
                updated += res.rows_affected();
            }
            updated
        }
        "assignPlan" | "assign_plan" => {
            let pid = body
                .plan_id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| AppError::bad_request("planId is required"))?;
            sqlx::query(
                r#"UPDATE "Client" SET "planId" = $2, "updatedAt" = $3 WHERE id = ANY($1::text[])"#,
            )
            .bind(&body.ids)
            .bind(pid)
            .bind(now)
            .execute(&state.db)
            .await?
            .rows_affected()
        }
        "assignNodes" | "assign_nodes" => {
            let mode = body
                .node_mode
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(if body.all_nodes == Some(true) {
                    "all"
                } else {
                    "replace"
                });
            if !matches!(mode, "replace" | "add" | "remove" | "all") {
                return Err(AppError::validation(
                    "nodeMode",
                    format!("Unsupported nodeMode: {mode}"),
                ));
            }

            let mut tx = state.db.begin().await?;
            let raw_ids = body.node_ids.as_deref().unwrap_or(&[]);
            let target_ids = normalize_and_validate_node_ids(&mut tx, raw_ids).await?;
            if matches!(mode, "add" | "remove") && target_ids.is_empty() {
                return Err(AppError::bad_request("nodeIds is required for add/remove mode"));
            }

            let client_rows = sqlx::query(
                r#"SELECT id, uuid, protocols, "allNodes", "accessRevision"
                   FROM "Client"
                   WHERE id = ANY($1::text[])
                   FOR UPDATE"#,
            )
            .bind(&body.ids)
            .fetch_all(&mut *tx)
            .await?;

            let all_db_node_ids: Vec<String> = if mode == "remove" {
                sqlx::query_scalar(r#"SELECT id FROM "Node" ORDER BY id ASC"#)
                    .fetch_all(&mut *tx)
                    .await?
            } else {
                Vec::new()
            };

            let mut affected_set: HashSet<String> = HashSet::new();
            let mut max_rev: i32 = 0;
            let mut updated = 0u64;

            for cr in client_rows {
                let cid: String = cr.try_get("id")?;
                let cuuid: String = cr.try_get("uuid")?;
                let protos_val: Value = cr.try_get("protocols")?;
                let protos = extract_protocols_from_json(&protos_val);
                let cur_all: bool = cr.try_get::<bool, _>("allNodes").unwrap_or(true);
                let rev: i32 = cr.try_get("accessRevision").unwrap_or(0);
                max_rev = max_rev.max(rev);

                let cur_pinned: Vec<String> = sqlx::query_scalar(
                    r#"SELECT "nodeId" FROM "ClientNode" WHERE "clientId" = $1 ORDER BY "createdAt" ASC"#,
                )
                .bind(&cid)
                .fetch_all(&mut *tx)
                .await?;

                let (next_all, next_ids): (bool, Vec<String>) = match mode {
                    "all" => (true, Vec::new()),
                    "replace" => {
                        if body.all_nodes == Some(true) {
                            (true, Vec::new())
                        } else {
                            (false, target_ids.clone())
                        }
                    }
                    "add" => {
                        let mut merged = cur_pinned.clone();
                        for nid in &target_ids {
                            if !merged.contains(nid) {
                                merged.push(nid.clone());
                            }
                        }
                        (false, merged)
                    }
                    "remove" => {
                        let base = if cur_all { &all_db_node_ids } else { &cur_pinned };
                        let remaining: Vec<String> = base
                            .iter()
                            .filter(|nid| !target_ids.contains(nid))
                            .cloned()
                            .collect();
                        (false, remaining)
                    }
                    _ => unreachable!(),
                };

                let changed_nodes = sync_client_nodes_and_inbounds_tx(
                    &mut tx,
                    &cid,
                    &cuuid,
                    &protos,
                    next_all,
                    &next_ids,
                    true,
                    now,
                )
                .await?;
                for nid in changed_nodes {
                    affected_set.insert(nid);
                }
                updated += 1;
            }

            let affected_nodes: Vec<String> = affected_set.into_iter().collect();
            for nid in &affected_nodes {
                outbox::enqueue_config_sync_tx(&mut tx, nid, json!({ "bulk": true }), max_rev)
                    .await?;
            }
            tx.commit().await?;

            for tok in &sub_tokens {
                state.cache.del_sub_token(tok);
            }
            state.invalidate_derived();
            outbox::push_config_many(&state, &affected_nodes).await;

            return Ok(Json(json!({
                "ok": true,
                "action": body.action,
                "count": updated,
                "updated": updated,
                "affected": updated,
                "deleted": 0,
                "affectedNodes": affected_nodes,
            })));
        }
        other => {
            return Err(AppError::validation(
                "action",
                format!("Unsupported bulk action: {other}"),
            ))
        }
    };

    for tok in &sub_tokens {
        state.cache.del_sub_token(tok);
    }
    state.invalidate_derived();
    outbox::broadcast_config_update(&state).await;
    Ok(Json(json!({
        "ok": true,
        "action": body.action,
        "count": count,
        "updated": count,
        "affected": count,
        "deleted": if body.action == "delete" { count } else { 0 }
    })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportRowReq {
    username: String,
    email: Option<String>,
    password: Option<String>,
    traffic_limit: Option<i64>,
    traffic_limit_gb: Option<i64>,
    expire_at: Option<String>,
    expire_days: Option<i64>,
    protocols: Option<Vec<String>>,
    note: Option<String>,
    device_limit: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportClientsReq {
    csv: Option<String>,
    rows: Option<Vec<ImportRowReq>>,
}

async fn import_clients(
    State(state): State<AppState>,
    user: AuthUser,
    AppJson(body): AppJson<ImportClientsReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    let mut items: Vec<ImportRowReq> = body.rows.unwrap_or_default();
    if items.is_empty() {
        if let Some(ref csv) = body.csv {
            for (idx, line) in csv.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let lower = trimmed.to_ascii_lowercase();
                if idx == 0 && (lower.starts_with("username") || lower.starts_with("name,")) {
                    continue;
                }
                let cols: Vec<&str> = trimmed.split(',').map(str::trim).collect();
                let Some(uname) = cols.first().filter(|s| !s.is_empty()) else {
                    continue;
                };
                let protocols = cols
                    .get(4)
                    .filter(|s| !s.is_empty())
                    .map(|s| {
                        s.split([';', '|'])
                            .map(str::trim)
                            .filter(|p| !p.is_empty())
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                    })
                    .filter(|v| !v.is_empty());
                items.push(ImportRowReq {
                    username: (*uname).to_string(),
                    email: cols.get(1).filter(|s| !s.is_empty()).map(|s| (*s).to_string()),
                    password: None,
                    traffic_limit: None,
                    traffic_limit_gb: cols.get(2).and_then(|s| s.parse::<i64>().ok()),
                    expire_at: None,
                    expire_days: cols.get(3).and_then(|s| s.parse::<i64>().ok()),
                    protocols,
                    note: cols.get(5).filter(|s| !s.is_empty()).map(|s| (*s).to_string()),
                    device_limit: None,
                });
            }
        }
    }

    let now = crate::db::now();
    let mut created = 0usize;
    let mut skipped = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for item in items {
        let uname = item.username.trim();
        if uname.len() < 3 {
            skipped += 1;
            errors.push(format!("Username '{uname}' must be at least 3 characters"));
            continue;
        }
        let protos = match item.protocols {
            Some(ref p) if !p.is_empty() => validate_protocols(p)?,
            _ => vec!["VLESS".to_string()],
        };
        let proto_val = serde_json::to_value(&protos)
            .map_err(|e| AppError::internal(format!("protocols serialize: {e}")))?;
        let traffic_limit = item
            .traffic_limit
            .or_else(|| item.traffic_limit_gb.map(|g| g.max(0).saturating_mul(1024 * 1024 * 1024)))
            .unwrap_or(0);
        let expire_at = if let Some(ref iso) = item.expire_at {
            if iso.trim().is_empty() {
                None
            } else {
                Some(crate::db::ts::parse_iso(iso).map_err(AppError::bad_request)?)
            }
        } else {
            item.expire_days
                .filter(|d| *d > 0)
                .map(|d| now + ChronoDuration::days(d))
        };

        let uuid = crate::db::new_id();
        let default_pw: String = uuid.chars().filter(|c| *c != '-').take(16).collect();
        let raw_pw = item
            .password
            .as_deref()
            .filter(|s| s.len() >= 6)
            .unwrap_or(&default_pw);
        let pw_hash = hash_password(raw_pw, 10).await?;

        let res = sqlx::query(
            r#"INSERT INTO "Client" (
                   id, uuid, username, email, password, "trafficLimit", "expireAt",
                   note, protocols, "deviceLimit", "subId", "subToken", "createdAt", "updatedAt"
               )
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13)
               ON CONFLICT DO NOTHING"#,
        )
        .bind(crate::db::new_id())
        .bind(&uuid)
        .bind(uname)
        .bind(item.email)
        .bind(pw_hash)
        .bind(traffic_limit)
        .bind(expire_at)
        .bind(item.note)
        .bind(&proto_val)
        .bind(item.device_limit)
        .bind(crate::db::new_id())
        .bind(crate::db::new_id())
        .bind(now)
        .execute(&state.db)
        .await?;

        if res.rows_affected() > 0 {
            created += 1;
        } else {
            skipped += 1;
            errors.push(format!("Skipped duplicate user '{uname}'"));
        }
    }

    state.invalidate_derived();
    outbox::broadcast_config_update(&state).await;
    Ok(Json(json!({
        "created": created,
        "imported": created,
        "skipped": skipped,
        "errors": errors,
    })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MigrateClientsReq {
    source: Option<String>,
    data: Option<Value>,
    dry_run: Option<bool>,
}

async fn migrate_clients(
    State(state): State<AppState>,
    user: AuthUser,
    AppJson(body): AppJson<MigrateClientsReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    let dry_run = body.dry_run.unwrap_or(false);
    let data = body
        .data
        .ok_or_else(|| AppError::bad_request("data payload is required"))?;
    let users_arr = data
        .get("users")
        .and_then(Value::as_array)
        .or_else(|| data.as_array())
        .ok_or_else(|| AppError::bad_request("Expected JSON object with 'users' array or top-level array"))?;

    let detected_source = body.source.unwrap_or_else(|| {
        if users_arr.first().and_then(|u| u.get("usage_limit_GB")).is_some() {
            "hiddify".to_string()
        } else {
            "marzban".to_string()
        }
    });

    let now = crate::db::now();
    let mut created = 0usize;
    let mut skipped = 0usize;
    let mut errors: Vec<String> = Vec::new();
    let mut preview: Vec<Value> = Vec::new();

    for entry in users_arr {
        let username = entry
            .get("username")
            .or_else(|| entry.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        if username.len() < 3 {
            skipped += 1;
            errors.push(format!("Invalid username in migration record: '{username}'"));
            continue;
        }

        let traffic_limit = entry
            .get("data_limit")
            .and_then(Value::as_i64)
            .or_else(|| {
                entry
                    .get("usage_limit_GB")
                    .and_then(Value::as_f64)
                    .map(|gb| (gb.max(0.0) * 1024.0 * 1024.0 * 1024.0) as i64)
            })
            .unwrap_or(0);

        let expire_at = entry
            .get("expire")
            .and_then(Value::as_i64)
            .filter(|ts| *ts > 0)
            .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0).map(|dt| dt.naive_utc()))
            .or_else(|| {
                entry
                    .get("package_days")
                    .and_then(Value::as_i64)
                    .filter(|d| *d > 0)
                    .map(|d| now + ChronoDuration::days(d))
            });

        let note = entry
            .get("note")
            .or_else(|| entry.get("comment"))
            .and_then(Value::as_str)
            .map(ToString::to_string);

        let uuid = entry
            .get("uuid")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(crate::db::new_id);

        if dry_run {
            let exists: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Client" WHERE username = $1"#)
                .bind(username)
                .fetch_one(&state.db)
                .await?;
            if exists > 0 {
                skipped += 1;
            } else {
                created += 1;
            }
            if preview.len() < 25 {
                preview.push(json!({
                    "username": username,
                    "trafficLimit": traffic_limit,
                    "wouldSkip": exists > 0,
                }));
            }
            continue;
        }

        let raw_pw: String = crate::db::new_id().chars().filter(|c| *c != '-').take(16).collect();
        let pw_hash = hash_password(&raw_pw, 10).await?;
        let proto_val = json!(["VLESS"]);
        let res = sqlx::query(
            r#"INSERT INTO "Client" (
                   id, uuid, username, password, "trafficLimit", "expireAt", note, protocols,
                   "subId", "subToken", "createdAt", "updatedAt"
               )
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)
               ON CONFLICT DO NOTHING"#,
        )
        .bind(crate::db::new_id())
        .bind(&uuid)
        .bind(username)
        .bind(pw_hash)
        .bind(traffic_limit)
        .bind(expire_at)
        .bind(note)
        .bind(&proto_val)
        .bind(crate::db::new_id())
        .bind(crate::db::new_id())
        .bind(now)
        .execute(&state.db)
        .await?;

        if res.rows_affected() > 0 {
            created += 1;
        } else {
            skipped += 1;
        }
    }

    if !dry_run && created > 0 {
        state.invalidate_derived();
        outbox::broadcast_config_update(&state).await;
    }

    Ok(Json(json!({
        "dryRun": dry_run,
        "source": detected_source,
        "total": users_arr.len(),
        "created": created,
        "imported": created,
        "skipped": skipped,
        "errors": errors,
        "preview": preview,
    })))
}

async fn get_client_inbounds(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<Value>>> {
    valid_id(&id)?;
    ensure_client_access(&state, &user, &id).await?;
    let rows = sqlx::query(
        r#"SELECT ci."inboundId", i.tag, i.protocol, i.port, i."nodeId", n.name AS "nodeName", ci.enable
           FROM "ClientInbound" ci
           JOIN "Inbound" i ON i.id = ci."inboundId"
           JOIN "Node" n ON n.id = i."nodeId"
           WHERE ci."clientId" = $1
           ORDER BY n.name ASC, i.port ASC"#,
    )
    .bind(&id)
    .fetch_all(&state.db)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let inbound_id: String = r.try_get("inboundId")?;
        let proto: DbProtocol = r.try_get("protocol")?;
        out.push(json!({
            "id": inbound_id,
            "inboundId": inbound_id,
            "tag": r.try_get::<String, _>("tag")?,
            "protocol": Protocol::from(proto).as_str(),
            "port": r.try_get::<i32, _>("port")?,
            "nodeId": r.try_get::<String, _>("nodeId")?,
            "nodeName": r.try_get::<String, _>("nodeName")?,
            "enable": r.try_get::<bool, _>("enable")?,
        }));
    }
    Ok(Json(out))
}

async fn reset_devices(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let res = sqlx::query(r#"DELETE FROM "Device" WHERE "clientId" = $1"#)
        .bind(&id)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true, "deleted": res.rows_affected() })))
}

async fn export_uci(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Response> {
    valid_id(&id)?;
    ensure_client_access(&state, &user, &id).await?;
    let (_username, nodes) = collect_client_openwrt_nodes(&state, &id).await?;
    let uci_cmds: Vec<_> = nodes.iter().flat_map(generate_openclash_uci).collect();
    let body = serde_json::to_string_pretty(&uci_cmds)
        .map_err(|e| AppError::internal(format!("UCI serialization failed: {e}")))?;

    let mut res = (StatusCode::OK, body).into_response();
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    Ok(res)
}

async fn export_singbox(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Response> {
    valid_id(&id)?;
    ensure_client_access(&state, &user, &id).await?;
    let c_row = sqlx::query(r#"SELECT uuid, protocols FROM "Client" WHERE id = $1"#)
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("Client not found"))?;
    let uuid: String = c_row.try_get("uuid")?;
    let protocols_val: Value = c_row.try_get("protocols")?;
    let entries = crate::api::sub::load_client_entries(&state, &id, &uuid, &protocols_val).await?;
    let cfg = if !entries.is_empty() {
        mypanel_proto::subscription::render_singbox_outbounds(&entries)
            .map_err(|e| AppError::internal(format!("Sing-box config generation failed: {e}")))?
    } else {
        let (_username, nodes) = collect_client_openwrt_nodes(&state, &id).await?;
        generate_singbox_subscription(&nodes)
            .map_err(|e| AppError::internal(format!("Sing-box config generation failed: {e}")))?
    };

    let mut res = (StatusCode::OK, cfg).into_response();
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    Ok(res)
}

async fn get_links(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    valid_id(&id)?;
    ensure_client_access(&state, &user, &id).await?;

    let r = sqlx::query(
        r#"SELECT uuid, username, "trafficLimit", "uploadTraffic", "downloadTraffic",
                  "expireAt", protocols
           FROM "Client" WHERE id = $1"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Client not found"))?;

    let uuid: String = r.try_get("uuid")?;
    let username: String = r.try_get("username")?;
    let traffic_limit: i64 = r.try_get("trafficLimit")?;
    let upload_traffic: i64 = r.try_get("uploadTraffic")?;
    let download_traffic: i64 = r.try_get("downloadTraffic")?;
    let expire_at: Option<NaiveDateTime> = r.try_get("expireAt").ok().flatten();
    let protocols_val: Value = r.try_get("protocols")?;

    let info = crate::api::sub::build_user_info(
        username,
        uuid.clone(),
        upload_traffic,
        download_traffic,
        traffic_limit,
        expire_at,
        state.cfg.sub_url.clone(),
    );

    let entries = crate::api::sub::load_client_entries(&state, &id, &uuid, &protocols_val).await?;
    let rendered = render_sub_json(&entries, &info)
        .map_err(|e| AppError::internal(format!("Failed to render client links: {e}")))?;
    let payload: Value = serde_json::from_str(&rendered)
        .map_err(|e| AppError::internal(format!("Failed to serialize client links: {e}")))?;
    Ok(Json(payload))
}

async fn export_ssr(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Response> {
    valid_id(&id)?;
    ensure_client_access(&state, &user, &id).await?;
    let (_username, nodes) = collect_client_openwrt_nodes(&state, &id).await?;
    let cfg = generate_ssr_subscription(&nodes)
        .map_err(|e| AppError::internal(format!("SSR config generation failed: {e}")))?;

    let mut res = (StatusCode::OK, cfg).into_response();
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    Ok(res)
}
