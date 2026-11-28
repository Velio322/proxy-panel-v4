use anyhow::{anyhow, Result};
use std::env;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub db_max_connections: u32,
    pub api_host: String,
    pub api_port: u16,
    pub api_url: String,
    /// Optional separate subscription base URL. Defaults to `api_url`.
    pub sub_url: String,
    pub frontend_url: String,
    pub cors_origins: Vec<String>,
    pub production: bool,
    pub jwt_secret: String,
    pub jwt_expires_in: Duration,
    pub encryption_key: Option<String>,
    pub trust_proxy: bool,
    pub rate_limit_window_ms: u64,
    pub rate_limit_max: u32,
    pub auth_rate_limit_max: u32,
    pub prometheus_enabled: bool,
    /// Fail-closed `/metrics` token. When `None` or empty, `/metrics` returns 403.
    pub metrics_token: Option<String>,
    pub telegram_bot_token: Option<String>,
    pub telegram_admin_ids: Vec<i64>,
    pub telegram_bot_username: Option<String>,
    pub backup_enabled: bool,
    pub backup_interval: String,
    pub backup_dir: PathBuf,
    pub backup_retention: u32,
    pub backup_telegram_token: Option<String>,
    pub backup_telegram_chat_id: Option<String>,
    pub backup_s3_bucket: Option<String>,
    pub backup_s3_region: Option<String>,
    pub backup_s3_access_key: Option<String>,
    pub backup_s3_secret_key: Option<String>,
    pub backup_s3_endpoint: Option<String>,
    pub data_dir: PathBuf,
    pub pg_dump_bin: String,
    pub ws_heartbeat_interval: Duration,
    pub ws_heartbeat_timeout: Duration,
    /// Defaults to `false` to block SSRF to private/loopback node addresses unless explicitly enabled.
    pub allow_private_nodes: bool,
    /// Defaults to `false` so non-loopback nodes require `https://`.
    pub node_api_plaintext: bool,
    pub body_limit_bytes: usize,
    /// Optional Redis URL for Redis Streams event exporter.
    pub redis_url: Option<String>,
    pub redis_stream_key: String,
    /// Optional ClickHouse HTTP endpoint for event exporter.
    pub clickhouse_url: Option<String>,
    pub clickhouse_table: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database_url: "postgresql://postgres:postgres@127.0.0.1:5432/proxpanel".to_string(),
            db_max_connections: 20,
            api_host: "0.0.0.0".to_string(),
            api_port: 3000,
            api_url: "http://localhost:3000".to_string(),
            sub_url: "http://localhost:3000".to_string(),
            frontend_url: "http://localhost:5173".to_string(),
            cors_origins: vec![
                "http://localhost:5173".to_string(),
                "http://localhost:3000".to_string(),
            ],
            production: false,
            jwt_secret: "change-this-in-production".to_string(),
            jwt_expires_in: Duration::from_secs(7 * 86400),
            encryption_key: None,
            trust_proxy: false,
            rate_limit_window_ms: 900_000,
            rate_limit_max: 200,
            auth_rate_limit_max: 20,
            prometheus_enabled: true,
            metrics_token: None,
            telegram_bot_token: None,
            telegram_admin_ids: Vec::new(),
            telegram_bot_username: None,
            backup_enabled: false,
            backup_interval: "0 3 * * *".to_string(),
            backup_dir: PathBuf::from("/var/lib/mypanel3/backups"),
            backup_retention: 14,
            backup_telegram_token: None,
            backup_telegram_chat_id: None,
            backup_s3_bucket: None,
            backup_s3_region: None,
            backup_s3_access_key: None,
            backup_s3_secret_key: None,
            backup_s3_endpoint: None,
            data_dir: PathBuf::from("/var/lib/mypanel3"),
            pg_dump_bin: "pg_dump".to_string(),
            ws_heartbeat_interval: Duration::from_secs(10),
            ws_heartbeat_timeout: Duration::from_secs(30),
            allow_private_nodes: false,
            node_api_plaintext: false,
            body_limit_bytes: 10 * 1024 * 1024,
            redis_url: None,
            redis_stream_key: "mypanel3:events".to_string(),
            clickhouse_url: None,
            clickhouse_table: "mypanel3_events".to_string(),
        }
    }
}

