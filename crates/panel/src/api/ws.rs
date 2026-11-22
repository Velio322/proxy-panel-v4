use crate::auth::crypto::{ct_eq, sha256_hex};
use crate::db::enums::NodeStatus;
use crate::error::{AppError, AppResult};
use crate::services::{core_updates, node_config, traffic_batcher};
use crate::state::AppState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{header, HeaderMap};
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use mypanel_proto::{ConfigUpdate, Envelope, TrafficAck};
use sqlx::Row;
use tokio::sync::mpsc;

fn extract_node_secret(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("x-node-secret")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            headers
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .map(str::trim)
                .filter(|s| !s.is_empty())
        })
}

async fn find_node_by_secret(state: &AppState, secret: &str) -> AppResult<(String, String)> {
    let secret_hash = sha256_hex(secret);
    let rows = sqlx::query(
        r#"SELECT id, name, secret FROM "Node"
           WHERE active = true AND ("secretHash" = $1 OR secret = $2)
           LIMIT 5"#,
    )
    .bind(&secret_hash)
    .bind(secret)
    .fetch_all(&state.db)
    .await?;

    for r in rows {
        let db_sec: String = r.try_get("secret")?;
        if ct_eq(secret, &db_sec) {
            let id: String = r.try_get("id")?;
            let name: String = r.try_get("name")?;
            return Ok((id, name));
        }
    }
    Err(AppError::unauthorized("Unauthorized"))
}

pub struct ValidNodeSecret(pub String);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for ValidNodeSecret {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let secret = extract_node_secret(&parts.headers)
            .ok_or_else(|| AppError::unauthorized("Unauthorized"))?;
        // Reject short/oversized secrets before DB query or WS upgrade
        if secret.len() < 16 || secret.len() > 256 {
            return Err(AppError::unauthorized("Invalid node secret"));
        }
        Ok(Self(secret.to_string()))
    }
}

