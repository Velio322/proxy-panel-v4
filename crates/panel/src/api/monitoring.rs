use crate::api::common::valid_id;
use crate::auth::extract::AuthUser;
use crate::error::{AppError, AppResult};
use crate::services::{host_telemetry, node_client};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Map, Value};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/host", get(host))
        .route("/nodes/{id}", get(node_metrics))
}

fn no_store_json(val: Value) -> Response {
    let mut res = (StatusCode::OK, Json(val)).into_response();
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );
    res
}

async fn host(user: AuthUser) -> AppResult<Response> {
    user.require_admin()?;
    let data = host_telemetry::get_host_telemetry().await;
    Ok(no_store_json(data))
}

async fn node_metrics(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Response> {
    user.require_admin()?;
    valid_id(&id)?;
    let node = node_client::NodeTarget::fetch(&state, &id).await?;
    let mut metrics = node_client::get_metrics(&state, &node)
        .await?
        .ok_or_else(|| AppError::service_unavailable("Node telemetry unavailable"))?;

    if let Some(obj) = metrics.as_object_mut() {
        if let Some(ht) = obj.get("hostTelemetry").cloned() {
            if !obj.contains_key("tcpConnections") {
                if let Some(tcp) = ht
                    .pointer("/connections/establishedTcp")
                    .or_else(|| obj.get("connections"))
                    .cloned()
                {
                    obj.insert("tcpConnections".into(), tcp);
                }
            }
            if !obj.contains_key("udpSockets") {
                if let Some(udp) = ht.pointer("/connections/udpSockets").cloned() {
                    obj.insert("udpSockets".into(), udp);
                }
            }
            if !obj.contains_key("uploadBps") {
                if let Some(up) = ht.pointer("/network/uploadBps").cloned() {
                    obj.insert("uploadBps".into(), up);
                }
            }
            if !obj.contains_key("downloadBps") {
                if let Some(down) = ht.pointer("/network/downloadBps").cloned() {
                    obj.insert("downloadBps".into(), down);
                }
            }
            if !obj.contains_key("onlineUsersCount") {
                if let Some(users) = ht.pointer("/connections/proxyUserCount").cloned() {
                    obj.insert("onlineUsersCount".into(), users);
                }
            }
        }

        if !obj.contains_key("coreStatuses") {
            if let Some(cores_obj) = obj.get("cores").and_then(Value::as_object).cloned() {
                let mut statuses = Map::new();
                for (name, info) in cores_obj {
                    if !info.is_object() {
                        continue;
                    }
                    let running = info.get("running").and_then(Value::as_bool).unwrap_or(false);
                    let state_str = if running { "running" } else { "stopped" };
                    let ver = info
                        .get("version")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty());
                    let label = match ver {
                        Some(v) => format!("{state_str} ({v})"),
                        None => state_str.to_string(),
                    };
                    statuses.insert(name, Value::String(label));
                }
                obj.insert("coreStatuses".into(), Value::Object(statuses));
            }
        }
    }

    Ok(no_store_json(json!({
        "nodeId": id,
        "metrics": metrics
    })))
}
