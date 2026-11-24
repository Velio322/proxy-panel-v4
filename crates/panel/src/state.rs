use crate::auth::{JwtService, MtlsCa};
use crate::auth::brute::RateLimiter;
use crate::cache::AppCache;
use crate::config::Config;
use crate::hub::WorkerHub;
use crate::metrics::MetricsRegistry;
use crate::traffic_queue::TrafficQueue;
use dashmap::DashMap;
use sqlx::PgPool;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};
use webauthn_rs::prelude::{PasskeyAuthentication, PasskeyRegistration};

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub cfg: Arc<Config>,
    pub jwt: JwtService,
    pub cache: Arc<AppCache>,
    pub hub: Arc<WorkerHub>,
    pub metrics: Arc<MetricsRegistry>,
    pub traffic_queue: Arc<TrafficQueue>,
    pub global_limiter: Arc<RateLimiter>,
    pub auth_limiter: Arc<RateLimiter>,
    pub transfer_limiter: Arc<RateLimiter>,
    /// Per-IP limiter for authenticated-looking node endpoints (replaces a blanket exemption).
    pub node_limiter: Arc<RateLimiter>,
    /// Public join endpoint: few attempts per IP and minute.
    pub join_limiter: Arc<RateLimiter>,
    pub started_at: Instant,
    pub encryption_key: Option<[u8; 32]>,
    pub backup_running: Arc<AtomicBool>,
    pub mtls_ca: MtlsCa,
    pub http_client: reqwest::Client,
    /// mTLS client for pull-mode nodes (panel -> node HTTPS with a CA-signed identity).
    pub node_mtls_client: reqwest::Client,
    pub passkey_reg_states: Arc<DashMap<String, PasskeyRegistration>>,
    pub passkey_auth_states: Arc<DashMap<String, (Option<String>, PasskeyAuthentication)>>,
}

impl AppState {
    pub fn new(db: PgPool, cfg: Config) -> Self {
        let jwt = JwtService::new(cfg.jwt_secret.clone(), cfg.jwt_expires_in);
        let encryption_key = cfg
            .encryption_key
            .as_deref()
            .and_then(|raw| crate::auth::crypto::parse_encryption_key(raw).ok());

        let global_limiter = Arc::new(RateLimiter::new(
            Duration::from_millis(cfg.rate_limit_window_ms.max(1000)),
            cfg.rate_limit_max.max(1),
        ));
        let auth_limiter = Arc::new(RateLimiter::new(
            Duration::from_millis(cfg.rate_limit_window_ms.max(1000)),
            cfg.auth_rate_limit_max.max(1),
        ));
        let transfer_limiter = Arc::new(RateLimiter::new(Duration::from_secs(60), 5));
        let node_limiter = Arc::new(RateLimiter::new(Duration::from_secs(60), 1200));
        let join_limiter = Arc::new(RateLimiter::new(Duration::from_secs(60), 10));

        let mtls_ca = MtlsCa::load_or_generate(Some(&cfg.data_dir))
            .unwrap_or_else(|_| {
                MtlsCa::generate_ephemeral().unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "failed to generate mTLS CA");
                    // Should never happen in practice; fallback handled by from_pem if needed
                    MtlsCa::generate_ephemeral().ok().unwrap_or_else(|| unreachable!())
                })
            });

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            // Certificates are verified. Only an explicit opt-in (self-signed https nodes in a
            // trusted network) turns that off.
            .danger_accept_invalid_certs(
                std::env::var("NODE_TLS_INSECURE").is_ok_and(|v| v.eq_ignore_ascii_case("true")),
            )
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let node_mtls_client = mtls_ca.build_node_client().unwrap_or_else(|e| {
            tracing::warn!(error = %e, "could not build node mTLS client; pull-mode nodes will be unreachable");
            reqwest::Client::new()
        });

        Self {
            db,
            cfg: Arc::new(cfg),
            jwt,
            cache: Arc::new(AppCache::new()),
            hub: Arc::new(WorkerHub::new()),
            metrics: Arc::new(MetricsRegistry::new()),
            traffic_queue: Arc::new(TrafficQueue::new()),
            global_limiter,
            auth_limiter,
            transfer_limiter,
            node_limiter,
            join_limiter,
            started_at: Instant::now(),
            encryption_key,
            backup_running: Arc::new(AtomicBool::new(false)),
            mtls_ca,
            http_client,
            node_mtls_client,
            passkey_reg_states: Arc::new(DashMap::new()),
            passkey_auth_states: Arc::new(DashMap::new()),
        }
    }

    pub fn encryption_key(&self) -> Option<&[u8; 32]> {
        self.encryption_key.as_ref()
    }

    pub fn invalidate_derived(&self) {
        self.cache.invalidate_derived();
    }
}
