use crate::services::{backup, enforcement, node_client, retention};
use crate::state::AppState;
use futures_util::stream::{self, StreamExt};
use std::time::Duration;

pub fn start(state: AppState) {
    // 1. Enforcement loop (every 60s)
    let st_enforce = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            if let Err(e) = enforcement::enforce_all_clients(&st_enforce).await {
                tracing::warn!(error = %e, "scheduled enforcement failed");
            }
        }
    });

    // 2. Node healthcheck loop (every 120s, concurrency 4)
    let st_nodes = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(120));
        loop {
            interval.tick().await;
            let Ok(rows) = sqlx::query(
                r#"SELECT id, name, host, "apiPort", secret FROM "Node" WHERE active = true"#,
            )
            .fetch_all(&st_nodes.db)
            .await
            else {
                continue;
            };

            let targets: Vec<node_client::NodeTarget> = rows
                .into_iter()
                .filter_map(|r| {
                    use sqlx::Row;
                    Some(node_client::NodeTarget {
                        id: r.try_get("id").ok()?,
                        name: r.try_get("name").ok()?,
                        host: r.try_get("host").ok()?,
                        api_port: r.try_get("apiPort").ok()?,
                        secret: r.try_get("secret").ok()?,
                    })
                })
                .collect();

            stream::iter(targets)
                .for_each_concurrent(4, |node| {
                    let st = st_nodes.clone();
                    async move {
                        let _ = node_client::check_status(&st, &node).await;
                    }
                })
                .await;
        }
    });

    // 3. Retention cleanup loop (every 24h)
    let st_ret = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(86_400));
        loop {
            interval.tick().await;
            if let Err(e) = retention::cleanup_old_records(&st_ret.db).await {
                tracing::warn!(error = %e, "retention cleanup failed");
            }
        }
    });

    // 4. Scheduled backup loop (checks cron expression every 60s when enabled)
    if state.cfg.backup_enabled {
        let st_bak = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            let mut last_run_minute = String::new();
            loop {
                interval.tick().await;
                let now = chrono::Utc::now();
                let minute_key = now.format("%Y-%m-%d %H:%M").to_string();
                if minute_key == last_run_minute {
                    continue;
                }
                if cron_matches(&st_bak.cfg.backup_interval, now) {
                    last_run_minute = minute_key;
                    let _ = backup::trigger_background_backup(st_bak.clone()).await;
                }
            }
        });
    }
}

fn cron_matches(expr: &str, now: chrono::DateTime<chrono::Utc>) -> bool {
    let normalized = if expr.split_whitespace().count() == 5 {
        format!("0 {expr}")
    } else {
        expr.to_string()
    };
    let Ok(schedule) = normalized.parse::<cron::Schedule>() else {
        return false;
    };
    let window_start = now - chrono::Duration::seconds(59);
    schedule
        .after(&window_start)
        .next()
        .is_some_and(|next| next <= now)
}
