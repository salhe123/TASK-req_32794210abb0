mod common;

use common::{db, login, req_json, wait_for_service};

#[test]
fn session_expires_after_8h_idle() {
    wait_for_service();
    let token = login("test_notif", "TestNotifPassword123");

    // Sanity: the session works right now.
    let (s_ok, _) = req_json("GET", "/api/auth/session", Some(&token), None, None);
    assert_eq!(s_ok, 200);

    // Push last_activity_at back 9 hours in the DB so the idle check trips.
    let mut c = db();
    let updated = c
        .execute(
            "UPDATE sessions
             SET last_activity_at = last_activity_at - interval '9 hours'
             WHERE token_hash = encode(digest($1, 'sha256'), 'hex')",
            &[&token],
        )
        .expect("update last_activity_at");
    assert!(updated >= 1, "expected at least one session row updated");

    let (status, body) = req_json("GET", "/api/auth/session", Some(&token), None, None);
    assert_eq!(status, 401, "idle session must be rejected: {}", body);
    assert_eq!(body["error"].as_str(), Some("session_expired"));
}
