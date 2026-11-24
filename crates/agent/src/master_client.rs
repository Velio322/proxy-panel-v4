use crate::api::execute_core_update;
use crate::config::{AgentConfig, AGENT_VERSION};
use crate::cores::write_atomic_0600;
use crate::process::ProcessManager;
use crate::traffic::CumulativeTrafficTracker;
use futures_util::{SinkExt, StreamExt};
use mypanel_proto::{
    ApplyConfigRequest, CommandAck, EmptyPayload, Envelope, NodeControlAction, RegisterRequest,
    RegisterSystemInfo, SessionsList, TrafficCounter, TrafficReport,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

/// Reconnect backoff schedule: 1s -> 2s -> 5s -> 15s (cap).
const RECONNECT_BACKOFF: [Duration; 4] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(15),
];
/// A session that lived at least this long is considered healthy and resets the backoff.
const STABLE_SESSION: Duration = Duration::from_secs(30);
/// Panel pings every ~25s; if nothing (data, Ping or Pong) arrives for this long the link is dead.
const READ_IDLE_TIMEOUT: Duration = Duration::from_secs(75);
/// Upper bound for one WS write (stalled TCP peer / NAT black hole).
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
/// Upper bound for collecting a status/traffic snapshot (e.g. `ProcessManager` lock contention).
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(20);
/// Initial registration must never delay the WS loop for minutes.
const INITIAL_REGISTER_ATTEMPTS: usize = 3;
const INITIAL_REGISTER_DELAY: Duration = Duration::from_secs(2);
const STATUS_INTERVAL: Duration = Duration::from_secs(30);
const TRAFFIC_INTERVAL: Duration = Duration::from_secs(60);
/// Max stale (unacked) batches re-sent per traffic tick, and on reconnect.
const REPLAY_PER_TICK: usize = 50;

/// Registers this node with the Master panel (`POST /api/v1/nodes/self/register`).
/// Retries up to `max_attempts` times (`5s` apart) before returning an error.
pub async fn register_node(
    client: &reqwest::Client,
    cfg: &AgentConfig,
    max_attempts: usize,
    retry_delay: Duration,
) -> Result<(), String> {
    if cfg.master_url.is_empty() {
        return Ok(());
    }
    let url = format!("{}/api/v1/nodes/self/register", cfg.master_url);
    let payload = RegisterRequest {
        token: cfg.node_secret.clone(),
        name: Some(cfg.node_name.clone()),
        host: Some(cfg.node_host.clone()),
        port: Some(443),
        api_port: Some(cfg.worker_port),
        system: Some(RegisterSystemInfo {
            release: AGENT_VERSION.to_string(),
        }),
        csr_pem: None,
        api_mode: Some("ws".to_string()),
    };

    let mut last_err = String::new();
    for attempt in 1..=max_attempts.max(1) {
        match client
            .post(&url)
            .json(&payload)
            .timeout(Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                tracing::info!(attempt, "Registered node with Master");
                return Ok(());
            }
            Ok(resp) => {
                last_err = format!("HTTP {}", resp.status());
            }
            Err(err) => {
                last_err = err.to_string();
            }
        }
        if attempt < max_attempts {
            tokio::time::sleep(retry_delay).await;
        }
    }

    Err(format!(
        "Node registration failed after {max_attempts} attempts: {last_err}"
    ))
}

/// Delay before reconnect attempt number `failures` (0-based count of consecutive failures).
fn reconnect_delay(failures: u32) -> Duration {
    let idx = usize::try_from(failures).unwrap_or(usize::MAX);
    RECONNECT_BACKOFF[idx.min(RECONNECT_BACKOFF.len() - 1)]
}

/// Adds jitter (20% of the delay, at least 3s) so a fleet of agents does not reconnect in
/// lockstep after a panel restart and make it rebuild every node config at once.
fn with_jitter(base: Duration) -> Duration {
    use rand::Rng;
    let max_ms = (base.as_millis() as u64 / 5).max(3_000);
    let extra_ms = rand::thread_rng().gen_range(0..=max_ms);
    base + Duration::from_millis(extra_ms)
}

