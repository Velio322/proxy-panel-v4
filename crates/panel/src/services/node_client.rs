use crate::db::enums::NodeStatus;
use crate::error::{AppError, AppResult};
use crate::net::{is_loopback_host, validate_node_host};
use crate::services::{core_updates, node_config};
use crate::state::AppState;
use mypanel_proto::{ApplyConfigRequest, CoreUpdateCmd, DropSessionCmd, StatusReport};
use serde_json::{json, Value};
use sqlx::Row;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct NodeTarget {
    pub id: String,
    pub name: String,
    pub host: String,
    pub api_port: i32,
    pub secret: String,
}

impl NodeTarget {
    pub async fn fetch(state: &AppState, node_id: &str) -> AppResult<Self> {
        let row = sqlx::query(
            r#"SELECT id, name, host, "apiPort", secret FROM "Node" WHERE id = $1"#,
        )
        .bind(node_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("Node not found"))?;

        Ok(Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            host: row.try_get("host")?,
            api_port: row.try_get("apiPort")?,
            secret: row.try_get("secret")?,
        })
    }
}

/// Resolves the node agent base URL enforcing `` (`https://` for non-loopback unless `NODE_API_PLAINTEXT=true`
/// and SSRF check unless `ALLOW_PRIVATE_NODES=true`).
pub fn base_url(
    node: &NodeTarget,
    allow_private: bool,
    allow_plaintext: bool,
) -> AppResult<(String, String)> {
    let host = node.host.trim();
    validate_node_host(host, allow_private)?;

    if host.contains("://") {
        let parsed = url::Url::parse(host).map_err(|_| AppError::bad_request("Invalid node URL"))?;
        let scheme = parsed.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(AppError::bad_request("Unsupported node protocol"));
        }
        if scheme == "http" && !allow_plaintext && !is_loopback_host(host) {
            return Err(AppError::bad_request(
                "Plaintext http:// node API disabled for non-loopback nodes",
            ));
        }
        let h = parsed
            .host_str()
            .ok_or_else(|| AppError::bad_request("Node URL has no host"))?;
        let port = parsed.port().unwrap_or(u16::try_from(node.api_port).unwrap_or(2222));
        let formatted_host = if h.contains(':') && !h.starts_with('[') {
            format!("[{h}]")
        } else {
            h.to_string()
        };
        return Ok((format!("{scheme}://{formatted_host}:{port}"), h.to_string()));
    }

    let is_loopback = is_loopback_host(host);
    let scheme = if node.api_port == 443 || (!allow_plaintext && !is_loopback) {
        "https"
    } else {
        "http"
    };
    let clean_host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(':')
        .next()
        .unwrap_or(host);
    let formatted_host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    Ok((
        format!("{scheme}://{formatted_host}:{}", node.api_port),
        clean_host.to_string(),
    ))
}

async fn node_get(state: &AppState, node: &NodeTarget, path: &str) -> AppResult<Value> {
    let (base, _) = base_url(node, state.cfg.allow_private_nodes, state.cfg.node_api_plaintext)?;
    let url = format!("{base}{path}");
    let resp = state
        .http_client
        .get(&url)
        .bearer_auth(&node.secret)
        .header("X-Node-Secret", &node.secret)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| AppError::service_unavailable(format!("Node request failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::service_unavailable(format!(
            "Node returned HTTP {}",
            resp.status()
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::service_unavailable(format!("Node response read failed: {e}")))?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(AppError::payload_too_large("Node response exceeded 8 MiB"));
    }
    serde_json::from_slice(&bytes)
        .map_err(|e| AppError::service_unavailable(format!("Invalid JSON from node: {e}")))
}

async fn node_post<T: serde::Serialize>(
    state: &AppState,
    node: &NodeTarget,
    path: &str,
    body: &T,
) -> AppResult<Value> {
    node_post_with_timeout(state, node, path, body, Duration::from_secs(15)).await
}

