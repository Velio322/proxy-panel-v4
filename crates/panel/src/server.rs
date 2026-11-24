use crate::api;
use crate::config::Config;
use crate::db::{connect_with_retry, run_migrations};
use crate::seed::seed_initial_data;
use crate::services::{scheduler, traffic_batcher};
use crate::state::AppState;
use anyhow::Result;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpListener;

pub async fn run(cfg: Config) -> Result<()> {
    let pool = connect_with_retry(&cfg.database_url, cfg.db_max_connections, 15, Duration::from_secs(3))
        .await?;

    run_migrations(&pool).await?;
    seed_initial_data(&pool).await?;

    let state = AppState::new(pool, cfg.clone());

    scheduler::start(state.clone());

    let app = api::router(state.clone());
    let addr: SocketAddr = format!("{}:{}", cfg.api_host, cfg.api_port).parse()?;
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, "ProxyPanel v4 HTTP/WS server listening");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(state.clone()))
    .await?;

    let flushed = traffic_batcher::drain_all(&state).await;
    tracing::info!(flushed, "final traffic flush complete");
    Ok(())
}

async fn shutdown_signal(state: AppState) {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("Shutdown signal received, draining...");
    // Close node sessions cleanly and give the frames a moment to leave before the runtime stops.
    state.hub.begin_shutdown();
    tokio::time::sleep(Duration::from_millis(300)).await;
}
