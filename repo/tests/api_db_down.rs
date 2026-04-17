mod common;

use common::{base_url, client};

/// Gated test: run only when the harness has stopped Postgres ahead of time.
/// `run_tests.sh` executes this with CIVICOPS_EXPECT_DB_DOWN=1 after taking
/// the Postgres container offline, then brings it back up for the remaining
/// phases.
#[test]
fn health_ready_returns_503_when_db_down() {
    if std::env::var("CIVICOPS_EXPECT_DB_DOWN").ok().as_deref() != Some("1") {
        eprintln!("skipping db-down test (CIVICOPS_EXPECT_DB_DOWN!=1)");
        return;
    }
    let r = client()
        .get(format!("{}/api/health/ready", base_url()))
        .send()
        .expect("send");
    assert_eq!(r.status(), 503, "expected 503 Service Unavailable when DB is down");
    let body: serde_json::Value = r.json().expect("envelope json");
    assert_eq!(body["error"].as_str(), Some("internal_error"));
    assert!(body["message"].is_string());
    assert!(body["details"].is_object());
}

#[test]
fn liveness_still_200_when_db_down() {
    if std::env::var("CIVICOPS_EXPECT_DB_DOWN").ok().as_deref() != Some("1") {
        return;
    }
    let r = client()
        .get(format!("{}/health", base_url()))
        .send()
        .expect("send");
    assert_eq!(r.status(), 200, "process liveness must stay up when DB is down");
}
