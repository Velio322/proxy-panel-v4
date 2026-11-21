use crate::error::AppResult;
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

pub async fn broadcast_config_update(state: &AppState) {
    let connected = state.hub.connected_node_ids();
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
        if state.hub.is_connected(nid) {
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
            let connected = state.hub.connected_node_ids();
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
