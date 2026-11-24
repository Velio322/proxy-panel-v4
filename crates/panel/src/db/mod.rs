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
                // A runaway query must not pin a pooled connection (and starve the traffic
                // flusher) forever.
                conn.execute("SET statement_timeout = '60s'").await?;
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

/// Arbitrary but fixed key for the migration advisory lock.
const MIGRATION_LOCK_KEY: i64 = 0x6d79_7061_6e65_6c33;

/// Applies every embedded migration (all are idempotent) in version order. A Postgres advisory
/// lock serialises concurrent panel starts, and new files in `migrations/` are picked up
/// automatically instead of needing a hand-maintained `include_str!` list.
pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::Error> {
    let mut conn = pool.acquire().await?;
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(MIGRATION_LOCK_KEY)
        .execute(&mut *conn)
        .await?;
    let mut result = Ok(());
    for m in MIGRATOR.iter().filter(|m| !m.migration_type.is_down_migration()) {
        if let Err(e) = sqlx::raw_sql(&m.sql).execute(&mut *conn).await {
            tracing::error!(version = m.version, description = %m.description, error = %e, "migration failed");
            result = Err(e);
            break;
        }
    }
    let _ = sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(MIGRATION_LOCK_KEY)
        .execute(&mut *conn)
        .await;
    result
}
