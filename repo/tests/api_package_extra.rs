mod common;

use common::{get_facility_id, login, req_json, wait_for_service};
use serde_json::json;

fn new_package(token: &str, facility: &str) -> String {
    let (_, body) = req_json(
        "POST",
        "/api/packages",
        Some(token),
        Some(json!({
            "facilityId": facility,
            "name": format!("Extra-{}", uuid::Uuid::new_v4()),
            "basePrice": "100.00"
        })),
        None,
    );
    body["id"].as_str().unwrap().to_string()
}

fn new_variant(token: &str, pid: &str, key: &str, price: &str) -> String {
    let (_, body) = req_json(
        "POST",
        &format!("/api/packages/{}/variants", pid),
        Some(token),
        Some(json!({ "combinationKey": key, "price": price })),
        None,
    );
    body["id"].as_str().unwrap().to_string()
}

#[test]
fn variant_update_changes_price_and_serializes_two_decimal() {
    wait_for_service();
    let pkg = login("test_pkg", "TestPkgPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let pid = new_package(&pkg, &facility);
    let vid = new_variant(&pkg, &pid, "k-update", "10.00");

    let (s, body) = req_json(
        "PUT",
        &format!("/api/packages/{}/variants/{}", pid, vid),
        Some(&pkg),
        Some(json!({ "price": "12.50" })),
        None,
    );
    assert_eq!(s, 200, "{}", body);
    assert_eq!(body["price"].as_str(), Some("12.50"));
}

#[test]
fn variant_update_rejects_negative_price() {
    wait_for_service();
    let pkg = login("test_pkg", "TestPkgPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let pid = new_package(&pkg, &facility);
    let vid = new_variant(&pkg, &pid, "k-neg", "5.00");

    let (s, body) = req_json(
        "PUT",
        &format!("/api/packages/{}/variants/{}", pid, vid),
        Some(&pkg),
        Some(json!({ "price": "-1.00" })),
        None,
    );
    assert_eq!(s, 400);
    assert_eq!(body["error"].as_str(), Some("validation_failed"));
    assert_eq!(body["details"]["field"].as_str(), Some("price"));
}

#[test]
fn variant_delete_removes_row_and_package_listing_reflects_it() {
    wait_for_service();
    let pkg = login("test_pkg", "TestPkgPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let pid = new_package(&pkg, &facility);
    let vid = new_variant(&pkg, &pid, "k-del", "9.00");

    let (ds, _) = req_json(
        "DELETE",
        &format!("/api/packages/{}/variants/{}", pid, vid),
        Some(&pkg),
        None,
        None,
    );
    assert_eq!(ds, 200);
    let (_, list) = req_json(
        "GET",
        &format!("/api/packages/{}/variants", pid),
        Some(&pkg),
        None,
        None,
    );
    let present = list["variants"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["id"].as_str() == Some(&vid));
    assert!(!present);
}

#[test]
fn package_delete_makes_get_return_404() {
    wait_for_service();
    let pkg = login("test_pkg", "TestPkgPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let pid = new_package(&pkg, &facility);
    let (ds, _) = req_json("DELETE", &format!("/api/packages/{}", pid), Some(&pkg), None, None);
    assert_eq!(ds, 200);
    let (s, body) = req_json("GET", &format!("/api/packages/{}", pid), Some(&pkg), None, None);
    assert_eq!(s, 404);
    assert_eq!(body["error"].as_str(), Some("not_found"));
}

#[test]
fn unpublish_from_non_published_returns_409() {
    wait_for_service();
    let pkg = login("test_pkg", "TestPkgPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let pid = new_package(&pkg, &facility);
    let (s, body) = req_json(
        "POST",
        &format!("/api/packages/{}/unpublish", pid),
        Some(&pkg),
        None,
        None,
    );
    assert_eq!(s, 409);
    assert_eq!(body["error"].as_str(), Some("invalid_transition"));
}
