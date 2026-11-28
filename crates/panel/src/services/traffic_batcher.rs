use crate::db::enums::Protocol;
use crate::error::AppResult;
use crate::services::enforcement;
use crate::state::AppState;
use crate::traffic_queue::PendingTrafficItem;
use mypanel_proto::TrafficReport;
use sqlx::Row;
use chrono::NaiveDateTime;
use std::collections::{BTreeMap, HashSet};

pub async fn resolve_client_id(
    state: &AppState,
    email_or_id: &str,
) -> Result<Option<String>, sqlx::Error> {
    if let Some(cached) = state.traffic_queue.get_cached_client_id(email_or_id) {
        return Ok(Some(cached));
    }
    let row = sqlx::query(
        r#"SELECT id FROM "Client" WHERE id = $1 OR email = $1 OR username = $1
           ORDER BY (id = $1) DESC, (email = $1) DESC LIMIT 1"#,
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
    /// The in-memory queue is full; nothing was queued and the batch must not be acked.
    QueueFull,
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

    let queued = state.traffic_queue.push(PendingTrafficItem {
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
    if queued {
        EntryOutcome::Queued
    } else {
        tracing::warn!(node_id, "traffic queue full; report entry deferred");
        EntryOutcome::QueueFull
    }
}

/// Result of one [`flush_batch`] round.
#[derive(Debug, Default)]
pub struct FlushResult {
    /// Items taken from the queue in this round (including replays already recorded).
    pub drained: usize,
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
        Ok((processed, committed_batches, to_enforce)) => {
            // Only clients that crossed a limit/expiry/reset threshold need the (locking,
            // config-broadcasting) enforcement pass; the periodic job covers the rest.
            if !to_enforce.is_empty() {
                enforcement::enforce_clients_batch(state, &to_enforce).await;
            }
            Ok(FlushResult {
                drained: items.len(),
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

/// True when a client row (after adding traffic) needs [`enforcement::enforce_client_policies`].
pub(crate) fn needs_enforcement(
    traffic_limit: i64,
    used: i64,
    restriction_reason: Option<&str>,
    next_reset_at: Option<NaiveDateTime>,
    expire_at: Option<NaiveDateTime>,
    now: NaiveDateTime,
) -> bool {
    (traffic_limit > 0 && used >= traffic_limit && restriction_reason.is_none())
        || next_reset_at.is_some_and(|t| t <= now)
        || (expire_at.is_some_and(|t| t <= now) && restriction_reason != Some("EXPIRED"))
}

async fn flush_tx(
    state: &AppState,
    items: &[PendingTrafficItem],
) -> AppResult<(usize, HashSet<String>, Vec<String>)> {
    let mut tx = state.db.begin().await?;

    // Items for clients or nodes that no longer exist would violate foreign keys and, being
    // requeued on failure, block the whole queue forever. Receipts are still recorded for them so
    // the agent's batch can be acknowledged; no log/counters are written.
    let client_ids: Vec<String> = items
        .iter()
        .map(|i| i.client_id.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let existing: HashSet<String> =
        sqlx::query_scalar(r#"SELECT id FROM "Client" WHERE id = ANY($1::text[])"#)
            .bind(&client_ids)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .collect();
    let unique_node_ids: Vec<String> = items
        .iter()
        .map(|i| i.node_id.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let existing_nodes: HashSet<String> =
        sqlx::query_scalar(r#"SELECT id FROM "Node" WHERE id = ANY($1::text[])"#)
            .bind(&unique_node_ids)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .collect();

    // Bulk idempotent receipt guard: only newly inserted ids are counted below.
    let receipt_ids: Vec<&str> = items.iter().map(|i| i.receipt_id.as_str()).collect();
    let batch_ids: Vec<Option<&str>> = items.iter().map(|i| i.batch_id.as_deref()).collect();
    let node_ids: Vec<&str> = items.iter().map(|i| i.node_id.as_str()).collect();
    let created: Vec<NaiveDateTime> = items.iter().map(|i| i.record_at).collect();
    let mut inserted_ids: HashSet<String> = sqlx::query_scalar(
        r#"INSERT INTO "TrafficReceipt" (id, "batchId", "nodeId", "createdAt")
           SELECT * FROM UNNEST($1::text[], $2::text[], $3::text[], $4::timestamp[])
           ON CONFLICT (id) DO NOTHING
           RETURNING id"#,
    )
    .bind(&receipt_ids)
    .bind(&batch_ids)
    .bind(&node_ids)
    .bind(&created)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();

    let mut committed_batches: HashSet<String> = HashSet::new();
    let mut fresh: Vec<&PendingTrafficItem> = Vec::new();
    let mut orphaned: HashSet<&str> = HashSet::new();
    let mut orphaned_nodes: HashSet<&str> = HashSet::new();
    for item in items {
        if let Some(batch) = item.batch_id.as_deref().filter(|b| !b.is_empty()) {
            committed_batches.insert(batch.to_string());
        }
        // `remove`, not `contains`: the same receipt can sit in the queue twice (a replayed
        // batch arriving while its first copy is still queued). Only the first copy counts.
        if !inserted_ids.remove(&item.receipt_id) {
            continue;
        }
        if !existing.contains(&item.client_id) {
            orphaned.insert(item.client_id.as_str());
            continue;
        }
        if !existing_nodes.contains(&item.node_id) {
            orphaned_nodes.insert(item.node_id.as_str());
            continue;
        }
        fresh.push(item);
    }
    for cid in &orphaned {
        state.traffic_queue.invalidate_client(cid);
        tracing::warn!(client_id = %cid, "traffic dropped: client no longer exists");
    }
    for nid in &orphaned_nodes {
        tracing::warn!(node_id = %nid, "traffic dropped: node no longer exists");
    }

    // BTreeMap: every flush locks Client/Reseller/Node rows in the same (sorted) order, which
    // rules out deadlocks between concurrent flushes.
    let mut client_totals: BTreeMap<&str, (i64, i64)> = BTreeMap::new();
    let mut node_totals: BTreeMap<&str, (i64, i64)> = BTreeMap::new();
    for item in &fresh {
        let ce = client_totals.entry(item.client_id.as_str()).or_insert((0, 0));
        ce.0 = ce.0.saturating_add(item.upload);
        ce.1 = ce.1.saturating_add(item.download);
        let ne = node_totals.entry(item.node_id.as_str()).or_insert((0, 0));
        ne.0 = ne.0.saturating_add(item.upload);
        ne.1 = ne.1.saturating_add(item.download);
    }

    if !fresh.is_empty() {
        let log_ids: Vec<String> = fresh.iter().map(|_| crate::db::new_id()).collect();
        let log_clients: Vec<&str> = fresh.iter().map(|i| i.client_id.as_str()).collect();
        let log_nodes: Vec<&str> = fresh.iter().map(|i| i.node_id.as_str()).collect();
        let log_up: Vec<i64> = fresh.iter().map(|i| i.upload).collect();
        let log_down: Vec<i64> = fresh.iter().map(|i| i.download).collect();
        // Re-parse so only valid enum labels ever reach the `::"Protocol"` cast below.
        let log_proto: Vec<Option<&'static str>> = fresh
            .iter()
            .map(|i| {
                i.protocol
                    .as_deref()
                    .and_then(|s| s.parse::<Protocol>().ok())
                    .map(|p| p.as_str())
            })
            .collect();
        let log_tags: Vec<Option<&str>> = fresh.iter().map(|i| i.inbound_tag.as_deref()).collect();
        let log_at: Vec<NaiveDateTime> = fresh.iter().map(|i| i.record_at).collect();
        sqlx::query(
            r#"INSERT INTO "TrafficLog" (id, "clientId", "nodeId", upload, download, protocol, "inboundTag", "recordAt")
               SELECT id, c, n, up, down, p::"Protocol", tag, at
               FROM UNNEST($1::text[], $2::text[], $3::text[], $4::bigint[], $5::bigint[],
                           $6::text[], $7::text[], $8::timestamp[])
                    AS t(id, c, n, up, down, p, tag, at)"#,
        )
        .bind(&log_ids)
        .bind(&log_clients)
        .bind(&log_nodes)
        .bind(&log_up)
        .bind(&log_down)
        .bind(&log_proto)
        .bind(&log_tags)
        .bind(&log_at)
        .execute(&mut *tx)
        .await?;
    }

    let now = crate::db::now();
    let mut reseller_totals: BTreeMap<String, i64> = BTreeMap::new();
    let mut to_enforce: Vec<String> = Vec::new();

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
               RETURNING "resellerId", "usedTraffic", "trafficLimit", "restrictionReason",
                         "nextTrafficResetAt", "expireAt""#,
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
            let used: i64 = r.try_get("usedTraffic")?;
            let limit: i64 = r.try_get("trafficLimit")?;
            let reason: Option<String> = r.try_get("restrictionReason")?;
            let next_reset: Option<NaiveDateTime> = r.try_get("nextTrafficResetAt")?;
            let expire: Option<NaiveDateTime> = r.try_get("expireAt")?;
            if needs_enforcement(limit, used, reason.as_deref(), next_reset, expire, now) {
                to_enforce.push((*cid).to_string());
            }
        }
    }

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
    Ok((fresh.len(), committed_batches, to_enforce))
}

/// Queues every entry of one agent report, flushes, and decides whether the agent may drop
/// the batch. The ack is based on this report's own receipts in the database, not on what this
/// particular flush committed: another node's concurrent flush may already have written our
/// items, and a replayed batch has all its receipts from the first delivery.
pub async fn record_report(state: &AppState, node_id: &str, report: TrafficReport) -> bool {
    if report.validate().is_err() {
        // A malformed batch can never become valid; ack so it stops replaying.
        tracing::warn!(node_id, batch_id = ?report.batch_id, "dropping invalid traffic batch");
        return true;
    }
    let batch_id = report.batch_id.clone();
    let batch_key = batch_id.as_deref().map(str::trim).filter(|b| !b.is_empty());
    let mut deferred = false;
    let mut expected_receipts: Vec<String> = Vec::new();
    let key_meta = report.key_meta.unwrap_or_default();
    for (email, stat) in report.stats {
        let receipt = receipt_id_for(node_id, batch_key, &email);
        let meta = key_meta.get(&email);
        let protocol = meta
            .and_then(|m| m.protocol.as_deref())
            .and_then(|s| s.parse::<Protocol>().ok());
        let inbound_tag = meta
            .and_then(|m| m.inbound_tag.clone())
            .filter(|s| !s.is_empty());
        let outcome = add_traffic(
            state,
            node_id,
            &email,
            i64::try_from(stat.upload).unwrap_or(0),
            i64::try_from(stat.download).unwrap_or(0),
            stat.unmetered,
            receipt.clone(),
            batch_id.clone(),
            protocol,
            inbound_tag,
        )
        .await;
        match outcome {
            EntryOutcome::Queued => expected_receipts.extend(receipt),
            EntryOutcome::ResolveFailed | EntryOutcome::QueueFull => deferred = true,
            EntryOutcome::Ignored | EntryOutcome::UnknownClient => {}
        }
    }
    if deferred {
        return false;
    }
    let flushed = flush_until_quiet(state).await;
    if let Err(e) = &flushed {
        tracing::warn!(node_id, error = %e, "traffic flush failed; batch stays pending");
    }
    let Some(batch) = batch_key else {
        // No batch id: nothing pending on the agent, the flush above was best effort.
        return true;
    };
    if expected_receipts.is_empty() {
        tracing::warn!(node_id, batch, "batch has no queueable entries; acking empty batch");
        return true;
    }
    match count_receipts(state, &expected_receipts).await {
        Ok(found) => found >= expected_receipts.len(),
        Err(e) => {
            tracing::warn!(node_id, error = %e, "receipt check failed; batch stays pending");
            false
        }
    }
}

async fn count_receipts(state: &AppState, ids: &[String]) -> Result<usize, sqlx::Error> {
    let n: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TrafficReceipt" WHERE id = ANY($1::text[])"#)
        .bind(ids)
        .fetch_one(&state.db)
        .await?;
    Ok(usize::try_from(n).unwrap_or(0))
}

/// Flushes until the queue is empty (bounded). A round that only re-read already recorded
/// items (replays) still counts as progress; only an empty drain or an error stops the loop.
async fn flush_until_quiet(state: &AppState) -> AppResult<()> {
    for _ in 0..200 {
        if state.traffic_queue.is_empty() {
            break;
        }
        if flush_batch(state).await?.drained == 0 {
            break;
        }
    }
    Ok(())
}

/// Flushes until the queue is empty (bounded), used on shutdown so a restart loses nothing queued.
pub async fn drain_all(state: &AppState) -> usize {
    let mut total = 0;
    for _ in 0..200 {
        if state.traffic_queue.is_empty() {
            break;
        }
        match flush_batch(state).await {
            Ok(res) if res.drained == 0 => break,
            Ok(res) => total += res.processed,
            Err(e) => {
                tracing::warn!(error = %e, queued = state.traffic_queue.len(), "final traffic flush failed");
                break;
            }
        }
    }
    total
}

/// One periodic flush pass (run by the scheduler under its watchdog).
pub async fn flush_cycle(state: AppState) {
    if let Err(e) = flush_batch(&state).await {
        tracing::warn!(error = %e, "traffic_batcher flush failed");
    }
}

#[cfg(test)]
mod tests {
    use super::{needs_enforcement, receipt_id_for};
    use chrono::NaiveDate;

    fn t(h: u32) -> chrono::NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 24).unwrap().and_hms_opt(h, 0, 0).unwrap()
    }

    #[test]
    fn enforcement_only_when_a_threshold_is_crossed() {
        let now = t(12);
        assert!(!needs_enforcement(0, 10, None, None, None, now));
        assert!(!needs_enforcement(100, 99, None, None, None, now));
        assert!(needs_enforcement(100, 100, None, None, None, now));
        assert!(!needs_enforcement(100, 150, Some("TRAFFIC_LIMIT"), None, None, now));
        assert!(needs_enforcement(0, 1, None, Some(t(11)), None, now));
        assert!(!needs_enforcement(0, 1, None, Some(t(13)), None, now));
        assert!(needs_enforcement(0, 1, None, None, Some(t(11)), now));
        assert!(!needs_enforcement(0, 1, Some("EXPIRED"), None, Some(t(11)), now));
    }

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
