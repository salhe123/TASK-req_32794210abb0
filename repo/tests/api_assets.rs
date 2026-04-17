mod common;

use common::{get_facility_id, login, req_json, wait_for_service};
use serde_json::json;

fn create_asset(token: &str, facility_id: &str, label: &str) -> String {
    let (status, body) = req_json(
        "POST",
        "/api/assets",
        Some(token),
        Some(json!({
            "facilityId": facility_id,
            "assetLabel": label,
            "name": format!("Asset {}", label),
        })),
        None,
    );
    assert_eq!(status, 201, "create asset: {}", body);
    assert_eq!(body["status"].as_str(), Some("INTAKE"));
    body["id"].as_str().unwrap().to_string()
}

#[test]
fn valid_single_transition_and_history() {
    wait_for_service();
    let asset_user = login("test_asset", "TestAssetPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let id = create_asset(&asset_user, &facility, &format!("SN-{}", uuid::Uuid::new_v4()));
    let (s, body) = req_json(
        "POST",
        &format!("/api/assets/{}/transition", id),
        Some(&asset_user),
        Some(json!({ "toState": "ASSIGNMENT" })),
        None,
    );
    assert_eq!(s, 200, "{}", body);
    assert_eq!(body["status"].as_str(), Some("ASSIGNMENT"));
    let (sh, hist) = req_json(
        "GET",
        &format!("/api/assets/{}/history", id),
        Some(&asset_user),
        None,
        None,
    );
    assert_eq!(sh, 200);
    let events = hist["events"].as_array().unwrap();
    assert!(events.len() >= 2);
}

#[test]
fn invalid_transition_returns_409() {
    wait_for_service();
    let asset_user = login("test_asset", "TestAssetPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let id = create_asset(&asset_user, &facility, &format!("SN-{}", uuid::Uuid::new_v4()));
    let (s, body) = req_json(
        "POST",
        &format!("/api/assets/{}/transition", id),
        Some(&asset_user),
        Some(json!({ "toState": "LOAN" })),
        None,
    );
    assert_eq!(s, 409);
    assert_eq!(body["error"].as_str(), Some("invalid_transition"));
}

#[test]
fn bulk_transition_mixes_committed_and_rejected() {
    wait_for_service();
    let asset_user = login("test_asset", "TestAssetPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");

    let mut valid_ids = Vec::new();
    for _ in 0..5 {
        valid_ids.push(create_asset(
            &asset_user,
            &facility,
            &format!("SN-{}", uuid::Uuid::new_v4()),
        ));
    }
    let mut invalid_ids = Vec::new();
    for _ in 0..3 {
        let id = create_asset(&asset_user, &facility, &format!("SN-{}", uuid::Uuid::new_v4()));
        let (_, _) = req_json(
            "POST",
            &format!("/api/assets/{}/transition", id),
            Some(&asset_user),
            Some(json!({ "toState": "ASSIGNMENT" })),
            None,
        );
        invalid_ids.push(id);
    }
    let mut all_ids: Vec<String> = valid_ids.iter().cloned().collect();
    all_ids.extend(invalid_ids.iter().cloned());

    let (s, body) = req_json(
        "POST",
        "/api/assets/bulk-transition",
        Some(&asset_user),
        Some(json!({ "ids": all_ids, "toState": "ASSIGNMENT" })),
        None,
    );
    assert_eq!(s, 200, "{}", body);
    assert_eq!(body["committed"].as_u64(), Some(5));
    let rejected = body["rejected"].as_array().unwrap();
    assert_eq!(rejected.len(), 3);
    for r in rejected {
        assert_eq!(r["reason"].as_str(), Some("invalid_transition"));
    }
}

#[test]
fn bulk_transition_rejects_501_ids() {
    wait_for_service();
    let asset_user = login("test_asset", "TestAssetPassword123");
    let ids: Vec<String> = (0..501).map(|_| uuid::Uuid::new_v4().to_string()).collect();
    let (s, body) = req_json(
        "POST",
        "/api/assets/bulk-transition",
        Some(&asset_user),
        Some(json!({ "ids": ids, "toState": "ASSIGNMENT" })),
        None,
    );
    assert_eq!(s, 400);
    assert_eq!(body["error"].as_str(), Some("validation_failed"));
    assert_eq!(body["details"]["limit"].as_u64(), Some(500));
}

#[test]
fn duplicate_asset_label_returns_409() {
    wait_for_service();
    let asset_user = login("test_asset", "TestAssetPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let label = format!("DUP-{}", uuid::Uuid::new_v4());
    let _ = create_asset(&asset_user, &facility, &label);
    let (s, body) = req_json(
        "POST",
        "/api/assets",
        Some(&asset_user),
        Some(json!({
            "facilityId": facility,
            "assetLabel": label,
            "name": "dup",
        })),
        None,
    );
    assert_eq!(s, 409);
    assert_eq!(body["error"].as_str(), Some("duplicate_asset_label"));
}
