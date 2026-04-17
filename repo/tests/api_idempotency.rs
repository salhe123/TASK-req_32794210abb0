mod common;

use common::{db, get_facility_id, login, req_json, wait_for_service};
use serde_json::json;

#[test]
fn idempotency_key_expires_after_24_hours() {
    wait_for_service();
    let pkg_user = login("test_pkg", "TestPkgPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let rid = uuid::Uuid::new_v4().to_string();
    let payload = json!({
        "facilityId": facility,
        "name": "Expiring Key Package",
        "basePrice": "10.00"
    });

    let (s1, b1) = req_json("POST", "/api/packages", Some(&pkg_user), Some(payload.clone()), Some(&rid));
    assert_eq!(s1, 201);

    // Replay while the key is still fresh → byte-identical body.
    let (s_fresh, b_fresh) = req_json("POST", "/api/packages", Some(&pkg_user), Some(payload.clone()), Some(&rid));
    assert_eq!(s_fresh, 201);
    assert_eq!(b1, b_fresh);

    // Fast-forward the key 25 hours in the DB so it's expired.
    let mut c = db();
    let updated = c
        .execute(
            "UPDATE idempotency_keys
             SET expires_at = expires_at - interval '25 hours'
             WHERE request_id = $1",
            &[&rid],
        )
        .expect("age key");
    assert!(updated >= 1);

    // With the key expired, the same request_id must no longer be a replay:
    // the new request either succeeds as a fresh write or hits the unique
    // (name, facility_id) path — the point is the old response body is NOT returned.
    let (s_after, b_after) = req_json(
        "POST",
        "/api/packages",
        Some(&pkg_user),
        Some(json!({
            "facilityId": facility,
            "name": "Post-Expiry Package",
            "basePrice": "11.00"
        })),
        Some(&rid),
    );
    assert_eq!(s_after, 201, "expired key should not replay: {}", b_after);
    assert_ne!(
        b_after, b1,
        "after expiry the old cached body must NOT be returned"
    );
    assert_eq!(b_after["name"].as_str(), Some("Post-Expiry Package"));
}

#[test]
fn same_user_same_request_id_works_across_different_endpoints() {
    // Audit round 3 / Blocker #1: the legacy UNIQUE(user_id, request_id)
    // constraint meant that reusing one request_id on two different endpoints
    // raised a DB unique-violation and surfaced as 500. The composite replay
    // key is (user_id, request_id, method, path), so this must succeed.
    wait_for_service();
    let pkg_user = login("test_pkg", "TestPkgPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let rid = uuid::Uuid::new_v4().to_string();

    // First write on endpoint A: create package.
    let (s1, b1) = req_json(
        "POST",
        "/api/packages",
        Some(&pkg_user),
        Some(json!({
            "facilityId": facility,
            "name": format!("Same-RID-A-{}", uuid::Uuid::new_v4().simple()),
            "basePrice": "10.00"
        })),
        Some(&rid),
    );
    assert_eq!(s1, 201, "first endpoint should accept: {}", b1);

    // Second write on endpoint B: different URL entirely. Reusing the SAME
    // rid must be treated as a fresh request, NOT an internal-error collision.
    let (s2, b2) = req_json(
        "POST",
        &format!("/api/packages/{}/publish", b1["id"].as_str().unwrap()),
        Some(&pkg_user),
        None,
        Some(&rid),
    );
    assert!(
        s2 == 200 || s2 == 409,
        "same user same rid on a different endpoint must not raise 500; got {} body={}",
        s2,
        b2
    );
}

#[test]
fn admin_idempotency_keys_list_only_returns_non_expired() {
    wait_for_service();
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let pkg_user = login("test_pkg", "TestPkgPassword123");

    let fresh_rid = uuid::Uuid::new_v4().to_string();
    let (_, _) = req_json(
        "POST",
        "/api/packages",
        Some(&pkg_user),
        Some(json!({
            "facilityId": facility,
            "name": "Key-List-Fresh",
            "basePrice": "1.00"
        })),
        Some(&fresh_rid),
    );

    let stale_rid = uuid::Uuid::new_v4().to_string();
    let (_, _) = req_json(
        "POST",
        "/api/packages",
        Some(&pkg_user),
        Some(json!({
            "facilityId": facility,
            "name": "Key-List-Stale",
            "basePrice": "2.00"
        })),
        Some(&stale_rid),
    );
    let mut c = db();
    c.execute(
        "UPDATE idempotency_keys
         SET expires_at = expires_at - interval '25 hours'
         WHERE request_id = $1",
        &[&stale_rid],
    )
    .expect("age stale key");

    let (s, body) = req_json("GET", "/api/admin/idempotency/keys", Some(&admin), None, None);
    assert_eq!(s, 200, "{}", body);
    let keys = body["keys"].as_array().unwrap();
    let fresh_present = keys.iter().any(|k| k["requestId"].as_str() == Some(&fresh_rid));
    let stale_present = keys.iter().any(|k| k["requestId"].as_str() == Some(&stale_rid));
    assert!(fresh_present, "fresh key must appear in admin listing");
    assert!(!stale_present, "expired key must NOT appear in admin listing");
}
