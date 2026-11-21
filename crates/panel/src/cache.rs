use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
struct CacheEntry<T> {
    value: T,
    expires_at: Instant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedSubscription {
    pub body: String,
    pub content_type: String,
    pub headers: Vec<(String, String)>,
}

#[derive(Debug, Default)]
pub struct AppCache {
    sub_cache: DashMap<String, CacheEntry<CachedSubscription>>,
    json_cache: DashMap<String, CacheEntry<Value>>,
}

impl AppCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_sub(&self, key: &str) -> Option<CachedSubscription> {
        let now = Instant::now();
        if let Some(entry) = self.sub_cache.get(key) {
            if entry.expires_at > now {
                return Some(entry.value.clone());
            }
        }
        self.sub_cache.remove(key);
        None
    }

    pub fn set_sub(&self, key: String, val: CachedSubscription, ttl: Duration) {
        let now = Instant::now();
        if self.sub_cache.len() > 20_000 {
            self.sub_cache.retain(|_, v| v.expires_at > now);
        }
        self.sub_cache.insert(
            key,
            CacheEntry {
                value: val,
                expires_at: now + ttl,
            },
        );
    }

    pub fn del_sub_token(&self, sub_token: &str) {
        let prefix = format!("sub:{sub_token}:");
        self.sub_cache.retain(|k, _| !k.starts_with(&prefix));
    }

    pub fn clear_all_subs(&self) {
        self.sub_cache.clear();
    }

    pub fn get_json(&self, key: &str) -> Option<Value> {
        let now = Instant::now();
        if let Some(entry) = self.json_cache.get(key) {
            if entry.expires_at > now {
                return Some(entry.value.clone());
            }
        }
        self.json_cache.remove(key);
        None
    }

    pub fn set_json(&self, key: impl Into<String>, val: Value, ttl: Duration) {
        let now = Instant::now();
        if self.json_cache.len() > 10_000 {
            self.json_cache.retain(|_, v| v.expires_at > now);
        }
        self.json_cache.insert(
            key.into(),
            CacheEntry {
                value: val,
                expires_at: now + ttl,
            },
        );
    }

    pub fn del_prefix(&self, prefix: &str) {
        self.json_cache.retain(|k, _| !k.starts_with(prefix));
    }

    pub fn invalidate_derived(&self) {
        self.sub_cache.clear();
        self.del_prefix("dashboard:");
    }
}
