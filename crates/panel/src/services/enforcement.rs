use crate::db::enums::TrafficResetStrategy;
use crate::error::AppResult;
use crate::services::outbox;
use crate::state::AppState;
use chrono::{Datelike, Duration as ChronoDuration, NaiveDate, NaiveDateTime};
use serde_json::json;
use sqlx::Row;
use std::str::FromStr;

pub fn next_reset_date(day: i32, after: NaiveDateTime) -> NaiveDateTime {
    let clamped_day = day.clamp(1, 28) as u32;
    let year = after.year();
    let month = after.month();

    let candidate = NaiveDate::from_ymd_opt(year, month, clamped_day)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .unwrap_or(after);

    if candidate > after {
        candidate
    } else {
        let (ny, nm) = if month == 12 {
            (year + 1, 1)
        } else {
            (year, month + 1)
        };
        NaiveDate::from_ymd_opt(ny, nm, clamped_day)
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .unwrap_or(after + ChronoDuration::days(30))
    }
}

pub fn calculate_next_reset(
    strategy: TrafficResetStrategy,
    reset_day: Option<i32>,
    from: NaiveDateTime,
) -> Option<NaiveDateTime> {
    match strategy {
        TrafficResetStrategy::NoReset | TrafficResetStrategy::OnHold => None,
        TrafficResetStrategy::Day => {
            let today_midnight = from.date().and_hms_opt(0, 0, 0).unwrap_or(from);
            Some(today_midnight + ChronoDuration::days(1))
        }
        TrafficResetStrategy::Week => {
            let days_to_mon = 7 - i64::from(from.weekday().num_days_from_monday());
            let today_midnight = from.date().and_hms_opt(0, 0, 0).unwrap_or(from);
            Some(today_midnight + ChronoDuration::days(days_to_mon))
        }
        TrafficResetStrategy::Month => {
            let day = reset_day.unwrap_or(1);
            Some(next_reset_date(day, from))
        }
        TrafficResetStrategy::MonthRolling => Some(from + ChronoDuration::days(30)),
    }
}

pub fn restriction_reason(
    expire_at: Option<NaiveDateTime>,
    traffic_limit: i64,
    used_traffic: i64,
    now: NaiveDateTime,
) -> Option<&'static str> {
    if let Some(exp) = expire_at {
        if exp <= now {
            return Some("EXPIRED");
        }
    }
    if traffic_limit > 0 && used_traffic >= traffic_limit {
        return Some("TRAFFIC");
    }
    None
}

pub fn is_client_allowed(
    banned: bool,
    on_hold: bool,
    expire_at: Option<NaiveDateTime>,
    traffic_limit: i64,
    used_traffic: i64,
    db_restriction_reason: Option<&str>,
    now: NaiveDateTime,
) -> bool {
    if banned || on_hold || db_restriction_reason.is_some() {
        return false;
    }
    restriction_reason(expire_at, traffic_limit, used_traffic, now).is_none()
}

