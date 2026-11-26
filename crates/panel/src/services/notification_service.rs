use crate::db::enums::{NodeStatus, NotificationType};
use crate::state::AppState;
use sqlx::PgPool;

/// Unread notifications of the same `type` + `title` within this window suppress duplicates,
/// so periodic cycles (healthcheck, outbox) cannot spam the feed or Telegram.
const DEDUP_WINDOW_MINUTES: i64 = 60;

/// Best-effort notification insert; failures are logged, never propagated.
pub async fn push(
    state: &AppState,
    ty: crate::db::enums::NotificationType,
    title: &str,
    message: &str,
    telegram: bool,
) {
    let now = crate::db::now();
    let recent: i64 = sqlx::query_scalar(
        r#"SELECT count(*) FROM "Notification"
           WHERE type = $1 AND title = $2 AND read = false AND "createdAt" > $3"#,
    )
    .bind(ty)
    .bind(title)
    .bind(now - chrono::Duration::minutes(DEDUP_WINDOW_MINUTES))
    .fetch_one(&state.db)
    .await
    .unwrap_or(1);
    if recent > 0 {
        return;
    }
    let res = sqlx::query(
        r#"INSERT INTO "Notification" (id, type, title, message, "targetUserId", "targetResellerId", read, "createdAt")
           VALUES ($1, $2, $3, $4, NULL, NULL, false, $5)"#,
    )
    .bind(crate::db::new_id())
    .bind(ty)
    .bind(title)
    .bind(message)
    .bind(now)
    .execute(&state.db)
    .await;
    if let Err(e) = res {
        tracing::warn!(error = %e, "failed to store notification");
        return;
    }
    if telegram {
        crate::services::telegram::notify_admins(state, title, message).await;
    }
}

/// Current status of a node, used by call sites to detect transitions.
pub async fn peek_node_status(db: &PgPool, node_id: &str) -> Option<NodeStatus> {
    sqlx::query_scalar::<_, NodeStatus>(r#"SELECT status FROM "Node" WHERE id = $1"#)
        .bind(node_id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
}

/// Fires a notification when a node's status actually changed. `prev = None` (row missing or
/// first observation) is treated as "no transition".
pub async fn node_status_changed(
    state: &AppState,
    node_id: &str,
    node_name: &str,
    prev: Option<NodeStatus>,
    new: NodeStatus,
    detail: &str,
) {
    if prev.is_none() || prev == Some(new) {
        return;
    }
    match new {
        NodeStatus::Offline => {
            let msg = format!("Node {node_name} is unreachable. {detail}");
            push(state, NotificationType::NodeOffline, "Node offline", &msg, true).await;
        }
        NodeStatus::Online if prev == Some(NodeStatus::Offline) => {
            let msg = format!("Node {node_name} is back online. {detail}");
            push(state, NotificationType::Info, "Node back online", &msg, false).await;
        }
        _ => {}
    }
}