fn env_opt(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn parse_bool(key: &str, default: bool) -> bool {
    match env_opt(key) {
        Some(v) => matches!(
            v.to_ascii_lowercase().as_str(),
            "true" | "1" | "yes" | "on"
        ),
        None => default,
    }
}

fn parse_num<T>(key: &str, default: T) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match env_opt(key) {
        Some(v) => v
            .parse::<T>()
            .map_err(|e| anyhow!("Invalid {key}='{v}': {e}")),
        None => Ok(default),
    }
}

pub fn parse_duration_str(raw: &str) -> Option<Duration> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(secs) = s.parse::<u64>() {
        return Some(Duration::from_secs(secs.max(1)));
    }
    let (num_part, unit_part) = s.split_at(s.len().saturating_sub(1));
    let val = num_part.trim().parse::<u64>().ok()?;
    let mult = match unit_part.to_ascii_lowercase().as_str() {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        "w" => 7 * 86_400,
        "y" => 365 * 86_400,
        _ => return None,
    };
    Some(Duration::from_secs(val.saturating_mul(mult).max(1)))
}

fn parse_duration(key: &str, default: &str) -> Duration {
    env_opt(key)
        .and_then(|v| parse_duration_str(&v))
        .or_else(|| parse_duration_str(default))
        .unwrap_or_else(|| Duration::from_secs(60))
}

/// Placeholder used when `JWT_SECRET` is unset; must never be accepted in production.
const DEFAULT_JWT_SECRET: &str = "change-this-in-production";