/// Activates an `ON_HOLD` client on first subscription/connection fetch.
pub async fn activate_on_hold_if_needed(state: &AppState, client_id: &str) -> AppResult<bool> {
    let now = crate::db::now();
    let mut tx = state.db.begin().await?;
    let row = sqlx::query(
        r#"SELECT "onHold", "onHoldDurationDays", "activatedAt", "expireAt"
           FROM "Client" WHERE id = $1 FOR UPDATE"#,
    )
    .bind(client_id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(row) = row else {
        tx.commit().await?;
        return Ok(false);
    };

    let on_hold: bool = row.try_get("onHold")?;
    let activated_at: Option<NaiveDateTime> = row.try_get("activatedAt")?;
    if !on_hold || activated_at.is_some() {
        tx.commit().await?;
        return Ok(false);
    }

    let days: i32 = row.try_get::<Option<i32>, _>("onHoldDurationDays")?.unwrap_or(30).max(1);
    let existing_exp: Option<NaiveDateTime> = row.try_get("expireAt")?;
    let new_exp = existing_exp.unwrap_or_else(|| now + ChronoDuration::days(i64::from(days)));

    sqlx::query(
        r#"UPDATE "Client"
           SET "onHold" = false, "activatedAt" = $2, "expireAt" = $3, "updatedAt" = $2
           WHERE id = $1"#,
    )
    .bind(client_id)
    .bind(now)
    .bind(new_exp)
    .execute(&mut *tx)
    .await?;

    let node_ids: Vec<String> = sqlx::query_scalar(
        r#"SELECT DISTINCT i."nodeId"
           FROM "ClientInbound" ci
           JOIN "Inbound" i ON i.id = ci."inboundId"
           WHERE ci."clientId" = $1"#,
    )
    .bind(client_id)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;
    state.invalidate_derived();
    if !node_ids.is_empty() {
        outbox::push_config_many(state, &node_ids).await;
    }
    Ok(true)
}

async fn enforce_client_policies_tx(
    state: &AppState,
    client_id: &str,
) -> AppResult<(bool, bool, Vec<String>)> {
    let now = crate::db::now();
    let mut tx = state.db.begin().await?;

    let row = sqlx::query(
        r#"SELECT id, "trafficLimit", "usedTraffic", "expireAt", banned, "restrictionMode",
                  "restrictionReason", "trafficResetDay", "nextTrafficResetAt", "resetStrategy",
                  "accessRevision", "onHold"
           FROM "Client"
           WHERE id = $1
           FOR UPDATE"#,
    )
    .bind(client_id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(row) = row else {
        tx.commit().await?;
        return Ok((false, false, Vec::new()));
    };

    let mut used_traffic: i64 = row.try_get("usedTraffic")?;
    let traffic_limit: i64 = row.try_get("trafficLimit")?;
    let expire_at: Option<NaiveDateTime> = row.try_get("expireAt")?;
    let restriction_mode: String = row.try_get("restrictionMode")?;
    let current_reason: Option<String> = row.try_get("restrictionReason")?;
    let reset_day: Option<i32> = row.try_get("trafficResetDay")?;
    let next_reset_at: Option<NaiveDateTime> = row.try_get("nextTrafficResetAt")?;
    let strategy_str: String = row
        .try_get::<Option<String>, _>("resetStrategy")?
        .unwrap_or_else(|| "NO_RESET".to_string());
    let mut strategy = TrafficResetStrategy::from_str(&strategy_str).unwrap_or_default();
    if strategy == TrafficResetStrategy::NoReset && reset_day.is_some() {
        strategy = TrafficResetStrategy::Month;
    }
    let access_rev: i32 = row.try_get("accessRevision")?;
    let on_hold: bool = row.try_get("onHold")?;

    let mut reset_performed = false;
    let mut new_next_reset = next_reset_at;

    if strategy != TrafficResetStrategy::NoReset && strategy != TrafficResetStrategy::OnHold && !on_hold {
        if let Some(nr) = next_reset_at {
            if nr <= now {
                used_traffic = 0;
                reset_performed = true;
                new_next_reset = calculate_next_reset(strategy, reset_day, now);
            }
        } else {
            new_next_reset = calculate_next_reset(strategy, reset_day, now);
        }
    }

    if reset_performed {
        sqlx::query(
            r#"UPDATE "Client"
               SET "usedTraffic" = 0, "uploadTraffic" = 0, "downloadTraffic" = 0,
                   "lastTrafficResetAt" = $2, "nextTrafficResetAt" = $3, "updatedAt" = $2
               WHERE id = $1"#,
        )
        .bind(client_id)
        .bind(now)
        .bind(new_next_reset)
        .execute(&mut *tx)
        .await?;
    } else if new_next_reset != next_reset_at {
        sqlx::query(
            r#"UPDATE "Client" SET "nextTrafficResetAt" = $2, "updatedAt" = $3 WHERE id = $1"#,
        )
        .bind(client_id)
        .bind(new_next_reset)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    let new_reason = restriction_reason(expire_at, traffic_limit, used_traffic, now);
    let changed = current_reason.as_deref() != new_reason;
    let mut affected_nodes: Vec<String> = Vec::new();

    if changed {
        let new_rev = access_rev.saturating_add(1);
        let restricted_at = new_reason.map(|_| now);
        sqlx::query(
            r#"UPDATE "Client"
               SET "restrictionReason" = $2, "restrictedAt" = $3, "accessRevision" = $4, "updatedAt" = $5
               WHERE id = $1"#,
        )
        .bind(client_id)
        .bind(new_reason)
        .bind(restricted_at)
        .bind(new_rev)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        affected_nodes = sqlx::query_scalar(
            r#"SELECT DISTINCT i."nodeId"
               FROM "ClientInbound" ci
               JOIN "Inbound" i ON i.id = ci."inboundId"
               WHERE ci."clientId" = $1"#,
        )
        .bind(client_id)
        .fetch_all(&mut *tx)
        .await?;

        let payload = json!({
            "clientId": client_id,
            "mode": restriction_mode,
            "reason": new_reason,
            "sessionTermination": "UNSUPPORTED: per-client termination not verified for deployed cores"
        });
        for nid in &affected_nodes {
            outbox::enqueue_config_sync_tx(&mut tx, nid, payload.clone(), new_rev).await?;
        }
    }

    tx.commit().await?;
    Ok((changed, reset_performed, affected_nodes))
}

/// Enforces traffic resets and expiry/quota restrictions for a single client.
pub async fn enforce_client_policies(state: &AppState, client_id: &str) -> AppResult<bool> {
    let (changed, reset_performed, node_ids) = enforce_client_policies_tx(state, client_id).await?;
    if changed || reset_performed {
        state.invalidate_derived();
    }
    if changed && !node_ids.is_empty() {
        outbox::push_config_many(state, &node_ids).await;
    }
    Ok(changed)
}

/// Enforces policies across a batch of client IDs with a single coalesced config push.
pub async fn enforce_clients_batch(state: &AppState, client_ids: &[String]) -> usize {
    let mut changed_count = 0usize;
    let mut any_dirty = false;
    let mut affected_nodes = Vec::new();
    for cid in client_ids {
        if let Ok((changed, reset_performed, nodes)) = enforce_client_policies_tx(state, cid).await {
            if changed {
                changed_count += 1;
                affected_nodes.extend(nodes);
            }
            if changed || reset_performed {
                any_dirty = true;
            }
        }
    }
    if any_dirty {
        state.invalidate_derived();
    }
    if !affected_nodes.is_empty() {
        outbox::push_config_many(state, &affected_nodes).await;
    }
    changed_count
}

/// Periodic enforcement pass across all clients that may need reset or restriction updates.
pub async fn enforce_all_clients(state: &AppState) -> AppResult<usize> {
    let now = crate::db::now();
    let ids: Vec<String> = sqlx::query_scalar(
        r#"SELECT id FROM "Client"
           WHERE ("nextTrafficResetAt" IS NOT NULL AND "nextTrafficResetAt" <= $1)
              OR ("resetStrategy" NOT IN ('NO_RESET', 'ON_HOLD') AND COALESCE("onHold", false) = false AND "nextTrafficResetAt" IS NULL)
              OR ("expireAt" IS NOT NULL AND "expireAt" <= $1 AND "restrictionReason" IS DISTINCT FROM 'EXPIRED')
              OR ("trafficLimit" > 0 AND "usedTraffic" >= "trafficLimit" AND "restrictionReason" IS NULL)
              OR ("restrictionReason" IS NOT NULL AND ("expireAt" IS NULL OR "expireAt" > $1) AND ("trafficLimit" = 0 OR "usedTraffic" < "trafficLimit"))"#,
    )
    .bind(now)
    .fetch_all(&state.db)
    .await?;

    Ok(enforce_clients_batch(state, &ids).await)
}
