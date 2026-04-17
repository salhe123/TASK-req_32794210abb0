mod common;

use common::{login, req_json, wait_for_service};
use serde_json::json;

#[test]
fn login_bucket_returns_429_after_burst() {
    wait_for_service();
    // Clear the bucket so this test is deterministic regardless of earlier
    // auth tests that may have drained tokens.
    let admin = login("test_admin", "TestAdminPassword123");
    let (s, _) = req_json(
        "POST",
        "/api/__diag/rate-limit/reset",
        Some(&admin),
        Some(json!({})),
        None,
    );
    assert_eq!(s, 200);

    let mut saw_429 = false;
    let mut saw_400 = false;
    // Use a non-existent username so we never trip the lockout path first.
    let fake_user = format!("no-such-user-{}", uuid::Uuid::new_v4().simple());
    for _ in 0..25 {
        let (status, body) = req_json(
            "POST",
            "/api/auth/login",
            None,
            Some(json!({ "username": fake_user, "password": "whatever-does-not-matter-123" })),
            None,
        );
        match status {
            429 => {
                assert_eq!(body["error"].as_str(), Some("rate_limited"));
                saw_429 = true;
                break;
            }
            400 => {
                saw_400 = true;
            }
            other => panic!("unexpected status {} body={}", other, body),
        }
    }
    assert!(saw_400, "some requests must pass the rate gate and return validation_failed");
    assert!(saw_429, "eventually the bucket must exhaust and return 429");
}
