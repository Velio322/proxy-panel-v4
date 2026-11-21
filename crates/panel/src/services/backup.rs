use crate::db::enums::{BackupStatus, BackupType};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use sqlx::Row;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tokio::process::Command;

/// Triggers a database backup in the background (`` async `tokio::fs`, no blocking `std::fs`).
pub async fn trigger_background_backup(state: AppState) -> AppResult<String> {
    if state
        .backup_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(AppError::conflict("Backup already running"));
    }

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
        state.backup_running.store(false, Ordering::SeqCst);
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
    cmd.arg("--no-password")
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

    let output = tokio::time::timeout(std::time::Duration::from_secs(300), cmd.output())
        .await
        .map_err(|_| AppError::internal("pg_dump timed out"))?
        .map_err(|e| AppError::internal(format!("pg_dump spawn failed: {e}")))?;

    if !output.status.success() {
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
