mod common;

use common::{get_facility_id, login, req_json, wait_for_service};
use serde_json::json;

#[test]
fn admin_route_rejects_desk_staff() {
    wait_for_service();
    let desk = login("test_desk", "TestDeskPassword123");
    let (s, body) = req_json("GET", "/api/admin/users", Some(&desk), None, None);
    assert_eq!(s, 403);
    assert_eq!(body["error"].as_str(), Some("forbidden"));
}

#[test]
fn audit_logs_hidden_from_non_admin() {
    wait_for_service();
    let desk = login("test_desk", "TestDeskPassword123");
    let (s, _) = req_json("GET", "/api/admin/audit/logs", Some(&desk), None, None);
    assert_eq!(s, 403);
}

#[test]
fn data_scope_filters_out_of_facility_access() {
    wait_for_service();
    let desk_other = login("test_other", "TestOtherPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let default_facility = get_facility_id(&admin, "DEFAULT");

    // test_other only has DESK_STAFF_OTHER_FACILITY (scoped to SECONDARY).
    let (s, body) = req_json(
        "POST",
        "/api/lost-found/items",
        Some(&desk_other),
        Some(json!({
            "facilityId": default_facility,
            "title": "x",
            "category": "other",
            "tags": [],
        })),
        None,
    );
    assert_eq!(s, 403, "body={}", body);
    assert_eq!(body["error"].as_str(), Some("out_of_scope"));
}

#[test]
fn volunteer_gov_id_masked_by_default_and_full_with_allowlist() {
    wait_for_service();
    let vol = login("test_vol", "TestVolPassword123");
    let vol_admin = login("test_vol_full", "TestVolFullPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");

    // Creating a volunteer WITH sensitive fields requires the field allowlist
    // (VOLUNTEER_ADMIN role), not just volunteers.write.
    let (s, body) = req_json(
        "POST",
        "/api/volunteers",
        Some(&vol_admin),
        Some(json!({
            "facilityId": facility,
            "fullName": "Alice Example",
            "govId": "SSN-123456789",
            "privateNotes": "sensitive note"
        })),
        None,
    );
    assert_eq!(s, 201, "create vol: {}", body);
    let id = body["id"].as_str().unwrap().to_string();

    // vol_admin is on the allowlist so the response unmasks these for them.
    assert_eq!(body["govId"].as_str(), Some("SSN-123456789"));
    assert_eq!(body["privateNotes"].as_str(), Some("sensitive note"));

    // Reading back as a non-allowlisted role returns the masked form.
    let (_, masked) = req_json(
        "GET",
        &format!("/api/volunteers/{}", id),
        Some(&vol),
        None,
        None,
    );
    assert!(masked["govId"].as_str().unwrap().ends_with("6789"));
    assert!(masked["govId"].as_str().unwrap().contains('*'));
    assert_eq!(masked["privateNotes"].as_str(), Some("****"));

    // Same row, but this time the allowlisted role gets the full values back.
    let (_, full) = req_json(
        "GET",
        &format!("/api/volunteers/{}", id),
        Some(&vol_admin),
        None,
        None,
    );
    assert_eq!(full["govId"].as_str(), Some("SSN-123456789"));
    assert_eq!(full["privateNotes"].as_str(), Some("sensitive note"));
}

#[test]
fn volunteer_sensitive_write_rejected_without_allowlist() {
    // Audit round 3 / High #3: roles with volunteers.write but no field
    // allowlist must NOT be able to set sensitive fields on create/update.
    wait_for_service();
    let vol = login("test_vol", "TestVolPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");

    let (s, body) = req_json(
        "POST",
        "/api/volunteers",
        Some(&vol),
        Some(json!({
            "facilityId": facility,
            "fullName": "No Secrets Allowed",
            "govId": "SSN-222333444"
        })),
        None,
    );
    assert_eq!(s, 403, "body={}", body);
    assert_eq!(body["error"].as_str(), Some("forbidden"));
}

#[test]
fn same_user_replay_returns_byte_identical_body() {
    wait_for_service();
    let pkg_user = login("test_pkg", "TestPkgPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let rid = uuid::Uuid::new_v4().to_string();
    let payload = json!({
        "facilityId": facility,
        "name": "Idempotent Package",
        "description": "via idempotency-key",
        "basePrice": "99.00"
    });
    let (s1, b1) = req_json(
        "POST",
        "/api/packages",
        Some(&pkg_user),
        Some(payload.clone()),
        Some(&rid),
    );
    assert_eq!(s1, 201);
    let (s2, b2) = req_json(
        "POST",
        "/api/packages",
        Some(&pkg_user),
        Some(payload),
        Some(&rid),
    );
    assert_eq!(s2, 201);
    assert_eq!(b1, b2);
}

#[test]
fn cross_user_request_id_returns_409_conflict() {
    wait_for_service();
    let pkg_user = login("test_pkg", "TestPkgPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let rid = uuid::Uuid::new_v4().to_string();
    let payload = json!({
        "facilityId": facility,
        "name": "Pkg A",
        "basePrice": "10.00"
    });
    let (s1, _) = req_json(
        "POST",
        "/api/packages",
        Some(&pkg_user),
        Some(payload.clone()),
        Some(&rid),
    );
    assert_eq!(s1, 201);
    let (s2, body2) = req_json(
        "POST",
        "/api/packages",
        Some(&admin),
        Some(payload),
        Some(&rid),
    );
    assert_eq!(s2, 409);
    assert_eq!(body2["error"].as_str(), Some("idempotency_conflict"));
}
