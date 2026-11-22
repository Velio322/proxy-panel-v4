use crate::api::common::{AppJson, OptionalJson};
use crate::auth::crypto::{ct_eq, sha256_hex};
use crate::db::enums::NodeStatus;
use crate::error::{AppError, AppResult};
use crate::net::ClientIp;
use crate::services::{core_updates, node_config, outbox, traffic_batcher};
use crate::state::AppState;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap};
use axum::routing::{get, post};
use axum::{Json, Router};
use mypanel_proto::{TrafficReport, MAX_SAFE_BYTES, MAX_TRAFFIC_ENTRIES, MAX_TRAFFIC_KEY_LEN};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/register", post(register))
        .route("/heartbeat", post(heartbeat))
        .route("/config", get(get_config))
        .route("/binary", get(crate::api::install_assets::download_node_binary))
        .route("/traffic", post(report_traffic))
        .route("/alert", post(report_alert))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigQuery {
    node_id: String,
}

async fn authenticate_node(
    state: &AppState,
    headers: &HeaderMap,
    node_id: &str,
) -> AppResult<()> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .or_else(|| {
            headers
                .get("x-node-secret")
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
        })
        .unwrap_or("");

    if token.len() < 16 || token.len() > 256 || node_id.is_empty() {
        return Err(AppError::unauthorized("Unauthorized"));
    }

    let secret_hash = sha256_hex(token);
    let row = sqlx::query(
        r#"SELECT secret FROM "Node" WHERE id = $1 AND active = true AND ("secretHash" = $2 OR secret = $3)"#,
    )
    .bind(node_id)
    .bind(&secret_hash)
    .bind(token)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::service_unavailable("Authentication unavailable"))?
    .ok_or_else(|| AppError::unauthorized("Unauthorized"))?;

    let db_secret: String = row.try_get("secret")?;
    if !ct_eq(token, &db_secret) {
        return Err(AppError::unauthorized("Unauthorized"));
    }
    Ok(())
}

/// Validates token length 16..=256 and uses indexed `secretHash` lookup.
async fn register(
    State(state): State<AppState>,
    OptionalJson(body): OptionalJson<Value>,
) -> AppResult<Json<Value>> {
    let token = body
        .get("token")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if token.len() < 16 || token.len() > 256 {
        return Err(AppError::unauthorized("Invalid auth token"));
    }

    let secret_hash = sha256_hex(token);
    let rows = sqlx::query(
        r#"SELECT id, name, host, port, "apiPort", secret, version
           FROM "Node"
           WHERE active = true AND ("secretHash" = $1 OR secret = $2)
           LIMIT 2"#,
    )
    .bind(&secret_hash)
    .bind(token)
    .fetch_all(&state.db)
    .await?;

    let matched: Vec<_> = rows
        .into_iter()
        .filter(|r| {
            r.try_get::<String, _>("secret")
                .is_ok_and(|s| ct_eq(token, &s))
        })
        .collect();

    if matched.len() != 1 {
        return Err(AppError::unauthorized(
            "Provision a node with a unique secret before enrollment",
        ));
    }

    let existing = &matched[0];
    let node_id: String = existing.try_get("id")?;
    let cur_name: String = existing.try_get("name")?;
    let cur_host: String = existing.try_get("host")?;
    let cur_port: i32 = existing.try_get("port")?;
    let cur_api_port: i32 = existing.try_get("apiPort")?;
    let cur_ver: Option<String> = existing.try_get("version")?;

    let req_host = body.get("host").and_then(Value::as_str).map(str::trim);
    let reported_host = if !cur_host.trim().is_empty() && cur_host != "0.0.0.0" && cur_host != "::" {
        cur_host
    } else {
        match req_host {
            Some(h) if !h.is_empty() && h != "0.0.0.0" && h != "::" => h.to_string(),
            _ => cur_host,
        }
    };
    let name = if !cur_name.trim().is_empty() {
        cur_name
    } else {
        body.get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .unwrap_or(cur_name)
    };
    let port = if cur_port > 0 {
        cur_port
    } else {
        body.get("port")
            .and_then(Value::as_i64)
            .and_then(|v| i32::try_from(v).ok())
            .filter(|&p| (1..=65535).contains(&p))
            .unwrap_or(443)
    };
    let api_port = if cur_api_port > 0 {
        cur_api_port
    } else {
        body.get("apiPort")
            .and_then(Value::as_i64)
            .and_then(|v| i32::try_from(v).ok())
            .filter(|&p| (1..=65535).contains(&p))
            .unwrap_or(2222)
    };
    let version = body
        .pointer("/system/release")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or(cur_ver);
    let core_info = core_updates::extract_core_info(body.get("cores"));

    let now = crate::db::now();
    sqlx::query(
        r#"UPDATE "Node"
           SET name = $2, host = $3, port = $4, "apiPort" = $5, "secretHash" = $6,
               status = 'ONLINE', "lastCheckAt" = $7, version = $8,
               "xrayVersion" = COALESCE($9, "xrayVersion"),
               "singboxVersion" = COALESCE($10, "singboxVersion"),
               "naiveVersion" = COALESCE($11, "naiveVersion"),
               "mieruVersion" = COALESCE($12, "mieruVersion"),
               arch = COALESCE($13, arch),
               "updatedAt" = $7
           WHERE id = $1"#,
    )
    .bind(&node_id)
    .bind(&name)
    .bind(reported_host)
    .bind(port)
    .bind(api_port)
    .bind(&secret_hash)
    .bind(now)
    .bind(version)
    .bind(&core_info.xray)
    .bind(&core_info.singbox)
    .bind(&core_info.naive)
    .bind(&core_info.mieru)
    .bind(&core_info.arch)
    .execute(&state.db)
    .await?;

    state.metrics.set_node_status(&node_id, &name, true);
    let state_bg = state.clone();
    let node_id_bg = node_id.clone();
    tokio::spawn(async move {
        let _ = outbox::push_config(&state_bg, &node_id_bg).await;
    });

    Ok(Json(json!({
        "nodeId": node_id,
        "status": "re-registered",
        "message": "Node updated successfully"
    })))
}