async fn node_post_with_timeout<T: serde::Serialize>(
    state: &AppState,
    node: &NodeTarget,
    path: &str,
    body: &T,
    timeout: Duration,
) -> AppResult<Value> {
    let (base, _) = base_url(node, state.cfg.allow_private_nodes, state.cfg.node_api_plaintext)?;
    let url = format!("{base}{path}");
    let resp = state
        .http_client
        .post(&url)
        .bearer_auth(&node.secret)
        .header("X-Node-Secret", &node.secret)
        .json(body)
        .timeout(timeout)
        .send()
        .await
        .map_err(|e| AppError::service_unavailable(format!("Node request failed: {e}")))?;

    let status = resp.status();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::service_unavailable(format!("Node response read failed: {e}")))?;

    if !status.is_success() {
        let parsed_err = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|v| {
                v.get("error")
                    .or_else(|| v.get("message"))
                    .and_then(Value::as_str)
                    .map(ToString::to_string)
            })
            .filter(|s| !s.trim().is_empty());
        let msg = parsed_err.unwrap_or_else(|| format!("Node returned HTTP {status}"));
        if status.is_client_error() {
            return Err(AppError::unprocessable(msg));
        }
        return Err(AppError::service_unavailable(msg));
    }

    if bytes.is_empty() {
        return Ok(json!({ "success": true }));
    }
    serde_json::from_slice(&bytes).or_else(|_| Ok(json!({ "success": true })))
}

/// Extracts node uptime (in seconds), preferring OS host uptime over agent process uptime.
pub fn extract_report_uptime(sr: &StatusReport) -> i64 {
    sr.host_telemetry
        .as_ref()
        .map(|ht| ht.uptime.host_seconds)
        .filter(|&s| s.is_finite() && s > 0.0)
        .map(|s| s.round().clamp(0.0, i64::MAX as f64) as i64)
        .unwrap_or_else(|| i64::try_from(sr.uptime).unwrap_or(i64::MAX).max(0))
}

/// Extracts node uptime (in seconds) from an agent JSON payload, preferring host uptime.
pub fn extract_uptime_seconds(val: &Value, parsed: Option<&StatusReport>) -> Option<i64> {
    if let Some(secs) = val
        .pointer("/hostTelemetry/uptime/hostSeconds")
        .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|n| n as f64)))
        .filter(|&s| s.is_finite() && s > 0.0)
    {
        return Some(secs.round().clamp(0.0, i64::MAX as f64) as i64);
    }

    if let Some(s) = parsed {
        return Some(extract_report_uptime(s));
    }

    val.get("uptime").and_then(|v| {
        v.as_i64().map(|n| n.max(0)).or_else(|| {
            v.as_f64()
                .filter(|&f| f.is_finite() && f >= 0.0)
                .map(|f| f.round().clamp(0.0, i64::MAX as f64) as i64)
        })
    })
}

async fn record_telemetry_snapshot(
    state: &AppState,
    node_id: &str,
    val: &Value,
    cpu: Option<f64>,
    mem: Option<f64>,
    uptime_secs: Option<i64>,
) {
    let net_up = val
        .pointer("/traffic/total/upload")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let net_down = val
        .pointer("/traffic/total/download")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let connections = val
        .get("connections")
        .and_then(Value::as_i64)
        .and_then(|v| i32::try_from(v).ok())
        .unwrap_or(0);
    let uptime = uptime_secs
        .unwrap_or(0)
        .clamp(0, i32::MAX as i64) as i32;
    let tcp_conns = val
        .pointer("/hostTelemetry/connections/establishedTcp")
        .and_then(Value::as_i64)
        .and_then(|v| i32::try_from(v).ok())
        .or(Some(connections));
    let udp_socks = val
        .pointer("/hostTelemetry/connections/udpSockets")
        .and_then(Value::as_i64)
        .and_then(|v| i32::try_from(v).ok());
    let upload_bps = val
        .pointer("/hostTelemetry/network/uploadBps")
        .and_then(Value::as_f64);
    let download_bps = val
        .pointer("/hostTelemetry/network/downloadBps")
        .and_then(Value::as_f64);

    if let (Some(c), Some(m)) = (cpu, mem) {
        state.metrics.set_node_telemetry(node_id, c, m);
    }

    let _ = sqlx::query(
        r#"INSERT INTO "NodeMetric" (id, "nodeId", "cpuUsage", "memUsage", "netUpload", "netDownload", connections, uptime, "tcpConnections", "udpSockets", "uploadBps", "downloadBps", "recordedAt")
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)"#,
    )
    .bind(crate::db::new_id())
    .bind(node_id)
    .bind(cpu)
    .bind(mem)
    .bind(net_up)
    .bind(net_down)
    .bind(connections)
    .bind(uptime)
    .bind(tcp_conns)
    .bind(udp_socks)
    .bind(upload_bps)
    .bind(download_bps)
    .bind(crate::db::now())
    .execute(&state.db)
    .await;
}

