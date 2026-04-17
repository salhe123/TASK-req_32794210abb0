mod common;

use common::{login, req_json, wait_for_service};
use serde_json::json;

#[test]
fn deactivated_user_token_rejected_with_forbidden() {
    wait_for_service();
    let admin = login("test_admin", "TestAdminPassword123");

    // Create a brand-new user so other tests aren't disturbed.
    let username = format!("deact_{}", uuid::Uuid::new_v4().simple());
    let password = "SomeValidPassword123";
    let (cs, cb) = req_json(
        "POST",
        "/api/admin/users",
        Some(&admin),
        Some(json!({
            "username": username,
            "password": password,
            "displayName": "To Be Deactivated",
        })),
        None,
    );
    assert_eq!(cs, 201, "{}", cb);
    let user_id = cb["id"].as_str().unwrap().to_string();

    // They can log in and call /session.
    let token = login(&username, password);
    let (s_ok, _) = req_json("GET", "/api/auth/session", Some(&token), None, None);
    assert_eq!(s_ok, 200);

    // Admin deactivates them.
    let (us, _) = req_json(
        "PUT",
        &format!("/api/admin/users/{}", user_id),
        Some(&admin),
        Some(json!({ "isActive": false })),
        None,
    );
    assert_eq!(us, 200);

    // Existing token must now be rejected.
    let (status, body) = req_json("GET", "/api/auth/session", Some(&token), None, None);
    assert_eq!(status, 403, "deactivated user's session must be forbidden: {}", body);
    assert_eq!(body["error"].as_str(), Some("forbidden"));
}