/// Converts an HTTP(S) `MASTER_URL` into a WS(S) `/ws/worker` URL.
pub fn build_ws_url(master_url: &str) -> Result<String, String> {
    let trimmed = master_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("MASTER_URL is empty".into());
    }
    let ws_base = if let Some(rest) = trimmed.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = trimmed.strip_prefix("http://") {
        format!("ws://{rest}")
    } else if trimmed.starts_with("ws://") || trimmed.starts_with("wss://") {
        trimmed.to_string()
    } else {
        return Err(format!("Unsupported MASTER_URL scheme: {trimmed}"));
    };
    Ok(format!("{ws_base}/ws/worker"))
}

/// Runs the Master registration + persistent WebSocket control loop until `cancel` is triggered.
pub async fn run_master_loop(
    cfg: AgentConfig,
    http_client: reqwest::Client,
    pm: Arc<Mutex<ProcessManager>>,
    tracker: Arc<Mutex<CumulativeTrafficTracker>>,
    cancel: CancellationToken,
) {
    if cfg.master_url.is_empty() {
        return;
    }

    if let Err(err) = register_node(
        &http_client,
        &cfg,
        INITIAL_REGISTER_ATTEMPTS,
        INITIAL_REGISTER_DELAY,
    )
    .await
    {
        tracing::warn!(error = %err, "Initial node registration failed; continuing to WS loop");
    }

    let last_applied_json: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let mut failures: u32 = 0;

    loop {
        if cancel.is_cancelled() {
            break;
        }

        let started = tokio::time::Instant::now();
        let result = connect_and_serve(
            &cfg,
            &http_client,
            &pm,
            &tracker,
            &last_applied_json,
            &cancel,
        )
        .await;

        match result {
            Ok(()) => break,
            Err(err) => {
                if started.elapsed() >= STABLE_SESSION {
                    failures = 0;
                }
                let delay = with_jitter(reconnect_delay(failures));
                failures = failures.saturating_add(1);
                tracing::warn!(
                    error = %err,
                    attempt = failures,
                    retry_in_ms = delay.as_millis() as u64,
                    "Master WS disconnected; reconnecting"
                );
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = tokio::time::sleep(delay) => {}
                }
            }
        }
    }
}

/// Takes a status snapshot without ever blocking the WS loop for longer than `SNAPSHOT_TIMEOUT`.
async fn status_snapshot(pm: &Arc<Mutex<ProcessManager>>) -> Result<mypanel_proto::StatusReport, String> {
    tokio::time::timeout(SNAPSHOT_TIMEOUT, async {
        let mut guard = pm.lock().await;
        guard.status_report(false).await
    })
    .await
    .map_err(|_| "status snapshot timed out".to_string())
}

