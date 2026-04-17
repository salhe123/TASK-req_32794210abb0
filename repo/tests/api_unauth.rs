mod common;

use common::{client, req_json, wait_for_service};
use serde_json::Value;

#[test]
fn missing_bearer_returns_401_unauthenticated_envelope() {
    wait_for_service();
    let (status, body) = req_json("GET", "/api/auth/session", None, None, None);
    assert_eq!(status, 401);
    assert_eq!(body["error"].as_str(), Some("unauthenticated"));
    assert!(body["message"].is_string());
    assert!(body["details"].is_object());
}

#[test]
fn bogus_bearer_returns_401() {
    wait_for_service();
    let (status, body) = req_json(
        "GET",
        "/api/auth/session",
        Some("this-is-not-a-real-token"),
        None,
        None,
    );
    assert_eq!(status, 401);
    assert_eq!(body["error"].as_str(), Some("unauthenticated"));
}

#[test]
fn logout_then_reuse_token_returns_session_expired() {
    wait_for_service();
    let token = common::login("test_pkg", "TestPkgPassword123");
    let (s_logout, _) = req_json("POST", "/api/auth/logout", Some(&token), None, None);
    assert_eq!(s_logout, 200);
    let (s_reuse, body) = req_json("GET", "/api/auth/session", Some(&token), None, None);
    assert_eq!(s_reuse, 401);
    assert_eq!(body["error"].as_str(), Some("session_expired"));
}

#[test]
fn any_protected_endpoint_rejects_no_bearer() {
    wait_for_service();
    let endpoints = [
        "/api/lost-found/items",
        "/api/assets",
        "/api/volunteers",
        "/api/packages",
        "/api/notifications/inbox",
        "/api/admin/users",
    ];
    for ep in endpoints {
        let r = client()
            .get(format!("{}{}", common::base_url(), ep))
            .send()
            .expect("send");
        assert_eq!(r.status(), 401, "{} should require auth", ep);
        let v: Value = r.json().expect("json");
        assert!(
            v["error"].as_str() == Some("unauthenticated")
                || v["error"].as_str() == Some("session_expired"),
            "{} envelope: {}",
            ep,
            v
        );
    }
}
