use crate::config::AGENT_VERSION;
use crate::cores::write_atomic_0600;
use crate::process::ProcessManager;
use crate::updater::{
    download_and_install_core, rollback_binary, CoreKind, UpdateCoreResult,
};
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use mypanel_proto::{
    ApplyConfigRequest, ApplyConfigResponse, CoreUpdateCmd, DropSessionCmd, SessionsList,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Instant;
use subtle::ConstantTimeEq;
use tokio::sync::Mutex;

/// Maximum JSON request payload size (`10 MiB`).
pub const MAX_API_BODY_BYTES: usize = 10 * 1024 * 1024;

/// Shared Axum application state for the worker HTTP API.
#[derive(Clone)]
pub struct ApiState {
    pub pm: Arc<Mutex<ProcessManager>>,
    pub http_client: reqwest::Client,
    pub node_secret: String,
    pub started_at: Instant,
}

impl ApiState {
    pub fn new(pm: Arc<Mutex<ProcessManager>>, http_client: reqwest::Client, node_secret: String) -> Self {
        Self {
            pm,
            http_client,
            node_secret,
            started_at: Instant::now(),
        }
    }
}

/// Constant-time shared-secret verification (`Authorization: Bearer <secret>` or `x-node-secret: <secret>`).
/// Refuses empty configured secrets.
pub fn constant_time_secret_match(expected: &str, provided: &str) -> bool {
    if expected.is_empty() || provided.is_empty() {
        return false;
    }
    let a = expected.as_bytes();
    let b = provided.as_bytes();
    a.len() == b.len() && bool::from(a.ct_eq(b))
}

fn extract_provided_secret(headers: &HeaderMap) -> Option<&str> {
    if let Some(val) = headers.get("x-node-secret").and_then(|v| v.to_str().ok()) {
        return Some(val.trim());
    }
    if let Some(auth) = headers.get("authorization").and_then(|v| v.to_str().ok()) {
        let trimmed = auth.trim();
        if let Some(token) = trimmed
            .strip_prefix("Bearer ")
            .or_else(|| trimmed.strip_prefix("bearer "))
        {
            return Some(token.trim());
        }
    }
    None
}

async fn require_node_auth(
    State(state): State<ApiState>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let authorized = extract_provided_secret(&headers)
        .is_some_and(|provided| constant_time_secret_match(&state.node_secret, provided));

    if !authorized {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Unauthorized" })),
        )
            .into_response();
    }
    next.run(request).await
}

/// Builds the Axum [`Router`] exposing `/health` and all authenticated `/api/*` endpoints.
pub fn build_router(state: ApiState) -> Router {
    let authed = Router::new()
        .route("/api/status", get(handle_status))
        .route("/api/metrics", get(handle_status))
        .route("/api/config", post(handle_apply_config))
        .route("/api/restart", post(handle_restart))
        .route("/api/stop", post(handle_stop))
        .route("/api/traffic", get(handle_traffic))
        .route("/api/inbounds", get(handle_inbounds))
        .route("/api/logs", get(handle_logs))
        .route("/api/sessions", get(handle_sessions))
        .route("/api/sessions/drop", post(handle_drop_session))
        .route("/api/core-update", post(handle_core_update))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_node_auth,
        ));

    Router::new()
        .route("/health", get(handle_health))
        .merge(authed)
        .layer(DefaultBodyLimit::max(MAX_API_BODY_BYTES))
        .with_state(state)
}

async fn handle_health(State(state): State<ApiState>) -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "version": AGENT_VERSION,
        "uptime": state.started_at.elapsed().as_secs_f64(),
    }))
}

async fn handle_status(State(state): State<ApiState>) -> impl IntoResponse {
    let mut pm = state.pm.lock().await;
    let report = pm.status_report(true).await;
    Json(report)
}

async fn handle_apply_config(
    State(state): State<ApiState>,
    Json(req): Json<ApplyConfigRequest>,
) -> impl IntoResponse {
    let mut pm = state.pm.lock().await;
    let desired_path = pm.config.desired_config_path();
    let res = pm
        .apply_config_with_profile(
            req.inbounds.clone(),
            req.routing_rules.clone(),
            req.xray_profile.clone(),
        )
        .await;
    drop(pm);

    let ts = Utc::now().timestamp_millis();
    match res {
        Ok(_) => {
            if let Ok(bytes) = serde_json::to_vec_pretty(&req) {
                let _ = write_atomic_0600(&desired_path, &bytes).await;
            }
            Json(ApplyConfigResponse {
                success: true,
                timestamp: ts,
                error: None,
            })
        }
        Err(err) => Json(ApplyConfigResponse {
            success: false,
            timestamp: ts,
            error: Some(err),
        }),
    }
}

async fn handle_restart(State(state): State<ApiState>) -> impl IntoResponse {
    let mut pm = state.pm.lock().await;
    match pm.restart().await {
        Ok(_) => Json(json!({ "success": true })),
        Err(err) => Json(json!({ "success": false, "error": err })),
    }
}

