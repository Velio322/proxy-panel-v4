use chrono::NaiveDateTime;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

/// Upper bound for queued items (memory guard while the database is unavailable).
pub const MAX_QUEUE_LEN: usize = 200_000;

#[derive(Debug, Clone)]
pub struct PendingTrafficItem {
    pub receipt_id: String,
    pub batch_id: Option<String>,
    pub node_id: String,
    pub client_id: String,
    pub upload: i64,
    pub download: i64,
    pub protocol: Option<String>,
    pub inbound_tag: Option<String>,
    pub record_at: NaiveDateTime,
}

#[derive(Debug, Default)]
pub struct TrafficQueue {
    queue: Mutex<VecDeque<PendingTrafficItem>>,
    email_cache: Mutex<HashMap<String, String>>,
}

impl TrafficQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues one item. Returns `false` (item not queued) when the queue is full, e.g. during a
    /// long database outage; the caller must then withhold the ack so the agent keeps the batch.
    pub fn push(&self, item: PendingTrafficItem) -> bool {
        let mut q = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        if q.len() >= MAX_QUEUE_LEN {
            return false;
        }
        q.push_back(item);
        true
    }

    pub fn push_batch(&self, items: Vec<PendingTrafficItem>) {
        let mut q = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        for item in items {
            q.push_back(item);
        }
    }

    pub fn drain_up_to(&self, max_items: usize) -> Vec<PendingTrafficItem> {
        let mut q = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        let take = max_items.min(q.len());
        q.drain(..take).collect()
    }

    pub fn requeue_front(&self, items: Vec<PendingTrafficItem>) {
        let mut q = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        for item in items.into_iter().rev() {
            q.push_front(item);
        }
    }

    pub fn len(&self) -> usize {
        self.queue.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
    }

    pub fn get_cached_client_id(&self, key: &str) -> Option<String> {
        self.email_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
            .cloned()
    }

    /// Forgets every cached key that resolved to `client_id` (client deleted or re-keyed).
    pub fn invalidate_client(&self, client_id: &str) {
        self.email_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, v| v != client_id);
    }

    pub fn cache_client_id(&self, key: String, client_id: String) {
        let mut map = self.email_cache.lock().unwrap_or_else(|e| e.into_inner());
        if map.len() >= 10_000 {
            if let Some(first_key) = map.keys().next().cloned() {
                map.remove(&first_key);
            }
        }
        map.insert(key, client_id);
    }
}
