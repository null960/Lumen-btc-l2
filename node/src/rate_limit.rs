//! IP-based rate limiter — no external deps, pure Rust
//! Uses sliding window counter per IP address.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use chrono::Utc;

#[derive(Debug, Clone)]
struct IpRecord {
    /// Timestamps of recent requests (unix seconds)
    requests: Vec<i64>,
    /// Total requests blocked (for monitoring)
    blocked: u64,
}

#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<Mutex<HashMap<String, IpRecord>>>,
    /// Max requests per window
    max_requests: usize,
    /// Window size in seconds
    window_secs: i64,
}

impl RateLimiter {
    pub fn new(max_requests: usize, window_secs: i64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            max_requests,
            window_secs,
        }
    }

    /// Returns true if request is allowed, false if rate limited
    pub fn check(&self, ip: &str) -> bool {
        let now = Utc::now().timestamp();
        let mut map = self.inner.lock().unwrap();
        let record = map.entry(ip.to_string()).or_insert(IpRecord {
            requests: Vec::new(),
            blocked: 0,
        });

        // Remove old requests outside the window
        record.requests.retain(|&t| now - t < self.window_secs);

        if record.requests.len() >= self.max_requests {
            record.blocked += 1;
            false
        } else {
            record.requests.push(now);
            true
        }
    }

    /// Returns seconds until next allowed request (0 if allowed now)
    pub fn retry_after(&self, ip: &str) -> i64 {
        let now = Utc::now().timestamp();
        let map = self.inner.lock().unwrap();
        if let Some(record) = map.get(ip) {
            if record.requests.len() >= self.max_requests {
                // Oldest request + window = when the slot opens
                if let Some(&oldest) = record.requests.first() {
                    let opens_at = oldest + self.window_secs;
                    return (opens_at - now).max(0);
                }
            }
        }
        0
    }

    /// Cleanup old entries (call periodically to prevent memory growth)
    pub fn cleanup(&self) {
        let now = Utc::now().timestamp();
        let mut map = self.inner.lock().unwrap();
        map.retain(|_, record| {
            record.requests.retain(|&t| now - t < 3600); // keep last hour
            !record.requests.is_empty()
        });
    }

    /// Stats for monitoring endpoint
    pub fn stats(&self) -> (usize, u64) {
        let map = self.inner.lock().unwrap();
        let active_ips = map.len();
        let total_blocked: u64 = map.values().map(|r| r.blocked).sum();
        (active_ips, total_blocked)
    }
}

/// Pre-configured limiters for different endpoint types
pub struct Limiters {
    /// Write operations: faucet, transfer, cmd — 20/min
    pub write: RateLimiter,
    /// Read operations: balance, state — 120/min
    pub read: RateLimiter,
    /// Faucet specifically — 3/hour (anti-abuse)
    pub faucet: RateLimiter,
}

impl Limiters {
    pub fn new() -> Self {
        Self {
            write:  RateLimiter::new(20, 60),
            read:   RateLimiter::new(120, 60),
            faucet: RateLimiter::new(3, 3600),
        }
    }
}