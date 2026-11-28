use crate::db::enums::{BackupStatus, BackupType};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use sqlx::Row;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tokio::process::Command;

struct BackupRunningGuard(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl Drop for BackupRunningGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Triggers a database backup in the background (`` async `tokio::fs`, no blocking `std::fs`).
pub async fn trigger_background_backup(state: AppState) -> AppResult<String> {
    if state
        .backup_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(AppError::conflict("Backup already running"));
    }
    let running_guard = BackupRunningGuard(state.backup_running.clone());

    let log_id = crate::db::new_id();
    let now = crate::db::now();
    sqlx::query(
        r#"INSERT INTO "BackupLog" (id, type, status, destination, "createdAt")
           VALUES ($1, $2, $3, 'local', $4)"#,
    )
    .bind(&log_id)
    .bind(BackupType::Database)
    .bind(BackupStatus::InProgress)
    .bind(now)
    .execute(&state.db)
    .await?;

    let lid = log_id.clone();
    tokio::spawn(async move {
        let _guard = running_guard;
        let res = run_pg_dump(&state).await;
        match res {
            Ok((path, size)) => {
                let _ = sqlx::query(
                    r#"UPDATE "BackupLog" SET status = $2, "filePath" = $3, "fileSize" = $4 WHERE id = $1"#,
                )
                .bind(&lid)
                .bind(BackupStatus::Success)
                .bind(path.to_string_lossy().to_string())
                .bind(i64::try_from(size).unwrap_or(0))
                .execute(&state.db)
                .await;
                let _ = prune_old_backups(&state).await;
            }
            Err(err) => {
                tracing::error!(error = %err, "Database backup failed");
                crate::services::notification_service::push(
                    &state,
                    crate::db::enums::NotificationType::Error,
                    "Database backup failed",
                    &format!("Scheduled database backup failed: {err}"),
                    true,
                )
                .await;
                let _ = sqlx::query(
                    r#"UPDATE "BackupLog" SET status = $2, error = $3 WHERE id = $1"#,
                )
                .bind(&lid)
                .bind(BackupStatus::Failed)
                .bind("Database backup failed; inspect server installation and database availability")
                .execute(&state.db)
                .await;
            }
        }
    });

    Ok(log_id)
}

async fn run_pg_dump(state: &AppState) -> AppResult<(PathBuf, u64)> {
    tokio::fs::create_dir_all(&state.cfg.backup_dir)
        .await
        .map_err(|e| AppError::internal(format!("Cannot create backup dir: {e}")))?;

    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let file_path = state
        .cfg
        .backup_dir
        .join(format!("proxpanel-{stamp}-{}.sql", &crate::db::new_id()[..8]));

    let parsed = url::Url::parse(&state.cfg.database_url)
        .map_err(|e| AppError::internal(format!("Invalid DATABASE_URL: {e}")))?;

    let mut cmd = Command::new(&state.cfg.pg_dump_bin);
    cmd.kill_on_drop(true)
        .arg("--no-password")
        .arg("--file")
        .arg(&file_path);

    if let Some(host) = parsed.host_str() {
        cmd.env("PGHOST", host);
    }
    if let Some(port) = parsed.port() {
        cmd.env("PGPORT", port.to_string());
    }
    if !parsed.username().is_empty() {
        cmd.env("PGUSER", parsed.username());
    }
    if let Some(pw) = parsed.password() {
        let dec = percent_encoding::percent_decode_str(pw)
            .decode_utf8()
            .map(|c| c.into_owned())
            .unwrap_or_else(|_| pw.to_string());
        cmd.env("PGPASSWORD", dec);
    }
    let db_name = parsed.path().trim_start_matches('/');
    if !db_name.is_empty() {
        cmd.env("PGDATABASE", db_name);
    }

    let output = match tokio::time::timeout(std::time::Duration::from_secs(300), cmd.output()).await
    {
        Ok(res) => res.map_err(|e| AppError::internal(format!("pg_dump spawn failed: {e}")))?,
        Err(_) => {
            let _ = tokio::fs::remove_file(&file_path).await;
            return Err(AppError::internal("pg_dump timed out"));
        }
    };

    if !output.status.success() {
        let stderr_raw = String::from_utf8_lossy(&output.stderr);
        let stderr_snip: String = stderr_raw.trim().chars().take(512).collect();
        if !stderr_snip.is_empty() {
            tracing::error!(stderr = %stderr_snip, "pg_dump failed");
        }
        let _ = tokio::fs::remove_file(&file_path).await;
        return Err(AppError::internal("pg_dump exited with non-zero status"));
    }

    let meta = tokio::fs::metadata(&file_path)
        .await
        .map_err(|e| AppError::internal(format!("stat backup failed: {e}")))?;
    Ok((file_path, meta.len()))
}

async fn prune_old_backups(state: &AppState) -> AppResult<()> {
    let keep = i64::from(state.cfg.backup_retention.max(1));
    let rows = sqlx::query(
        r#"SELECT id, "filePath" FROM "BackupLog"
           WHERE status = 'SUCCESS' AND "filePath" IS NOT NULL
           ORDER BY "createdAt" DESC
           OFFSET $1"#,
    )
    .bind(keep)
    .fetch_all(&state.db)
    .await?;

    for row in rows {
        let id: String = row.try_get("id")?;
        if let Ok(Some(fp)) = row.try_get::<Option<String>, _>("filePath") {
            let _ = tokio::fs::remove_file(&fp).await;
        }
        let _ = sqlx::query(r#"DELETE FROM "BackupLog" WHERE id = $1"#)
            .bind(id)
            .execute(&state.db)
            .await;
    }
    Ok(())
}
