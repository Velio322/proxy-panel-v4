use dashmap::DashMap;
use mypanel_proto::{ActiveSession, CommandAck, Envelope, LogChunk};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use tokio::sync::{broadcast, mpsc, oneshot};

const MAX_LOG_RING: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredLogEntry {
    pub node_id: String,
    pub timestamp: String,
    pub core: String,
    pub level: String,
    pub stream: String,
    pub message: String,
}

#[derive(Debug)]
struct WorkerConn {
    conn_id: u64,
    tx: mpsc::UnboundedSender<Envelope>,
}

#[derive(Debug)]
pub struct WorkerHub {
    conns: DashMap<String, WorkerConn>,
    pending_acks: DashMap<String, oneshot::Sender<CommandAck>>,
    logs: DashMap<String, VecDeque<StoredLogEntry>>,
    sessions: DashMap<String, Vec<ActiveSession>>,
    /// Latest `StatusReport` per node, serialised as JSON (served as metrics without any HTTP call).
    last_status: DashMap<String, (std::time::Instant, serde_json::Value)>,
    log_tx: broadcast::Sender<StoredLogEntry>,
    next_conn_id: std::sync::atomic::AtomicU64,
    /// Cancelled when the panel is shutting down so sessions can send a proper Close frame.
    shutdown: tokio_util::sync::CancellationToken,
}

impl Default for WorkerHub {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkerHub {
    pub fn new() -> Self {
        let (log_tx, _) = broadcast::channel(1024);
        Self {
            conns: DashMap::new(),
            pending_acks: DashMap::new(),
            logs: DashMap::new(),
            sessions: DashMap::new(),
            last_status: DashMap::new(),
            log_tx,
            next_conn_id: std::sync::atomic::AtomicU64::new(1),
            shutdown: tokio_util::sync::CancellationToken::new(),
        }
    }

    pub fn register(&self, node_id: &str, tx: mpsc::UnboundedSender<Envelope>) -> u64 {
        let conn_id = self
            .next_conn_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.conns
            .insert(node_id.to_string(), WorkerConn { conn_id, tx });
        conn_id
    }

    pub fn unregister(&self, node_id: &str, conn_id: u64) -> bool {
        if let Some(existing) = self.conns.get(node_id) {
            if existing.conn_id == conn_id {
                drop(existing);
                self.conns.remove(node_id);
                return true;
            }
        }
        false
    }

    /// Remembers the newest status report pushed by a node over WebSocket.
    pub fn set_status(&self, node_id: &str, status: serde_json::Value) {
        self.last_status
            .insert(node_id.to_string(), (std::time::Instant::now(), status));
    }

    /// Last WebSocket status of `node_id` when it is younger than `max_age`.
    pub fn fresh_status(
        &self,
        node_id: &str,
        max_age: std::time::Duration,
    ) -> Option<serde_json::Value> {
        let e = self.last_status.get(node_id)?;
        (e.0.elapsed() <= max_age).then(|| e.1.clone())
    }

    pub fn shutdown_token(&self) -> tokio_util::sync::CancellationToken {
        self.shutdown.clone()
    }

    /// Tells every node session to send `Close(1001 going away)` and finish.
    pub fn begin_shutdown(&self) {
        self.shutdown.cancel();
    }

    pub fn is_connected(&self, node_id: &str) -> bool {
        self.conns.contains_key(node_id)
    }

    pub fn connected_node_ids(&self) -> Vec<String> {
        self.conns.iter().map(|e| e.key().clone()).collect()
    }

    pub fn send(&self, node_id: &str, msg: Envelope) -> bool {
        if let Some(conn) = self.conns.get(node_id) {
            return conn.tx.send(msg).is_ok();
        }
        false
    }

    pub fn resolve_ack(&self, ack: CommandAck) -> bool {
        if let Some((_, tx)) = self.pending_acks.remove(&ack.command_id) {
            let _ = tx.send(ack);
            return true;
        }
        false
    }

    pub async fn send_and_wait_ack(
        &self,
        node_id: &str,
        command_id: &str,
        msg: Envelope,
        timeout: std::time::Duration,
    ) -> Result<CommandAck, String> {
        let (tx, rx) = oneshot::channel();
        self.pending_acks.insert(command_id.to_string(), tx);
        if !self.send(node_id, msg) {
            self.pending_acks.remove(command_id);
            return Err("Node WebSocket is not connected".to_string());
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(ack)) => Ok(ack),
            Ok(Err(_)) => {
                self.pending_acks.remove(command_id);
                Err("Node command acknowledgement channel closed".to_string())
            }
            Err(_) => {
                self.pending_acks.remove(command_id);
                Err("Timed out waiting for node acknowledgement".to_string())
            }
        }
    }

