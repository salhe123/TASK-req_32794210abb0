mod common;

use common::{login, req_json, wait_for_service};
use serde_json::json;

/// Drain rate-limit state so this test's raw login calls aren't swallowed by
/// the bucket that earlier tests may have emptied. Uses the diag reset via
/// the admin token (whose login goes through the retry helper).
fn reset_rate_limit_bucket() {
    let admin = login("test_admin", "TestAdminPassword123");
    let (_s, _b) = req_json(
        "POST",
        "/api/__diag/rate-limit/reset",
        Some(&admin),
        Some(json!({})),
        None,
    );
}

#[test]
fn login_success_returns_token_and_session_fields() {
    wait_for_service();
    reset_rate_limit_bucket();
    let (status, body) = req_json(
        "POST",
        "/api/auth/login",
        None,
        Some(json!({ "username": "test_admin", "password": "TestAdminPassword123" })),
        None,
    );
    assert_eq!(status, 200, "body={}", body);
    assert!(body["token"].as_str().unwrap().len() > 20);
    assert_eq!(body["username"].as_str(), Some("test_admin"));
    assert!(body["userId"].is_string());
    assert!(body["expiresAt"].is_string());
}

#[test]
fn wrong_password_returns_validation_envelope() {
    wait_for_service();
    reset_rate_limit_bucket();
    let (status, body) = req_json(
        "POST",
        "/api/auth/login",
        None,
        Some(json!({ "username": "test_admin", "password": "Wrong-Password-1234" })),
        None,
    );
    assert_eq!(status, 400);
    assert_eq!(body["error"].as_str(), Some("validation_failed"));
    assert!(body["message"].is_string());
}

#[test]
fn five_failed_attempts_lock_account_out() {
    wait_for_service();
    reset_rate_limit_bucket();
    let username = "test_pkg";
    for _ in 0..5 {
        let (_s, _b) = req_json(
            "POST",
            "/api/auth/login",
            None,
            Some(json!({ "username": username, "password": "this-is-wrong-1234" })),
            None,
        );
    }
    let (status, body) = req_json(
        "POST",
        "/api/auth/login",
        None,
        Some(json!({ "username": username, "password": "TestPkgPassword123" })),
        None,
    );
    assert_eq!(status, 423, "body={}", body);
    assert_eq!(body["error"].as_str(), Some("account_locked"));

    // Have admin unlock the user.
    let admin_token = login("test_admin", "TestAdminPassword123");
    let (ul_status, _) = req_json("GET", "/api/admin/users", Some(&admin_token), None, None);
    assert_eq!(ul_status, 200);
    let (_, users_body) = req_json("GET", "/api/admin/users", Some(&admin_token), None, None);
    let user_id = users_body["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["username"].as_str() == Some(username))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (unlock_status, _) = req_json(
        "PUT",
        &format!("/api/admin/users/{}/unlock", user_id),
        Some(&admin_token),
        None,
        None,
    );
    assert_eq!(unlock_status, 200);
}

#[test]
fn session_endpoint_returns_user_identity() {
    wait_for_service();
    let token = login("test_admin", "TestAdminPassword123");
    let (status, body) = req_json("GET", "/api/auth/session", Some(&token), None, None);
    assert_eq!(status, 200);
    assert_eq!(body["username"].as_str(), Some("test_admin"));
    assert!(body["permissions"].is_array());
    assert!(body["roles"].is_array());
}

#[test]
fn change_password_rejects_short_new_password() {
    wait_for_service();
    let token = login("test_notif", "TestNotifPassword123");
    let (status, body) = req_json(
        "POST",
        "/api/auth/change-password",
        Some(&token),
        Some(json!({ "currentPassword": "TestNotifPassword123", "newPassword": "short" })),
        None,
    );
    assert_eq!(status, 400);
    assert_eq!(body["error"].as_str(), Some("validation_failed"));
    assert_eq!(body["details"]["field"].as_str(), Some("password"));
}

#[test]
fn logout_revokes_session() {
    wait_for_service();
    let token = login("test_notif", "TestNotifPassword123");
    let (status, _) = req_json("POST", "/api/auth/logout", Some(&token), None, None);
    assert_eq!(status, 200);
    let (status2, body2) = req_json("GET", "/api/auth/session", Some(&token), None, None);
    assert_eq!(status2, 401, "body={}", body2);
}