pub async fn check_status(state: &AppState, node: &NodeTarget) -> AppResult<Value> {
    let started = Instant::now();
    let now = crate::db::now();
    match node_get(state, node, "/api/status").await {
        Ok(val) => {
            let ping_ms = i32::try_from(started.elapsed().as_millis()).unwrap_or(0);
            let parsed: Option<StatusReport> = serde_json::from_value(val.clone()).ok();

            let (version, info) = match &parsed {
                Some(s) => (
                    Some(s.version.clone()),
                    core_updates::extract_core_info(s.cores.as_ref()),
                ),
                None => (
                    val.get("version").and_then(Value::as_str).map(ToString::to_string),
                    core_updates::extract_core_info(val.get("cores")),
                ),
            };
            let cpu = parsed
                .as_ref()
                .and_then(|s| s.cpu_usage)
                .or_else(|| val.get("cpuUsage").and_then(Value::as_f64));
            let mem = parsed
                .as_ref()
                .and_then(|s| s.mem_usage)
                .or_else(|| val.get("memUsage").and_then(Value::as_f64));
            let uptime_secs = extract_uptime_seconds(&val, parsed.as_ref());

            let _ = sqlx::query(
                r#"UPDATE "Node"
                   SET status = $2, "lastCheckAt" = $3, "lastPingMs" = $4,
                       version = COALESCE($5, version),
                       "xrayVersion" = COALESCE($6, "xrayVersion"),
                       "singboxVersion" = COALESCE($7, "singboxVersion"),
                       "naiveVersion" = COALESCE($8, "naiveVersion"),
                       "mieruVersion" = COALESCE($9, "mieruVersion"),
                       arch = COALESCE($10, arch),
                       "cpuUsage" = COALESCE($11, "cpuUsage"),
                       "memUsage" = COALESCE($12, "memUsage"),
                       uptime = COALESCE($13, uptime),
                       "updatedAt" = $3
                   WHERE id = $1"#,
            )
            .bind(&node.id)
            .bind(NodeStatus::Online)
            .bind(now)
            .bind(ping_ms)
            .bind(&version)
            .bind(&info.xray)
            .bind(&info.singbox)
            .bind(&info.naive)
            .bind(&info.mieru)
            .bind(&info.arch)
            .bind(cpu)
            .bind(mem)
            .bind(uptime_secs)
            .execute(&state.db)
            .await;

            record_telemetry_snapshot(state, &node.id, &val, cpu, mem, uptime_secs).await;

            state.metrics.set_node_status(&node.id, &node.name, true);
            let mut out = val;
            if let Some(obj) = out.as_object_mut() {
                obj.insert("nodeId".into(), json!(node.id));
                obj.insert("status".into(), json!("ONLINE"));
                obj.insert("pingMs".into(), json!(ping_ms));
            }
            Ok(out)
        }
        Err(_) => {
            if state.hub.is_connected(&node.id) {
                let _ = sqlx::query(
                    r#"UPDATE "Node" SET status = $2, "lastCheckAt" = $3, "updatedAt" = $3 WHERE id = $1"#,
                )
                .bind(&node.id)
                .bind(NodeStatus::Online)
                .bind(now)
                .execute(&state.db)
                .await;
                state.metrics.set_node_status(&node.id, &node.name, true);
                return Ok(json!({
                    "nodeId": node.id,
                    "status": "ONLINE",
                    "transport": "websocket",
                    "httpReachable": false
                }));
            }
            let _ = sqlx::query(
                r#"UPDATE "Node" SET status = $2, "lastCheckAt" = $3, uptime = NULL, "updatedAt" = $3 WHERE id = $1"#,
            )
            .bind(&node.id)
            .bind(NodeStatus::Offline)
            .bind(now)
            .execute(&state.db)
            .await;
            state.metrics.set_node_status(&node.id, &node.name, false);
            Ok(json!({
                "nodeId": node.id,
                "status": "OFFLINE",
                "error": "Failed to connect"
            }))
        }
    }
}

