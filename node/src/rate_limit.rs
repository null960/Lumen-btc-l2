use std::collections::HashMap;
use std::sync::Mutex;
use chrono::Utc;

// Limiter
pub struct RateLimiter {
    requests: Mutex<HashMap<String, (u64, i64)>>,
}

impl RateLimiter {
    // Init
    pub fn new() -> Self {
        Self { requests: Mutex::new(HashMap::new()) }
    }

    // Check
    pub fn check(&self, client_id: &str, limit: u64, window_sec: i64) -> bool {
        let mut reqs = self.requests.lock().unwrap();
        let now = Utc::now().timestamp();
        let entry = reqs.entry(client_id.to_string()).or_insert((0, now));
        
        if now - entry.1 > window_sec {
            entry.0 = 1;
            entry.1 = now;
            true
        } else if entry.0 < limit {
            entry.0 += 1;
            true
        } else {
            false
        }
    }
}