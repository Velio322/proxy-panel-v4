use dashmap::DashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
struct Bucket {
    count: u32,
    reset_at: Instant,
}

#[derive(Debug, Clone)]
pub struct RateLimitOutcome {
    pub allowed: bool,
    pub limit: u32,
    pub remaining: u32,
    pub reset_secs: u64,
}

#[derive(Debug)]
pub struct RateLimiter {
    buckets: DashMap<String, Bucket>,
    window: Duration,
    max_requests: u32,
}

impl RateLimiter {
    pub fn new(window: Duration, max_requests: u32) -> Self {
        Self {
            buckets: DashMap::new(),
            window: window.max(Duration::from_millis(1)),
            max_requests: max_requests.max(1),
        }
    }

    pub fn check(&self, key: &str) -> RateLimitOutcome {
        let now = Instant::now();
        if self.buckets.len() > 20_000 {
            self.buckets.retain(|_, b| b.reset_at > now);
        }

        let mut entry = self.buckets.entry(key.to_string()).or_insert_with(|| Bucket {
            count: 0,
            reset_at: now + self.window,
        });

        if now >= entry.reset_at {
            entry.count = 0;
            entry.reset_at = now + self.window;
        }

        entry.count = entry.count.saturating_add(1);
        let allowed = entry.count <= self.max_requests;
        let remaining = self.max_requests.saturating_sub(entry.count);
        let reset_secs = entry
            .reset_at
            .saturating_duration_since(now)
            .as_secs()
            .max(1);

        RateLimitOutcome {
            allowed,
            limit: self.max_requests,
            remaining,
            reset_secs,
        }
    }
}
