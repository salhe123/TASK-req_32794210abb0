use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;

static BUCKETS: Lazy<Mutex<HashMap<String, Bucket>>> = Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone)]
struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

const CAPACITY: f64 = 10.0;
const REFILL_PER_SEC: f64 = 1.0;

pub fn check(key: &str) -> bool {
    let mut guard = BUCKETS.lock().unwrap();
    let now = Instant::now();
    let b = guard.entry(key.to_string()).or_insert(Bucket {
        tokens: CAPACITY,
        last_refill: now,
    });
    let elapsed = now.duration_since(b.last_refill).as_secs_f64();
    b.tokens = (b.tokens + elapsed * REFILL_PER_SEC).min(CAPACITY);
    b.last_refill = now;
    if b.tokens >= 1.0 {
        b.tokens -= 1.0;
        true
    } else {
        false
    }
}

pub fn reset(key: &str) {
    let mut guard = BUCKETS.lock().unwrap();
    guard.remove(key);
}

pub fn reset_all() {
    let mut guard = BUCKETS.lock().unwrap();
    guard.clear();
}

#[allow(dead_code)]
pub fn prune_older_than(max_idle: Duration) {
    let mut guard = BUCKETS.lock().unwrap();
    let now = Instant::now();
    guard.retain(|_, b| now.duration_since(b.last_refill) < max_idle);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_limits_requests() {
        let key = "test-rate-limit-isolated-key";
        reset(key);
        for _ in 0..10 {
            assert!(check(key));
        }
        assert!(!check(key));
    }
}
