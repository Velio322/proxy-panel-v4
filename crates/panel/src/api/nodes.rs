use crate::api::common::{like_pattern, ok, valid_id, AppJson};
use crate::api::inbounds::{
    fetch_inbounds_of_node, fetch_port_shares_bulk, redact_inbound_secrets, to_value,
};
use crate::audit::{self, AuditEntry};
use crate::auth::crypto::sha256_hex;
use crate::auth::extract::AuthUser;
use crate::db::enums::{DbProtocol, NodeStatus, Protocol};
use crate::error::{AppError, AppResult};
use crate::net::{is_loopback_host, user_agent, validate_node_host, ClientIp};
use crate::services::{core_updates, node_client};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{Duration as ChronoDuration, NaiveDateTime};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{QueryBuilder, Row};
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/join-tokens", post(crate::api::node_join::create_join_token))
        .route("/{id}", get(get_one).put(update).delete(remove))
        .route("/{id}/check", post(check))
        .route("/{id}/push-config", post(push_config))
        .route("/{id}/restart", post(restart))
        .route("/{id}/stop", post(stop))
        .route("/{id}/stats", get(stats))
        .route("/{id}/inbounds", get(node_inbounds))
        .route("/{id}/metrics", get(metrics))
        .route("/{id}/install-command", get(install_command))
        .route("/{id}/mtls/issue", post(issue_mtls_cert))
        .route("/{id}/mtls-cert", get(get_mtls_cert).delete(revoke_mtls_cert))
        .route("/{id}/mtls-cert/rotate", post(issue_mtls_cert))
        .route("/{id}/core-update", post(trigger_core_update))
}

