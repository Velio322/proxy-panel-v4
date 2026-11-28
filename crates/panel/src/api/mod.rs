pub mod audit;
pub mod auth;
pub mod backup;
pub mod branding;
pub mod clients;
pub mod common;
pub mod config_transfer;
pub mod dashboard;
pub mod hosts;
pub mod inbounds;
pub mod infra_billing;
pub mod install_assets;
pub mod logs;
pub mod middleware;
pub mod monitoring;
pub mod node_api;
pub mod node_join;
pub mod nodes;
pub mod notifications;
pub mod openapi;
pub mod passkeys;
pub mod plans;
pub mod public_config;
pub mod resellers;
pub mod routing;
pub mod sessions_explorer;
pub mod settings;
pub mod srh;
pub mod static_files;
pub mod sub;
pub mod sub_page;
pub mod templates;
pub mod tg_oauth;
pub mod twofa;
pub mod users;
pub mod webhooks;
pub mod ws;

use crate::auth::crypto::ct_eq;
use crate::state::AppState;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware as mw;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};

pub fn router(state: AppState) -> Router {
    let auth_router = auth::router().layer(mw::from_fn_with_state(
        state.clone(),
        middleware::auth_rate_limit,
    ));

    let v1 = Router::new()
        .nest("/auth", auth_router)
        .nest(
            "/auth/2fa",
            twofa::router().layer(mw::from_fn_with_state(
                state.clone(),
                middleware::auth_rate_limit,
            )),
        )
        .nest(
            "/auth/passkeys",
            passkeys::router().layer(mw::from_fn_with_state(
                state.clone(),
                middleware::auth_rate_limit,
            )),
        )
        .nest(
            "/auth/telegram",
            tg_oauth::router().layer(mw::from_fn_with_state(
                state.clone(),
                middleware::auth_rate_limit,
            )),
        )
        .nest("/users", users::router())
        .nest("/clients", clients::router())
        .nest("/nodes/self", node_api::router())
        .nest("/nodes", nodes::router())
        .nest("/inbounds", inbounds::router())
        .nest("/dashboard", dashboard::router())
        .nest("/resellers", resellers::router())
        .nest("/audit", audit::router())
        .nest("/settings", settings::router())
        .nest("/backup", backup::router())
        .nest("/monitoring", monitoring::router())
        .nest("/routing", routing::router())
        .nest("/plans", plans::router())
        .nest("/client", sub::router())
        .nest("/hosts", hosts::router())
        .nest("/templates", templates::router())
        .nest("/webhooks", webhooks::router())
        .nest("/notifications", notifications::router())
        .nest("/public-config", public_config::router())
        .nest("/branding", branding::router())
        .nest("/sessions", sessions_explorer::router())
        .nest("/logs", logs::router())
        .nest("/inspector", srh::router())
        .nest("/infra-billing", infra_billing::router())
        .nest("/cores", nodes::cores_router())
        .nest("/srr", templates::srr_router());

    let mut app = Router::new()
        .route("/api/health", get(health))
        .route("/ws/worker", get(ws::upgrade))
        .route("/install.sh", get(install_assets::install_sh))
        .route("/install-node.sh", get(install_assets::install_node_sh))
        .route("/node-upgrade.sh", get(install_assets::node_upgrade_sh))
        .route("/node-tgweb.sh", get(install_assets::node_tgweb_sh))
        .route("/sub/{sub_token}", get(sub::get_subscription))
        .nest("/uploads/branding", branding::uploads_router())
        .nest("/api", openapi::router())
        .nest("/api/v1/config-transfer", config_transfer::router(state.clone()))
        .nest("/api/v1", v1);

    if state.cfg.prometheus_enabled {
        app = app.route("/metrics", get(metrics));
    }

    app.fallback(static_files::static_handler)
        .layer(mw::from_fn_with_state(
            state.clone(),
            middleware::metrics_middleware,
        ))
        .layer(mw::from_fn_with_state(
            state.clone(),
            middleware::csrf_origin_check,
        ))
        .layer(mw::from_fn_with_state(
            state.clone(),
            middleware::global_rate_limit,
        ))
        .layer(DefaultBodyLimit::max(state.cfg.body_limit_bytes))
        .layer(CompressionLayer::new())
        .layer(cors_layer(&state.cfg.cors_origins))
        .layer(mw::from_fn_with_state(
            state.clone(),
            middleware::security_headers,
        ))
        .with_state(state)
}

fn cors_layer(origins: &[String]) -> CorsLayer {
    let parsed: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|o| HeaderValue::from_str(o).ok())
        .collect();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(parsed))
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::HEAD,
            Method::PUT,
            Method::PATCH,
            Method::POST,
            Method::DELETE,
        ])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::ACCEPT,
            header::HeaderName::from_static("x-api-key"),
        ])
}

async fn health(State(st): State<AppState>) -> Response {
    match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&st.db)
        .await
    {
        Ok(_) => (
            StatusCode::OK,
            Json(json!({
                "status": "ok",
                "timestamp": crate::db::ts::format_iso(&crate::db::now()),
                "version": env!("CARGO_PKG_VERSION")
            })),
        )
            .into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "health check: database unreachable");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "error" })),
            )
                .into_response()
        }
    }
}

/// Fail-closed `/metrics` endpoint — returns 403 when `METRICS_TOKEN` is unset or empty.
async fn metrics(State(st): State<AppState>, headers: HeaderMap) -> Response {
    let Some(tok) = st.cfg.metrics_token.as_deref().filter(|t| !t.is_empty()) else {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Metrics endpoint disabled: METRICS_TOKEN is not configured" })),
        )
            .into_response();
    };
    let given = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    if !ct_eq(given, tok) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Unauthorized" })),
        )
            .into_response();
    }
    let body = st.metrics.render(st.started_at.elapsed().as_secs_f64());
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
        .into_response()
}
