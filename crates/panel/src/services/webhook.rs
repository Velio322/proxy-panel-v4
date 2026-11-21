use crate::state::AppState;
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::Sha256;
use sqlx::Row;
use std::time::Duration;

type HmacSha256 = Hmac<Sha256>;

/// Computes lowercase hex HMAC-SHA256 signature for webhook payload verification.
pub fn sign_payload(secret: &str, payload_bytes: &[u8]) -> String {
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return String::new();
    };
    mac.update(payload_bytes);
    hex::encode(mac.finalize().into_bytes())
}

/// Dispatches an event asynchronously to all active matching webhooks.
pub fn emit(state: &AppState, scope: &str, event: &str, data: Value) {
    let st = state.clone();
    let scope_s = scope.to_string();
    let event_s = event.to_string();
    tokio::spawn(async move {
        dispatch(&st, &scope_s, &event_s, data).await;
    });
}

pub async fn dispatch(state: &AppState, scope: &str, event: &str, data: Value) {
    let Ok(rows) = sqlx::query(
        r#"SELECT id, url, secret, scopes, events FROM "Webhook" WHERE active = true"#,
    )
    .fetch_all(&state.db)
    .await
    else {
        return;
    };

    let timestamp = crate::db::ts::format_iso(&crate::db::now());
    let envelope = json!({
        "event": event,
        "scope": scope,
        "timestamp": timestamp,
        "data": data
    });
    let Ok(body_bytes) = serde_json::to_vec(&envelope) else {
        return;
    };

    for row in rows {
        let id: String = row.try_get("id").unwrap_or_default();
        let url: String = row.try_get("url").unwrap_or_default();
        let secret: String = row.try_get("secret").unwrap_or_default();
        let scopes: Vec<String> = row.try_get("scopes").unwrap_or_default();
        let events: Vec<String> = row.try_get("events").unwrap_or_default();

        let scope_ok = scopes.is_empty() || scopes.iter().any(|s| s == "*" || s == scope);
        let event_ok = events.is_empty() || events.iter().any(|e| e == "*" || e == event);
        if !scope_ok || !event_ok {
            continue;
        }

        let sig = sign_payload(&secret, &body_bytes);
        let res = state
            .http_client
            .post(&url)
            .timeout(Duration::from_secs(10))
            .header("Content-Type", "application/json")
            .header("X-Webhook-Event", event)
            .header("X-Webhook-Timestamp", &timestamp)
            .header("X-Webhook-Signature", format!("sha256={sig}"))
            .body(body_bytes.clone())
            .send()
            .await;

        let now = crate::db::now();
        match res {
            Ok(resp) => {
                let code = i32::from(resp.status().as_u16());
                let err_msg = if resp.status().is_success() {
                    None
                } else {
                    Some(format!("HTTP {code}"))
                };
                let _ = sqlx::query(
                    r#"UPDATE "Webhook" SET "lastStatus" = $2, "lastError" = $3, "lastDeliveredAt" = $4, "updatedAt" = $4 WHERE id = $1"#,
                )
                .bind(&id)
                .bind(code)
                .bind(err_msg)
                .bind(now)
                .execute(&state.db)
                .await;
            }
            Err(e) => {
                let _ = sqlx::query(
                    r#"UPDATE "Webhook" SET "lastStatus" = 0, "lastError" = $2, "updatedAt" = $3 WHERE id = $1"#,
                )
                .bind(&id)
                .bind(e.to_string())
                .bind(now)
                .execute(&state.db)
                .await;
            }
        }
    }
}
