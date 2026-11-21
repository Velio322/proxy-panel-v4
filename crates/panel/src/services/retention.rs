use crate::error::AppResult;
use chrono::Duration as ChronoDuration;
use sqlx::PgPool;

/// Prunes expired sessions and old telemetry/audit/history rows to prevent unbounded table growth.
pub async fn cleanup_old_records(pool: &PgPool) -> AppResult<()> {
    let now = crate::db::now();
    let traffic_cutoff = now - ChronoDuration::days(90);
    let metric_cutoff = now - ChronoDuration::days(30);
    let receipt_cutoff = now - ChronoDuration::days(7);
    let srh_cutoff = now - ChronoDuration::days(30);
    let audit_cutoff = now - ChronoDuration::days(180);

    sqlx::query(r#"DELETE FROM "UserSession" WHERE "expiresAt" <= $1"#)
        .bind(now)
        .execute(pool)
        .await?;

    sqlx::query(r#"DELETE FROM "TrafficReceipt" WHERE "createdAt" < $1"#)
        .bind(receipt_cutoff)
        .execute(pool)
        .await?;

    sqlx::query(r#"DELETE FROM "NodeMetric" WHERE "recordedAt" < $1"#)
        .bind(metric_cutoff)
        .execute(pool)
        .await?;

    sqlx::query(r#"DELETE FROM "SubRequestHistory" WHERE "createdAt" < $1"#)
        .bind(srh_cutoff)
        .execute(pool)
        .await?;

    sqlx::query(r#"DELETE FROM "TrafficLog" WHERE "recordAt" < $1"#)
        .bind(traffic_cutoff)
        .execute(pool)
        .await?;

    sqlx::query(r#"DELETE FROM "AuditLog" WHERE "createdAt" < $1"#)
        .bind(audit_cutoff)
        .execute(pool)
        .await?;

    Ok(())
}
