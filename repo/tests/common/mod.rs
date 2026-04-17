// Each tests/api_*.rs file is its own crate and imports `mod common`, but
// only uses a subset of the helpers — silence the expected dead_code warnings.
#![allow(dead_code)]

use std::time::{Duration, Instant};

use reqwest::blocking::{Client, Response};
use serde_json::{json, Value};

pub fn base_url() -> String {
    std::env::var("CIVICOPS_URL").unwrap_or_else(|_| "http://civicops:8080".to_string())
}

pub fn client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("reqwest client")
}

pub fn wait_for_service() {
    let c = client();
    let url = format!("{}/health", base_url());
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(120) {
        if let Ok(r) = c.get(&url).send() {
            if r.status().is_success() {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    panic!("service did not become healthy at {}", url);
}

pub fn login(username: &str, password: &str) -> String {
    let c = client();
    let url = format!("{}/api/auth/login", base_url());
    // The login rate limiter is keyed by peer IP. All tests share one peer IP
    // (the test-runner container), so under a big test sweep the bucket can
    // exhaust. Retry with linear backoff — the bucket refills at ~1 token/sec.
    let mut last_status = reqwest::StatusCode::OK;
    let mut last_body = String::new();
    for attempt in 0..20 {
        // Each attempt gets a fresh request_id so retries after a 429 are
        // treated as new writes, not replays of the rate-limited request.
        let rid = uuid::Uuid::new_v4().to_string();
        let r = c
            .post(&url)
            .header("X-Request-Id", &rid)
            .json(&json!({ "username": username, "password": password }))
            .send()
            .unwrap_or_else(|e| panic!("login send to {}: {}", url, e));
        last_status = r.status();
        last_body = r.text().unwrap_or_default();
        if last_status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            std::thread::sleep(Duration::from_millis(600 + 200 * attempt as u64));
            continue;
        }
        break;
    }
    let body: Value = serde_json::from_str(&last_body).unwrap_or_else(|e| {
        panic!(
            "login {} returned non-JSON (status={}): body={:?} err={}",
            url, last_status, last_body, e
        )
    });
    assert!(
        last_status.is_success(),
        "login failed for {}: status={} body={}",
        username,
        last_status,
        body
    );
    body["token"]
        .as_str()
        .unwrap_or_else(|| panic!("no token in login response: {}", body))
        .to_string()
}

pub fn req_json(
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<Value>,
    request_id: Option<&str>,
) -> (u16, Value) {
    let c = client();
    let url = format!("{}{}", base_url(), path);
    let mut builder = match method {
        "GET" => c.get(&url),
        "POST" => c.post(&url),
        "PUT" => c.put(&url),
        "DELETE" => c.delete(&url),
        _ => panic!("unsupported method {}", method),
    };
    if let Some(t) = token {
        builder = builder.bearer_auth(t);
    }
    // Mutating routes require X-Request-Id server-side (strict idempotency
    // contract). Auto-generate a UUID when the test does not supply one so
    // legacy tests keep working without per-call plumbing.
    let effective_rid: Option<String> = match request_id {
        Some(r) => Some(r.to_string()),
        None if matches!(method, "POST" | "PUT" | "DELETE") => {
            Some(uuid::Uuid::new_v4().to_string())
        }
        None => None,
    };
    if let Some(rid) = effective_rid.as_deref() {
        builder = builder.header("X-Request-Id", rid);
    }
    let resp: Response = if let Some(b) = body {
        builder.json(&b).send().expect("send")
    } else {
        builder.send().expect("send")
    };
    let status = resp.status().as_u16();
    let text = resp.text().unwrap_or_default();
    let v: Value = if text.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&text).unwrap_or(Value::String(text))
    };
    (status, v)
}

pub fn db() -> postgres::Client {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL required for tests that need direct DB access");
    postgres::Client::connect(&url, postgres::NoTls).expect("connect db")
}

pub fn get_facility_id(token: &str, code: &str) -> String {
    let (status, body) = req_json("GET", "/api/admin/facilities", Some(token), None, None);
    assert_eq!(status, 200, "facilities list: {}", body);
    let arr = body["facilities"].as_array().expect("facilities array");
    for f in arr {
        if f["code"].as_str() == Some(code) {
            return f["id"].as_str().unwrap().to_string();
        }
    }
    panic!("facility {} not found in {}", code, body);
}