pub fn cores_router() -> Router<AppState> {
    Router::new().route("/versions", get(list_core_versions))
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    status: Option<String>,
    search: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateNodeReq {
    name: String,
    host: String,
    port: Option<i32>,
    api_port: Option<i32>,
    secret: String,
    country: Option<String>,
    city: Option<String>,
    isp: Option<String>,
    tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateNodeReq {
    name: Option<String>,
    host: Option<String>,
    port: Option<i32>,
    api_port: Option<i32>,
    secret: Option<String>,
    country: Option<Option<String>>,
    city: Option<Option<String>>,
    isp: Option<Option<String>>,
    tags: Option<Vec<String>>,
    active: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct StatsQuery {
    days: Option<String>,
}

/// Enforce 16..=256 char node secret without whitespace/control characters.
fn check_secret(raw: &str) -> AppResult<String> {
    let s = raw.trim();
    if s.len() < 16
        || s.len() > 256
        || s.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(AppError::validation(
            "secret",
            "secret must be 16 to 256 characters without whitespace",
        ));
    }
    Ok(s.to_string())
}

fn node_row_to_json(r: &sqlx::postgres::PgRow, include_secret: bool) -> AppResult<Value> {
    let created: NaiveDateTime = r.try_get("createdAt")?;
    let updated: NaiveDateTime = r.try_get("updatedAt")?;
    let last_check: Option<NaiveDateTime> = r.try_get("lastCheckAt")?;
    let mtls_exp: Option<NaiveDateTime> = r.try_get("mtlsExpiresAt").ok().flatten();
    let mut obj = json!({
        "id": r.try_get::<String, _>("id")?,
        "name": r.try_get::<String, _>("name")?,
        "host": r.try_get::<String, _>("host")?,
        "port": r.try_get::<i32, _>("port")?,
        "apiPort": r.try_get::<i32, _>("apiPort")?,
        "country": r.try_get::<Option<String>, _>("country")?,
        "city": r.try_get::<Option<String>, _>("city")?,
        "isp": r.try_get::<Option<String>, _>("isp")?,
        "status": r.try_get::<NodeStatus, _>("status")?,
        "lastCheckAt": last_check.map(|d| crate::db::ts::format_iso(&d)),
        "lastPingMs": r.try_get::<Option<i32>, _>("lastPingMs")?,
        "uptime": r.try_get::<Option<i64>, _>("uptime").ok().flatten(),
        "version": r.try_get::<Option<String>, _>("version")?,
        "cpuUsage": r.try_get::<Option<f64>, _>("cpuUsage")?,
        "memUsage": r.try_get::<Option<f64>, _>("memUsage")?,
        "netUpload": r.try_get::<i64, _>("netUpload")?,
        "netDownload": r.try_get::<i64, _>("netDownload")?,
        "xrayVersion": r.try_get::<Option<String>, _>("xrayVersion")?,
        "singboxVersion": r.try_get::<Option<String>, _>("singboxVersion")?,
        "naiveVersion": r.try_get::<Option<String>, _>("naiveVersion").ok().flatten(),
        "mieruVersion": r.try_get::<Option<String>, _>("mieruVersion").ok().flatten(),
        "telemtVersion": r.try_get::<Option<String>, _>("telemtVersion").ok().flatten(),
        "arch": r.try_get::<Option<String>, _>("arch").ok().flatten().unwrap_or_else(|| "amd64".to_string()),
        "features": r.try_get::<Value, _>("features")?,
        "tags": r.try_get::<Option<Vec<String>>, _>("tags")?.unwrap_or_default(),
        "apiMode": r.try_get::<String, _>("apiMode").unwrap_or_else(|_| "ws".to_string()),
        "mtlsFingerprint": r.try_get::<Option<String>, _>("mtlsFingerprint").ok().flatten(),
        "mtlsExpiresAt": mtls_exp.map(|d| crate::db::ts::format_iso(&d)),
        "active": r.try_get::<bool, _>("active")?,
        "createdAt": crate::db::ts::format_iso(&created),
        "updatedAt": crate::db::ts::format_iso(&updated),
    });
    if include_secret {
        if let Some(map) = obj.as_object_mut() {
            map.insert("secret".into(), json!(r.try_get::<String, _>("secret")?));
        }
    }
    Ok(obj)
}

async fn list(
    State(state): State<AppState>,
    _user: AuthUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<Value>>> {
    let mut qb = QueryBuilder::new(
        r#"SELECT id, name, host, port, "apiPort", secret, country, city, isp, status,
                  "lastCheckAt", "lastPingMs", uptime, version, "cpuUsage", "memUsage", "netUpload", "netDownload",
                  "xrayVersion", "singboxVersion", "naiveVersion", "mieruVersion", "telemtVersion", arch,
                  features, COALESCE(tags, '{}') AS tags,
                  "mtlsFingerprint", "mtlsExpiresAt", active, "createdAt", "updatedAt"
           FROM "Node" WHERE 1=1"#,
    );
    if let Some(ref st) = q.status.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let status = NodeStatus::from_str(st).map_err(AppError::bad_request)?;
        qb.push(" AND status = ").push_bind(status);
    }
    if let Some(ref s) = q.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let pat = like_pattern(s);
        qb.push(" AND (name ILIKE ")
            .push_bind(pat.clone())
            .push(" OR host ILIKE ")
            .push_bind(pat)
            .push(")");
    }
    qb.push(r#" ORDER BY "createdAt" DESC"#);

    let rows = qb.build().fetch_all(&state.db).await?;
    let node_ids: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String, _>("id").ok())
        .collect();

    let mut inbounds_map: HashMap<String, Vec<Value>> = HashMap::new();
    let mut metrics_map: HashMap<String, Vec<Value>> = HashMap::new();

    if !node_ids.is_empty() {
        let ib_rows = sqlx::query(
            r#"SELECT id, "nodeId", protocol, tag, enable, port
               FROM "Inbound" WHERE "nodeId" = ANY($1::text[]) ORDER BY port ASC"#,
        )
        .bind(&node_ids)
        .fetch_all(&state.db)
        .await?;
        for ir in ib_rows {
            let nid: String = ir.try_get("nodeId")?;
            let proto: Protocol = ir.try_get::<DbProtocol, _>("protocol")?.into();
            inbounds_map.entry(nid).or_default().push(json!({
                "id": ir.try_get::<String, _>("id")?,
                "protocol": proto,
                "tag": ir.try_get::<String, _>("tag")?,
                "enable": ir.try_get::<bool, _>("enable")?,
                "port": ir.try_get::<i32, _>("port")?
            }));
        }

        // Windowed query fetching at most 12 latest metrics per node
        let m_rows = sqlx::query(
            r#"SELECT "nodeId", "cpuUsage", "memUsage", "tcpConnections", "udpSockets", "uploadBps", "downloadBps", uptime, "recordedAt"
               FROM (
                   SELECT "nodeId", "cpuUsage", "memUsage", "tcpConnections", "udpSockets", "uploadBps", "downloadBps", uptime, "recordedAt",
                          ROW_NUMBER() OVER (PARTITION BY "nodeId" ORDER BY "recordedAt" DESC, id DESC) AS rn
                   FROM "NodeMetric"
                   WHERE "nodeId" = ANY($1::text[])
               ) ranked
               WHERE rn <= 12
               ORDER BY "recordedAt" DESC"#,
        )
        .bind(&node_ids)
        .fetch_all(&state.db)
        .await?;

        for mr in m_rows {
            let nid: String = mr.try_get("nodeId")?;
            let rec: NaiveDateTime = mr.try_get("recordedAt")?;
            metrics_map.entry(nid).or_default().push(json!({
                "cpuUsage": mr.try_get::<Option<f64>, _>("cpuUsage")?,
                "memUsage": mr.try_get::<Option<f64>, _>("memUsage")?,
                "tcpConnections": mr.try_get::<Option<i32>, _>("tcpConnections")?,
                "udpSockets": mr.try_get::<Option<i32>, _>("udpSockets")?,
                "uploadBps": mr.try_get::<Option<f64>, _>("uploadBps")?,
                "downloadBps": mr.try_get::<Option<f64>, _>("downloadBps")?,
                "uptime": mr.try_get::<Option<i32>, _>("uptime").ok().flatten(),
                "recordedAt": crate::db::ts::format_iso(&rec)
            }));
        }
    }

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let nid: String = r.try_get("id")?;
        let mut obj = node_row_to_json(&r, false)?;
        let ibs = inbounds_map.remove(&nid).unwrap_or_default();
        let ib_count = ibs.len();
        let ms = metrics_map.remove(&nid).unwrap_or_default();
        if let Some(m) = obj.as_object_mut() {
            m.insert("inbounds".into(), Value::Array(ibs));
            m.insert("nodeMetrics".into(), Value::Array(ms));
            m.insert("_count".into(), json!({ "inbounds": ib_count }));
        }
        out.push(obj);
    }
    Ok(Json(out))
}

