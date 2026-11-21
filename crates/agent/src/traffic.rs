use crate::cores::write_atomic_0600;
use crate::cores::xray_grpc::StatEntry;
use base64::Engine as _;
use mypanel_proto::{ActiveSession, Protocol, TrafficCounter, TrafficReport, MAX_TRAFFIC_ENTRIES};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader};

const MAX_TRACKER_BASELINE_ENTRIES: usize = 10_000;

/// Parses Xray `StatsService` entries (`user>>><email>>>>traffic>>>uplink|downlink`),
/// strictly filtering on the `"user>>>"` prefix (`03-worker-and-cores.md §5.4`).
pub fn parse_xray_stats(entries: &[StatEntry]) -> HashMap<String, TrafficCounter> {
    let mut out: HashMap<String, TrafficCounter> = HashMap::new();
    for entry in entries {
        if !entry.name.starts_with("user>>>") {
            continue;
        }
        let parts: Vec<&str> = entry.name.split(">>>").collect();
        if parts.len() < 4 {
            continue;
        }
        let email = parts[1].trim();
        if email.is_empty() {
            continue;
        }
        let direction = parts[parts.len() - 1];
        let val = u64::try_from(entry.value).unwrap_or(0);
        let counter = out.entry(email.to_string()).or_insert_with(|| TrafficCounter {
            upload: 0,
            download: 0,
            unmetered: false,
        });
        match direction {
            "uplink" => counter.upload = counter.upload.saturating_add(val),
            "downlink" => counter.download = counter.download.saturating_add(val),
            _ => {}
        }
    }
    out
}

/// Parses Xray CLI `xray api statsquery --server=...` JSON stdout (`{"stat": [...]}`).
pub fn parse_xray_stats_json(json_str: &str) -> Result<HashMap<String, TrafficCounter>, String> {
    let val: Value =
        serde_json::from_str(json_str).map_err(|e| format!("Invalid xray stats JSON: {e}"))?;
    let mut entries = Vec::new();
    if let Some(arr) = val.get("stat").and_then(Value::as_array) {
        for item in arr {
            let Some(name) = item.get("name").and_then(Value::as_str) else {
                continue;
            };
            let value = match item.get("value") {
                Some(Value::Number(n)) => n.as_i64().unwrap_or(0),
                Some(Value::String(s)) => s.trim().parse::<i64>().unwrap_or(0),
                _ => 0,
            };
            entries.push(StatEntry {
                name: name.to_string(),
                value,
            });
        }
    }
    Ok(parse_xray_stats(&entries))
}

/// Tracks Sing-box Clash API `/connections` cumulative counters per connection ID
/// and accumulates per-user upload/download byte totals (`unmetered: false`).
#[derive(Debug, Default)]
pub struct SingboxConnectionTracker {
    /// Last observed `(upload, download)` per connection `id`.
    last_conn_bytes: HashMap<String, (u64, u64)>,
    /// Cumulative per-user `(upload, download)` since agent start.
    user_totals: HashMap<String, TrafficCounter>,
}