async fn heartbeat(
    State(state): State<AppState>,
    headers: HeaderMap,
    AppJson(body): AppJson<Value>,
) -> AppResult<Json<Value>> {
    let node_id = body
        .get("nodeId")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::unauthorized("Unauthorized"))?;
    authenticate_node(&state, &headers, node_id).await?;

    let status = body
        .get("status")
        .and_then(Value::as_str)
        .and_then(|s| s.parse::<NodeStatus>().ok())
        .unwrap_or(NodeStatus::Online);
    let cpu = body.get("cpuUsage").and_then(Value::as_f64);
    let mem = body.get("memUsage").and_then(Value::as_f64);
    let connections = body
        .get("connections")
        .and_then(Value::as_i64)
        .and_then(|v| i32::try_from(v).ok())
        .unwrap_or(0);
    let uptime_secs = crate::services::node_client::extract_uptime_seconds(&body, None);
    let uptime_metric = uptime_secs.unwrap_or(0).clamp(0, i32::MAX as i64) as i32;
    let core_info = core_updates::extract_core_info(body.get("cores"));

    let now = crate::db::now();
    sqlx::query(
        r#"UPDATE "Node"
           SET status = $2, "cpuUsage" = $3, "memUsage" = $4,
               "xrayVersion" = COALESCE($5, "xrayVersion"),
               "singboxVersion" = COALESCE($6, "singboxVersion"),
               "naiveVersion" = COALESCE($7, "naiveVersion"),
               "mieruVersion" = COALESCE($8, "mieruVersion"),
               arch = COALESCE($9, arch),
               uptime = COALESCE($11, uptime),
               "lastCheckAt" = $10, "lastPingMs" = NULL, "updatedAt" = $10
           WHERE id = $1"#,
    )
    .bind(node_id)
    .bind(status)
    .bind(cpu)
    .bind(mem)
    .bind(&core_info.xray)
    .bind(&core_info.singbox)
    .bind(&core_info.naive)
    .bind(&core_info.mieru)
    .bind(&core_info.arch)
    .bind(now)
    .bind(uptime_secs)
    .execute(&state.db)
    .await?;

    let _ = sqlx::query(
        r#"INSERT INTO "NodeMetric" (id, "nodeId", "cpuUsage", "memUsage", "netUpload", "netDownload", connections, uptime, "recordedAt")
           VALUES ($1, $2, $3, $4, 0, 0, $5, $6, $7)"#,
    )
    .bind(crate::db::new_id())
    .bind(node_id)
    .bind(cpu)
    .bind(mem)
    .bind(connections)
    .bind(uptime_metric)
    .bind(now)
    .execute(&state.db)
    .await;

    if let (Some(c), Some(m)) = (cpu, mem) {
        state.metrics.set_node_telemetry(node_id, c, m);
    }

    Ok(Json(json!({ "status": "ok" })))
}

