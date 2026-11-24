use crate::db::enums::{DbProtocol, Protocol};
use crate::error::AppResult;
use crate::services::enforcement;
use crate::state::AppState;
use crate::traffic_queue::PendingTrafficItem;
use mypanel_proto::TrafficReport;
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

pub async fn resolve_client_id(
    state: &AppState,
    email_or_id: &str,
) -> Result<Option<String>, sqlx::Error> {
    if let Some(cached) = state.traffic_queue.get_cached_client_id(email_or_id) {
        return Ok(Some(cached));
    }
    let row = sqlx::query(
        r#"SELECT id FROM "Client" WHERE id = $1 OR email = $1 OR username = $1 LIMIT 1"#,
    )
    .bind(email_or_id)
    .fetch_optional(&state.db)
    .await?;

    let id: Option<String> = row.map(|r| r.try_get("id")).transpose()?;
    if let Some(id) = &id {
        state
            .traffic_queue
            .cache_client_id(email_or_id.to_string(), id.clone());
    }
    Ok(id)
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

/// Queueing result for one report entry (see [`add_traffic`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryOutcome {
    /// Queued for the next flush.
    Queued,
    /// Nothing to record (unmetered or zero bytes); only `lastActiveAt` was touched.
    Ignored,
    /// No matching client; the bytes are unattributable and were dropped (warn-logged).
    UnknownClient,
    /// Transient DB failure while resolving the client; nothing was queued.
    ResolveFailed,
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
) -> EntryOutcome {
    let client_id = match resolve_client_id(state, email_or_id).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            tracing::warn!(
                node_id,
                key = %email_or_id,
                upload,
                download,
                "traffic dropped: no client matches report key"
            );
            return EntryOutcome::UnknownClient;
        }
        Err(e) => {
            tracing::warn!(
                node_id,
                key = %email_or_id,
                error = %e,
                "traffic deferred: client lookup failed"
            );
            return EntryOutcome::ResolveFailed;
        }
    };
    let now = crate::db::now();

    if unmetered || (upload <= 0 && download <= 0) {
        let _ = sqlx::query(r#"UPDATE "Client" SET "lastActiveAt" = $2 WHERE id = $1"#)
            .bind(&client_id)
            .bind(now)
            .execute(&state.db)
            .await;
        return EntryOutcome::Ignored;
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
    EntryOutcome::Queued
}

/// Result of one [`flush_batch`] round.
#[derive(Debug, Default)]
pub struct FlushResult {
    pub processed: usize,
    /// Batch ids whose entries are durably written (`TrafficReceipt` + `TrafficLog` + counters).
    /// Only batches in this set may be acknowledged to the agent.
    pub committed_batches: HashSet<String>,
}

/// Flushes up to `500` queued traffic items in a single transaction, deduplicating by `TrafficReceipt`
/// and updating `Client` counters as well as `Reseller.usedTraffic`.
pub async fn flush_batch(state: &AppState) -> AppResult<FlushResult> {
    let items = state.traffic_queue.drain_up_to(500);
    if items.is_empty() {
        return Ok(FlushResult::default());
    }

    match flush_tx(state, &items).await {
        Ok((processed, committed_batches, affected_clients)) => {
            for cid in affected_clients {
                let _ = enforcement::enforce_client_policies(state, &cid).await;
            }
            Ok(FlushResult {
                processed,
                committed_batches,
            })
        }
        Err(e) => {
            // Transaction rolled back (tx dropped); put the items back at the front of the queue.
            state.traffic_queue.requeue_front(items);
            Err(e)
        }
    }
}

async fn flush_tx(
    state: &AppState,
    items: &[PendingTrafficItem],
) -> AppResult<(usize, HashSet<String>, HashSet<String>)> {
    let mut tx = state.db.begin().await?;

    let mut client_totals: HashMap<String, (i64, i64)> = HashMap::new();
    let mut node_totals: HashMap<String, (i64, i64)> = HashMap::new();
    let mut affected_clients: HashSet<String> = HashSet::new();
    let mut committed_batches: HashSet<String> = HashSet::new();
    let mut processed = 0usize;

    for item in items {
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
        .await?;

        if let Some(batch) = item.batch_id.as_deref().filter(|b| !b.is_empty()) {
            committed_batches.insert(batch.to_string());
        }
        if inserted.rows_affected() == 0 {
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

    tx.commit().await?;
    Ok((processed, committed_batches, affected_clients))
}

/// Queues every entry of one agent report, flushes, and decides whether the agent may drop
/// the batch: an ack is only returned once every entry is durably recorded (or nothing was
/// recordable at all). A failed flush keeps the batch pending on the agent for replay.
pub async fn record_report(state: &AppState, node_id: &str, report: TrafficReport) -> bool {
    if report.validate().is_err() {
        // A malformed batch can never become valid; ack so it stops replaying.
        tracing::warn!(node_id, batch_id = ?report.batch_id, "dropping invalid traffic batch");
        return true;
    }
    let batch_id = report.batch_id.clone();
    let mut resolve_failed = false;
    let mut queued = 0usize;
    for (email, stat) in report.stats {
        let outcome = add_traffic(
            state,
            node_id,
            &email,
            i64::try_from(stat.upload).unwrap_or(0),
            i64::try_from(stat.download).unwrap_or(0),
            stat.unmetered,
            None,
            batch_id.clone(),
            None,
            None,
        )
        .await;
        match outcome {
            EntryOutcome::Queued => queued += 1,
            EntryOutcome::ResolveFailed => resolve_failed = true,
            EntryOutcome::Ignored | EntryOutcome::UnknownClient => {}
        }
    }
    if resolve_failed {
        return false;
    }
    match batch_id.as_deref().map(str::trim).filter(|b| !b.is_empty()) {
        // No batch id: nothing pending on the agent, but flush promptly all the same.
        None => {
            if queued > 0 {
                let _ = flush_until_quiet(state).await;
            }
            true
        }
        Some(batch) if queued == 0 => {
            tracing::warn!(node_id, batch, "batch has no queueable entries; acking empty batch");
            true
        }
        Some(batch) => match flush_until_quiet(state).await {
            Ok(committed) => committed.contains(batch),
            Err(e) => {
                tracing::warn!(node_id, error = %e, "traffic flush failed; batch stays pending");
                false
            }
        },
    }
}

/// Flushes until the queue is quiet (bounded) and returns every durably committed batch id.
async fn flush_until_quiet(state: &AppState) -> AppResult<HashSet<String>> {
    let mut committed = HashSet::new();
    for _ in 0..200 {
        if state.traffic_queue.is_empty() {
            break;
        }
        let res = flush_batch(state).await?;
        if res.processed == 0 {
            break;
        }
        committed.extend(res.committed_batches);
    }
    Ok(committed)
}

/// Flushes until the queue is empty (bounded), used on shutdown so a restart loses nothing queued.
pub async fn drain_all(state: &AppState) -> usize {
    let mut total = 0;
    for _ in 0..200 {
        if state.traffic_queue.is_empty() {
            break;
        }
        match flush_batch(state).await {
            Ok(res) if res.processed == 0 => break,
            Ok(res) => total += res.processed,
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