impl SingboxConnectionTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ingests a `/connections` JSON payload from Sing-box Clash API, returning
    /// the updated cumulative per-user traffic map and active sessions list.
    pub fn ingest_connections_json(
        &mut self,
        body: &str,
    ) -> Result<(HashMap<String, TrafficCounter>, Vec<ActiveSession>), String> {
        let root: Value = serde_json::from_str(body)
            .map_err(|e| format!("Invalid Sing-box /connections JSON: {e}"))?;
        let conns = root
            .get("connections")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let mut active_ids = std::collections::HashSet::with_capacity(conns.len());
        let mut sessions = Vec::with_capacity(conns.len());

        for conn in conns {
            let Some(id) = conn.get("id").and_then(Value::as_str).map(str::trim) else {
                continue;
            };
            if id.is_empty() {
                continue;
            }
            active_ids.insert(id.to_string());

            let cur_up = conn.get("upload").and_then(Value::as_u64).unwrap_or(0);
            let cur_down = conn.get("download").and_then(Value::as_u64).unwrap_or(0);

            let (prev_up, prev_down) = self
                .last_conn_bytes
                .get(id)
                .copied()
                .unwrap_or((0, 0));
            let delta_up = if cur_up >= prev_up {
                cur_up - prev_up
            } else {
                cur_up
            };
            let delta_down = if cur_down >= prev_down {
                cur_down - prev_down
            } else {
                cur_down
            };
            self.last_conn_bytes
                .insert(id.to_string(), (cur_up, cur_down));

            let meta = conn.get("metadata").unwrap_or(&Value::Null);
            let user_key = meta
                .get("inboundUser")
                .or_else(|| meta.get("user"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty());

            if let Some(uk) = user_key {
                let entry = self
                    .user_totals
                    .entry(uk.to_string())
                    .or_insert_with(|| TrafficCounter {
                        upload: 0,
                        download: 0,
                        unmetered: false,
                    });
                entry.upload = entry.upload.saturating_add(delta_up);
                entry.download = entry.download.saturating_add(delta_down);
            }

            let inbound_tag = meta
                .get("inboundTag")
                .or_else(|| meta.get("inboundName"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let source_ip = meta
                .get("sourceIP")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let dest_host = meta
                .get("host")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .or_else(|| meta.get("destinationIP").and_then(Value::as_str))
                .unwrap_or("");
            let dest_port = meta
                .get("destinationPort")
                .and_then(Value::as_str)
                .unwrap_or("");
            let destination = if dest_host.is_empty() {
                None
            } else if dest_port.is_empty() {
                Some(dest_host.to_string())
            } else {
                Some(format!("{dest_host}:{dest_port}"))
            };
            let protocol = meta
                .get("inbound")
                .or_else(|| meta.get("type"))
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<Protocol>().ok());

            sessions.push(ActiveSession {
                id: id.to_string(),
                client_id: user_key.map(ToString::to_string),
                username: user_key.map(ToString::to_string),
                inbound_tag,
                protocol,
                source_ip,
                destination,
                uptime_secs: 0,
                upload: cur_up,
                download: cur_down,
            });
        }

        // Prune closed connections so the map stays bounded
        self.last_conn_bytes.retain(|id, _| active_ids.contains(id));

        Ok((self.user_totals.clone(), sessions))
    }

    pub fn current_totals(&self) -> HashMap<String, TrafficCounter> {
        self.user_totals.clone()
    }
}

/// Fetches `/connections` from Sing-box Clash API (`http://<addr>/connections`).
pub async fn fetch_singbox_connections_json(
    client: &reqwest::Client,
    clash_api_addr: &str,
) -> Result<String, String> {
    let addr = clash_api_addr.trim().trim_start_matches("http://");
    let url = format!("http://{addr}/connections");
    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
        .map_err(|e| format!("Sing-box Clash API request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Sing-box Clash API returned HTTP {}", resp.status()));
    }
    resp.text()
        .await
        .map_err(|e| format!("Failed to read Sing-box Clash API body: {e}"))
}

/// Terminates a single Sing-box connection via Clash API `DELETE /connections/<id>`.
pub async fn drop_singbox_connection(
    client: &reqwest::Client,
    clash_api_addr: &str,
    conn_id: &str,
) -> Result<(), String> {
    let addr = clash_api_addr.trim().trim_start_matches("http://");
    let url = format!("http://{addr}/connections/{conn_id}");
    let resp = client
        .delete(&url)
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
        .map_err(|e| format!("Sing-box DELETE /connections failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "Sing-box DELETE /connections/{conn_id} returned HTTP {}",
            resp.status()
        ));
    }
    Ok(())
}

/// Parses Mieru (`mita get metrics`) JSON output into per-user traffic counters (`unmetered: false`).
///
/// Supports both array forms (`{"users": [...]}` / `{"metrics": [...]}` / `{"userMetrics": [...]}` / `[...]`)
/// and map forms (`{"alice": {"uploadBytes": 100, "downloadBytes": 200}}`).
pub fn parse_mieru_metrics_json(json_str: &str) -> Result<HashMap<String, TrafficCounter>, String> {
    let val: Value =
        serde_json::from_str(json_str).map_err(|e| format!("Invalid Mieru metrics JSON: {e}"))?;
    let mut out: HashMap<String, TrafficCounter> = HashMap::new();

    let array_opt = val
        .as_array()
        .or_else(|| val.get("users").and_then(Value::as_array))
        .or_else(|| val.get("metrics").and_then(Value::as_array))
        .or_else(|| val.get("userMetrics").and_then(Value::as_array));

    if let Some(arr) = array_opt {
        for item in arr {
            let Some(name) = item
                .get("name")
                .or_else(|| item.get("user"))
                .or_else(|| item.get("username"))
                .or_else(|| item.get("email"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            let (up, down) = extract_up_down(item);
            let entry = out.entry(name.to_string()).or_insert_with(|| TrafficCounter {
                upload: 0,
                download: 0,
                unmetered: false,
            });
            entry.upload = entry.upload.saturating_add(up);
            entry.download = entry.download.saturating_add(down);
        }
        return Ok(out);
    }

    if let Some(obj) = val.as_object() {
        for (name, item) in obj {
            if !item.is_object() {
                continue;
            }
            let clean = name.trim();
            if clean.is_empty() {
                continue;
            }
            let (up, down) = extract_up_down(item);
            let entry = out.entry(clean.to_string()).or_insert_with(|| TrafficCounter {
                upload: 0,
                download: 0,
                unmetered: false,
            });
            entry.upload = entry.upload.saturating_add(up);
            entry.download = entry.download.saturating_add(down);
        }
    }

    Ok(out)
}

fn extract_up_down(item: &Value) -> (u64, u64) {
    let up = item
        .get("uploadBytes")
        .or_else(|| item.get("upload"))
        .or_else(|| item.get("uplink"))
        .or_else(|| item.get("tx"))
        .and_then(json_u64)
        .unwrap_or(0);
    let down = item
        .get("downloadBytes")
        .or_else(|| item.get("download"))
        .or_else(|| item.get("downlink"))
        .or_else(|| item.get("rx"))
        .and_then(json_u64)
        .unwrap_or(0);
    (up, down)
}

fn json_u64(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse::<u64>().ok()))
}

