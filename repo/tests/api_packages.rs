mod common;

use common::{get_facility_id, login, req_json, wait_for_service};
use serde_json::json;

fn new_package(token: &str, facility_id: &str) -> String {
    let (s, body) = req_json(
        "POST",
        "/api/packages",
        Some(token),
        Some(json!({
            "facilityId": facility_id,
            "name": "Portrait Session",
            "description": "1-hour portrait package",
            "basePrice": "125.00"
        })),
        None,
    );
    assert_eq!(s, 201, "create pkg: {}", body);
    assert_eq!(body["basePrice"].as_str(), Some("125.00"));
    assert_eq!(body["status"].as_str(), Some("DRAFT"));
    body["id"].as_str().unwrap().to_string()
}

fn new_variant(token: &str, package_id: &str, key: &str, price: &str) -> String {
    let (s, body) = req_json(
        "POST",
        &format!("/api/packages/{}/variants", package_id),
        Some(token),
        Some(json!({ "combinationKey": key, "price": price })),
        None,
    );
    assert_eq!(s, 201, "create variant: {}", body);
    assert_eq!(body["price"].as_str(), Some(price));
    body["id"].as_str().unwrap().to_string()
}

#[test]
fn price_serializes_as_two_decimal_string() {
    wait_for_service();
    let pkg_user = login("test_pkg", "TestPkgPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let pid = new_package(&pkg_user, &facility);
    let _vid = new_variant(&pkg_user, &pid, "k1", "35.00");

    let (s, body) = req_json("GET", &format!("/api/packages/{}", pid), Some(&pkg_user), None, None);
    assert_eq!(s, 200);
    assert_eq!(body["basePrice"].as_str(), Some("125.00"));
    let variants = body["variants"].as_array().unwrap();
    assert_eq!(variants[0]["price"].as_str(), Some("35.00"));
}

#[test]
fn reject_21st_variant() {
    wait_for_service();
    let pkg_user = login("test_pkg", "TestPkgPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let pid = new_package(&pkg_user, &facility);
    for i in 0..20 {
        let _ = new_variant(&pkg_user, &pid, &format!("k{}", i), "10.00");
    }
    let (s, body) = req_json(
        "POST",
        &format!("/api/packages/{}/variants", pid),
        Some(&pkg_user),
        Some(json!({ "combinationKey": "k20", "price": "10.00" })),
        None,
    );
    assert_eq!(s, 400);
    assert_eq!(body["error"].as_str(), Some("validation_failed"));
    assert_eq!(body["details"]["limit"].as_u64(), Some(20));
}

#[test]
fn reject_cross_facility_inventory_link_on_publish() {
    wait_for_service();
    let pkg_user = login("test_pkg", "TestPkgPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let pid = new_package(&pkg_user, &facility);
    // link to a random (non-existent / cross-facility) inventory id:
    let fake_inv = uuid::Uuid::new_v4().to_string();
    let (vs, vb) = req_json(
        "POST",
        &format!("/api/packages/{}/variants", pid),
        Some(&pkg_user),
        Some(json!({ "combinationKey": "x", "price": "10.00", "inventoryItemId": fake_inv })),
        None,
    );
    assert_eq!(vs, 201, "create variant: {}", vb);
    let (s, body) = req_json(
        "POST",
        &format!("/api/packages/{}/publish", pid),
        Some(&pkg_user),
        None,
        None,
    );
    assert_eq!(s, 400);
    assert_eq!(body["error"].as_str(), Some("validation_failed"));
    assert_eq!(body["details"]["inventoryItemId"].as_str(), Some(&*fake_inv));
}

#[test]
fn publish_is_idempotent_with_same_request_id() {
    wait_for_service();
    let pkg_user = login("test_pkg", "TestPkgPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let pid = new_package(&pkg_user, &facility);
    let _v = new_variant(&pkg_user, &pid, "k", "20.00");
    let rid = uuid::Uuid::new_v4().to_string();
    let (s1, b1) = req_json(
        "POST",
        &format!("/api/packages/{}/publish", pid),
        Some(&pkg_user),
        None,
        Some(&rid),
    );
    assert_eq!(s1, 200);
    assert_eq!(b1["status"].as_str(), Some("PUBLISHED"));

    let (s2, b2) = req_json(
        "POST",
        &format!("/api/packages/{}/publish", pid),
        Some(&pkg_user),
        None,
        Some(&rid),
    );
    assert_eq!(s2, 200);
    assert_eq!(b1, b2, "replay must return byte-identical body");
}
