use crate::error::AppResult;
use crate::services::node_client::{self, NodeTarget};
use crate::services::node_config;
use crate::state::AppState;
use mypanel_proto::{ConfigUpdate, Envelope};
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use std::collections::HashSet;
use std::time::Duration;

pub async fn enqueue_config_sync_tx(
    tx: &mut Transaction<'_, Postgres>,
    node_id: &str,
    payload: Value,
    revision: i32,
) -> AppResult<()> {
    sqlx::query(
        r#"INSERT INTO "NodeCommand" (id, "nodeId", kind, payload, revision, status, attempts, "createdAt")
           VALUES ($1, $2, 'CONFIG_SYNC', $3, $4, 'PENDING', 0, $5)"#,
    )
    .bind(crate::db::new_id())
    .bind(node_id)
    .bind(payload)
    .bind(revision)
    .bind(crate::db::now())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Coalescing config push for a single node: reuses oldest PENDING `CONFIG_SYNC` or creates one,
/// then sends `Envelope::ConfigUpdate` over `WorkerHub` if connected.
pub async fn push_config(state: &AppState, node_id: &str) -> AppResult<()> {
    let now = crate::db::now();
    let existing = sqlx::query(
        r#"SELECT id, revision FROM "NodeCommand"
           WHERE "nodeId" = $1 AND kind = 'CONFIG_SYNC' AND status = 'PENDING'
           ORDER BY "createdAt" ASC LIMIT 1"#,
    )
    .bind(node_id)
    .fetch_optional(&state.db)
    .await?;

    let (cmd_id, revision) = if let Some(row) = existing {
        let id: String = row.try_get("id")?;
        let rev: i32 = row.try_get("revision")?;
        sqlx::query(r#"UPDATE "NodeCommand" SET attempts = attempts + 1 WHERE id = $1"#)
            .bind(&id)
            .execute(&state.db)
            .await?;
        (id, rev)
    } else {
        let id = crate::db::new_id();
        sqlx::query(
            r#"INSERT INTO "NodeCommand" (id, "nodeId", kind, payload, revision, status, attempts, "createdAt")
               VALUES ($1, $2, 'CONFIG_SYNC', $3, 0, 'PENDING', 1, $4)"#,
        )
        .bind(&id)
        .bind(node_id)
        .bind(json!({}))
        .bind(now)
        .execute(&state.db)
        .await?;
        (id, 0)
    };

    if node_is_pull(state, node_id).await {
        deliver_pull(state, node_id, &cmd_id).await;
        return Ok(());
    }

    if state.hub.is_connected(node_id) {
        let inbounds = node_config::build(state, node_id).await?;
        let routing_rules = node_config::build_routing_rules(state, node_id).await?;
        let xray_profile = node_config::build_xray_profile(state, node_id).await?;
        let msg = Envelope::ConfigUpdate(ConfigUpdate {
            command_id: cmd_id,
            revision,
            inbounds,
            routing_rules,
            xray_profile,
            timestamp: chrono::Utc::now().timestamp_millis(),
        });
        state.hub.send(node_id, msg);
    }

    Ok(())
}

/// True when the node is polled over mTLS HTTPS instead of holding a WebSocket.
pub async fn node_is_pull(state: &AppState, node_id: &str) -> bool {
    sqlx::query_scalar::<_, String>(r#"SELECT "apiMode" FROM "Node" WHERE id = $1 AND active = true"#)
        .bind(node_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .is_some_and(|m| m == "pull")
}

/// Ids of active nodes that are reached over mTLS HTTPS.
pub async fn pull_node_ids(state: &AppState) -> Vec<String> {
    sqlx::query_scalar::<_, String>(r#"SELECT id FROM "Node" WHERE active = true AND "apiMode" = 'pull'"#)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
}

/// Pushes the node's config over mTLS HTTPS and settles the `NodeCommand`.
///
/// Failures leave the command `PENDING`; the outbox loop retries it, so a node that is briefly
/// unreachable (or a panel that restarts mid-delivery) still converges.
async fn deliver_pull(state: &AppState, node_id: &str, cmd_id: &str) {
    let target = match NodeTarget::fetch(state, node_id).await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(node_id, error = %e, "pull delivery: node lookup failed");
            return;
        }
    };
    let result = tokio::time::timeout(
        Duration::from_secs(60),
        node_client::push_config_detailed(state, &target),
    )
    .await;
    let (ok, note) = match result {
        Ok(Ok(resp)) if resp.success => (
            true,
            (!resp.warnings.is_empty()).then(|| resp.warnings.join("; ")),
        ),
        Ok(Ok(resp)) => (false, resp.error),
        Ok(Err(e)) => (false, Some(e.to_string())),
        Err(_) => (false, Some("timed out delivering config".to_string())),
    };
    let note: Option<String> = note.map(|n| n.chars().take(1000).collect());
    if ok {
        let _ = sqlx::query(
            r#"UPDATE "NodeCommand"
               SET status = 'ACKNOWLEDGED', "acknowledgedAt" = $2, "lastError" = $3
               WHERE id = $1 AND status = 'PENDING'"#,
        )
        .bind(cmd_id)
        .bind(crate::db::now())
        .bind(note)
        .execute(&state.db)
        .await;
    } else {
        tracing::warn!(node_id, error = ?note, "pull config delivery failed; will retry");
        let _ = sqlx::query(r#"UPDATE "NodeCommand" SET "lastError" = $2 WHERE id = $1 AND status = 'PENDING'"#)
            .bind(cmd_id)
            .bind(note)
            .execute(&state.db)
            .await;
    }
}

pub async fn broadcast_config_update(state: &AppState) {
    let mut connected = state.hub.connected_node_ids();
    connected.extend(pull_node_ids(state).await);
    connected.sort();
    connected.dedup();
    for nid in connected {
        if let Err(e) = push_config(state, &nid).await {
            tracing::warn!(node_id = %nid, error = %e, "broadcast_config_update failed");
        }
    }
}

pub async fn push_config_many(state: &AppState, node_ids: &[String]) {
    let mut seen = HashSet::new();
    for nid in node_ids {
        if !seen.insert(nid.as_str()) {
            continue;
        }
        if state.hub.is_connected(nid) || node_is_pull(state, nid).await {
            if let Err(e) = push_config(state, nid).await {
                tracing::warn!(node_id = %nid, error = %e, "push_config_many failed");
            }
        }
    }
}

pub fn spawn_outbox_loop(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(15));
        loop {
            interval.tick().await;
            let mut connected = state.hub.connected_node_ids();
            connected.extend(pull_node_ids(&state).await);
            connected.sort();
            connected.dedup();
            if connected.is_empty() {
                continue;
            }
            let Ok(rows) = sqlx::query(
                r#"SELECT DISTINCT "nodeId" FROM "NodeCommand"
                   WHERE status = 'PENDING' AND kind = 'CONFIG_SYNC' AND "nodeId" = ANY($1::text[])"#,
            )
            .bind(&connected)
            .fetch_all(&state.db)
            .await
            else {
                continue;
            };
            for r in rows {
                if let Ok(nid) = r.try_get::<String, _>("nodeId") {
                    let _ = push_config(&state, &nid).await;
                }
            }
        }
    });
}
