use crate::cores::write_atomic_0600;
use crate::cores::xray_grpc::StatEntry;
use base64::Engine as _;
use mypanel_proto::{
    ActiveSession, Protocol, TrafficCounter, TrafficKeyMeta, TrafficReport, MAX_TRAFFIC_ENTRIES,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader};

const MAX_TRACKER_BASELINE_ENTRIES: usize = 10_000;
const CORE_KEY_SEP: char = '\u{1f}';

/// Cores whose cumulative counters survive an agent restart (mita `metrics.pb`, the naive
/// access log). Their first sighting must be seeded silently: the counter already holds
/// traffic that was reported (or predates accounting) and would otherwise be counted again.
/// Xray and sing-box are restarted together with the agent, so their counters start from zero
/// and everything they report is new traffic.
const PERSISTENT_CORES: &[&str] = &["mieru", "naive"];
// telemt counters live in the process and restart from zero with it, like xray: they are volatile.

fn core_is_persistent(core: &str) -> bool {
    PERSISTENT_CORES.contains(&core)
}

/// Baseline key for one user counter within one proxy core.
fn core_key(core: &str, user_id: &str) -> String {
    format!("{core}{CORE_KEY_SEP}{user_id}")
}

/// Attaches per-user `(protocol, inboundTag)` metadata to a report, restricted to the keys the
/// report actually carries. Unknown keys (e.g. stale batches whose users were deleted) simply
/// stay unmapped and the panel records them without protocol/tag, as before.
pub fn attach_key_meta(
    report: &mut TrafficReport,
    sources: &HashMap<String, TrafficKeyMeta>,
) {
    let meta: HashMap<String, TrafficKeyMeta> = report
        .stats
        .keys()
        .filter_map(|k| sources.get(k).cloned().map(|m| (k.clone(), m)))
        .collect();
    report.key_meta = if meta.is_empty() { None } else { Some(meta) };
}

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
    secret: Option<&str>,
) -> Result<String, String> {
    let addr = clash_api_addr.trim().trim_start_matches("http://");
    let url = format!("http://{addr}/connections");
    let mut req = client.get(&url).timeout(std::time::Duration::from_secs(2));
    if let Some(s) = secret.map(str::trim).filter(|s| !s.is_empty()) {
        req = req.bearer_auth(s);
    }
    let resp = req
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

fn is_valid_clash_conn_id(conn_id: &str) -> bool {
    !conn_id.is_empty()
        && conn_id.len() <= 128
        && conn_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

/// Terminates a single Sing-box connection via Clash API `DELETE /connections/<id>`.
pub async fn drop_singbox_connection(
    client: &reqwest::Client,
    clash_api_addr: &str,
    secret: Option<&str>,
    conn_id: &str,
) -> Result<(), String> {
    if !is_valid_clash_conn_id(conn_id) {
        return Err("Invalid Sing-box connection ID".into());
    }
    let addr = clash_api_addr.trim().trim_start_matches("http://");
    let url = format!("http://{addr}/connections/{conn_id}");
    let mut req = client.delete(&url).timeout(std::time::Duration::from_secs(2));
    if let Some(s) = secret.map(str::trim).filter(|s| !s.is_empty()) {
        req = req.bearer_auth(s);
    }
    let resp = req
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
/// Supported shapes (keys matched case-insensitively):
/// - array forms: `{"users": [...]}` / `{"metrics": [...]}` / `{"userMetrics": [...]}` / `[...]`
///   with `name`/`user`/`username`/`email` identifying the user;
/// - map forms: `{"<user>": {"uploadBytes": 100, "downloadBytes": 200}}` (top level) and
///   `{"users": {"<user>": {"UploadBytes": 100, "DownloadBytes": 200}}}` (mita 3.39+).
pub fn parse_mieru_metrics_json(json_str: &str) -> Result<HashMap<String, TrafficCounter>, String> {
    let val: Value =
        serde_json::from_str(json_str).map_err(|e| format!("Invalid Mieru metrics JSON: {e}"))?;
    let mut out: HashMap<String, TrafficCounter> = HashMap::new();

    let array_opt = val.as_array().or_else(|| {
        ["users", "metrics", "userMetrics"]
            .iter()
            .find_map(|k| json_get_ci(&val, k).and_then(Value::as_array))
    });

    if let Some(arr) = array_opt {
        for item in arr {
            let Some(name) = ["name", "user", "username", "email"]
                .iter()
                .find_map(|k| json_get_ci(item, k))
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
        // mita 3.39+ emits {"users": {"<client-id>": {"UploadBytes": N, ...}}}.
        if let Some(users) = ["users", "metrics", "userMetrics"]
            .iter()
            .find_map(|k| json_get_ci(&val, k))
            .and_then(Value::as_object)
        {
            for (name, item) in users {
                record_mieru_entry(&mut out, name, item);
            }
            return Ok(out);
        }
        for (name, item) in obj {
            if !item.is_object() {
                continue;
            }
            record_mieru_entry(&mut out, name, item);
        }
    }

    Ok(out)
}

/// Case-insensitive object key lookup (`UploadBytes` / `uploadBytes` / ...).
fn json_get_ci<'a>(val: &'a Value, key: &str) -> Option<&'a Value> {
    val.as_object()?
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v)
}

fn record_mieru_entry(out: &mut HashMap<String, TrafficCounter>, name: &str, item: &Value) {
    let clean = name.trim();
    if clean.is_empty() {
        return;
    }
    let (up, down) = extract_up_down(item);
    let entry = out
        .entry(clean.to_string())
        .or_insert_with(|| TrafficCounter {
            upload: 0,
            download: 0,
            unmetered: false,
        });
    entry.upload = entry.upload.saturating_add(up);
    entry.download = entry.download.saturating_add(down);
}

fn extract_up_down(item: &Value) -> (u64, u64) {
    let up = ["uploadBytes", "upload", "uplink", "tx"]
        .iter()
        .find_map(|k| json_get_ci(item, k))
        .and_then(json_u64)
        .unwrap_or(0);
    let down = ["downloadBytes", "download", "downlink", "rx"]
        .iter()
        .find_map(|k| json_get_ci(item, k))
        .and_then(json_u64)
        .unwrap_or(0);
    (up, down)
}

fn json_u64(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse::<u64>().ok()))
}

