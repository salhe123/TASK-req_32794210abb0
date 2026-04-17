mod common;

use common::{get_facility_id, login, req_json, wait_for_service};
use serde_json::json;

#[test]
fn create_and_list_maintenance_records() {
    wait_for_service();
    let asset_user = login("test_asset", "TestAssetPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");

    let (_, a) = req_json(
        "POST",
        "/api/assets",
        Some(&asset_user),
        Some(json!({
            "facilityId": facility,
            "assetLabel": format!("MNT-{}", uuid::Uuid::new_v4()),
            "name": "maint target",
        })),
        None,
    );
    let aid = a["id"].as_str().unwrap().to_string();

    let (s1, b1) = req_json(
        "POST",
        &format!("/api/assets/{}/maintenance-records", aid),
        Some(&asset_user),
        Some(json!({
            "summary": "Replaced battery",
            "details": "Swapped A23 cell, serial 1234",
        })),
        None,
    );
    assert_eq!(s1, 201, "{}", b1);
    assert_eq!(b1["summary"].as_str(), Some("Replaced battery"));
    let rec_id = b1["id"].as_str().unwrap().to_string();

    let (s2, b2) = req_json(
        "GET",
        &format!("/api/assets/{}/maintenance-records", aid),
        Some(&asset_user),
        None,
        None,
    );
    assert_eq!(s2, 200);
    let records = b2["records"].as_array().unwrap();
    assert!(records.iter().any(|r| r["id"].as_str() == Some(&rec_id)));
}

#[test]
fn maintenance_requires_summary() {
    wait_for_service();
    let asset_user = login("test_asset", "TestAssetPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let (_, a) = req_json(
        "POST",
        "/api/assets",
        Some(&asset_user),
        Some(json!({
            "facilityId": facility,
            "assetLabel": format!("MNT2-{}", uuid::Uuid::new_v4()),
            "name": "bad",
        })),
        None,
    );
    let aid = a["id"].as_str().unwrap().to_string();
    let (s, body) = req_json(
        "POST",
        &format!("/api/assets/{}/maintenance-records", aid),
        Some(&asset_user),
        Some(json!({ "summary": "" })),
        None,
    );
    assert_eq!(s, 400);
    assert_eq!(body["error"].as_str(), Some("validation_failed"));
    assert_eq!(body["details"]["field"].as_str(), Some("summary"));
}