/// Refuses to start in production with a missing, default or short `JWT_SECRET`.
fn validate_production_secrets(jwt_secret: &str) -> Result<()> {
    if jwt_secret == DEFAULT_JWT_SECRET || jwt_secret.len() < 32 {
        return Err(anyhow!(
            "JWT_SECRET must be set to a random value of at least 32 characters in production"
        ));
    }
    Ok(())
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let database_url = env_opt("DATABASE_URL")
            .unwrap_or_else(|| "postgresql://postgres:postgres@127.0.0.1:5432/proxpanel".into());
        let api_port: u16 = parse_num("API_PORT", 3000u16)?;
        let api_host = env_opt("API_HOST").unwrap_or_else(|| "0.0.0.0".into());
        let api_url = env_opt("API_URL")
            .unwrap_or_else(|| format!("http://localhost:{api_port}"))
            .trim_end_matches('/')
            .to_string();
        let sub_url = env_opt("SUB_URL")
            .unwrap_or_else(|| api_url.clone())
            .trim_end_matches('/')
            .to_string();
        let frontend_url = env_opt("FRONTEND_URL")
            .unwrap_or_else(|| "http://localhost:5173".into())
            .trim_end_matches('/')
            .to_string();

        let mut cors_origins: Vec<String> = match env_opt("CORS_ORIGINS") {
            Some(list) => list
                .split(',')
                .map(|s| s.trim().trim_end_matches('/').to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            None => vec![frontend_url.clone()],
        };
        if !cors_origins.contains(&api_url) {
            cors_origins.push(api_url.clone());
        }

        let production = env_opt("NODE_ENV")
            .is_some_and(|v| v.eq_ignore_ascii_case("production"));
        let jwt_secret = env_opt("JWT_SECRET")
            .unwrap_or_else(|| "change-this-in-production".into());
        let jwt_expires_in = parse_duration("JWT_EXPIRES_IN", "7d");
        if production {
            validate_production_secrets(&jwt_secret)?;
        }
        let encryption_key = env_opt("ENCRYPTION_KEY");
        let trust_proxy = parse_bool("TRUST_PROXY", false);
        let rate_limit_window_ms = parse_num("RATE_LIMIT_WINDOW", 900_000u64)?;
        let rate_limit_max = parse_num("RATE_LIMIT_MAX", 200u32)?;
        let auth_rate_limit_max = parse_num("AUTH_RATE_LIMIT_MAX", 20u32)?;

        let prometheus_enabled = env_opt("PROMETHEUS_ENABLED")
            .map(|v| !v.eq_ignore_ascii_case("false"))
            .unwrap_or(true);
        let metrics_token = env_opt("METRICS_TOKEN");

        let telegram_bot_token = env_opt("TELEGRAM_BOT_TOKEN");
        let telegram_admin_ids = env_opt("TELEGRAM_ADMIN_IDS")
            .map(|s| {
                s.split(',')
                    .filter_map(|p| p.trim().parse::<i64>().ok())
                    .collect()
            })
            .unwrap_or_default();
        let telegram_bot_username = env_opt("TELEGRAM_BOT_USERNAME");

        let data_dir = env_opt("DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/var/lib/mypanel3"));
        let backup_dir = env_opt("BACKUP_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| data_dir.join("backups"));

        Ok(Self {
            database_url,
            db_max_connections: parse_num("DB_MAX_CONNECTIONS", 20u32)?,
            api_host,
            api_port,
            api_url,
            sub_url,
            frontend_url,
            cors_origins,
            production,
            jwt_secret,
            jwt_expires_in,
            encryption_key,
            trust_proxy,
            rate_limit_window_ms,
            rate_limit_max,
            auth_rate_limit_max,
            prometheus_enabled,
            metrics_token,
            telegram_bot_token,
            telegram_admin_ids,
            telegram_bot_username,
            backup_enabled: parse_bool("BACKUP_ENABLED", false),
            backup_interval: env_opt("BACKUP_INTERVAL").unwrap_or_else(|| "0 3 * * *".into()),
            backup_dir,
            backup_retention: parse_num("BACKUP_RETENTION", 14u32)?,
            backup_telegram_token: env_opt("BACKUP_TELEGRAM_TOKEN"),
            backup_telegram_chat_id: env_opt("BACKUP_TELEGRAM_CHAT_ID"),
            backup_s3_bucket: env_opt("BACKUP_S3_BUCKET"),
            backup_s3_region: env_opt("BACKUP_S3_REGION"),
            backup_s3_access_key: env_opt("BACKUP_S3_ACCESS_KEY"),
            backup_s3_secret_key: env_opt("BACKUP_S3_SECRET_KEY"),
            backup_s3_endpoint: env_opt("BACKUP_S3_ENDPOINT"),
            data_dir,
            pg_dump_bin: env_opt("PG_DUMP_BIN").unwrap_or_else(|| "pg_dump".into()),
            ws_heartbeat_interval: parse_duration("WS_HEARTBEAT_INTERVAL", "10s"),
            ws_heartbeat_timeout: parse_duration("WS_HEARTBEAT_TIMEOUT", "30s"),
            allow_private_nodes: parse_bool("ALLOW_PRIVATE_NODES", false),
            node_api_plaintext: parse_bool("NODE_API_PLAINTEXT", false),
            body_limit_bytes: parse_num("BODY_LIMIT_BYTES", 10 * 1024 * 1024usize)?,
            redis_url: env_opt("REDIS_URL"),
            redis_stream_key: env_opt("REDIS_STREAM_KEY")
                .unwrap_or_else(|| "mypanel3:events".into()),
            clickhouse_url: env_opt("CLICKHOUSE_URL"),
            clickhouse_table: env_opt("CLICKHOUSE_TABLE")
                .unwrap_or_else(|| "mypanel3_events".into()),
        })
    }
}

#[cfg(test)]
mod secret_tests {
    use super::validate_production_secrets;

    #[test]
    fn production_rejects_default_and_short_jwt_secrets() {
        assert!(validate_production_secrets("change-this-in-production").is_err());
        assert!(validate_production_secrets("short").is_err());
        assert!(validate_production_secrets(&"a".repeat(31)).is_err());
        assert!(validate_production_secrets(&"a".repeat(32)).is_ok());
    }
}