    pub fn push_log_chunk(&self, node_id: &str, chunk: LogChunk) {
        let mut ring = self
            .logs
            .entry(node_id.to_string())
            .or_insert_with(|| VecDeque::with_capacity(MAX_LOG_RING));
        for entry in chunk.entries {
            let level = infer_log_level(&entry.line, &entry.stream);
            let stored = StoredLogEntry {
                node_id: node_id.to_string(),
                timestamp: entry.time,
                core: chunk.core.clone(),
                level,
                stream: entry.stream,
                message: entry.line,
            };
            if ring.len() >= MAX_LOG_RING {
                ring.pop_front();
            }
            ring.push_back(stored.clone());
            let _ = self.log_tx.send(stored);
        }
    }

    pub fn push_single_log(
        &self,
        node_id: &str,
        core: &str,
        stream: &str,
        line: impl Into<String>,
    ) {
        let msg = line.into();
        let level = infer_log_level(&msg, stream);
        let stored = StoredLogEntry {
            node_id: node_id.to_string(),
            timestamp: crate::db::ts::format_iso(&crate::db::now()),
            core: core.to_string(),
            level,
            stream: stream.to_string(),
            message: msg,
        };
        let mut ring = self
            .logs
            .entry(node_id.to_string())
            .or_insert_with(|| VecDeque::with_capacity(MAX_LOG_RING));
        if ring.len() >= MAX_LOG_RING {
            ring.pop_front();
        }
        ring.push_back(stored.clone());
        let _ = self.log_tx.send(stored);
    }

    pub fn get_logs(
        &self,
        node_id: Option<&str>,
        core: Option<&str>,
        stream: Option<&str>,
        limit: usize,
    ) -> Vec<StoredLogEntry> {
        let mut out = Vec::new();
        let max_items = limit.clamp(1, 500);
        if let Some(nid) = node_id {
            if let Some(ring) = self.logs.get(nid) {
                for item in ring.iter().rev() {
                    if core.is_some_and(|c| !c.is_empty() && !item.core.eq_ignore_ascii_case(c)) {
                        continue;
                    }
                    if stream
                        .is_some_and(|s| !s.is_empty() && !item.stream.eq_ignore_ascii_case(s))
                    {
                        continue;
                    }
                    out.push(item.clone());
                    if out.len() >= max_items {
                        break;
                    }
                }
            }
        } else {
            for entry in self.logs.iter() {
                for item in entry.value().iter().rev() {
                    if core.is_some_and(|c| !c.is_empty() && !item.core.eq_ignore_ascii_case(c)) {
                        continue;
                    }
                    if stream
                        .is_some_and(|s| !s.is_empty() && !item.stream.eq_ignore_ascii_case(s))
                    {
                        continue;
                    }
                    out.push(item.clone());
                    if out.len() >= max_items {
                        break;
                    }
                }
            }
        }
        out.reverse();
        out
    }

    pub fn subscribe_logs(&self) -> broadcast::Receiver<StoredLogEntry> {
        self.log_tx.subscribe()
    }

    pub fn update_sessions(&self, node_id: &str, sessions: Vec<ActiveSession>) {
        self.sessions.insert(node_id.to_string(), sessions);
    }

    pub fn get_sessions(&self, node_id: Option<&str>) -> Vec<(String, ActiveSession)> {
        let mut out = Vec::new();
        if let Some(nid) = node_id {
            if let Some(list) = self.sessions.get(nid) {
                for s in list.iter() {
                    out.push((nid.to_string(), s.clone()));
                }
            }
        } else {
            for entry in self.sessions.iter() {
                let nid = entry.key().clone();
                for s in entry.value().iter() {
                    out.push((nid.clone(), s.clone()));
                }
            }
        }
        out
    }

    pub fn drop_local_session(
        &self,
        node_id: &str,
        session_id: Option<&str>,
        client_id: Option<&str>,
        source_ip: Option<&str>,
    ) {
        if let Some(mut list) = self.sessions.get_mut(node_id) {
            list.retain(|s| {
                let match_sid = session_id.is_some_and(|sid| !sid.is_empty() && s.id == sid);
                let match_cid = client_id
                    .is_some_and(|cid| !cid.is_empty() && s.client_id.as_deref() == Some(cid));
                let match_ip =
                    source_ip.is_some_and(|ip| !ip.is_empty() && s.source_ip == ip);
                !(match_sid || match_cid || match_ip)
            });
        }
    }
}

fn infer_log_level(line: &str, stream: &str) -> String {
    let lower = line.to_ascii_lowercase();
    if lower.contains("error") || lower.contains("fatal") || lower.contains("panic") {
        "ERROR".to_string()
    } else if lower.contains("warn") {
        "WARN".to_string()
    } else if lower.contains("debug") {
        "DEBUG".to_string()
    } else if stream.eq_ignore_ascii_case("stderr") {
        "WARN".to_string()
    } else {
        "INFO".to_string()
    }
}
