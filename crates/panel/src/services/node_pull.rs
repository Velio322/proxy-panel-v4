//! Polling of `apiMode = 'pull'` nodes over mutual-TLS HTTPS (the Remnawave model).
//!
//! There is no persistent session: every cycle makes short requests, so restarting the panel
//! changes nothing for the nodes. Traffic is idempotent end to end: the agent keeps a batch until
//! the panel acknowledges it, and receipt ids are derived from `(node, batch, user)`.

use crate::services::node_client::{self, NodeTarget};
use crate::services::{outbox, traffic_batcher};
use crate::state::AppState;
use futures_util::stream::{self, StreamExt};
use mypanel_proto::{
    SessionsList, StatusReport, TrafficAckRequest, TrafficCollectResponse, TrafficReport,
};
use serde_json::{json, Value};
use std::time::Duration;

const POLL_TIMEOUT: Duration = Duration::from_secs(15);

/// One polling cycle over every active pull node (concurrency 4).
pub async fn poll_cycle(state: AppState) {
    let ids = outbox::pull_node_ids(&state).await;
    stream::iter(ids)
        .for_each_concurrent(4, |id| {
            let st = state.clone();
            async move {
                match tokio::time::timeout(POLL_TIMEOUT * 2, poll_node(&st, &id)).await {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => tracing::warn!(node_id = %id, error = %e, "pull poll failed"),
                    Err(_) => tracing::warn!(node_id = %id, "pull poll timed out"),
                }
            }
        })
        .await;
}

async fn poll_node(state: &AppState, node_id: &str) -> Result<(), String> {
    let node = NodeTarget::fetch(state, node_id).await.map_err(|e| e.to_string())?;

    // Status first: it doubles as the liveness check (check_status updates Node + metrics).
    node_client::check_status_scheduled(state, &node)
        .await
        .map_err(|e| e.to_string())?;

    // Traffic: collect -> record -> ack, so nothing is lost if any step fails.
    let collected = tokio::time::timeout(
        POLL_TIMEOUT,
        node_client::node_post(state, &node, "/api/traffic/collect", &json!({})),
    )
    .await
    .map_err(|_| "traffic collect timed out".to_string())?
    .map_err(|e| e.to_string())?;
    let resp: TrafficCollectResponse =
        serde_json::from_value(collected).map_err(|e| format!("bad traffic response: {e}"))?;
    let acked = record_reports(state, &node.id, resp.reports).await;
    if !acked.is_empty() {
        let _ = node_client::node_post(
            state,
            &node,
            "/api/traffic/ack",
            &TrafficAckRequest { batch_ids: acked },
        )
        .await;
    }

    // Sessions for the sessions explorer (best effort).
    if let Ok(v) = node_client::node_get(state, &node, "/api/sessions").await {
        if let Ok(list) = serde_json::from_value::<SessionsList>(v) {
            state.hub.update_sessions(&node.id, list.sessions);
        }
    }
    Ok(())
}

/// Queues every entry of the reports and returns the batch ids that are safe to acknowledge.
///
/// A batch id is only returned once its entries are durably written; anything else stays
/// pending on the agent and is replayed (receipt ids keep the replay idempotent).
pub async fn record_reports(
    state: &AppState,
    node_id: &str,
    reports: Vec<TrafficReport>,
) -> Vec<String> {
    let mut acked = Vec::new();
    for report in reports {
        let batch_id = report.batch_id.clone();
        // record_report validates; an invalid batch can never become valid, so ack it.
        if traffic_batcher::record_report(state, node_id, report).await {
            if let Some(id) = batch_id.filter(|id| !id.is_empty()) {
                acked.push(id);
            }
        }
    }
    acked
}

/// Latest status report converted for [`StatusReport`] consumers; kept for parity with the WS path.
pub fn parse_status(v: &Value) -> Option<StatusReport> {
    serde_json::from_value(v.clone()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_status_accepts_agent_payload() {
        let v = json!({"status":"ONLINE","xrayRunning":true,"singboxRunning":false,
            "naiveRunning":false,"mieruRunning":false,"uptime":5,"version":"3.0.0"});
        assert!(parse_status(&v).is_some());
        assert!(parse_status(&json!("nope")).is_none());
    }
}
