use crate::state::AppState;
use serde_json::{json, Value};
use std::time::Duration;

/// Exports panel events to Redis Streams (`REDIS_URL`) and/or ClickHouse HTTP (`CLICKHOUSE_URL`) when configured.
pub fn emit_event(state: &AppState, event_type: &str, payload: Value) {
    if state.cfg.redis_url.is_none() && state.cfg.clickhouse_url.is_none() {
        return;
    }
    let st = state.clone();
    let et = event_type.to_string();
    tokio::spawn(async move {
        let ts = crate::db::ts::format_iso(&crate::db::now());
        let data_str = payload.to_string();

        if let Some(ref redis_url) = st.cfg.redis_url {
            if let Ok(client) = redis::Client::open(redis_url.as_str()) {
                if let Ok(mut conn) = client.get_multiplexed_async_connection().await {
                    let _: Result<String, _> = redis::cmd("XADD")
                        .arg(&st.cfg.redis_stream_key)
                        .arg("MAXLEN")
                        .arg("~")
                        .arg(100_000)
                        .arg("*")
                        .arg("event")
                        .arg(&et)
                        .arg("timestamp")
                        .arg(&ts)
                        .arg("payload")
                        .arg(&data_str)
                        .query_async(&mut conn)
                        .await;
                }
            }
        }

        if let Some(ref ch_url) = st.cfg.clickhouse_url {
            let row = json!({
                "event": et,
                "timestamp": ts,
                "payload": data_str
            });
            let url = format!(
                "{}/?query=INSERT%20INTO%20{}%20FORMAT%20JSONEachRow",
                ch_url.trim_end_matches('/'),
                st.cfg.clickhouse_table
            );
            let _ = st
                .http_client
                .post(url)
                .timeout(Duration::from_secs(5))
                .body(format!("{row}\n"))
                .send()
                .await;
        }
    });
}
