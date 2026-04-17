mod common;

use common::{db, login, req_json, wait_for_service};

#[test]
fn session_hard_expiry_is_enforced() {
    wait_for_service();
    let token = login("test_pkg", "TestPkgPassword123");

    let (s, _) = req_json("GET", "/api/auth/session", Some(&token), None, None);
    assert_eq!(s, 200);

    // Move both expires_at AND last_activity_at into the past so the TTL
    // branch is the one doing the rejection (idle is already covered
    // separately in api_session_idle.rs).
    let mut c = db();
    let updated = c
        .execute(
            "UPDATE sessions
             SET expires_at = NOW() - interval '1 minute',
                 last_activity_at = NOW()
             WHERE token_hash = encode(digest($1, 'sha256'), 'hex')",
            &[&token],
        )
        .expect("age session");
    assert!(updated >= 1);

    let (status, body) = req_json("GET", "/api/auth/session", Some(&token), None, None);
    assert_eq!(status, 401, "hard-expired session must be rejected: {}", body);
    assert_eq!(body["error"].as_str(), Some("session_expired"));
}

#[test]
fn revoked_session_rejected() {
    wait_for_service();
    let token = login("test_pkg", "TestPkgPassword123");
    let mut c = db();
    c.execute(
        "UPDATE sessions
         SET revoked = true
         WHERE token_hash = encode(digest($1, 'sha256'), 'hex')",
        &[&token],
    )
    .expect("revoke");
    let (status, body) = req_json("GET", "/api/auth/session", Some(&token), None, None);
    assert_eq!(status, 401);
    assert_eq!(body["error"].as_str(), Some("session_expired"));
}