async fn get_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ConfigQuery>,
) -> AppResult<Json<Value>> {
    authenticate_node(&state, &headers, &q.node_id).await?;
    let node_name: Option<String> =
        sqlx::query_scalar(r#"SELECT name FROM "Node" WHERE id = $1"#)
            .bind(&q.node_id)
            .fetch_optional(&state.db)
            .await?;

    let inbounds = node_config::build(&state, &q.node_id).await?;
    let routing_rules = node_config::build_routing_rules(&state, &q.node_id).await?;
    let xray_profile = node_config::build_xray_profile(&state, &q.node_id).await?;

    Ok(Json(json!({
        "nodeId": q.node_id,
        "nodeName": node_name,
        "inbounds": inbounds,
        "routingRules": routing_rules,
        "xrayProfile": xray_profile,
        "timestamp": chrono::Utc::now().timestamp_millis()
    })))
}

async fn report_traffic(
    State(state): State<AppState>,
    headers: HeaderMap,
    AppJson(body): AppJson<Value>,
) -> AppResult<Json<Value>> {
    let node_id = body
        .get("nodeId")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("nodeId and stats required"))?;
    authenticate_node(&state, &headers, node_id).await?;

    if let Ok(report) = serde_json::from_value::<TrafficReport>(body.clone()) {
        report
            .validate()
            .map_err(|e| AppError::bad_request(e.to_string()))?;
        let batch_id = report.batch_id.clone();
        let count = report.stats.len();
        for (email, stat) in report.stats {
            traffic_batcher::add_traffic(
                &state,
                node_id,
                &email,
                i64::try_from(stat.upload).unwrap_or(0),
                i64::try_from(stat.download).unwrap_or(0),
                stat.unmetered,
                None,
                batch_id.clone(),
                None,
                None,
            )
            .await;
        }
        return Ok(Json(json!({ "status": "ok", "processed": count })));
    }

    let stats_obj = body
        .get("stats")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::bad_request("nodeId and stats required"))?;
    if stats_obj.len() > MAX_TRAFFIC_ENTRIES {
        return Err(AppError::bad_request("Too many traffic entries"));
    }

    for (email, st) in stats_obj {
        if email.is_empty() || email.len() > MAX_TRAFFIC_KEY_LEN {
            return Err(AppError::bad_request("Invalid traffic key"));
        }
        let up = st.get("upload").and_then(Value::as_u64).unwrap_or(0);
        let down = st.get("download").and_then(Value::as_u64).unwrap_or(0);
        if up > MAX_SAFE_BYTES || down > MAX_SAFE_BYTES {
            return Err(AppError::bad_request("Traffic counter exceeds safe limit"));
        }
        let unmetered = st.get("unmetered").and_then(Value::as_bool).unwrap_or(false);
        traffic_batcher::add_traffic(
            &state,
            node_id,
            email,
            i64::try_from(up).unwrap_or(0),
            i64::try_from(down).unwrap_or(0),
            unmetered,
            None,
            None,
            None,
            None,
        )
        .await;
    }

    Ok(Json(json!({ "status": "ok", "processed": stats_obj.len() })))
}

async fn report_alert(
    State(state): State<AppState>,
    headers: HeaderMap,
    ip: ClientIp,
    AppJson(body): AppJson<Value>,
) -> AppResult<Json<Value>> {
    let node_id = body
        .get("nodeId")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::unauthorized("Unauthorized"))?;
    authenticate_node(&state, &headers, node_id).await?;

    let alert_type = body.get("type").and_then(Value::as_str).unwrap_or("");
    let core = body.get("core").and_then(Value::as_str).unwrap_or("");
    let message = body.get("message").and_then(Value::as_str).unwrap_or("");

    let _ = sqlx::query(
        r#"INSERT INTO "AuditLog" (id, action, resource, "resourceId", details, ip, "createdAt")
           VALUES ($1, 'ALERT', 'node', $2, $3, $4, $5)"#,
    )
    .bind(crate::db::new_id())
    .bind(node_id)
    .bind(json!({
        "core": core,
        "type": alert_type,
        "message": message,
        "source": "worker-alert"
    }))
    .bind(&*ip)
    .bind(crate::db::now())
    .execute(&state.db)
    .await;

    if matches!(alert_type, "crash" | "start_failed") {
        let _ = sqlx::query(
            r#"UPDATE "Node" SET status = 'ERROR', "updatedAt" = $2 WHERE id = $1"#,
        )
        .bind(node_id)
        .bind(crate::db::now())
        .execute(&state.db)
        .await;
    }

    Ok(Json(json!({ "status": "acknowledged" })))
}