pub async fn upgrade(
    ValidNodeSecret(secret): ValidNodeSecret,
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> AppResult<Response> {
    let (node_id, node_name) = find_node_by_secret(&state, &secret).await?;
    Ok(ws.on_upgrade(move |socket| handle_socket(state, node_id, node_name, socket)))
}

async fn handle_socket(state: AppState, node_id: String, node_name: String, socket: WebSocket) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Envelope>();
    let conn_id = state.hub.register(&node_id, tx.clone());

    let _ = sqlx::query(
        r#"UPDATE "Node" SET status = 'ONLINE', "lastCheckAt" = $2, "updatedAt" = $2 WHERE id = $1"#,
    )
    .bind(&node_id)
    .bind(crate::db::now())
    .execute(&state.db)
    .await;
    state.metrics.set_node_status(&node_id, &node_name, true);

    let writer = tokio::spawn(async move {
        while let Some(env) = rx.recv().await {
            let Ok(text) = serde_json::to_string(&env) else {
                continue;
            };
            if ws_tx.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(msg)) = ws_rx.next().await {
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Close(_) => break,
            _ => continue,
        };
        let Ok(env) = serde_json::from_str::<Envelope>(&text) else {
            continue;
        };

        match env {
            Envelope::ConfigRequest(_) => {
                if let (Ok(inbounds), Ok(routing_rules)) = (
                    node_config::build(&state, &node_id).await,
                    node_config::build_routing_rules(&state, &node_id).await,
                ) {
                    let xray_profile = node_config::build_xray_profile(&state, &node_id)
                        .await
                        .ok()
                        .flatten();
                    let _ = tx.send(Envelope::ConfigUpdate(ConfigUpdate {
                        command_id: crate::db::new_id(),
                        revision: 0,
                        inbounds,
                        routing_rules,
                        xray_profile,
                        timestamp: chrono::Utc::now().timestamp_millis(),
                    }));
                }
            }
            Envelope::CommandAck(ack) => {
                state.hub.resolve_ack(ack.clone());
                let now = crate::db::now();
                if ack.ok {
                    let _ = sqlx::query(
                        r#"UPDATE "NodeCommand"
                           SET status = 'ACKNOWLEDGED', "acknowledgedAt" = $2, "lastError" = NULL
                           WHERE id = $1 AND "nodeId" = $3 AND status = 'PENDING'"#,
                    )
                    .bind(&ack.command_id)
                    .bind(now)
                    .bind(&node_id)
                    .execute(&state.db)
                    .await;
                } else {
                    let err_msg: String = ack
                        .error
                        .unwrap_or_else(|| "Apply failed".into())
                        .chars()
                        .take(1000)
                        .collect();
                    let _ = sqlx::query(
                        r#"UPDATE "NodeCommand" SET "lastError" = $2
                           WHERE id = $1 AND "nodeId" = $3 AND status = 'PENDING'"#,
                    )
                    .bind(&ack.command_id)
                    .bind(err_msg)
                    .bind(&node_id)
                    .execute(&state.db)
                    .await;
                }
            }
            Envelope::StatusReport(sr) => {
                let now = crate::db::now();
                let info = core_updates::extract_core_info(sr.cores.as_ref());
                let uptime_secs = crate::services::node_client::extract_report_uptime(&sr);

                let _ = sqlx::query(
                    r#"UPDATE "Node"
                       SET status = $2, version = $3, "cpuUsage" = $4, "memUsage" = $5,
                           "xrayVersion" = COALESCE($6, "xrayVersion"),
                           "singboxVersion" = COALESCE($7, "singboxVersion"),
                           "naiveVersion" = COALESCE($8, "naiveVersion"),
                           "mieruVersion" = COALESCE($9, "mieruVersion"),
                           arch = COALESCE($10, arch),
                           uptime = COALESCE($12, uptime),
                           "lastCheckAt" = $11, "updatedAt" = $11
                       WHERE id = $1"#,
                )
                .bind(&node_id)
                .bind(NodeStatus::Online)
                .bind(&sr.version)
                .bind(sr.cpu_usage)
                .bind(sr.mem_usage)
                .bind(info.xray)
                .bind(info.singbox)
                .bind(info.naive)
                .bind(info.mieru)
                .bind(info.arch)
                .bind(now)
                .bind(uptime_secs)
                .execute(&state.db)
                .await;

                if let (Some(c), Some(m)) = (sr.cpu_usage, sr.mem_usage) {
                    state.metrics.set_node_telemetry(&node_id, c, m);
                }
            }
            Envelope::TrafficReport(report) => {
                if report.validate().is_ok() {
                    let batch_id = report.batch_id.clone().unwrap_or_default();
                    for (email, stat) in report.stats {
                        traffic_batcher::add_traffic(
                            &state,
                            &node_id,
                            &email,
                            i64::try_from(stat.upload).unwrap_or(0),
                            i64::try_from(stat.download).unwrap_or(0),
                            stat.unmetered,
                            None,
                            (!batch_id.is_empty()).then(|| batch_id.clone()),
                            None,
                            None,
                        )
                        .await;
                    }
                    let _ = tx.send(Envelope::TrafficAck(TrafficAck { batch_id }));
                }
            }
            Envelope::Heartbeat(_) => {
                let _ = sqlx::query(
                    r#"UPDATE "Node" SET "lastCheckAt" = $2, status = 'ONLINE' WHERE id = $1"#,
                )
                .bind(&node_id)
                .bind(crate::db::now())
                .execute(&state.db)
                .await;
            }
            Envelope::LogChunk(chunk) => {
                state.hub.push_log_chunk(&node_id, chunk);
            }
            Envelope::SessionsList(list) => {
                state.hub.update_sessions(&node_id, list.sessions);
            }
            _ => {}
        }
    }

    writer.abort();
    if state.hub.unregister(&node_id, conn_id) {
        state.metrics.set_node_status(&node_id, &node_name, false);
    }
}
