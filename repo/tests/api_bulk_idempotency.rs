mod common;

use common::{get_facility_id, login, req_json, wait_for_service};
use serde_json::json;

fn new_asset(token: &str, facility: &str) -> String {
    let (_, body) = req_json(
        "POST",
        "/api/assets",
        Some(token),
        Some(json!({
            "facilityId": facility,
            "assetLabel": format!("BULK-{}", uuid::Uuid::new_v4()),
            "name": "bulk idem",
        })),
        None,
    );
    body["id"].as_str().unwrap().to_string()
}

#[test]
fn bulk_transition_same_request_id_returns_byte_identical_body() {
    wait_for_service();
    let user = login("test_asset", "TestAssetPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let ids: Vec<String> = (0..3).map(|_| new_asset(&user, &facility)).collect();

    let rid = uuid::Uuid::new_v4().to_string();
    let (s1, b1) = req_json(
        "POST",
        "/api/assets/bulk-transition",
        Some(&user),
        Some(json!({ "ids": ids, "toState": "ASSIGNMENT" })),
        Some(&rid),
    );
    assert_eq!(s1, 200, "{}", b1);
    assert_eq!(b1["committed"].as_u64(), Some(3));

    // Replay: exact same body expected.
    let (s2, b2) = req_json(
        "POST",
        "/api/assets/bulk-transition",
        Some(&user),
        Some(json!({ "ids": ids, "toState": "ASSIGNMENT" })),
        Some(&rid),
    );
    assert_eq!(s2, 200);
    assert_eq!(b1, b2, "bulk-transition must be idempotent per X-Request-Id");
}

#[test]
fn bulk_transition_cross_user_replay_is_409_conflict() {
    wait_for_service();
    let user = login("test_asset", "TestAssetPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let ids: Vec<String> = (0..2).map(|_| new_asset(&user, &facility)).collect();

    let rid = uuid::Uuid::new_v4().to_string();
    let (s1, _) = req_json(
        "POST",
        "/api/assets/bulk-transition",
        Some(&user),
        Some(json!({ "ids": ids, "toState": "ASSIGNMENT" })),
        Some(&rid),
    );
    assert_eq!(s1, 200);

    let (s2, body) = req_json(
        "POST",
        "/api/assets/bulk-transition",
        Some(&admin),
        Some(json!({ "ids": ids, "toState": "ASSIGNMENT" })),
        Some(&rid),
    );
    assert_eq!(s2, 409);
    assert_eq!(body["error"].as_str(), Some("idempotency_conflict"));
}