pub async fn push_config_http(state: &AppState, node: &NodeTarget) -> AppResult<bool> {
    let inbounds = node_config::build(state, &node.id).await?;
    let routing_rules = node_config::build_routing_rules(state, &node.id).await?;
    let xray_profile = node_config::build_xray_profile(state, &node.id).await?;
    let req = ApplyConfigRequest {
        inbounds,
        routing_rules,
        xray_profile,
    };
    match node_post(state, node, "/api/config", &req).await {
        Ok(v) => Ok(v.get("success").and_then(Value::as_bool).unwrap_or(true)),
        Err(e) => {
            tracing::warn!(node_id = %node.id, error = %e, "HTTP push_config failed");
            Ok(false)
        }
    }
}

pub async fn restart_node(state: &AppState, node: &NodeTarget) -> AppResult<bool> {
    match node_post(state, node, "/api/restart", &json!({})).await {
        Ok(v) => Ok(v.get("success").and_then(Value::as_bool).unwrap_or(true)),
        Err(_) => Ok(false),
    }
}

pub async fn stop_node(state: &AppState, node: &NodeTarget) -> AppResult<bool> {
    match node_post(state, node, "/api/stop", &json!({})).await {
        Ok(v) => Ok(v.get("success").and_then(Value::as_bool).unwrap_or(true)),
        Err(_) => Ok(false),
    }
}

pub async fn get_metrics(state: &AppState, node: &NodeTarget) -> AppResult<Option<Value>> {
    let cache_key = format!("node_metrics:{}", node.id);
    if let Some(cached) = state.cache.get_json(&cache_key) {
        return Ok(Some(cached));
    }

    let Ok(val) = node_get(state, node, "/api/metrics").await else {
        return Ok(None);
    };

    let cpu = val.get("cpuUsage").and_then(Value::as_f64);
    let mem = val.get("memUsage").and_then(Value::as_f64);
    let uptime_secs = extract_uptime_seconds(&val, None);

    let _ = sqlx::query(
        r#"UPDATE "Node"
           SET uptime = COALESCE($2, uptime),
               "cpuUsage" = COALESCE($3, "cpuUsage"),
               "memUsage" = COALESCE($4, "memUsage")
           WHERE id = $1"#,
    )
    .bind(&node.id)
    .bind(uptime_secs)
    .bind(cpu)
    .bind(mem)
    .execute(&state.db)
    .await;

    record_telemetry_snapshot(state, &node.id, &val, cpu, mem, uptime_secs).await;

    state
        .cache
        .set_json(cache_key, val.clone(), Duration::from_secs(30));

    Ok(Some(val))
}

pub async fn drop_session(
    state: &AppState,
    node_id: &str,
    cmd: DropSessionCmd,
) -> AppResult<bool> {
    if state.hub.is_connected(node_id) {
        return Ok(state
            .hub
            .send(node_id, mypanel_proto::Envelope::DropSessionCmd(cmd)));
    }
    let node = NodeTarget::fetch(state, node_id).await?;
    Ok(node_post(state, &node, "/api/sessions/drop", &cmd)
        .await
        .is_ok())
}

