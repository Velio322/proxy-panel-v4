pub mod api;
pub mod config;
pub mod cores;
pub mod master_client;
pub mod mtls;
pub mod port_share;
pub mod process;
pub mod telemetry;
pub mod traffic;
pub mod updater;

pub use config::AgentConfig;

use anyhow::Result;
use cores::write_atomic_0600;
use mypanel_proto::ApplyConfigRequest;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// Persists `desired_config.json` with `0600` permissions for cold-start restoration.
pub async fn save_desired_config(path: &Path, req: &ApplyConfigRequest) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(req)
        .map_err(|e| format!("Failed to serialize desired_config.json: {e}"))?;
    write_atomic_0600(path, &bytes).await
}

/// Loads `desired_config.json` from disk if present.
pub async fn load_desired_config(path: &Path) -> Result<Option<ApplyConfigRequest>, String> {
    match tokio::fs::read_to_string(path).await {
        Ok(raw) => {
            let parsed: ApplyConfigRequest = serde_json::from_str(&raw)
                .map_err(|e| format!("Failed to parse {}: {e}", path.display()))?;
            Ok(Some(parsed))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Failed to read {}: {e}", path.display())),
    }
}

/// Restores `desired_config.json` into `pm` on cold start before Master WS connects.
pub async fn restore_desired_config_on_cold_start(
    cfg: &AgentConfig,
    pm: &Arc<Mutex<process::ProcessManager>>,
) -> Result<bool, String> {
    let Some(req) = load_desired_config(&cfg.desired_config_path()).await? else {
        return Ok(false);
    };
    if req.inbounds.is_empty() && req.routing_rules.is_empty() {
        return Ok(false);
    }
    let mut guard = pm.lock().await;
    guard
        .apply_config_with_profile(req.inbounds, req.routing_rules, req.xray_profile)
        .await?;
    Ok(true)
}

/// Runs the `mypanel-agent` daemon:
/// 1. Loads pending traffic deltas & restores `desired_config.json` before Master WS connects.
/// 2. Starts the Axum HTTP API server (`WORKER_HOST:WORKER_PORT`).
/// 3. If `MASTER_URL` is configured, spawns the Master registration + WS control loop.
/// 4. Waits for shutdown signal (`Ctrl+C` / `SIGTERM`), stops all cores cleanly, and exits.
pub async fn run(cfg: AgentConfig) -> Result<()> {
    let http_client = mtls::build_reqwest_client(&cfg)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    let pm = Arc::new(Mutex::new(process::ProcessManager::new(
        cfg.clone(),
        http_client.clone(),
    )));
    let tracker = Arc::new(Mutex::new(traffic::CumulativeTrafficTracker::new(
        cfg.traffic_pending_path(),
    )));

    {
        let mut tr = tracker.lock().await;
        if let Err(err) = tr.load_pending().await {
            tracing::warn!(error = %err, "Failed to load traffic-pending.json");
        }
    }

    if let Err(err) = restore_desired_config_on_cold_start(&cfg, &pm).await {
        tracing::warn!(error = %err, "Cold-start desired_config.json restore failed");
    }

    let state = api::ApiState::new(pm.clone(), http_client.clone(), cfg.node_secret.clone())
        .with_allowed_panel_ips(cfg.allowed_panel_ips.clone());
    let router = api::build_router(state);

    let bind_addr = format!("{}:{}", cfg.worker_host, cfg.worker_port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(
        addr = %bind_addr,
        allowed_panel_ips = cfg.allowed_panel_ips.len(),
        "mypanel-agent HTTP API listening"
    );

    let cancel = CancellationToken::new();
    let master_handle = if !cfg.master_url.is_empty() {
        let m_cfg = cfg.clone();
        let m_client = http_client.clone();
        let m_pm = pm.clone();
        let m_tracker = tracker.clone();
        let m_cancel = cancel.clone();
        Some(tokio::spawn(async move {
            master_client::run_master_loop(m_cfg, m_client, m_pm, m_tracker, m_cancel).await;
        }))
    } else {
        None
    };

    let server_cancel = cancel.clone();
    let server_task = tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            server_cancel.cancelled().await;
        })
        .await;
    });

    wait_for_shutdown_signal().await;
    tracing::info!("Shutdown signal received, stopping proxy cores");
    cancel.cancel();

    if let Some(h) = master_handle {
        let _ = h.await;
    }
    let _ = server_task.await;

    let mut guard = pm.lock().await;
    guard.stop_all().await;

    Ok(())
}

async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = async {
                if let Some(ref mut s) = sigterm {
                    s.recv().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