async fn connect_and_serve(
    cfg: &AgentConfig,
    http_client: &reqwest::Client,
    pm: &Arc<Mutex<ProcessManager>>,
    tracker: &Arc<Mutex<CumulativeTrafficTracker>>,
    last_applied_json: &Arc<Mutex<Option<String>>>,
    cancel: &CancellationToken,
) -> Result<(), String> {
    let ws_url = build_ws_url(&cfg.master_url)?;
    let mut request = ws_url
        .into_client_request()
        .map_err(|e| format!("Invalid WS request: {e}"))?;

    let bearer = format!("Bearer {}", cfg.node_secret);
    let bearer_hv =
        HeaderValue::from_str(&bearer).map_err(|e| format!("Invalid Bearer header: {e}"))?;
    let secret_hv = HeaderValue::from_str(&cfg.node_secret)
        .map_err(|e| format!("Invalid X-Node-Secret header: {e}"))?;
    request.headers_mut().insert("Authorization", bearer_hv);
    request.headers_mut().insert("X-Node-Secret", secret_hv);

    let (ws_stream, _) = tokio::time::timeout(
        Duration::from_secs(10),
        tokio_tungstenite::connect_async(request),
    )
    .await
    .map_err(|_| "WS handshake timed out".to_string())?
    .map_err(|e| format!("WS connect failed: {e}"))?;

    let (mut write, mut read) = ws_stream.split();

    // Send initial status_report + config_request
    {
        let status = status_snapshot(pm).await?;
        send_envelope(&mut write, &Envelope::StatusReport(status)).await?;
        send_envelope(&mut write, &Envelope::ConfigRequest(EmptyPayload {})).await?;

        // Replay any unacknowledged pending traffic batches
        let pending = {
            let guard = tracker.lock().await;
            guard.pending_replay(REPLAY_PER_TICK, &std::collections::HashSet::new())
        };
        for report in pending {
            send_envelope(&mut write, &Envelope::TrafficReport(report)).await?;
        }
    }

    let (job_tx, job_rx) = mpsc::channel::<Envelope>(64);
    let (out_tx, mut out_rx) = mpsc::channel::<Envelope>(64);
    tokio::spawn(command_worker(
        job_rx,
        out_tx,
        cfg.clone(),
        http_client.clone(),
        pm.clone(),
        last_applied_json.clone(),
    ));

    let mut status_tick = tokio::time::interval(STATUS_INTERVAL);
    status_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut traffic_tick = tokio::time::interval(TRAFFIC_INTERVAL);
    traffic_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let mut last_seen = tokio::time::Instant::now();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = tokio::time::timeout(WRITE_TIMEOUT, write.close()).await;
                return Ok(());
            }
            _ = tokio::time::sleep_until(last_seen + READ_IDLE_TIMEOUT) => {
                return Err(format!(
                    "no frames from Master for {}s (dead connection)",
                    READ_IDLE_TIMEOUT.as_secs()
                ));
            }
            Some(env) = out_rx.recv() => {
                send_envelope(&mut write, &env).await?;
            }
            _ = status_tick.tick() => {
                // A busy ProcessManager (config apply / core update in the worker) must not
                // drop the link: skip this tick instead.
                match status_snapshot(pm).await {
                    Ok(status) => send_envelope(&mut write, &Envelope::StatusReport(status)).await?,
                    Err(err) => tracing::warn!(error = %err, "skipping status report"),
                }
            }
            _ = traffic_tick.tick() => {
                let Ok(per_core) = tokio::time::timeout(SNAPSHOT_TIMEOUT, async {
                    let mut guard = pm.lock().await;
                    guard.collect_traffic_by_core().await
                })
                .await
                else {
                    tracing::warn!("traffic snapshot timed out; skipping this tick");
                    continue;
                };
                let cur_refs: Vec<(&str, &HashMap<String, TrafficCounter>)> =
                    per_core.iter().map(|(c, m)| (*c, m)).collect();
                let (report, extra) = {
                    let mut tr = tracker.lock().await;
                    let report = match tr.compute_delta_multi(&cur_refs).await {
                        Ok(r) => r,
                        Err(err) => {
                            tracing::warn!(error = %err, "traffic delta failed; will retry next tick");
                            continue;
                        }
                    };
                    (report, tr.take_overflow_reports())
                };
                let sent_batch = report.batch_id.clone();
                send_envelope(&mut write, &Envelope::TrafficReport(report)).await?;
                let mut sent_ids: std::collections::HashSet<String> = extra
                    .iter()
                    .filter_map(|r| r.batch_id.clone())
                    .collect();
                sent_ids.extend(sent_batch);
                for r in extra {
                    send_envelope(&mut write, &Envelope::TrafficReport(r)).await?;
                }
                // Re-deliver batches whose ack never arrived (panel dedups by batch id).
                let stale = {
                    let tr = tracker.lock().await;
                    tr.pending_replay(REPLAY_PER_TICK, &sent_ids)
                };
                for r in stale {
                    send_envelope(&mut write, &Envelope::TrafficReport(r)).await?;
                }
            }
            msg = read.next() => {
                if matches!(msg, Some(Ok(_))) {
                    last_seen = tokio::time::Instant::now();
                }
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(env) = serde_json::from_str::<Envelope>(&text) {
                            match env {
                                Envelope::TrafficAck(ack) => {
                                    let mut tr = tracker.lock().await;
                                    let _ = tr.acknowledge_batch(&ack.batch_id).await;
                                }
                                Envelope::ConfigUpdate(_)
                                | Envelope::ApplyConfigRequest(_)
                                | Envelope::NodeControlCmd(_)
                                | Envelope::DropSessionCmd(_)
                                | Envelope::CoreUpdateCmd(_) => {
                                    // Long handlers run in the serial worker so this loop keeps
                                    // answering pings and never trips the panel's pong timeout.
                                    if job_tx.try_send(env).is_err() {
                                        tracing::warn!("command queue full or worker gone; command dropped");
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        send_frame(&mut write, Message::Pong(payload)).await?;
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        return Err("WS closed by Master".into());
                    }
                    Some(Err(e)) => {
                        return Err(format!("WS read error: {e}"));
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Executes panel commands one at a time, outside the WS read loop. Ends when the connection
/// drops (`job_tx` closed) after finishing the command in flight.
async fn command_worker(
    mut jobs: mpsc::Receiver<Envelope>,
    out: mpsc::Sender<Envelope>,
    cfg: AgentConfig,
    http_client: reqwest::Client,
    pm: Arc<Mutex<ProcessManager>>,
    last_applied_json: Arc<Mutex<Option<String>>>,
) {
    while let Some(env) = jobs.recv().await {
        handle_command(env, &cfg, &http_client, &pm, &last_applied_json, &out).await;
    }
}

async fn emit(out: &mpsc::Sender<Envelope>, env: Envelope) {
    // The connection may already be gone; the panel re-sends config on reconnect.
    let _ = out.send(env).await;
}

async fn handle_command(
    env: Envelope,
    cfg: &AgentConfig,
    http_client: &reqwest::Client,
    pm: &Arc<Mutex<ProcessManager>>,
    last_applied_json: &Arc<Mutex<Option<String>>>,
    out: &mpsc::Sender<Envelope>,
) {
    match env {
        Envelope::ConfigUpdate(cmd) => {
            let fingerprint =
                serde_json::to_string(&(&cmd.inbounds, &cmd.routing_rules, &cmd.xray_profile)).ok();
            let unchanged = {
                let last = last_applied_json.lock().await;
                fingerprint.is_some() && fingerprint == *last
            };
            if unchanged {
                // Same config as last time: only skip when every expected core is alive,
                // otherwise fall through and re-apply so a crashed core is brought back.
                let (problems, healthy) = {
                    let mut guard = pm.lock().await;
                    let healthy = guard.cores_healthy().await;
                    (guard.problems().to_vec(), healthy)
                };
                if healthy {
                    let ack = CommandAck {
                        command_id: cmd.command_id,
                        ok: true,
                        error: (!problems.is_empty()).then(|| problems.join("; ")),
                    };
                    emit(out, Envelope::CommandAck(ack)).await;
                    return;
                }
            }

            let apply_res = {
                let mut guard = pm.lock().await;
                guard
                    .apply_config_with_profile(
                        cmd.inbounds.clone(),
                        cmd.routing_rules.clone(),
                        cmd.xray_profile.clone(),
                    )
                    .await
            };

            let ack = match apply_res {
                Ok(_) => {
                    *last_applied_json.lock().await = fingerprint;
                    let problems = {
                        let guard = pm.lock().await;
                        guard.problems().to_vec()
                    };
                    let desired = ApplyConfigRequest {
                        inbounds: cmd.inbounds,
                        routing_rules: cmd.routing_rules,
                        xray_profile: cmd.xray_profile,
                    };
                    if let Ok(bytes) = serde_json::to_vec_pretty(&desired) {
                        let _ = write_atomic_0600(&cfg.desired_config_path(), &bytes).await;
                    }
                    // Applied, but some inbound may not have come up: ok=true keeps the command from
                    // being re-queued forever, `error` carries the warning for the panel to store.
                    CommandAck {
                        command_id: cmd.command_id,
                        ok: true,
                        error: (!problems.is_empty()).then(|| problems.join("; ")),
                    }
                }
                Err(err) => CommandAck {
                    command_id: cmd.command_id,
                    ok: false,
                    error: Some(err),
                },
            };
            emit(out, Envelope::CommandAck(ack)).await;
        }
        Envelope::ApplyConfigRequest(req) => {
            let mut guard = pm.lock().await;
            if guard
                .apply_config_with_profile(
                    req.inbounds.clone(),
                    req.routing_rules.clone(),
                    req.xray_profile.clone(),
                )
                .await
                .is_ok()
            {
                if let Ok(bytes) = serde_json::to_vec_pretty(&req) {
                    let _ = write_atomic_0600(&cfg.desired_config_path(), &bytes).await;
                }
            }
        }
        Envelope::NodeControlCmd(cmd) => {
            let result: Result<(), String> = match cmd.action {
                NodeControlAction::Restart => {
                    let mut guard = pm.lock().await;
                    guard.restart().await.map(|_| ())
                }
                NodeControlAction::Stop => {
                    let mut guard = pm.lock().await;
                    guard.stop_all().await;
                    Ok(())
                }
                NodeControlAction::Report => Ok(()),
            };
            // Always follow up with a fresh status so the panel sees the new state immediately.
            if let Ok(status) = status_snapshot(pm).await {
                emit(out, Envelope::StatusReport(status)).await;
            }
            let ack = CommandAck {
                command_id: cmd.command_id,
                ok: result.is_ok(),
                error: result.err(),
            };
            emit(out, Envelope::CommandAck(ack)).await;
        }
        Envelope::DropSessionCmd(cmd) => {
            let res = {
                let mut guard = pm.lock().await;
                guard.drop_session(&cmd).await
            };
            let ack = match res {
                Ok(_) => {
                    let sessions = {
                        let mut guard = pm.lock().await;
                        guard.get_sessions().await
                    };
                    emit(out, Envelope::SessionsList(SessionsList { sessions })).await;
                    CommandAck {
                        command_id: cmd.command_id,
                        ok: true,
                        error: None,
                    }
                }
                Err(err) => CommandAck {
                    command_id: cmd.command_id,
                    ok: false,
                    error: Some(err),
                },
            };
            emit(out, Envelope::CommandAck(ack)).await;
        }
        Envelope::CoreUpdateCmd(cmd) => {
            let ack = match execute_core_update(pm, http_client, &cmd).await {
                Ok(_) => {
                    if let Ok(status) = status_snapshot(pm).await {
                        emit(out, Envelope::StatusReport(status)).await;
                    }
                    CommandAck {
                        command_id: cmd.command_id,
                        ok: true,
                        error: None,
                    }
                }
                Err(err) => CommandAck {
                    command_id: cmd.command_id,
                    ok: false,
                    error: Some(err),
                },
            };
            emit(out, Envelope::CommandAck(ack)).await;
        }
        _ => {}
    }
}

async fn send_frame<S>(write: &mut S, msg: Message) -> Result<(), String>
where
    S: SinkExt<Message> + Unpin,
    S::Error: std::fmt::Display,
{
    match tokio::time::timeout(WRITE_TIMEOUT, write.send(msg)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(format!("WS send failed: {e}")),
        Err(_) => Err(format!("WS send timed out after {}s", WRITE_TIMEOUT.as_secs())),
    }
}

async fn send_envelope<S>(write: &mut S, env: &Envelope) -> Result<(), String>
where
    S: SinkExt<Message> + Unpin,
    S::Error: std::fmt::Display,
{
    let text = serde_json::to_string(env)
        .map_err(|e| format!("Failed to serialize WS envelope: {e}"))?;
    send_frame(write, Message::Text(text.into())).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_follows_1_2_5_15_and_caps() {
        let secs: Vec<u64> = (0..7).map(|i| reconnect_delay(i).as_secs()).collect();
        assert_eq!(secs, vec![1, 2, 5, 15, 15, 15, 15]);
        assert_eq!(reconnect_delay(u32::MAX).as_secs(), 15);
    }

    #[test]
    fn jitter_never_shrinks_and_stays_bounded() {
        for _ in 0..200 {
            let base = Duration::from_secs(15);
            let d = with_jitter(base);
            assert!(d >= base && d <= base + Duration::from_secs(3));
        }
    }

    #[test]
    fn ws_url_scheme_mapping() {
        assert_eq!(
            build_ws_url("https://p.example.com/").unwrap(),
            "wss://p.example.com/ws/worker"
        );
        assert_eq!(
            build_ws_url("http://10.0.0.1:8080").unwrap(),
            "ws://10.0.0.1:8080/ws/worker"
        );
        assert!(build_ws_url("").is_err());
    }
}