/// Persisted reader position, so an agent restart or a log rotation neither re-counts nor drops
/// traffic: `totals` stays a monotonic cumulative that the traffic tracker can baseline against.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct NaiveReaderState {
    #[serde(default)]
    inode: Option<u64>,
    #[serde(default)]
    offset: u64,
    #[serde(default)]
    totals: HashMap<String, TrafficCounter>,
}

/// Incremental Caddy JSON access log reader for NaiveProxy (`<CONFIG_DIR>/naive-access.log`).
#[derive(Debug, Default)]
pub struct NaiveAccessLogReader {
    offset: u64,
    inode: Option<u64>,
    totals: HashMap<String, TrafficCounter>,
    state_path: Option<PathBuf>,
    state_loaded: bool,
}

#[cfg(unix)]
fn inode_of(meta: &std::fs::Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    Some(meta.ino())
}

#[cfg(not(unix))]
fn inode_of(_meta: &std::fs::Metadata) -> Option<u64> {
    None
}

impl NaiveAccessLogReader {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reader that persists its position and totals to `state_path` (loaded lazily on first poll).
    pub fn with_state_file(state_path: impl AsRef<Path>) -> Self {
        Self {
            state_path: Some(state_path.as_ref().to_path_buf()),
            ..Self::default()
        }
    }

    async fn load_state(&mut self) {
        if self.state_loaded {
            return;
        }
        self.state_loaded = true;
        let Some(path) = &self.state_path else { return };
        let Ok(raw) = tokio::fs::read_to_string(path).await else { return };
        if let Ok(st) = serde_json::from_str::<NaiveReaderState>(&raw) {
            self.inode = st.inode;
            self.offset = st.offset;
            self.totals = st.totals;
        }
    }

    async fn save_state(&self) {
        let Some(path) = &self.state_path else { return };
        let st = NaiveReaderState {
            inode: self.inode,
            offset: self.offset,
            totals: self.totals.clone(),
        };
        if let Ok(bytes) = serde_json::to_vec(&st) {
            if let Err(e) = write_atomic_0600(path, &bytes).await {
                tracing::warn!(error = %e, "failed to persist naive reader state");
            }
        }
    }

