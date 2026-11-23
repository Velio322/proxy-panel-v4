pub mod enums;
pub mod ts;

pub use enums::*;

use chrono::{NaiveDateTime, Timelike, Utc};
use sqlx::migrate::Migrator;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Executor, PgPool};
use std::str::FromStr;
use std::time::Duration;
use uuid::Uuid;

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

pub fn now() -> NaiveDateTime {
    let n = Utc::now().naive_utc();
    // Truncate nanoseconds to milliseconds to match PostgreSQL TIMESTAMP(3)
    let millis = n.nanosecond() / 1_000_000;
    n.with_nanosecond(millis * 1_000_000).unwrap_or(n)
}

/// Strips Prisma-only query parameters (`schema`, `connection_limit`, `pool_timeout`) before passing to SQLx.
pub fn sanitize_database_url(raw: &str) -> String {
    let Ok(mut parsed) = url::Url::parse(raw.trim()) else {
        return raw.trim().to_string();
    };
    let pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .filter(|(k, _)| !matches!(k.as_ref(), "schema" | "connection_limit" | "pool_timeout"))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if pairs.is_empty() {
        parsed.set_query(None);
    } else {
        let mut ser = url::form_urlencoded::Serializer::new(String::new());
        for (k, v) in pairs {
            ser.append_pair(&k, &v);
        }
        let q = ser.finish();
        parsed.set_query(Some(&q));
    }
    parsed.to_string()
}

pub async fn connect_pool(database_url: &str, max_connections: u32) -> Result<PgPool, sqlx::Error> {
    let clean = sanitize_database_url(database_url);
    let opts = PgConnectOptions::from_str(&clean)?;
    PgPoolOptions::new()
        .max_connections(max_connections.max(2))
        .acquire_timeout(Duration::from_secs(10))
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                conn.execute("SET TIME ZONE 'UTC'").await?;
                Ok(())
            })
        })
        .connect_with(opts)
        .await
}

pub async fn connect_with_retry(
    database_url: &str,
    max_connections: u32,
    retries: usize,
    delay: Duration,
) -> Result<PgPool, sqlx::Error> {
    let mut attempt = 0usize;
    loop {
        match connect_pool(database_url, max_connections).await {
            Ok(pool) => return Ok(pool),
            Err(e) => {
                attempt += 1;
                if attempt >= retries.max(1) {
                    return Err(e);
                }
                tracing::warn!(attempt, error = %e, "database connect failed, retrying...");
                tokio::time::sleep(delay).await;
            }
        }
    }
}

pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::Error> {
    let baseline = include_str!("../../migrations/20261112000001_baseline.sql");
    let parity = include_str!("../../migrations/20261119000002_p1_p3_parity.sql");
    let remnawave = include_str!("../../migrations/20261121000003_remnawave_parity.sql");
    let client_nodes = include_str!("../../migrations/20261121000004_client_nodes.sql");
    let node_core_versions = include_str!("../../migrations/20261121000005_node_core_versions.sql");
    let node_api_port_2222 = include_str!("../../migrations/20261122000006_node_api_port_2222.sql");
    let node_uptime = include_str!("../../migrations/20261122000007_node_uptime.sql");
    let node_last_online = include_str!("../../migrations/20261123000008_node_last_online.sql");
    let node_api_mode = include_str!("../../migrations/20261123000009_node_api_mode.sql");
    sqlx::raw_sql(baseline).execute(pool).await?;
    sqlx::raw_sql(parity).execute(pool).await?;
    sqlx::raw_sql(remnawave).execute(pool).await?;
    sqlx::raw_sql(client_nodes).execute(pool).await?;
    sqlx::raw_sql(node_core_versions).execute(pool).await?;
    sqlx::raw_sql(node_api_port_2222).execute(pool).await?;
    sqlx::raw_sql(node_uptime).execute(pool).await?;
    sqlx::raw_sql(node_last_online).execute(pool).await?;
    sqlx::raw_sql(node_api_mode).execute(pool).await?;
    Ok(())
}