/// Incremental Caddy JSON access log reader for NaiveProxy (`<CONFIG_DIR>/naive-access.log`).
#[derive(Debug, Default)]
pub struct NaiveAccessLogReader {
    offset: u64,
    totals: HashMap<String, TrafficCounter>,
}

impl NaiveAccessLogReader {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads any newly appended JSON lines from `log_path` since the last offset
    /// (handling log rotation/truncation) and returns cumulative per-user counters (`unmetered: false`).
    pub async fn poll(&mut self, log_path: &Path) -> HashMap<String, TrafficCounter> {
        let Ok(meta) = tokio::fs::metadata(log_path).await else {
            return self.totals.clone();
        };
        let len = meta.len();
        if len < self.offset {
            // File was truncated or rotated
            self.offset = 0;
        }
        if len == self.offset {
            return self.totals.clone();
        }

        let Ok(mut file) = tokio::fs::File::open(log_path).await else {
            return self.totals.clone();
        };
        if file.seek(std::io::SeekFrom::Start(self.offset)).await.is_err() {
            return self.totals.clone();
        }

        let mut reader = BufReader::new(file);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => break,
                Ok(n) => {
                    self.offset = self.offset.saturating_add(n as u64);
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        self.ingest_line(trimmed);
                    }
                }
                Err(_) => break,
            }
        }

        self.totals.clone()
    }

    /// Parses a single Caddy JSON access log line and accumulates bytes into `self.totals`.
    pub fn ingest_line(&mut self, line: &str) {
        let Ok(val) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let Some(user) = extract_naive_user(&val) else {
            return;
        };
        let upload = val
            .get("bytes_read")
            .or_else(|| val.get("request").and_then(|r| r.get("bytes_read")))
            .and_then(json_u64)
            .unwrap_or(0);
        let download = val
            .get("size")
            .or_else(|| val.get("bytes_written"))
            .and_then(json_u64)
            .unwrap_or(0);

        if upload == 0 && download == 0 {
            return;
        }

        let entry = self.totals.entry(user).or_insert_with(|| TrafficCounter {
            upload: 0,
            download: 0,
            unmetered: false,
        });
        entry.upload = entry.upload.saturating_add(upload);
        entry.download = entry.download.saturating_add(download);
    }

    pub fn totals(&self) -> &HashMap<String, TrafficCounter> {
        &self.totals
    }
}

fn extract_naive_user(val: &Value) -> Option<String> {
    if let Some(uid) = val
        .get("user_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(uid.to_string());
    }
    let auth_header = val
        .get("request")
        .and_then(|r| r.get("headers"))
        .and_then(|h| h.get("Proxy-Authorization"))
        .and_then(|arr| arr.as_array())
        .and_then(|arr| arr.first())
        .and_then(Value::as_str)
        .map(str::trim)?;

    let b64 = auth_header
        .strip_prefix("Basic ")
        .or_else(|| auth_header.strip_prefix("basic "))?
        .trim();
    if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(b64) {
        if let Ok(creds) = String::from_utf8(decoded) {
            let user = creds.split(':').next().unwrap_or("").trim();
            if !user.is_empty() {
                return Some(user.to_string());
            }
        }
    }
    None
}

/// Durable representation of unacknowledged traffic deltas persisted to `<CONFIG_DIR>/traffic-pending.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingTrafficFile {
    #[serde(default)]
    batches: HashMap<String, HashMap<String, TrafficCounter>>,
}

/// Tracks cumulative per-user counters across all 4 proxy cores (`unmetered: false`),
/// computes deltas with reset detection, and durably persists unacknowledged batches to
/// `<CONFIG_DIR>/traffic-pending.json`.
#[derive(Debug)]
pub struct CumulativeTrafficTracker {
    previous: HashMap<String, (u64, u64)>,
    pending_batches: HashMap<String, HashMap<String, TrafficCounter>>,
    pending_path: PathBuf,
}

