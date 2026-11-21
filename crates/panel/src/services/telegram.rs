use crate::state::AppState;
use serde_json::json;

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
    let Some(token) = state
        .cfg
        .telegram_bot_token
        .as_deref()
        .filter(|t| !t.is_empty())
    else {
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
            tracing::warn!(error = %e, chat_id, "Telegram sendMessage failed");
            false
        }
    }
}

pub async fn notify_admins(state: &AppState, title: &str, body: &str) {
    if state.cfg.telegram_admin_ids.is_empty() {
        return;
    }
    let msg = format!("<b>{}</b>\n{}", esc(title), esc(body));
    for &admin_id in &state.cfg.telegram_admin_ids {
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