async fn get_one(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    valid_id(&id)?;
    let r = sqlx::query(
        r#"SELECT id, name, host, port, "apiPort", secret, country, city, isp, status,
                  "lastCheckAt", "lastPingMs", uptime, version, "cpuUsage", "memUsage", "netUpload", "netDownload",
                  "xrayVersion", "singboxVersion", "naiveVersion", "mieruVersion", "telemtVersion", arch,
                  features, COALESCE(tags, '{}') AS tags,
                  "mtlsFingerprint", "mtlsExpiresAt", active, "createdAt", "updatedAt"
           FROM "Node" WHERE id = $1"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Node not found"))?;

    let mut obj = node_row_to_json(&r, false)?;
    let inbounds = fetch_inbounds_of_node(&state.db, &id).await?;
    let ib_ids: Vec<String> = inbounds.iter().map(|i| i.id.clone()).collect();
    let shares = fetch_port_shares_bulk(&state.db, &ib_ids).await?;

    let mut ib_vals = Vec::with_capacity(inbounds.len());
    for ib in inbounds {
        let mut iv = to_value(&ib)?;
        redact_inbound_secrets(&mut iv, user.is_admin());
        if let Some(m) = iv.as_object_mut() {
            let mut svs = shares.get(&ib.id).cloned().unwrap_or_default();
            svs.iter_mut()
                .for_each(|sv| redact_inbound_secrets(sv, user.is_admin()));
            m.insert("portShares".into(), Value::Array(svs));
        }
        ib_vals.push(iv);
    }

    let traffic_logs: i64 =
        sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM (SELECT 1 FROM "TrafficLog" WHERE "nodeId" = $1 LIMIT 100001) s"#,
        )
            .bind(&id)
            .fetch_one(&state.db)
            .await?;

    if let Some(m) = obj.as_object_mut() {
        let ib_len = ib_vals.len();
        m.insert("inbounds".into(), Value::Array(ib_vals));
        m.insert(
            "_count".into(),
            json!({ "inbounds": ib_len, "trafficLogs": traffic_logs }),
        );
    }
    Ok(Json(obj))
}