async fn handle_stop(State(state): State<ApiState>) -> impl IntoResponse {
    let mut pm = state.pm.lock().await;
    pm.stop_all().await;
    Json(json!({ "success": true }))
}

async fn handle_traffic(State(state): State<ApiState>) -> impl IntoResponse {
    let mut pm = state.pm.lock().await;
    let stats = pm.collect_traffic().await;
    Json(stats)
}

/// Uses `tokio::fs::read_to_string` (never `std::fs::read_to_string`) in async handler.
async fn handle_inbounds(State(state): State<ApiState>) -> impl IntoResponse {
    let xray_path = {
        let pm = state.pm.lock().await;
        pm.config.xray_config_path()
    };
    let Ok(raw) = tokio::fs::read_to_string(&xray_path).await else {
        return Json(Value::Array(Vec::new()));
    };
    let Ok(parsed) = serde_json::from_str::<Value>(&raw) else {
        return Json(Value::Array(Vec::new()));
    };
    let inbounds = parsed
        .get("inbounds")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Json(Value::Array(inbounds))
}

#[derive(Debug, Deserialize)]
struct LogsQuery {
    core: Option<String>,
    limit: Option<usize>,
}

async fn handle_logs(
    State(state): State<ApiState>,
    Query(query): Query<LogsQuery>,
) -> impl IntoResponse {
    let logs = {
        let pm = state.pm.lock().await;
        pm.logs.clone()
    };
    let entries = logs
        .recent(query.core.as_deref(), query.limit.unwrap_or(200))
        .await;
    Json(json!({ "entries": entries }))
}

async fn handle_sessions(State(state): State<ApiState>) -> impl IntoResponse {
    let mut pm = state.pm.lock().await;
    let sessions = pm.get_sessions().await;
    Json(SessionsList { sessions })
}

async fn handle_drop_session(
    State(state): State<ApiState>,
    Json(cmd): Json<DropSessionCmd>,
) -> impl IntoResponse {
    let mut pm = state.pm.lock().await;
    match pm.drop_session(&cmd).await {
        Ok(dropped) => Json(json!({ "success": true, "dropped": dropped })),
        Err(err) => Json(json!({ "success": false, "dropped": 0, "error": err })),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HttpCoreUpdateRequest {
    #[serde(default)]
    command_id: String,
    core: String,
    #[serde(default)]
    version: String,
    download_url: String,
    sha256: String,
}

async fn handle_core_update(
    State(state): State<ApiState>,
    Json(req): Json<HttpCoreUpdateRequest>,
) -> Response {
    let cmd = CoreUpdateCmd {
        command_id: req.command_id,
        core: req.core,
        version: req.version,
        download_url: req.download_url,
        sha256: req.sha256,
    };
    match execute_core_update(&state.pm, &state.http_client, &cmd).await {
        Ok(res) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "sha256": res.sha256,
                "version": res.detected_version,
            })),
        )
            .into_response(),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "error": err })),
        )
            .into_response(),
    }
}

static CORE_UPDATE_LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();

/// Downloads, SHA-256 verifies, extracts, smoke-tests, and atomically replaces a core binary,
/// then restarts active cores with automatic `.bak` rollback if the restart fails.
pub async fn execute_core_update(
    pm: &Arc<Mutex<ProcessManager>>,
    client: &reqwest::Client,
    cmd: &CoreUpdateCmd,
) -> Result<UpdateCoreResult, String> {
    let _update_guard = CORE_UPDATE_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;

    let kind = CoreKind::parse(&cmd.core)?;
    let (target_path, xray_asset_dir) = {
        let guard = pm.lock().await;
        (
            kind.bin_path(&guard.config).clone(),
            guard.config.xray_asset_dir.clone(),
        )
    };

    let mut res = download_and_install_core(
        client,
        kind,
        &target_path,
        xray_asset_dir.as_deref(),
        &cmd.download_url,
        &cmd.sha256,
    )
    .await?;

    let mut guard = pm.lock().await;
    guard.invalidate_core_versions();

    if guard.core_is_in_use(kind.canonical_name()) {
        if let Err(restart_err) = guard.restart().await {
            let _ = rollback_binary(&target_path).await;
            guard.invalidate_core_versions();
            let _ = guard.restart().await;
            return Err(format!(
                "Core restart failed after update (rolled back): {restart_err}"
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        if !guard.is_core_running(kind.canonical_name()) {
            let _ = rollback_binary(&target_path).await;
            guard.invalidate_core_versions();
            let _ = guard.restart().await;
            return Err(format!(
                "{} exited immediately after restart (rolled back)",
                kind.canonical_name()
            ));
        }
    }

    let versions = guard.refresh_core_versions().await;
    if res.detected_version.is_none() {
        res.detected_version = versions.get(kind.canonical_name()).cloned();
    }

    Ok(res)
}
