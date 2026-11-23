use crate::db::enums::{DbProtocol, Protocol};
use crate::error::AppResult;
use crate::services::enforcement;
use crate::state::AppState;
use crate::traffic_queue::PendingTrafficItem;
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

pub async fn resolve_client_id(state: &AppState, email_or_id: &str) -> Option<String> {
    if let Some(cached) = state.traffic_queue.get_cached_client_id(email_or_id) {
        return Some(cached);
    }
    let row = sqlx::query(
        r#"SELECT id FROM "Client" WHERE id = $1 OR email = $1 OR username = $1 LIMIT 1"#,
    )
    .bind(email_or_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()?;

    let id: String = row.try_get("id").ok()?;
    state
        .traffic_queue
        .cache_client_id(email_or_id.to_string(), id.clone());
    Some(id)
}

/// Idempotency key for one node report entry.
///
/// A report is replayed by the agent until acknowledged (e.g. after a panel restart), and a random
/// receipt id per replay would count the same bytes twice. Deriving the id from
/// `(node, batch, key)` makes `ON CONFLICT DO NOTHING` in [`flush_batch`] drop the duplicate.
pub fn receipt_id_for(node_id: &str, batch_id: Option<&str>, key: &str) -> Option<String> {
    let batch = batch_id.map(str::trim).filter(|b| !b.is_empty())?;
    Some(crate::auth::crypto::sha256_hex(&format!("{node_id}\u{1f}{batch}\u{1f}{key}")))
}

pub async fn add_traffic(
    state: &AppState,
    node_id: &str,
    email_or_id: &str,
    upload: i64,
    download: i64,
    unmetered: bool,
    receipt_id: Option<String>,
    batch_id: Option<String>,
    protocol: Option<Protocol>,
    inbound_tag: Option<String>,
) {
    let Some(client_id) = resolve_client_id(state, email_or_id).await else {
        return;
    };
    let now = crate::db::now();

    if unmetered || (upload <= 0 && download <= 0) {
        let _ = sqlx::query(r#"UPDATE "Client" SET "lastActiveAt" = $2 WHERE id = $1"#)
            .bind(&client_id)
            .bind(now)
            .execute(&state.db)
            .await;
        return;
    }

    state.traffic_queue.push(PendingTrafficItem {
        receipt_id: receipt_id
            .or_else(|| receipt_id_for(node_id, batch_id.as_deref(), email_or_id))
            .unwrap_or_else(crate::db::new_id),
        batch_id,
        node_id: node_id.to_string(),
        client_id,
        upload: upload.max(0),
        download: download.max(0),
        protocol: protocol.map(|p| p.as_str().to_string()),
        inbound_tag,
        record_at: now,
    });
}

/// Flushes up to `500` queued traffic items in a single transaction, deduplicating by `TrafficReceipt`
/// and updating `Client` counters as well as `Reseller.usedTraffic`.
pub async fn flush_batch(state: &AppState) -> AppResult<usize> {
    let items = state.traffic_queue.drain_up_to(500);
    if items.is_empty() {
        return Ok(0);
    }

    let mut tx = match state.db.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            state.traffic_queue.requeue_front(items);
            return Err(e.into());
        }
    };

    let mut client_totals: HashMap<String, (i64, i64)> = HashMap::new();
    let mut node_totals: HashMap<String, (i64, i64)> = HashMap::new();
    let mut affected_clients: HashSet<String> = HashSet::new();
    let mut processed = 0usize;

    for item in &items {
        // Idempotent receipt guard
        let inserted = sqlx::query(
            r#"INSERT INTO "TrafficReceipt" (id, "batchId", "nodeId", "createdAt")
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (id) DO NOTHING"#,
        )
        .bind(&item.receipt_id)
        .bind(&item.batch_id)
        .bind(&item.node_id)
        .bind(item.record_at)
        .execute(&mut *tx)
        .await;

        let Ok(res) = inserted else {
            continue;
        };
        if res.rows_affected() == 0 {
            continue;
        }

        let proto_enum: Option<DbProtocol> = item
            .protocol
            .as_deref()
            .and_then(|s| s.parse::<Protocol>().ok())
            .map(DbProtocol::from);

        sqlx::query(
            r#"INSERT INTO "TrafficLog" (id, "clientId", "nodeId", upload, download, protocol, "inboundTag", "recordAt")
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
        )
        .bind(crate::db::new_id())
        .bind(&item.client_id)
        .bind(&item.node_id)
        .bind(item.upload)
        .bind(item.download)
        .bind(proto_enum)
        .bind(&item.inbound_tag)
        .bind(item.record_at)
        .execute(&mut *tx)
        .await?;

        let ce = client_totals.entry(item.client_id.clone()).or_insert((0, 0));
        ce.0 = ce.0.saturating_add(item.upload);
        ce.1 = ce.1.saturating_add(item.download);

        let ne = node_totals.entry(item.node_id.clone()).or_insert((0, 0));
        ne.0 = ne.0.saturating_add(item.upload);
        ne.1 = ne.1.saturating_add(item.download);

        affected_clients.insert(item.client_id.clone());
        processed += 1;
    }

    let now = crate::db::now();
    let mut reseller_totals: HashMap<String, i64> = HashMap::new();

    for (cid, (up, down)) in &client_totals {
        let total = up.saturating_add(*down);
        let row = sqlx::query(
            r#"UPDATE "Client"
               SET "uploadTraffic" = "uploadTraffic" + $2,
                   "downloadTraffic" = "downloadTraffic" + $3,
                   "usedTraffic" = "usedTraffic" + $4,
                   "lastActiveAt" = $5,
                   "updatedAt" = $5
               WHERE id = $1
               RETURNING "resellerId""#,
        )
        .bind(cid)
        .bind(up)
        .bind(down)
        .bind(total)
        .bind(now)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(r) = row {
            if let Ok(Some(rid)) = r.try_get::<Option<String>, _>("resellerId") {
                let re = reseller_totals.entry(rid).or_insert(0);
                *re = re.saturating_add(total);
            }
        }
    }

    // Update Reseller.usedTraffic
    for (rid, total) in &reseller_totals {
        sqlx::query(
            r#"UPDATE "Reseller"
               SET "usedTraffic" = "usedTraffic" + $2, "updatedAt" = $3
               WHERE id = $1"#,
        )
        .bind(rid)
        .bind(total)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    for (nid, (up, down)) in &node_totals {
        sqlx::query(
            r#"UPDATE "Node"
               SET "netUpload" = "netUpload" + $2,
                   "netDownload" = "netDownload" + $3,
                   "updatedAt" = $4
               WHERE id = $1"#,
        )
        .bind(nid)
        .bind(up)
        .bind(down)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    if let Err(e) = tx.commit().await {
        state.traffic_queue.requeue_front(items);
        return Err(e.into());
    }

    for cid in affected_clients {
        let _ = enforcement::enforce_client_policies(state, &cid).await;
    }

    Ok(processed)
}

/// Flushes until the queue is empty (bounded), used on shutdown so a restart loses nothing queued.
pub async fn drain_all(state: &AppState) -> usize {
    let mut total = 0;
    for _ in 0..200 {
        if state.traffic_queue.is_empty() {
            break;
        }
        match flush_batch(state).await {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(e) => {
                tracing::warn!(error = %e, queued = state.traffic_queue.len(), "final traffic flush failed");
                break;
            }
        }
    }
    total
}

pub fn spawn_flush_loop(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            if let Err(e) = flush_batch(&state).await {
                tracing::warn!(error = %e, "traffic_batcher flush failed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::receipt_id_for;

    #[test]
    fn receipt_id_is_stable_per_batch_entry_and_distinct_otherwise() {
        let a = receipt_id_for("n1", Some("b1"), "alice").unwrap();
        assert_eq!(a, receipt_id_for("n1", Some("b1"), "alice").unwrap());
        assert_ne!(a, receipt_id_for("n1", Some("b1"), "bob").unwrap());
        assert_ne!(a, receipt_id_for("n1", Some("b2"), "alice").unwrap());
        assert_ne!(a, receipt_id_for("n2", Some("b1"), "alice").unwrap());
    }

    #[test]
    fn no_batch_means_no_deterministic_id() {
        assert!(receipt_id_for("n1", None, "alice").is_none());
        assert!(receipt_id_for("n1", Some("  "), "alice").is_none());
    }
}