async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<CreateNodeReq>,
) -> AppResult<Response> {
    user.require_admin()?;
    if body.name.trim().is_empty() {
        return Err(AppError::validation("name", "Node name is required"));
    }
    validate_node_host(&body.host, state.cfg.allow_private_nodes)?;
    let secret = check_secret(&body.secret)?;
    let port = body.port.unwrap_or(443);
    let api_port = body.api_port.unwrap_or(2222);
    if !(1..=65535).contains(&port) || !(1..=65535).contains(&api_port) {
        return Err(AppError::validation("port", "Ports must be 1..=65535"));
    }

    let is_local = is_loopback_host(&body.host);
    let reachable =
        is_local || node_client::tcp_probe(&body.host, api_port as u16, Duration::from_millis(1500)).await;

    let id = crate::db::new_id();
    let now = crate::db::now();
    let secret_hash = sha256_hex(&secret);
    let tags = body.tags.unwrap_or_default();

    let r = sqlx::query(
        r#"INSERT INTO "Node" (id, name, host, port, "apiPort", secret, "secretHash", country, city, isp, tags, active, "createdAt", "updatedAt")
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, true, $12, $12)
           RETURNING id, name, host, port, "apiPort", secret, country, city, isp, status, "lastCheckAt", "lastPingMs", uptime, version, "cpuUsage", "memUsage", "netUpload", "netDownload", "xrayVersion", "singboxVersion", "naiveVersion", "mieruVersion", "telemtVersion", arch, features, COALESCE(tags, '{}') AS tags, "mtlsFingerprint", "mtlsExpiresAt", active, "createdAt", "updatedAt""#,
    )
    .bind(&id)
    .bind(body.name.trim())
    .bind(body.host.trim())
    .bind(port)
    .bind(api_port)
    .bind(&secret)
    .bind(&secret_hash)
    .bind(&body.country)
    .bind(&body.city)
    .bind(&body.isp)
    .bind(&tags)
    .bind(now)
    .fetch_one(&state.db)
    .await?;

    state.invalidate_derived();
    audit::spawn(
        &state.db,
        AuditEntry::new("CREATE", "node")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    let mut out = node_row_to_json(&r, true)?;
    let install_payload = build_install_payload(
        &state,
        &headers,
        &id,
        body.name.trim(),
        body.host.trim(),
        port,
        api_port,
        &secret,
    );
    if let (Some(dst), Some(src)) = (out.as_object_mut(), install_payload.as_object()) {
        dst.insert("reachable".into(), json!(reachable));
        for key in [
            "isDomain",
            "panelUrl",
            "wsUrl",
            "scriptUrl",
            "binaryUrl",
            "command",
            "installCommand",
            "setupInstructions",
        ] {
            if let Some(val) = src.get(key) {
                dst.insert(key.into(), val.clone());
            }
        }
    }

    Ok((StatusCode::CREATED, Json(out)).into_response())
}

