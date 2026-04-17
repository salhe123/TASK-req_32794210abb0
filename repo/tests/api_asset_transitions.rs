mod common;

use common::{db, get_facility_id, login, req_json, wait_for_service};
use serde_json::json;

fn new_asset(token: &str, facility_id: &str) -> String {
    let (s, body) = req_json(
        "POST",
        "/api/assets",
        Some(token),
        Some(json!({
            "facilityId": facility_id,
            "assetLabel": format!("TR-{}", uuid::Uuid::new_v4()),
            "name": "matrix",
        })),
        None,
    );
    assert_eq!(s, 201, "{}", body);
    body["id"].as_str().unwrap().to_string()
}

fn force_status(asset_id: &str, status: &str) {
    let mut c = db();
    let id: uuid::Uuid = asset_id.parse().unwrap();
    c.execute(
        "UPDATE assets SET status = $1 WHERE id = $2",
        &[&status, &id],
    )
    .expect("force status");
}

fn transition(token: &str, asset_id: &str, to: &str) -> (u16, serde_json::Value) {
    req_json(
        "POST",
        &format!("/api/assets/{}/transition", asset_id),
        Some(token),
        Some(json!({ "toState": to })),
        None,
    )
}

/// The binding transition table from plan §D3. `Some(extra)` indicates a
/// special case (e.g. INVENTORY_COUNT → prior state).
const STATES: &[&str] = &[
    "INTAKE",
    "ASSIGNMENT",
    "LOAN",
    "TRANSFER",
    "MAINTENANCE",
    "REPAIR",
    "INVENTORY_COUNT",
    "DISPOSAL",
];

fn is_allowed(from: &str, to: &str) -> bool {
    match from {
        "INTAKE" => matches!(to, "ASSIGNMENT" | "INVENTORY_COUNT"),
        "ASSIGNMENT" => matches!(to, "LOAN" | "TRANSFER" | "MAINTENANCE" | "INVENTORY_COUNT"),
        "LOAN" => matches!(to, "ASSIGNMENT" | "MAINTENANCE"),
        "TRANSFER" => matches!(to, "ASSIGNMENT"),
        "MAINTENANCE" => matches!(to, "REPAIR" | "ASSIGNMENT"),
        "REPAIR" => matches!(to, "ASSIGNMENT" | "DISPOSAL"),
        _ => false,
    }
}

#[test]
fn full_transition_matrix_enforced_at_api() {
    wait_for_service();
    let user = login("test_asset", "TestAssetPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");

    for from in STATES {
        for to in STATES {
            if from == to {
                continue;
            }
            // INVENTORY_COUNT has its own "return to prior" path exercised elsewhere.
            if *from == "INVENTORY_COUNT" {
                continue;
            }
            let id = new_asset(&user, &facility);
            force_status(&id, from);
            let (status, body) = transition(&user, &id, to);
            if is_allowed(from, to) {
                assert_eq!(
                    status, 200,
                    "{} -> {} should be allowed: {}",
                    from, to, body
                );
                assert_eq!(body["status"].as_str(), Some(*to));
            } else {
                assert_eq!(
                    status, 409,
                    "{} -> {} should be blocked: {}",
                    from, to, body
                );
                assert_eq!(body["error"].as_str(), Some("invalid_transition"));
            }
        }
    }
}

#[test]
fn inventory_count_round_trip_returns_to_prior_state() {
    wait_for_service();
    let user = login("test_asset", "TestAssetPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");

    let id = new_asset(&user, &facility);
    // Walk INTAKE → ASSIGNMENT → INVENTORY_COUNT → ASSIGNMENT
    let (s1, b1) = transition(&user, &id, "ASSIGNMENT");
    assert_eq!(s1, 200, "{}", b1);
    let (s2, b2) = transition(&user, &id, "INVENTORY_COUNT");
    assert_eq!(s2, 200, "{}", b2);
    assert_eq!(b2["status"].as_str(), Some("INVENTORY_COUNT"));
    assert_eq!(b2["priorStatus"].as_str(), Some("ASSIGNMENT"));

    // Only "return to prior" is allowed from INVENTORY_COUNT.
    let (s_bad, b_bad) = transition(&user, &id, "LOAN");
    assert_eq!(s_bad, 409, "{}", b_bad);
    assert_eq!(b_bad["error"].as_str(), Some("invalid_transition"));

    let (s3, b3) = transition(&user, &id, "ASSIGNMENT");
    assert_eq!(s3, 200, "{}", b3);
    assert_eq!(b3["status"].as_str(), Some("ASSIGNMENT"));
    assert!(b3["priorStatus"].is_null(), "prior cleared after return: {}", b3);
}

#[test]
fn disposal_is_terminal() {
    wait_for_service();
    let user = login("test_asset", "TestAssetPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let id = new_asset(&user, &facility);
    force_status(&id, "DISPOSAL");
    for to in ["ASSIGNMENT", "REPAIR", "MAINTENANCE", "LOAN"] {
        let (s, body) = transition(&user, &id, to);
        assert_eq!(s, 409, "DISPOSAL -> {} must be blocked: {}", to, body);
    }
}
