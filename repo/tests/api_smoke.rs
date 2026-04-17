mod common;

use common::{base_url, client, wait_for_service};

#[test]
fn health_ok() {
    wait_for_service();
    let r = client()
        .get(format!("{}/health", base_url()))
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    let v: serde_json::Value = r.json().unwrap();
    assert_eq!(v["status"].as_str(), Some("ok"));
}

#[test]
fn api_health_scope() {
    wait_for_service();
    let r = client()
        .get(format!("{}/api/health", base_url()))
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
}

#[test]
fn health_ready_returns_200_when_db_up() {
    wait_for_service();
    let r = client()
        .get(format!("{}/api/health/ready", base_url()))
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    let v: serde_json::Value = r.json().unwrap();
    assert_eq!(v["status"].as_str(), Some("ready"));
    assert_eq!(v["db"].as_str(), Some("ok"));
}

#[test]
fn metrics_payload_shape() {
    wait_for_service();
    let r = client()
        .get(format!("{}/api/metrics", base_url()))
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    let v: serde_json::Value = r.json().unwrap();
    for key in &[
        "requestsTotal",
        "errorsTotal",
        "outboxPending",
        "outboxDead",
        "activeSessions",
    ] {
        assert!(v[key].is_number(), "metrics payload missing {}: {}", key, v);
    }
}
