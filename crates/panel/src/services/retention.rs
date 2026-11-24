use crate::error::AppResult;
use chrono::{Duration as ChronoDuration, NaiveDateTime};
use sqlx::PgPool;
use std::time::Duration;

/// Rows removed per statement; keeps locks, WAL bursts and table bloat small.
const BATCH: i64 = 10_000;
/// Upper bound of statements per table and run (10M rows).
const MAX_BATCHES: usize = 1_000;

/// Deletes rows of `table` older than `cutoff` in small batches. `table`, `column` and `filter`
/// are compile-time constants, never user input.
async fn prune(
    pool: &PgPool,
    table: &str,
    column: &str,
    filter: &str,
    cutoff: NaiveDateTime,
) -> AppResult<u64> {
    let sql = format!(
        r#"DELETE FROM "{table}" WHERE ctid = ANY(ARRAY(
               SELECT ctid FROM "{table}" WHERE "{column}" < $1 AND ({filter}) LIMIT {BATCH}))"#
    );
    let mut total = 0u64;
    for _ in 0..MAX_BATCHES {
        let n = sqlx::query(&sql).bind(cutoff).execute(pool).await?.rows_affected();
        total += n;
        if n < BATCH as u64 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Ok(total)
}

/// Prunes expired sessions and old telemetry/audit/history rows to prevent unbounded table growth.
pub async fn cleanup_old_records(pool: &PgPool) -> AppResult<()> {
    let now = crate::db::now();
    let days = |d: i64| now - ChronoDuration::days(d);

    prune(pool, "UserSession", "expiresAt", "true", now).await?;
    prune(pool, "TrafficReceipt", "createdAt", "true", days(7)).await?;
    prune(pool, "NodeMetric", "recordedAt", "true", days(30)).await?;
    prune(pool, "SubRequestHistory", "createdAt", "true", days(30)).await?;
    prune(pool, "TrafficLog", "recordAt", "true", days(90)).await?;
    prune(pool, "AuditLog", "createdAt", "true", days(180)).await?;
    // Delivered commands are history; commands pending for a month belong to nodes that are gone
    // (a connecting node always receives the full current config anyway).
    prune(pool, "NodeCommand", "createdAt", "status = 'ACKNOWLEDGED'", days(7)).await?;
    prune(pool, "NodeCommand", "createdAt", "status = 'PENDING'", days(30)).await?;

    Ok(())
}
