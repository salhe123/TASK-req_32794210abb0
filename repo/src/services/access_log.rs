use std::collections::VecDeque;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use serde_json::{json, Value};

const CAPACITY: usize = 200;

static BUFFER: Lazy<Mutex<VecDeque<Value>>> = Lazy::new(|| Mutex::new(VecDeque::with_capacity(CAPACITY)));

pub fn record(entry: Value) {
    let mut buf = BUFFER.lock().unwrap();
    if buf.len() >= CAPACITY {
        buf.pop_front();
    }
    buf.push_back(entry);
}

pub fn recent(limit: usize) -> Vec<Value> {
    let buf = BUFFER.lock().unwrap();
    let take = limit.min(buf.len());
    buf.iter().rev().take(take).cloned().collect()
}

pub fn build_entry(
    request_id: Option<&str>,
    user_id: Option<uuid::Uuid>,
    facility_id: Option<uuid::Uuid>,
    method: &str,
    path: &str,
    status: u16,
    duration_ms: u128,
) -> Value {
    json!({
        "request_id": request_id,
        "user_id": user_id,
        "facility_id": facility_id,
        "method": method,
        "path": path,
        "status": status,
        "duration_ms": duration_ms,
    })
}
