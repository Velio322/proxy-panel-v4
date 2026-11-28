use crate::state::AppState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use std::time::Duration;

const TELEGRAM_CFG_CACHE_KEY: &str = "settings:telegram_runtime";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramRuntimeConfig {
    pub bot_token: Option<String>,
    pub bot_username: Option<String>,
    pub admin_ids: Vec<i64>,
    pub notifications_enabled: bool,
}

fn value_as_nonempty_str(v: &Value) -> Option<String> {
    v.as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

fn parse_admin_ids_value(v: &Value) -> Vec<i64> {
    match v {
        Value::String(s) => s
            .split(',')
            .filter_map(|p| p.trim().parse::<i64>().ok())
            .collect(),
        Value::Array(arr) => arr
            .iter()
            .filter_map(|item| {
                item.as_i64()
                    .or_else(|| item.as_str().and_then(|s| s.trim().parse::<i64>().ok()))
            })
            .collect(),
        Value::Number(n) => n.as_i64().into_iter().collect(),
        _ => Vec::new(),
    }
}

/// Loads effective Telegram settings from `"SystemSetting"` (cached under `settings:` so
/// `/api/v1/settings` updates invalidate it immediately), falling back to env config.
pub async fn load_telegram_config(state: &AppState) -> TelegramRuntimeConfig {
    if let Some(cached) = state.cache.get_json(TELEGRAM_CFG_CACHE_KEY) {
        if let Ok(parsed) = serde_json::from_value::<TelegramRuntimeConfig>(cached) {
            return parsed;
        }
    }

    let mut cfg = TelegramRuntimeConfig {
        bot_token: state.cfg.telegram_bot_token.clone(),
        bot_username: state.cfg.telegram_bot_username.clone(),
        admin_ids: state.cfg.telegram_admin_ids.clone(),
        notifications_enabled: true,
    };

    if let Ok(rows) = sqlx::query(
        r#"SELECT key, value FROM "SystemSetting"
           WHERE key IN ('telegram_bot_token', 'telegram_bot_username', 'telegram_admin_ids', 'telegram_notifications')"#,
    )
    .fetch_all(&state.db)
    .await
    {
        for r in rows {
            let Ok(k) = r.try_get::<String, _>("key") else {
                continue;
            };
            let Ok(v) = r.try_get::<Value, _>("value") else {
                continue;
            };
            match k.as_str() {
                "telegram_bot_token" => {
                    if let Some(tok) = value_as_nonempty_str(&v) {
                        cfg.bot_token = Some(tok);
                    }
                }
                "telegram_bot_username" => {
                    if let Some(uname) = value_as_nonempty_str(&v) {
                        cfg.bot_username = Some(uname);
                    }
                }
                "telegram_admin_ids" => {
                    let ids = parse_admin_ids_value(&v);
                    if !ids.is_empty() {
                        cfg.admin_ids = ids;
                    }
                }
                "telegram_notifications" => {
                    cfg.notifications_enabled = match &v {
                        Value::Bool(b) => *b,
                        Value::String(s) => {
                            !matches!(s.trim().to_ascii_lowercase().as_str(), "false" | "0" | "off" | "no")
                        }
                        Value::Number(n) => n.as_i64() != Some(0),
                        _ => true,
                    };
                }
                _ => {}
            }
        }
    }

    if let Ok(val) = serde_json::to_value(&cfg) {
        state
            .cache
            .set_json(TELEGRAM_CFG_CACHE_KEY, val, Duration::from_secs(60));
    }
    cfg
}

/// HTML-escapes untrusted text for Telegram `parse_mode: "HTML"`.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

pub async fn send_message(state: &AppState, chat_id: i64, html_text: &str) -> bool {
    let tg_cfg = load_telegram_config(state).await;
    let Some(token) = tg_cfg.bot_token.as_deref().filter(|t| !t.is_empty()) else {
        return false;
    };

    let url = format!("https://api.telegram.org/bot{token}/sendMessage");
    match state
        .http_client
        .post(url)
        .json(&json!({
            "chat_id": chat_id,
            "text": html_text,
            "parse_mode": "HTML",
            "disable_web_page_preview": true
        }))
        .send()
        .await
    {
        Ok(resp) => resp.status().is_success(),
        Err(e) => {
            tracing::warn!(error = %e.without_url(), chat_id, "Telegram sendMessage failed");
            false
        }
    }
}

pub async fn notify_admins(state: &AppState, title: &str, body: &str) {
    let tg_cfg = load_telegram_config(state).await;
    if !tg_cfg.notifications_enabled || tg_cfg.admin_ids.is_empty() {
        return;
    }
    let msg = format!("<b>{}</b>\n{}", esc(title), esc(body));
    for admin_id in tg_cfg.admin_ids {
        send_message(state, admin_id, &msg).await;
    }
}

pub fn format_node_alert(node_name: &str, status: &str, detail: &str) -> String {
    format!(
        "<b>Node Alert</b>\nNode: <code>{}</code>\nStatus: <b>{}</b>\n{}",
        esc(node_name),
        esc(status),
        esc(detail)
    )
}