    /// Finds the rotated sibling of `log_path` that still has inode `old_inode` (Caddy renames
    /// the file on rotation, so the inode survives; compressed `.gz` copies are not readable).
    async fn find_rotated(log_path: &Path, old_inode: u64) -> Option<PathBuf> {
        let dir = log_path.parent()?;
        let stem = log_path.file_stem()?.to_string_lossy().to_string();
        let mut rd = tokio::fs::read_dir(dir).await.ok()?;
        while let Ok(Some(entry)) = rd.next_entry().await {
            let path = entry.path();
            if path == log_path {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with(&stem) || name.ends_with(".gz") {
                continue;
            }
            if let Ok(meta) = entry.metadata().await {
                if inode_of(&meta) == Some(old_inode) {
                    return Some(path);
                }
            }
        }
        None
    }

    /// Ingests complete lines of `path` from `from`, returning the new offset. A trailing
    /// partial line (Caddy mid-write) is left for the next poll instead of being parsed half.
    async fn read_from(&mut self, path: &Path, from: u64) -> u64 {
        let Ok(mut file) = tokio::fs::File::open(path).await else {
            return from;
        };
        if file.seek(std::io::SeekFrom::Start(from)).await.is_err() {
            return from;
        }
        let mut reader = BufReader::new(file);
        let mut offset = from;
        let mut raw_buf: Vec<u8> = Vec::new();
        loop {
            raw_buf.clear();
            match reader.read_until(b'\n', &mut raw_buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if !raw_buf.ends_with(b"\n") {
                        break;
                    }
                    offset = offset.saturating_add(n as u64);
                    let line = String::from_utf8_lossy(&raw_buf);
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        self.ingest_line(trimmed);
                    }
                }
                Err(_) => break,
            }
        }
        offset
    }

    /// Reads any newly appended JSON lines from `log_path` since the last offset and returns
    /// cumulative per-user counters (`unmetered: false`). Rotation is detected by inode change;
    /// the unread tail of the rotated file is consumed first so no bytes are lost.
    pub async fn poll(&mut self, log_path: &Path) -> HashMap<String, TrafficCounter> {
        self.load_state().await;
        let Ok(meta) = tokio::fs::metadata(log_path).await else {
            return self.totals.clone();
        };
        let len = meta.len();
        let ino = inode_of(&meta);
        let before = (self.offset, self.inode, self.totals.len());

        let rotated = matches!((self.inode, ino), (Some(old), Some(new)) if old != new);
        if rotated {
            let old_inode = self.inode.unwrap_or_default();
            match Self::find_rotated(log_path, old_inode).await {
                Some(old_path) => {
                    let from = self.offset;
                    self.read_from(&old_path, from).await;
                }
                None => tracing::warn!(
                    "naive access log rotated and the old file is gone or compressed; \
                     its unread tail could not be counted"
                ),
            }
            self.offset = 0;
        } else if len < self.offset {
            // Truncated in place.
            self.offset = 0;
        }
        self.inode = ino;

        if len > self.offset {
            let from = self.offset;
            self.offset = self.read_from(log_path, from).await;
        }

        let after_len = self.totals.len();
        if before.0 != self.offset || before.1 != self.inode || before.2 != after_len || rotated {
            self.save_state().await;
        } else if self.state_path.is_some() {
            // Totals may have changed without the offset moving only on rotation; cheap enough
            // to skip here.
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
    #[serde(default)]
    batch_cores: HashMap<String, String>,
    /// Cumulative counter baselines the deltas were computed against. Persisted so an agent
    /// restart does not re-count the full mieru/xray cumulative as a fresh delta.
    #[serde(default)]
    baselines: HashMap<String, (u64, u64)>,
}

/// Tracks cumulative per-user counters across all 4 proxy cores (`unmetered: false`),
/// computes deltas with reset detection, and durably persists unacknowledged batches to
/// `<CONFIG_DIR>/traffic-pending.json`.
#[derive(Debug)]
pub struct CumulativeTrafficTracker {
    previous: HashMap<String, (u64, u64)>,
    pending_batches: HashMap<String, HashMap<String, TrafficCounter>>,
    pending_batch_cores: HashMap<String, String>,
    /// Batch ids produced by the last compute beyond the first `MAX_TRAFFIC_ENTRIES` entries.
    overflow: Vec<String>,
    pending_path: PathBuf,
}

impl CumulativeTrafficTracker {
    pub fn new(pending_path: impl AsRef<Path>) -> Self {
        Self {
            previous: HashMap::new(),
            pending_batches: HashMap::new(),
            pending_batch_cores: HashMap::new(),
            overflow: Vec::new(),
            pending_path: pending_path.as_ref().to_path_buf(),
        }
    }

    /// Returns the core name associated with a pending `batch_id`, if known.
    pub fn batch_core(&self, batch_id: &str) -> Option<&str> {
        self.pending_batch_cores.get(batch_id).map(String::as_str)
    }

    /// Loads any unacknowledged traffic batches and the counter baselines from
    /// `<CONFIG_DIR>/traffic-pending.json`. Legacy entries keyed by plain user id (merged
    /// across cores) are dropped so persistent cores get re-seeded instead of miscounted.
    pub async fn load_pending(&mut self) -> Result<(), String> {
        let Ok(raw) = tokio::fs::read_to_string(&self.pending_path).await else {
            return Ok(());
        };
        let parsed: PendingTrafficFile = serde_json::from_str(&raw)
            .map_err(|e| format!("Failed to parse {}: {e}", self.pending_path.display()))?;
        self.pending_batches = parsed.batches;
        self.pending_batch_cores = parsed.batch_cores;
        self.previous = parsed
            .baselines
            .into_iter()
            .filter(|(k, _)| k.contains(CORE_KEY_SEP))
            .collect();
        Ok(())
    }

    /// Whether the tracker has already established baselines for `core`.
    fn core_known(&self, core: &str) -> bool {
        self.previous
            .keys()
            .any(|k| k.split(CORE_KEY_SEP).next() == Some(core))
    }

    /// Computes per-core deltas from `per_core` cumulative counters, advances the baselines,
    /// records an unacknowledged batch in `traffic-pending.json`, and returns a [`TrafficReport`].
    ///
    /// Baselines are tracked per `(core, user)` so cores with different lifecycles stay
    /// independent: a volatile core resetting its counters (xray restart) can never mask or
    /// double-count a persistent core's cumulative (mita metrics.pb, naive access log).
    /// The first sighting of a persistent core (`PERSISTENT_CORES`) is silently seeded (no delta)
    /// so totals accumulated before an agent deploy or a parser fix are not re-counted. Volatile
    /// cores (xray, sing-box) count from zero, as their counters restart with the agent.
    pub async fn compute_delta_multi(
        &mut self,
        per_core: &[(&str, &HashMap<String, TrafficCounter>)],
    ) -> Result<TrafficReport, String> {
        // Transactional: if validation or persisting fails the baselines are rolled back, so the
        // delta is recomputed on the next tick instead of being lost.
        let backup = self.previous.clone();
        let batches_before: HashSet<String> = self.pending_batches.keys().cloned().collect();
        match self.compute_delta_multi_inner(per_core).await {
            Ok(report) => Ok(report),
            Err(err) => {
                self.previous = backup;
                self.pending_batches.retain(|k, _| batches_before.contains(k));
                self.pending_batch_cores.retain(|k, _| batches_before.contains(k));
                self.overflow.clear();
                Err(err)
            }
        }
    }

    /// Reports for batches that did not fit into the report returned by the last
    /// [`Self::compute_delta_multi`] call (more than `MAX_TRAFFIC_ENTRIES` active users).
    pub fn take_overflow_reports(&mut self) -> Vec<TrafficReport> {
        std::mem::take(&mut self.overflow)
            .into_iter()
            .filter_map(|id| {
                self.pending_batches.get(&id).map(|stats| TrafficReport {
                    batch_id: Some(id.clone()),
                    stats: stats.clone(),
                    key_meta: None,
                })
            })
            .collect()
    }

    async fn compute_delta_multi_inner(
        &mut self,
        per_core: &[(&str, &HashMap<String, TrafficCounter>)],
    ) -> Result<TrafficReport, String> {
        self.overflow.clear();
        let mut per_core_deltas: Vec<(&str, HashMap<String, TrafficCounter>)> = Vec::new();
        let mut baseline_changed = false;

        for (core, users) in per_core {
            let seed_core = core_is_persistent(core) && !self.core_known(core);
            let mut core_delta: HashMap<String, TrafficCounter> = HashMap::new();
            for (id, cur) in users.iter() {
                let key = core_key(core, id);
                let (prev_up, prev_down) = self.previous.get(&key).copied().unwrap_or((0, 0));
                let (upload, download) = if seed_core {
                    // First sighting of a persistent core: seed its cumulative silently.
                    (0, 0)
                } else {
                    // Detect upload and download resets independently so a reset on one
                    // counter does not double-count the other direction.
                    let up = if cur.upload >= prev_up {
                        cur.upload - prev_up
                    } else {
                        cur.upload
                    };
                    let down = if cur.download >= prev_down {
                        cur.download - prev_down
                    } else {
                        cur.download
                    };
                    (up, down)
                };

                if prev_up != cur.upload || prev_down != cur.download {
                    baseline_changed = true;
                }
                if upload > 0 || download > 0 {
                    let entry = core_delta
                        .entry(id.clone())
                        .or_insert_with(|| TrafficCounter {
                            upload: 0,
                            download: 0,
                            unmetered: false,
                        });
                    entry.upload = entry.upload.saturating_add(upload);
                    entry.download = entry.download.saturating_add(download);
                    entry.unmetered = false;
                }
                self.previous.insert(key, (cur.upload, cur.download));
            }
            if !core_delta.is_empty() {
                per_core_deltas.push((*core, core_delta));
            }
        }

        if self.previous.len() > MAX_TRACKER_BASELINE_ENTRIES {
            let current_keys: HashSet<String> = per_core
                .iter()
                .flat_map(|(core, users)| users.keys().map(move |id| core_key(core, id)))
                .collect();
            self.previous.retain(|k, _| {
                if current_keys.contains(k) {
                    return true;
                }
                // Preserve persistent core baselines so a temporary poll timeout never forces re-seeding/double-count.
                k.split_once(CORE_KEY_SEP)
                    .is_some_and(|(core, _)| core_is_persistent(core))
            });
            baseline_changed = true;
        }

        if per_core_deltas.is_empty() {
            // No new traffic, but the baseline may still have moved; keep it durable so a
            // restart cannot re-count the last observed cumulative as a fresh delta.
            if baseline_changed {
                self.save_pending().await?;
            }
            return Ok(TrafficReport {
                batch_id: None,
                stats: HashMap::new(),
                key_meta: None,
            });
        }

        // Emit separate batches per core (and chunk by MAX_TRAFFIC_ENTRIES) so per-batch
        // `(protocol, inboundTag)` metadata stays accurate when a user runs multiple cores.
        let mut first: Option<TrafficReport> = None;
        for (core, core_delta) in per_core_deltas {
            let mut entries: Vec<(String, TrafficCounter)> = core_delta.into_iter().collect();
            while !entries.is_empty() {
                let take = entries.len().min(MAX_TRAFFIC_ENTRIES);
                let chunk: HashMap<String, TrafficCounter> = entries.drain(..take).collect();
                let batch_id = uuid::Uuid::new_v4().to_string();
                let report = TrafficReport {
                    batch_id: Some(batch_id.clone()),
                    stats: chunk.clone(),
                    key_meta: None,
                };
                report
                    .validate()
                    .map_err(|e| format!("TrafficReport validation failed: {e}"))?;
                self.pending_batches.insert(batch_id.clone(), chunk);
                self.pending_batch_cores
                    .insert(batch_id.clone(), core.to_string());
                if first.is_none() {
                    first = Some(report);
                } else {
                    self.overflow.push(batch_id);
                }
            }
        }
        self.save_pending().await?;

        first.ok_or_else(|| "no traffic batch produced".to_string())
    }

    /// Returns all currently unacknowledged pending batches to be replayed after reconnect.
    pub fn pending_reports(&self) -> Vec<TrafficReport> {
        self.pending_batches
            .iter()
            .map(|(batch_id, stats)| TrafficReport {
                batch_id: Some(batch_id.clone()),
                stats: stats.clone(),
                key_meta: None,
            })
            .collect()
    }

    /// Up to `limit` unacknowledged batches (excluding `skip`) for periodic re-delivery in WS
    /// mode, so a lost ack does not leave batches stuck until the next reconnect. The panel
    /// deduplicates by batch id, so re-sending is safe.
    pub fn pending_replay(&self, limit: usize, skip: &HashSet<String>) -> Vec<TrafficReport> {
        self.pending_batches
            .iter()
            .filter(|(id, _)| !skip.contains(*id))
            .take(limit)
            .map(|(batch_id, stats)| TrafficReport {
                batch_id: Some(batch_id.clone()),
                stats: stats.clone(),
                key_meta: None,
            })
            .collect()
    }

    /// Number of unacknowledged batches.
    pub fn pending_len(&self) -> usize {
        self.pending_batches.len()
    }

    /// Removes an acknowledged batch and updates `<CONFIG_DIR>/traffic-pending.json`.
    pub async fn acknowledge_batch(&mut self, batch_id: &str) -> Result<bool, String> {
        self.pending_batch_cores.remove(batch_id);
        let removed = self.pending_batches.remove(batch_id).is_some();
        if removed {
            self.save_pending().await?;
        }
        Ok(removed)
    }

    /// Removes a list of acknowledged batches with a single atomic persistence flush.
    pub async fn acknowledge_batches(&mut self, batch_ids: &[String]) -> Result<usize, String> {
        let mut removed = 0usize;
        for id in batch_ids {
            self.pending_batch_cores.remove(id);
            if self.pending_batches.remove(id).is_some() {
                removed += 1;
            }
        }
        if removed > 0 {
            self.save_pending().await?;
        }
        Ok(removed)
    }

    async fn save_pending(&self) -> Result<(), String> {
        let file = PendingTrafficFile {
            batches: self.pending_batches.clone(),
            batch_cores: self.pending_batch_cores.clone(),
            baselines: self.previous.clone(),
        };
        // Compact JSON: this file is rewritten every minute and can hold many batches.
        let bytes = serde_json::to_vec(&file)
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
        merge_into(&mut merged, m);
    }
    merged
}

/// Merges labeled per-core maps (values summed per user across cores).
pub fn merge_traffic_maps_refs(
    maps: &[(&str, &HashMap<String, TrafficCounter>)],
) -> HashMap<String, TrafficCounter> {
    let mut merged: HashMap<String, TrafficCounter> = HashMap::new();
    for (_, m) in maps {
        merge_into(&mut merged, m);
    }
    merged
}

pub(crate) fn merge_into(
    merged: &mut HashMap<String, TrafficCounter>,
    m: &HashMap<String, TrafficCounter>,
) {
    for (k, v) in m {
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

#[cfg(test)]
mod tests {
    use super::*;

    type CoreMaps = Vec<(&'static str, HashMap<String, TrafficCounter>)>;

    fn counter(upload: u64, download: u64) -> TrafficCounter {
        TrafficCounter {
            upload,
            download,
            unmetered: false,
        }
    }

    fn stats(entries: &[(&str, u64, u64)]) -> HashMap<String, TrafficCounter> {
        entries
            .iter()
            .map(|(k, up, down)| (k.to_string(), counter(*up, *down)))
            .collect()
    }

    /// Builds per-core cumulative snapshots: `&[("xray", &[("velio", 100, 200)]), ...]`.
    fn per_core(cores: &[(&'static str, &[(&str, u64, u64)])]) -> CoreMaps {
        cores
            .iter()
            .map(|(core, users)| (*core, stats(users)))
            .collect()
    }

    fn as_ref_maps(per_core: &CoreMaps) -> Vec<(&'static str, &HashMap<String, TrafficCounter>)> {
        per_core.iter().map(|(c, m)| (*c, m)).collect()
    }

    fn pending_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "mypanel-agent-tracker-test-{tag}-{}.json",
            uuid::Uuid::new_v4()
        ))
    }

    #[tokio::test]
    async fn first_sight_seeds_persistent_core_without_recount() {
        let path = pending_path("seed-first");
        let mut t = CumulativeTrafficTracker::new(&path);
        // Deploy scenario: mita metrics.pb already holds 6.6 GB; the tracker sees it for the
        // first time and must seed the baseline instead of reporting it as fresh traffic.
        let cur = per_core(&[("mieru", &[("velio", 6_600_000_000, 4_000_000_000)])]);
        let report = t
            .compute_delta_multi(&as_ref_maps(&cur))
            .await
            .expect("first delta");
        assert!(
            report.stats.is_empty(),
            "persistent cumulative must be seeded, not counted"
        );
        assert!(report.batch_id.is_none());

        // Growth after seeding is counted normally.
        let grown = per_core(&[("mieru", &[("velio", 6_600_000_050, 4_000_000_100)])]);
        let delta = t.compute_delta_multi(&as_ref_maps(&grown)).await.unwrap();
        assert_eq!(delta.stats["velio"].upload, 50);
        assert_eq!(delta.stats["velio"].download, 100);
        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn volatile_core_traffic_before_first_poll_is_counted() {
        let path = pending_path("volatile-first");
        let mut t = CumulativeTrafficTracker::new(&path);
        // Xray restarts together with the agent, so whatever it already counted when we first
        // see a user is new traffic. Seeding it would lose that traffic for good.
        let cur = per_core(&[
            ("xray", &[("velio", 5_000, 9_000)]),
            ("singbox", &[("velio", 70, 80)]),
        ]);
        let report = t.compute_delta_multi(&as_ref_maps(&cur)).await.unwrap();
        let extra = t.take_overflow_reports();
        assert_eq!(extra.len(), 1, "each core emits its own batch");
        assert_eq!(t.batch_core(report.batch_id.as_deref().unwrap()), Some("xray"));
        assert_eq!(t.batch_core(extra[0].batch_id.as_deref().unwrap()), Some("singbox"));
        let total_up = report.stats["velio"].upload + extra[0].stats["velio"].upload;
        let total_down = report.stats["velio"].download + extra[0].stats["velio"].download;
        assert_eq!(total_up, 5_070);
        assert_eq!(total_down, 9_080);
        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn restart_counts_volatile_core_but_seeds_persistent_core() {
        let path = pending_path("restart-mixed");
        let mut t1 = CumulativeTrafficTracker::new(&path);
        // Normal operation before the restart: only mieru is known (xray was idle).
        let before = per_core(&[("mieru", &[("velio", 1_000, 2_000)])]);
        t1.compute_delta_multi(&as_ref_maps(&before)).await.unwrap();

        // Agent restart: baselines are reloaded, xray shows up for the first time with traffic
        // that flowed before the first poll, mieru kept its persistent cumulative.
        let mut t2 = CumulativeTrafficTracker::new(&path);
        t2.load_pending().await.unwrap();
        let after = per_core(&[
            ("mieru", &[("velio", 1_000, 2_000)]),
            ("xray", &[("velio", 300, 400)]),
        ]);
        let report = t2.compute_delta_multi(&as_ref_maps(&after)).await.unwrap();
        assert_eq!(report.stats["velio"].upload, 300, "xray counted, mieru not re-counted");
        assert_eq!(report.stats["velio"].download, 400);
        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn baseline_survives_tracker_restart_without_double_count() {
        let path = pending_path("baseline-survives");
        let mut cur = per_core(&[("mieru", &[("velio", 100, 200)])]);
        let mut t1 = CumulativeTrafficTracker::new(&path);
        let seeded = t1.compute_delta_multi(&as_ref_maps(&cur)).await.unwrap();
        assert!(seeded.stats.is_empty(), "first sight must seed");
        cur[0].1.get_mut("velio").unwrap().upload = 150;
        cur[0].1.get_mut("velio").unwrap().download = 250;
        let first = t1.compute_delta_multi(&as_ref_maps(&cur)).await.unwrap();
        assert_eq!(first.stats["velio"].upload, 50);
        assert_eq!(first.stats["velio"].download, 50);

        // Simulate an agent restart: mita counters persist on disk, the tracker does not.
        let mut t2 = CumulativeTrafficTracker::new(&path);
        t2.load_pending().await.expect("load pending");
        cur[0].1.get_mut("velio").unwrap().upload = 200;
        cur[0].1.get_mut("velio").unwrap().download = 300;
        let second = t2.compute_delta_multi(&as_ref_maps(&cur)).await.unwrap();
        assert_eq!(second.stats["velio"].upload, 50, "must not re-count baseline");
        assert_eq!(second.stats["velio"].download, 50);
        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn counter_reset_after_restart_counts_from_new_zero() {
        let path = pending_path("counter-reset");
        let mut t1 = CumulativeTrafficTracker::new(&path);
        t1.compute_delta_multi(&as_ref_maps(&per_core(&[(
            "xray",
            &[("velio", 100, 200)],
        )])))
        .await
        .unwrap();

        let mut t2 = CumulativeTrafficTracker::new(&path);
        t2.load_pending().await.unwrap();
        // Xray restarted and lost its counters: current < restored baseline.
        let after_reset = per_core(&[("xray", &[("velio", 30, 40)])]);
        let report = t2.compute_delta_multi(&as_ref_maps(&after_reset)).await.unwrap();
        assert_eq!(report.stats["velio"].upload, 30);
        assert_eq!(report.stats["velio"].download, 40);
        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn mixed_core_lifecycles_do_not_double_count() {
        let path = pending_path("mixed-cores");
        let mut t = CumulativeTrafficTracker::new(&path);
        // Seed both cores.
        let seed = per_core(&[
            ("xray", &[("velio", 100, 200)]),
            ("mieru", &[("velio", 1000, 2000)]),
        ]);
        t.compute_delta_multi(&as_ref_maps(&seed)).await.unwrap();

        // Both cores report growth.
        let grown = per_core(&[
            ("xray", &[("velio", 110, 210)]),
            ("mieru", &[("velio", 1000, 2000)]),
        ]);
        let report = t.compute_delta_multi(&as_ref_maps(&grown)).await.unwrap();
        assert_eq!(report.stats["velio"].upload, 10);
        assert_eq!(report.stats["velio"].download, 10);

        // Xray restarts (counters reset) while persistent mieru keeps growing: only the new
        // bytes may be counted, never mieru's whole cumulative again.
        let mixed = per_core(&[
            ("xray", &[("velio", 5, 8)]),
            ("mieru", &[("velio", 1200, 2600)]),
        ]);
        let report = t.compute_delta_multi(&as_ref_maps(&mixed)).await.unwrap();
        let extra = t.take_overflow_reports();
        assert_eq!(extra.len(), 1);
        assert_eq!(t.batch_core(report.batch_id.as_deref().unwrap()), Some("xray"));
        assert_eq!(t.batch_core(extra[0].batch_id.as_deref().unwrap()), Some("mieru"));
        let total_up = report.stats["velio"].upload + extra[0].stats["velio"].upload;
        let total_down = report.stats["velio"].download + extra[0].stats["velio"].download;
        assert_eq!(total_up, 5 + 200, "xray reset + mieru delta");
        assert_eq!(total_down, 8 + 600);
        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn late_core_is_seeded_without_recounting_persistent_totals() {
        let path = pending_path("late-core");
        let mut t1 = CumulativeTrafficTracker::new(&path);
        let only_xray = per_core(&[("xray", &[("velio", 100, 200)])]);
        t1.compute_delta_multi(&as_ref_maps(&only_xray)).await.unwrap();
        let grown = per_core(&[("xray", &[("velio", 150, 250)])]);
        let delta = t1.compute_delta_multi(&as_ref_maps(&grown)).await.unwrap();
        assert_eq!(delta.stats["velio"].upload, 50);

        // Agent restart; afterwards the Mieru parser fix lands and mieru shows up for the
        // first time with a large persistent cumulative. It must be seeded, not reported.
        let mut t2 = CumulativeTrafficTracker::new(&path);
        t2.load_pending().await.unwrap();
        let with_mieru = per_core(&[
            ("xray", &[("velio", 160, 260)]),
            ("mieru", &[("velio", 6_600_000_000, 4_000_000_000)]),
        ]);
        let report = t2.compute_delta_multi(&as_ref_maps(&with_mieru)).await.unwrap();
        assert_eq!(report.stats["velio"].upload, 10, "only xray growth; mieru seeded");
        assert_eq!(report.stats["velio"].download, 10);

        // Mieru growth after seeding is counted normally.
        let mieru_grown = per_core(&[
            ("xray", &[("velio", 160, 260)]),
            ("mieru", &[("velio", 6_600_000_070, 4_000_000_100)]),
        ]);
        let next = t2.compute_delta_multi(&as_ref_maps(&mieru_grown)).await.unwrap();
        assert_eq!(next.stats["velio"].upload, 70);
        assert_eq!(next.stats["velio"].download, 100);
        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn load_pending_accepts_legacy_file_without_baselines() {
        let path = pending_path("legacy");
        tokio::fs::write(&path, r#"{"batches": {}}"#)
            .await
            .expect("write legacy file");
        let mut t = CumulativeTrafficTracker::new(&path);
        t.load_pending().await.expect("legacy file must load");
        let seed = t
            .compute_delta_multi(&as_ref_maps(&per_core(&[("mieru", &[("u", 7, 9)])])))
            .await
            .unwrap();
        assert!(seed.stats.is_empty(), "legacy file has no per-core baselines: seed");
        let delta = t
            .compute_delta_multi(&as_ref_maps(&per_core(&[("mieru", &[("u", 17, 19)])])))
            .await
            .unwrap();
        assert_eq!(delta.stats["u"].upload, 10);
        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn legacy_merged_baselines_are_ignored_and_seeded() {
        let path = pending_path("legacy-merged");
        // Pre-fix pending file: baselines keyed by plain user id (merged across cores).
        tokio::fs::write(
            &path,
            r#"{"batches": {}, "baselines": {"velio": [474297882, 4162145186]}}"#,
        )
        .await
        .expect("write legacy file");
        let mut t = CumulativeTrafficTracker::new(&path);
        t.load_pending().await.expect("load legacy baselines");
        // Merged baselines cannot be attributed to a core: seed silently instead of
        // misinterpreting the cumulative as a delta.
        let cur = per_core(&[("mieru", &[("velio", 474_297_882, 4_162_145_186)])]);
        let report = t.compute_delta_multi(&as_ref_maps(&cur)).await.unwrap();
        assert!(report.stats.is_empty(), "merged legacy baselines must not explode into a delta");
        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn baseline_only_change_is_persisted_without_batch() {
        let path = pending_path("baseline-only");
        let mut t1 = CumulativeTrafficTracker::new(&path);
        let cur = per_core(&[("xray", &[("u", 10, 20)])]);
        t1.compute_delta_multi(&as_ref_maps(&cur)).await.unwrap();
        // Same counters again: empty delta, baseline already persisted.
        let again = t1
            .compute_delta_multi(&as_ref_maps(&per_core(&[("xray", &[("u", 10, 20)])])))
            .await
            .unwrap();
        assert!(again.batch_id.is_none());

        let mut t2 = CumulativeTrafficTracker::new(&path);
        t2.load_pending().await.unwrap();
        let grown = per_core(&[("xray", &[("u", 35, 45)])]);
        let delta = t2.compute_delta_multi(&as_ref_maps(&grown)).await.unwrap();
        assert_eq!(delta.stats["u"].upload, 25);
        assert_eq!(delta.stats["u"].download, 25);
        let _ = tokio::fs::remove_file(&path).await;
    }

    #[test]
    fn parse_mieru_users_object_pascal_case() {
        let raw = r#"{"users": {"b5120507-c984-47af-ba08-2da90b2f78a2": {"UploadBytes": 1133610314, "DownloadBytes": 6662479713}, "": {"UploadBytes": 5, "DownloadBytes": 5}}}"#;
        let parsed = parse_mieru_metrics_json(raw).expect("parse mieru metrics");
        assert_eq!(parsed.len(), 1, "empty user names must be skipped");
        let c = &parsed["b5120507-c984-47af-ba08-2da90b2f78a2"];
        assert_eq!(c.upload, 1_133_610_314);
        assert_eq!(c.download, 6_662_479_713);
        assert!(!c.unmetered);
    }

    #[test]
    fn parse_mieru_legacy_formats_still_parse() {
        // Array form.
        let arr = parse_mieru_metrics_json(
            r#"{"users": [{"name": "alice", "uploadBytes": 1, "downloadBytes": 2}]}"#,
        )
        .unwrap();
        assert_eq!(arr["alice"].upload, 1);
        assert_eq!(arr["alice"].download, 2);

        // Bare top-level map form.
        let map = parse_mieru_metrics_json(
            r#"{"alice": {"uploadBytes": 3, "downloadBytes": 4}}"#,
        )
        .unwrap();
        assert_eq!(map["alice"].upload, 3);
        assert_eq!(map["alice"].download, 4);

        // Lowercase users-object form.
        let lower = parse_mieru_metrics_json(
            r#"{"users": {"bob": {"uploadbytes": 5, "downloadbytes": 6}}}"#,
        )
        .unwrap();
        assert_eq!(lower["bob"].upload, 5);
        assert_eq!(lower["bob"].download, 6);
    }

    #[tokio::test]
    async fn more_than_max_entries_is_split_not_truncated() {
        let mut t = CumulativeTrafficTracker::new(pending_path("split"));
        let total = MAX_TRAFFIC_ENTRIES + 123;
        let users: HashMap<String, TrafficCounter> =
            (0..total).map(|i| (format!("u{i}"), counter(1, 2))).collect();
        let cur: CoreMaps = vec![("xray", users)];
        let first = t.compute_delta_multi(&as_ref_maps(&cur)).await.unwrap();
        let extra = t.take_overflow_reports();
        let sent: usize = first.stats.len() + extra.iter().map(|r| r.stats.len()).sum::<usize>();
        assert_eq!(sent, total);
        assert_eq!(first.stats.len(), MAX_TRAFFIC_ENTRIES);
        assert_eq!(t.pending_len(), 2);
        assert!(t.take_overflow_reports().is_empty());
    }

    #[tokio::test]
    async fn failed_persist_rolls_back_baselines() {
        let dir = std::env::temp_dir().join(format!("mypanel-nodir-{}", uuid::Uuid::new_v4()));
        // Parent is a regular file, so the atomic write must fail.
        std::fs::write(&dir, b"x").unwrap();
        let mut t = CumulativeTrafficTracker::new(dir.join("pending.json"));
        let cur = per_core(&[("xray", &[("velio", 10, 20)])]);
        assert!(t.compute_delta_multi(&as_ref_maps(&cur)).await.is_err());
        assert_eq!(t.pending_len(), 0);
        assert!(t.previous.is_empty());
        let _ = std::fs::remove_file(&dir);
    }

    #[tokio::test]
    async fn replay_skips_listed_batches_and_respects_limit() {
        let mut t = CumulativeTrafficTracker::new(pending_path("replay"));
        let a = per_core(&[("xray", &[("a", 1, 1)])]);
        let first = t.compute_delta_multi(&as_ref_maps(&a)).await.unwrap();
        let b = per_core(&[("xray", &[("a", 2, 2)])]);
        let second = t.compute_delta_multi(&as_ref_maps(&b)).await.unwrap();
        let skip: HashSet<String> = [second.batch_id.clone().unwrap()].into();
        let replay = t.pending_replay(10, &skip);
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].batch_id, first.batch_id);
        assert_eq!(t.pending_replay(1, &HashSet::new()).len(), 1);
    }

    fn naive_line(user: &str, up: u64, down: u64) -> String {
        format!("{{\"user_id\":\"{user}\",\"bytes_read\":{up},\"size\":{down}}}\n")
    }

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mypanel-naive-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[tokio::test]
    async fn naive_reader_resumes_after_restart_without_recounting() {
        let dir = tmp_dir("resume");
        let log = dir.join("naive-access.log");
        let state = dir.join("naive-reader.json");
        std::fs::write(&log, naive_line("a", 10, 100) + &naive_line("b", 1, 2)).unwrap();

        let mut r1 = NaiveAccessLogReader::with_state_file(&state);
        let t1 = r1.poll(&log).await;
        assert_eq!((t1["a"].upload, t1["a"].download), (10, 100));

        // "Agent restart": a new reader loads the persisted position and totals.
        let mut r2 = NaiveAccessLogReader::with_state_file(&state);
        let t2 = r2.poll(&log).await;
        assert_eq!((t2["a"].upload, t2["a"].download), (10, 100), "must not recount");

        let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
        std::io::Write::write_all(&mut f, naive_line("a", 5, 50).as_bytes()).unwrap();
        let t3 = r2.poll(&log).await;
        assert_eq!((t3["a"].upload, t3["a"].download), (15, 150));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn naive_reader_reads_tail_of_rotated_file_then_the_new_one() {
        let dir = tmp_dir("rotate");
        let log = dir.join("naive-access.log");
        std::fs::write(&log, naive_line("a", 10, 100)).unwrap();
        let mut r = NaiveAccessLogReader::with_state_file(dir.join("st.json"));
        r.poll(&log).await;

        // Unread line lands in the old file, then Caddy rotates and starts a fresh one.
        let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
        std::io::Write::write_all(&mut f, naive_line("a", 7, 70).as_bytes()).unwrap();
        drop(f);
        std::fs::rename(&log, dir.join("naive-access-2026-11-24T10-00-00.000.log")).unwrap();
        std::fs::write(&log, naive_line("a", 3, 30)).unwrap();

        let t = r.poll(&log).await;
        assert_eq!((t["a"].upload, t["a"].download), (20, 200));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn naive_reader_ignores_a_partial_trailing_line() {
        let dir = tmp_dir("partial");
        let log = dir.join("naive-access.log");
        let full = naive_line("a", 4, 40);
        let half = &naive_line("a", 9, 90)[..20];
        std::fs::write(&log, format!("{full}{half}")).unwrap();
        let mut r = NaiveAccessLogReader::new();
        let t = r.poll(&log).await;
        assert_eq!((t["a"].upload, t["a"].download), (4, 40));

        // The writer finishes the line; it is counted exactly once.
        std::fs::write(&log, format!("{full}{}", naive_line("a", 9, 90))).unwrap();
        let t = r.poll(&log).await;
        assert_eq!((t["a"].upload, t["a"].download), (13, 130));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn persistent_core_baseline_survives_eviction_over_max_entries() {
        let path = pending_path("eviction");
        let mut t = CumulativeTrafficTracker::new(&path);

        // Seed persistent mieru user.
        let mieru_seed = per_core(&[("mieru", &[("persist_u", 1_000, 2_000)])]);
        t.compute_delta_multi(&as_ref_maps(&mieru_seed)).await.unwrap();

        // Overflow MAX_TRACKER_BASELINE_ENTRIES using volatile xray users while mieru is omitted.
        let bulk: HashMap<String, TrafficCounter> = (0..=MAX_TRACKER_BASELINE_ENTRIES)
            .map(|i| (format!("u{i}"), counter(10, 20)))
            .collect();
        let cur: CoreMaps = vec![("xray", bulk)];
        let _ = t.compute_delta_multi(&as_ref_maps(&cur)).await.unwrap();
        let _ = t.take_overflow_reports();

        // Add one more volatile key in a new tick (with only that 1 key in per_core) to force
        // eviction check where persistent mieru key is NOT in current_keys.
        let single = per_core(&[("xray", &[("other_u", 5, 5)])]);
        let _ = t.compute_delta_multi(&as_ref_maps(&single)).await.unwrap();

        assert!(
            t.previous.contains_key(&core_key("mieru", "persist_u")),
            "persistent core baseline must survive eviction"
        );
        // When mieru returns with unchanged cumulative counter, it must not recount 1000/2000.
        let mieru_back = per_core(&[("mieru", &[("persist_u", 1_050, 2_100)])]);
        let delta = t.compute_delta_multi(&as_ref_maps(&mieru_back)).await.unwrap();
        assert_eq!(delta.stats["persist_u"].upload, 50);
        assert_eq!(delta.stats["persist_u"].download, 100);
        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn two_cores_produce_distinct_core_tagged_batches() {
        let path = pending_path("multi-core-batches");
        let mut t = CumulativeTrafficTracker::new(&path);
        let cur = per_core(&[
            ("xray", &[("shared_user", 100, 200)]),
            ("telemt", &[("shared_user", 300, 400)]),
        ]);
        let first = t.compute_delta_multi(&as_ref_maps(&cur)).await.unwrap();
        let extra = t.take_overflow_reports();
        assert_eq!(extra.len(), 1);
        let mut pairs = vec![
            (
                t.batch_core(first.batch_id.as_deref().unwrap()).unwrap().to_string(),
                first.stats["shared_user"].upload,
            ),
            (
                t.batch_core(extra[0].batch_id.as_deref().unwrap()).unwrap().to_string(),
                extra[0].stats["shared_user"].upload,
            ),
        ];
        pairs.sort();
        assert_eq!(
            pairs,
            vec![("telemt".to_string(), 300), ("xray".to_string(), 100)]
        );
        let _ = tokio::fs::remove_file(&path).await;
    }
}