async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<UpdateNodeReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;

    let ex = sqlx::query(
        r#"SELECT name, host, port, "apiPort", secret, "secretHash", country, city, isp, COALESCE(tags, '{}') AS tags, active
           FROM "Node" WHERE id = $1"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Node not found"))?;

    let host = match body.host {
        Some(ref h) => {
            validate_node_host(h, state.cfg.allow_private_nodes)?;
            h.trim().to_string()
        }
        None => ex.try_get("host")?,
    };
    let secret = match body.secret {
        Some(ref s) => check_secret(s)?,
        None => ex.try_get("secret")?,
    };
    let secret_hash = sha256_hex(&secret);
    let now = crate::db::now();

    let r = sqlx::query(
        r#"UPDATE "Node"
           SET name = $2, host = $3, port = $4, "apiPort" = $5, secret = $6, "secretHash" = $7,
               country = $8, city = $9, isp = $10, tags = $11, active = $12, "updatedAt" = $13
           WHERE id = $1
           RETURNING id, name, host, port, "apiPort", secret, country, city, isp, status, "lastCheckAt", "lastPingMs", uptime, version, "cpuUsage", "memUsage", "netUpload", "netDownload", "xrayVersion", "singboxVersion", "naiveVersion", "mieruVersion", "telemtVersion", arch, features, COALESCE(tags, '{}') AS tags, "mtlsFingerprint", "mtlsExpiresAt", active, "createdAt", "updatedAt""#,
    )
    .bind(&id)
    .bind(body.name.unwrap_or_else(|| ex.try_get("name").unwrap_or_default()))
    .bind(host)
    .bind(body.port.unwrap_or_else(|| ex.try_get("port").unwrap_or(443)))
    .bind(body.api_port.unwrap_or_else(|| ex.try_get("apiPort").unwrap_or(2222)))
    .bind(secret)
    .bind(secret_hash)
    .bind(body.country.unwrap_or_else(|| ex.try_get("country").ok().flatten()))
    .bind(body.city.unwrap_or_else(|| ex.try_get("city").ok().flatten()))
    .bind(body.isp.unwrap_or_else(|| ex.try_get("isp").ok().flatten()))
    .bind(body.tags.unwrap_or_else(|| ex.try_get("tags").unwrap_or_default()))
    .bind(body.active.unwrap_or_else(|| ex.try_get("active").unwrap_or(true)))
    .bind(now)
    .fetch_one(&state.db)
    .await?;

    // Invalidate cached subscriptions/dashboard when node host/port/active changes
    state.invalidate_derived();

    audit::spawn(
        &state.db,
        AuditEntry::new("UPDATE", "node")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );
    Ok(Json(node_row_to_json(&r, true)?))
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
    let res = sqlx::query(r#"DELETE FROM "Node" WHERE id = $1"#)
        .bind(&id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("Node not found"));
    }
    state.invalidate_derived();
    audit::spawn(
        &state.db,
        AuditEntry::new("DELETE", "node")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );
    Ok(ok())
}

async fn check(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let node = node_client::NodeTarget::fetch(&state, &id).await?;
    let res = node_client::check_status(&state, &node).await?;
    Ok(Json(res))
}

async fn push_config(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let node = node_client::NodeTarget::fetch(&state, &id).await?;
    let mut success = false;
    let mut transport = "none";
    if state.hub.is_connected(&id)
        && crate::services::outbox::push_config(&state, &id).await.is_ok()
    {
        success = true;
        transport = "websocket";
    }
    if !success && node_client::push_config_http(&state, &node).await.unwrap_or(false) {
        success = true;
        transport = "http";
    }
    audit::spawn(
        &state.db,
        AuditEntry::new("PUSH_CONFIG", "node")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );
    Ok(Json(json!({ "success": success, "transport": transport })))
}

async fn restart(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let node = node_client::NodeTarget::fetch(&state, &id).await?;
    let success = node_client::restart_node(&state, &node).await.unwrap_or(false);
    audit::spawn(
        &state.db,
        AuditEntry::new("RESTART", "node")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );
    Ok(Json(json!({ "success": success })))
}

async fn stop(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let node = node_client::NodeTarget::fetch(&state, &id).await?;
    let success = node_client::stop_node(&state, &node).await.unwrap_or(false);
    audit::spawn(
        &state.db,
        AuditEntry::new("STOP", "node")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );
    Ok(Json(json!({ "success": success })))
}