pub async fn core_update(
    state: &AppState,
    node_id: &str,
    cmd: CoreUpdateCmd,
) -> AppResult<Option<String>> {
    if state.hub.is_connected(node_id) {
        let cmd_id = cmd.command_id.clone();
        let ack = state
            .hub
            .send_and_wait_ack(
                node_id,
                &cmd_id,
                mypanel_proto::Envelope::CoreUpdateCmd(cmd),
                Duration::from_secs(180),
            )
            .await
            .map_err(AppError::service_unavailable)?;
        if !ack.ok {
            return Err(AppError::unprocessable(
                ack.error
                    .unwrap_or_else(|| "Core update failed on node".to_string()),
            ));
        }
        return Ok(None);
    }

    let node = NodeTarget::fetch(state, node_id).await?;
    let resp = node_post_with_timeout(
        state,
        &node,
        "/api/core-update",
        &cmd,
        Duration::from_secs(180),
    )
    .await?;
    if resp.get("success").and_then(Value::as_bool) == Some(false) {
        let msg = resp
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("Core update failed on node");
        return Err(AppError::unprocessable(msg));
    }
    Ok(resp
        .get("version")
        .and_then(Value::as_str)
        .map(ToString::to_string))
}

pub async fn tcp_probe(host: &str, port: u16, timeout: Duration) -> bool {
    let clean = host
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("")
        .trim_start_matches('[')
        .trim_end_matches(']');
    let addr = if clean.contains(':') {
        format!("[{clean}]:{port}")
    } else {
        format!("{clean}:{port}")
    };
    tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&addr))
        .await
        .is_ok_and(|res| res.is_ok())
}

#[cfg(test)]
mod tests {
    use super::extract_uptime_seconds;
    use mypanel_proto::{
        ConnectionsTelemetry, CpuTelemetry, HostTelemetry, MemoryTelemetry, NetworkTelemetry,
        ProcessMemoryTelemetry, ProcessTelemetry, StatusReport, UptimeTelemetry,
    };
    use serde_json::json;

    #[test]
    fn extract_uptime_prefers_host_seconds_over_agent_uptime() {
        let payload = json!({
            "uptime": 42,
            "hostTelemetry": {
                "uptime": {
                    "hostSeconds": 86401.6,
                    "processSeconds": 42.0
                }
            }
        });
        assert_eq!(extract_uptime_seconds(&payload, None), Some(86402));

        let report = StatusReport {
            status: "ONLINE".into(),
            uptime: 42,
            version: "4.0.0".into(),
            host_telemetry: Some(HostTelemetry {
                sampled_at: String::new(),
                scope: String::new(),
                cpu: CpuTelemetry {
                    count: 1,
                    usage_percent: None,
                    load_average: [0.0; 3],
                },
                memory: MemoryTelemetry {
                    total_bytes: 0,
                    free_bytes: 0,
                    used_bytes: 0,
                    usage_percent: None,
                },
                uptime: UptimeTelemetry {
                    host_seconds: 172800.4,
                    process_seconds: 42.0,
                },
                connections: ConnectionsTelemetry {
                    established_tcp: None,
                    udp_sockets: None,
                    scope: String::new(),
                    proxy_user_count: None,
                },
                network: NetworkTelemetry {
                    scope: String::new(),
                    received_bytes: None,
                    transmitted_bytes: None,
                    upload_bps: None,
                    download_bps: None,
                },
                process: ProcessTelemetry {
                    memory: ProcessMemoryTelemetry::default(),
                    pid: 1,
                },
            }),
            ..StatusReport::default()
        };
        let encoded = serde_json::to_value(&report).unwrap();
        assert_eq!(extract_uptime_seconds(&encoded, Some(&report)), Some(172800));
    }

    #[test]
    fn extract_uptime_falls_back_to_agent_uptime_when_host_seconds_missing_or_zero() {
        let partial_zero = json!({
            "uptime": 315.4,
            "hostTelemetry": {
                "uptime": {
                    "hostSeconds": 0.0
                }
            }
        });
        assert_eq!(extract_uptime_seconds(&partial_zero, None), Some(315));

        let report_no_ht = StatusReport {
            status: "ONLINE".into(),
            uptime: 912,
            version: "4.0.0".into(),
            host_telemetry: None,
            ..StatusReport::default()
        };
        let encoded = serde_json::to_value(&report_no_ht).unwrap();
        assert_eq!(
            extract_uptime_seconds(&encoded, Some(&report_no_ht)),
            Some(912)
        );
    }

    #[test]
    fn extract_uptime_returns_none_when_absent() {
        assert_eq!(extract_uptime_seconds(&json!({}), None), None);
    }
}
