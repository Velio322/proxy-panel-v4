//! Auto-installs proxy cores that an enabled inbound needs but the node agent reports as absent.
//!
//! Triggered from node status reports (WS and HTTP). Per `(node, core)` a cooldown prevents
//! download storms when an install keeps failing (e.g. unsupported arch, GitHub rate limit).

use crate::services::core_updates;
use crate::state::AppState;
use serde_json::Value;
use std::time::Duration;

const RETRY_COOLDOWN: Duration = Duration::from_secs(600);

/// Maps a DB inbound protocol to the core binary that serves it, if provisionable.
fn core_for_protocol(protocol: &str) -> Option<&'static str> {
    match protocol {
        "NAIVEPROXY" => Some("naive"),
        "MIERU" => Some("mieru"),
        _ => None,
    }
}

/// Cores reported as explicitly not installed (`installed: false`) in a status report.
/// Old agents omit the field, so absence is never treated as "missing".
pub fn missing_cores(cores: Option<&Value>) -> Vec<&'static str> {
    let Some(c) = cores else { return Vec::new() };
    ["naive", "mieru"]
        .into_iter()
        .filter(|name| {
            c.get(*name)
                .and_then(|v| v.get("installed"))
                .and_then(Value::as_bool)
                == Some(false)
        })
        .collect()
}

/// Spawns background installs for missing cores that have at least one enabled inbound.
pub fn spawn_if_needed(state: &AppState, node_id: &str, cores: Option<&Value>) {
    let missing = missing_cores(cores);
    if missing.is_empty() {
        return;
    }
    let state = state.clone();
    let node_id = node_id.to_string();
    tokio::spawn(async move {
        let needed: Vec<String> = sqlx::query_scalar(
            r#"SELECT DISTINCT protocol::text FROM "Inbound"
               WHERE "nodeId" = $1 AND enable = true AND protocol::text IN ('NAIVEPROXY', 'MIERU')"#,
        )
        .bind(&node_id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        for core in missing {
            if !needed
                .iter()
                .any(|p| core_for_protocol(p).is_some_and(|c| c == core))
            {
                continue;
            }
            let key = format!("core_provision:{node_id}:{core}");
            if state.cache.get_json(&key).is_some() {
                continue;
            }
            state
                .cache
                .set_json(key, Value::Bool(true), RETRY_COOLDOWN);

            tracing::info!(node_id = %node_id, core, "core missing for enabled inbound; installing");
            match core_updates::trigger_core_update(
                &state,
                &node_id,
                core.to_string(),
                None,
                None,
                None,
            )
            .await
            {
                Ok(out) => tracing::info!(node_id = %node_id, core, version = %out.version, "core installed"),
                Err(e) => tracing::warn!(node_id = %node_id, core, error = %e, "core auto-install failed"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_explicit_installed_false_counts_as_missing() {
        let cores = json!({
            "naive": {"running": false, "installed": false},
            "mieru": {"running": false},
            "xray": {"installed": false}
        });
        assert_eq!(missing_cores(Some(&cores)), vec!["naive"]);
        assert!(missing_cores(None).is_empty());
        assert!(missing_cores(Some(&json!({"mieru": {"installed": true}}))).is_empty());
    }

    #[test]
    fn protocol_to_core_mapping() {
        assert_eq!(core_for_protocol("MIERU"), Some("mieru"));
        assert_eq!(core_for_protocol("NAIVEPROXY"), Some("naive"));
        assert_eq!(core_for_protocol("VLESS"), None);
    }
}