impl CumulativeTrafficTracker {
    pub fn new(pending_path: impl AsRef<Path>) -> Self {
        Self {
            previous: HashMap::new(),
            pending_batches: HashMap::new(),
            pending_path: pending_path.as_ref().to_path_buf(),
        }
    }

    /// Loads any unacknowledged traffic batches from `<CONFIG_DIR>/traffic-pending.json`.
    pub async fn load_pending(&mut self) -> Result<(), String> {
        let Ok(raw) = tokio::fs::read_to_string(&self.pending_path).await else {
            return Ok(());
        };
        let parsed: PendingTrafficFile = serde_json::from_str(&raw)
            .map_err(|e| format!("Failed to parse {}: {e}", self.pending_path.display()))?;
        self.pending_batches = parsed.batches;
        Ok(())
    }

    /// Computes delta from `current` cumulative counters, advances the baseline,
    /// records an unacknowledged batch in `traffic-pending.json`, and returns a [`TrafficReport`].
    pub async fn compute_delta(
        &mut self,
        current: &HashMap<String, TrafficCounter>,
    ) -> Result<TrafficReport, String> {
        let mut delta_stats: HashMap<String, TrafficCounter> = HashMap::new();

        for (id, cur) in current {
            let (prev_up, prev_down) = self.previous.get(id).copied().unwrap_or((0, 0));
            let upload = if cur.upload >= prev_up {
                cur.upload - prev_up
            } else {
                cur.upload
            };
            let download = if cur.download >= prev_down {
                cur.download - prev_down
            } else {
                cur.download
            };

            if upload > 0 || download > 0 {
                delta_stats.insert(
                    id.clone(),
                    TrafficCounter {
                        upload,
                        download,
                        unmetered: false,
                    },
                );
            }
            self.previous.insert(id.clone(), (cur.upload, cur.download));
        }

        if self.previous.len() > MAX_TRACKER_BASELINE_ENTRIES {
            self.previous.retain(|k, _| current.contains_key(k));
        }

        // Cap stats to MAX_TRAFFIC_ENTRIES
        if delta_stats.len() > MAX_TRAFFIC_ENTRIES {
            delta_stats = delta_stats.into_iter().take(MAX_TRAFFIC_ENTRIES).collect();
        }

        if delta_stats.is_empty() {
            return Ok(TrafficReport {
                batch_id: None,
                stats: HashMap::new(),
            });
        }

        let batch_id = uuid::Uuid::new_v4().to_string();
        let report = TrafficReport {
            batch_id: Some(batch_id.clone()),
            stats: delta_stats.clone(),
        };
        report
            .validate()
            .map_err(|e| format!("TrafficReport validation failed: {e}"))?;

        self.pending_batches.insert(batch_id, delta_stats);
        self.save_pending().await?;

        Ok(report)
    }

    /// Returns all currently unacknowledged pending batches to be replayed after reconnect.
    pub fn pending_reports(&self) -> Vec<TrafficReport> {
        self.pending_batches
            .iter()
            .map(|(batch_id, stats)| TrafficReport {
                batch_id: Some(batch_id.clone()),
                stats: stats.clone(),
            })
            .collect()
    }

    /// Removes an acknowledged batch and updates `<CONFIG_DIR>/traffic-pending.json`.
    pub async fn acknowledge_batch(&mut self, batch_id: &str) -> Result<bool, String> {
        let removed = self.pending_batches.remove(batch_id).is_some();
        if removed {
            self.save_pending().await?;
        }
        Ok(removed)
    }

    async fn save_pending(&self) -> Result<(), String> {
        let file = PendingTrafficFile {
            batches: self.pending_batches.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&file)
            .map_err(|e| format!("Failed to serialize traffic-pending.json: {e}"))?;
        write_atomic_0600(&self.pending_path, &bytes).await
    }
}

/// Merges per-user traffic maps from all 4 proxy cores (`unmetered: false`).
pub fn merge_traffic_maps(
    maps: &[&HashMap<String, TrafficCounter>],
) -> HashMap<String, TrafficCounter> {
    let mut merged: HashMap<String, TrafficCounter> = HashMap::new();
    for m in maps {
        for (k, v) in *m {
            let entry = merged.entry(k.clone()).or_insert_with(|| TrafficCounter {
                upload: 0,
                download: 0,
                unmetered: false,
            });
            entry.upload = entry.upload.saturating_add(v.upload);
            entry.download = entry.download.saturating_add(v.download);
            entry.unmetered = false;
        }
    }
    merged
}