async fn stats(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Query(q): Query<StatsQuery>,
) -> AppResult<Json<Vec<Value>>> {
    user.require_admin()?;
    valid_id(&id)?;
    let days = q
        .days
        .as_deref()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(7)
        .clamp(1, 366);
    let start = crate::db::now() - ChronoDuration::days(days);
    let rows = sqlx::query(
        r#"SELECT "clientId", COALESCE(SUM(upload), 0)::bigint AS up, COALESCE(SUM(download), 0)::bigint AS down
           FROM "TrafficLog" WHERE "nodeId" = $1 AND "recordAt" >= $2
           GROUP BY "clientId""#,
    )
    .bind(&id)
    .bind(start)
    .fetch_all(&state.db)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(json!({
            "clientId": r.try_get::<String, _>("clientId")?,
            "_sum": {
                "upload": r.try_get::<i64, _>("up")?,
                "download": r.try_get::<i64, _>("down")?
            }
        }));
    }
    Ok(Json(out))
}

async fn node_inbounds(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<Value>>> {
    valid_id(&id)?;
    let inbounds = fetch_inbounds_of_node(&state.db, &id).await?;
    let ib_ids: Vec<String> = inbounds.iter().map(|i| i.id.clone()).collect();
    let shares = fetch_port_shares_bulk(&state.db, &ib_ids).await?;

    let mut out = Vec::with_capacity(inbounds.len());
    for row in inbounds {
        let mut v = to_value(&row)?;
        redact_inbound_secrets(&mut v, user.is_admin());
        if let Some(obj) = v.as_object_mut() {
            let mut svs = shares.get(&row.id).cloned().unwrap_or_default();
            svs.iter_mut()
                .for_each(|sv| redact_inbound_secrets(sv, user.is_admin()));
            obj.insert("portShares".into(), Value::Array(svs));
        }
        out.push(v);
    }
    Ok(Json(out))
}

async fn metrics(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let node = node_client::NodeTarget::fetch(&state, &id).await?;
    let m = node_client::get_metrics(&state, &node).await?;
    Ok(Json(m.unwrap_or(Value::Null)))
}

async fn issue_mtls_cert(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let node = node_client::NodeTarget::fetch(&state, &id).await?;
    let issued = state.mtls_ca.issue_node_cert(&node.id, &node.host)?;

    sqlx::query(
        r#"UPDATE "Node"
           SET "mtlsCertPem" = $2, "mtlsSerial" = $3, "mtlsFingerprint" = $4, "mtlsExpiresAt" = $5, "updatedAt" = $6
           WHERE id = $1"#,
    )
    .bind(&id)
    .bind(&issued.cert_pem)
    .bind(&issued.serial)
    .bind(&issued.fingerprint)
    .bind(issued.expires_at)
    .bind(crate::db::now())
    .execute(&state.db)
    .await?;

    audit::spawn(
        &state.db,
        AuditEntry::new("ISSUE_MTLS_CERT", "node")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    Ok(Json(json!({
        "nodeId": issued.node_id,
        "certPem": issued.cert_pem,
        "keyPem": issued.key_pem,
        "caPem": issued.ca_pem,
        "serial": issued.serial,
        "fingerprint": issued.fingerprint,
        "subject": issued.subject,
        "expiresAt": crate::db::ts::format_iso(&issued.expires_at)
    })))
}

fn shell_single_quote(s: &str) -> String {
    s.replace('\'', "'\\''")
}

fn is_domain_host(host: &str) -> bool {
    let h = host.trim().trim_matches(|c| c == '[' || c == ']');
    !h.is_empty() && h.parse::<std::net::IpAddr>().is_err()
}

pub(crate) fn resolve_panel_urls(state: &AppState, headers: &HeaderMap) -> (String, String) {
    let req_host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(axum::http::header::HOST))
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let panel_url = if let Some(host_hdr) = req_host {
        let scheme = headers
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .map(str::trim)
            .filter(|&s| s == "http" || s == "https")
            .unwrap_or_else(|| {
                if state.cfg.api_url.starts_with("http://")
                    && (host_hdr.starts_with("127.") || host_hdr.starts_with("localhost"))
                {
                    "http"
                } else {
                    "https"
                }
            });
        format!("{scheme}://{host_hdr}")
    } else {
        state.cfg.api_url.trim_end_matches('/').to_string()
    };

    let ws_url = if let Some(rest) = panel_url.strip_prefix("https://") {
        format!("wss://{rest}/ws/worker")
    } else if let Some(rest) = panel_url.strip_prefix("http://") {
        format!("ws://{rest}/ws/worker")
    } else {
        format!("wss://{panel_url}/ws/worker")
    };
    (panel_url, ws_url)
}

#[allow(clippy::too_many_arguments)]
fn build_install_payload(
    state: &AppState,
    headers: &HeaderMap,
    id: &str,
    name: &str,
    host: &str,
    port: i32,
    api_port: i32,
    secret: &str,
) -> Value {
    let (panel_url, ws_url) = resolve_panel_urls(state, headers);
    let script_url = format!("{panel_url}/install.sh");
    let binary_url = format!("{panel_url}/api/v1/nodes/self/binary");
    let is_domain = is_domain_host(host);

    let q_panel = shell_single_quote(&panel_url);
    let q_host = shell_single_quote(host);
    let q_name = shell_single_quote(name);
    let q_secret = shell_single_quote(secret);

    let mut flags = format!(
        "--mode node --master-url '{q_panel}' --node-domain '{q_host}' --node-name '{q_name}' --node-secret '{q_secret}'"
    );
    if api_port != 2222 {
        flags.push_str(&format!(" --api-port '{api_port}'"));
    }

    let command = format!("curl -fsSL '{q_panel}/install.sh' | sudo bash -s -- {flags}");
    let setup_instructions = format!(
        "curl -fsSL '{q_panel}/install.sh' -o /tmp/mypanel3-install.sh\n\
         sudo bash /tmp/mypanel3-install.sh {flags}\n\
         rm -f /tmp/mypanel3-install.sh\n\n\
         # Optional TLS on worker API (:{api_port}) if node host is a domain with ports 80 & {api_port} open:\n\
         #   append: --tls-mode port2222 --email admin@example.com\n\
         # By default (--tls-mode none), port 443 remains free for VLESS Reality and the agent connects via outbound WSS."
    );

    json!({
        "nodeId": id,
        "name": name,
        "host": host,
        "port": port,
        "apiPort": api_port,
        "secret": secret,
        "isDomain": is_domain,
        "panelUrl": panel_url,
        "wsUrl": ws_url,
        "scriptUrl": script_url,
        "binaryUrl": binary_url,
        "command": command,
        "installCommand": command,
        "setupInstructions": setup_instructions,
    })
}

async fn install_command(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let row = sqlx::query(
        r#"SELECT id, name, host, port, "apiPort", secret FROM "Node" WHERE id = $1"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Node not found"))?;

    let name: String = row.try_get("name")?;
    let host: String = row.try_get("host")?;
    let port: i32 = row.try_get("port")?;
    let api_port: i32 = row.try_get("apiPort")?;
    let secret: String = row.try_get("secret")?;

    Ok(Json(build_install_payload(
        &state,
        &headers,
        &id,
        &name,
        &host,
        port,
        api_port,
        &secret,
    )))
}

async fn get_mtls_cert(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let row = sqlx::query(
        r#"SELECT id, "mtlsCertPem", "mtlsSerial", "mtlsFingerprint", "mtlsExpiresAt" FROM "Node" WHERE id = $1"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Node not found"))?;

    let cert_pem: Option<String> = row.try_get("mtlsCertPem")?;
    let serial: Option<String> = row.try_get("mtlsSerial")?;
    let fingerprint: Option<String> = row.try_get("mtlsFingerprint")?;
    let expires_at: Option<NaiveDateTime> = row.try_get("mtlsExpiresAt")?;

    Ok(Json(json!({
        "nodeId": id,
        "issued": cert_pem.is_some(),
        "certPem": cert_pem,
        "serial": serial,
        "fingerprint": fingerprint,
        "expiresAt": expires_at.map(|d| crate::db::ts::format_iso(&d)),
    })))
}

async fn revoke_mtls_cert(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let res = sqlx::query(
        r#"UPDATE "Node"
           SET "mtlsCertPem" = NULL, "mtlsSerial" = NULL, "mtlsFingerprint" = NULL, "mtlsExpiresAt" = NULL, "updatedAt" = $2
           WHERE id = $1"#,
    )
    .bind(&id)
    .bind(crate::db::now())
    .execute(&state.db)
    .await?;

    if res.rows_affected() == 0 {
        return Err(AppError::not_found("Node not found"));
    }

    audit::spawn(
        &state.db,
        AuditEntry::new("REVOKE_MTLS_CERT", "node")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );
    Ok(ok())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoreUpdateReq {
    core: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    download_url: Option<String>,
    #[serde(default)]
    sha256: Option<String>,
}

async fn trigger_core_update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<CoreUpdateReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let core = body.core.trim().to_string();
    if core.is_empty() {
        return Err(AppError::bad_request("core is required"));
    }

    let outcome = core_updates::trigger_core_update(
        &state,
        &id,
        core,
        body.version,
        body.download_url,
        body.sha256,
    )
    .await?;

    audit::spawn(
        &state.db,
        AuditEntry::new("CORE_UPDATE", "node")
            .user(&user.id)
            .resource_id(&id)
            .details(json!({
                "core": outcome.core,
                "version": outcome.version,
                "sha256": outcome.sha256,
                "downloadUrl": outcome.download_url,
            }))
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    Ok(Json(json!({
        "ok": true,
        "success": true,
        "core": outcome.core,
        "version": outcome.version,
        "sha256": outcome.sha256,
        "message": outcome.message,
    })))
}

#[derive(Debug, Deserialize)]
struct CoreVersionsQuery {
    refresh: Option<bool>,
    arch: Option<String>,
}

async fn list_core_versions(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<CoreVersionsQuery>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    let rows = sqlx::query(
        r#"SELECT id, name, status, arch, "xrayVersion", "singboxVersion", "naiveVersion", "mieruVersion", "telemtVersion"
           FROM "Node" WHERE active = true ORDER BY name ASC"#,
    )
    .fetch_all(&state.db)
    .await?;

    let mut nodes = Vec::with_capacity(rows.len());
    let mut default_arch: Option<String> = None;
    for r in rows {
        let arch = r
            .try_get::<Option<String>, _>("arch")
            .ok()
            .flatten()
            .unwrap_or_else(|| "amd64".to_string());
        if default_arch.is_none() {
            default_arch = Some(arch.clone());
        }
        nodes.push(json!({
            "id": r.try_get::<String, _>("id")?,
            "name": r.try_get::<String, _>("name")?,
            "status": r.try_get::<NodeStatus, _>("status")?,
            "arch": arch,
            "xrayVersion": r.try_get::<Option<String>, _>("xrayVersion")?,
            "singboxVersion": r.try_get::<Option<String>, _>("singboxVersion")?,
            "naiveVersion": r.try_get::<Option<String>, _>("naiveVersion").ok().flatten(),
            "mieruVersion": r.try_get::<Option<String>, _>("mieruVersion").ok().flatten(),
            "telemtVersion": r.try_get::<Option<String>, _>("telemtVersion").ok().flatten(),
        }));
    }

    let arch = core_updates::normalize_arch(q.arch.as_deref().or(default_arch.as_deref()));
    let catalog = core_updates::list_catalog(&state, arch, q.refresh.unwrap_or(false)).await;

    Ok(Json(json!({
        "cores": catalog,
        "nodes": nodes,
    })))
}
