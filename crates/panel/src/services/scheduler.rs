use crate::services::{backup, enforcement, node_client, retention};
use crate::state::AppState;
use futures_util::stream::{self, StreamExt};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

/// Hard cap for checking one node (HTTP GET is capped at `NODE_GET_TIMEOUT`, rest is DB writes).
const NODE_CHECK_TIMEOUT: Duration = Duration::from_secs(15);
/// How often the watchdog inspects supervised jobs.
const WATCHDOG_TICK: Duration = Duration::from_secs(30);
/// Extra slack on top of `period + max_cycle` before a job is declared stuck.
const WATCHDOG_SLACK: Duration = Duration::from_secs(30);

type JobFuture = futures_util::future::BoxFuture<'static, ()>;
type JobFn = Arc<dyn Fn(AppState) -> JobFuture + Send + Sync>;

struct JobSpec {
    name: &'static str,
    period: Duration,
    /// A single cycle is cancelled when it runs longer than this.
    max_cycle: Duration,
    run: JobFn,
}

fn unix_now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Runs one job forever: tick, execute one cycle under a hard timeout, log the outcome.
/// `heartbeat` is refreshed on every loop iteration so the watchdog can detect a stuck loop.
async fn job_loop(state: AppState, spec: Arc<JobSpec>, heartbeat: Arc<AtomicI64>) {
    let mut interval = tokio::time::interval(spec.period);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut cycle: u64 = 0;
    loop {
        interval.tick().await;
        heartbeat.store(unix_now(), Ordering::Relaxed);
        cycle += 1;
        let started = Instant::now();
        tracing::debug!(job = spec.name, cycle, "scheduler cycle started");
        match tokio::time::timeout(spec.max_cycle, (spec.run)(state.clone())).await {
            Ok(()) => tracing::info!(
                job = spec.name,
                cycle,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "scheduler cycle finished"
            ),
            Err(_) => tracing::error!(
                job = spec.name,
                cycle,
                limit_secs = spec.max_cycle.as_secs(),
                "scheduler cycle exceeded limit and was cancelled"
            ),
        }
        heartbeat.store(unix_now(), Ordering::Relaxed);
    }
}

/// Decides whether a job loop must be restarted.
fn is_stuck(last_beat_unix: i64, now_unix: i64, limit: Duration) -> bool {
    now_unix.saturating_sub(last_beat_unix) > i64::try_from(limit.as_secs()).unwrap_or(i64::MAX)
}

/// Spawns `job_loop` under a watchdog that aborts and respawns it when it finished
/// unexpectedly (panic/early return) or has not made progress for too long.
fn supervise(state: AppState, spec: JobSpec) {
    let spec = Arc::new(spec);
    tokio::spawn(async move {
        let stuck_after = spec.period + spec.max_cycle + WATCHDOG_SLACK;
        let spawn_loop = |state: AppState, spec: Arc<JobSpec>| {
            let beat = Arc::new(AtomicI64::new(unix_now()));
            let handle = tokio::spawn(job_loop(state, spec, beat.clone()));
            (handle, beat)
        };

        let (mut handle, mut beat) = spawn_loop(state.clone(), spec.clone());
        let mut watchdog = tokio::time::interval(WATCHDOG_TICK);
        watchdog.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            watchdog.tick().await;
            let finished = handle.is_finished();
            let stuck = is_stuck(beat.load(Ordering::Relaxed), unix_now(), stuck_after);
            if finished || stuck {
                tracing::error!(
                    job = spec.name,
                    finished,
                    stuck,
                    "scheduler watchdog restarting job"
                );
                handle.abort();
                (handle, beat) = spawn_loop(state.clone(), spec.clone());
            }
        }
    });
}

async fn check_node_guarded(state: &AppState, node: &node_client::NodeTarget) {
    match tokio::time::timeout(NODE_CHECK_TIMEOUT, node_client::check_status_scheduled(state, node)).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => tracing::warn!(node_id = %node.id, error = %e, "node health check failed"),
        Err(_) => tracing::warn!(
            node_id = %node.id,
            limit_secs = NODE_CHECK_TIMEOUT.as_secs(),
            "node health check timed out"
        ),
    }
}

async fn node_healthcheck_cycle(state: AppState) {
    let Ok(rows) = sqlx::query(
        r#"SELECT id, name, host, "apiPort", secret, "apiMode" FROM "Node" WHERE active = true AND "apiMode" <> 'pull'"#,
    )
    .fetch_all(&state.db)
    .await
    else {
        tracing::warn!("node healthcheck: failed to load node list");
        return;
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
                api_mode: r.try_get("apiMode").unwrap_or_else(|_| "ws".to_string()),
            })
        })
        .collect();

    let total = targets.len();
    stream::iter(targets)
        .for_each_concurrent(4, |node| {
            let st = state.clone();
            async move { check_node_guarded(&st, &node).await }
        })
        .await;
    tracing::debug!(nodes = total, "node healthcheck cycle covered all nodes");
}

pub fn start(state: AppState) {
    // 1. Enforcement loop (every 60s)
    supervise(
        state.clone(),
        JobSpec {
            name: "enforcement",
            period: Duration::from_secs(60),
            max_cycle: Duration::from_secs(120),
            run: Arc::new(|st| {
                Box::pin(async move {
                    if let Err(e) = enforcement::enforce_all_clients(&st).await {
                        tracing::warn!(error = %e, "scheduled enforcement failed");
                    }
                })
            }),
        },
    );

    // 2. Node healthcheck loop (every 120s, concurrency 4, per-node hard timeout)
    supervise(
        state.clone(),
        JobSpec {
            name: "node_healthcheck",
            period: Duration::from_secs(120),
            max_cycle: Duration::from_secs(100),
            run: Arc::new(|st| Box::pin(node_healthcheck_cycle(st))),
        },
    );

    // 2b. Pull-mode nodes: status, traffic (collect/ack) and sessions over mTLS HTTPS (every 30s)
    supervise(
        state.clone(),
        JobSpec {
            name: "node_pull",
            period: Duration::from_secs(30),
            max_cycle: Duration::from_secs(120),
            run: Arc::new(|st| Box::pin(crate::services::node_pull::poll_cycle(st))),
        },
    );

    // 3. Retention cleanup loop (every 24h)
    supervise(
        state.clone(),
        JobSpec {
            name: "retention",
            period: Duration::from_secs(86_400),
            max_cycle: Duration::from_secs(3_600),
            run: Arc::new(|st| {
                Box::pin(async move {
                    if let Err(e) = retention::cleanup_old_records(&st.db).await {
                        tracing::warn!(error = %e, "retention cleanup failed");
                    }
                })
            }),
        },
    );

    // 4. Scheduled backup loop (checks cron expression every 60s when enabled)
    if state.cfg.backup_enabled {
        let last_run_minute = Arc::new(std::sync::Mutex::new(String::new()));
        supervise(
            state,
            JobSpec {
                name: "backup_cron",
                period: Duration::from_secs(60),
                max_cycle: Duration::from_secs(60),
                run: Arc::new(move |st| {
                    let last_run_minute = last_run_minute.clone();
                    Box::pin(async move {
                        let now = chrono::Utc::now();
                        let minute_key = now.format("%Y-%m-%d %H:%M").to_string();
                        {
                            let Ok(last) = last_run_minute.lock() else { return };
                            if *last == minute_key {
                                return;
                            }
                        }
                        if cron_matches(&st.cfg.backup_interval, now) {
                            if let Ok(mut last) = last_run_minute.lock() {
                                *last = minute_key;
                            }
                            // Detached so a cycle timeout / watchdog abort can never cancel it between
                            // setting `backup_running` and spawning the dump (flag would stay stuck).
                            tokio::spawn(async move {
                                if let Err(e) = backup::trigger_background_backup(st).await {
                                    tracing::warn!(error = %e, "scheduled backup not started");
                                }
                            });
                        }
                    })
                }),
            },
        );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stuck_detection_uses_limit() {
        let limit = Duration::from_secs(100);
        assert!(!is_stuck(1_000, 1_100, limit));
        assert!(is_stuck(1_000, 1_101, limit));
        assert!(!is_stuck(2_000, 1_000, limit));
    }

    #[test]
    fn cron_every_minute_matches() {
        assert!(cron_matches("* * * * *", chrono::Utc::now()));
        assert!(!cron_matches("not a cron", chrono::Utc::now()));
    }
}
