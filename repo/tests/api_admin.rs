mod common;

use common::{login, req_json, wait_for_service};
use serde_json::json;

#[test]
fn admin_user_update_and_reset_password() {
    wait_for_service();
    let admin = login("test_admin", "TestAdminPassword123");

    // Create a brand-new user via admin API.
    let username = format!("admin_created_{}", uuid::Uuid::new_v4().simple());
    let (cs, cb) = req_json(
        "POST",
        "/api/admin/users",
        Some(&admin),
        Some(json!({
            "username": username,
            "password": "FirstPassword12345",
            "displayName": "Made By Admin",
        })),
        None,
    );
    assert_eq!(cs, 201, "{}", cb);
    let user_id = cb["id"].as_str().unwrap().to_string();
    assert_eq!(cb["displayName"].as_str(), Some("Made By Admin"));

    // Update displayName and deactivate.
    let (us, ub) = req_json(
        "PUT",
        &format!("/api/admin/users/{}", user_id),
        Some(&admin),
        Some(json!({
            "displayName": "Renamed",
            "isActive": false,
        })),
        None,
    );
    assert_eq!(us, 200, "{}", ub);
    assert_eq!(ub["displayName"].as_str(), Some("Renamed"));
    assert_eq!(ub["isActive"].as_bool(), Some(false));

    // Reset password policy enforcement.
    let (ps, pb) = req_json(
        "POST",
        &format!("/api/admin/users/{}/reset-password", user_id),
        Some(&admin),
        Some(json!({ "newPassword": "short" })),
        None,
    );
    assert_eq!(ps, 400);
    assert_eq!(pb["error"].as_str(), Some("validation_failed"));

    let (ps2, pb2) = req_json(
        "POST",
        &format!("/api/admin/users/{}/reset-password", user_id),
        Some(&admin),
        Some(json!({ "newPassword": "NewValidPassword12345" })),
        None,
    );
    assert_eq!(ps2, 200, "{}", pb2);
}

#[test]
fn admin_role_crud_roundtrip() {
    wait_for_service();
    let admin = login("test_admin", "TestAdminPassword123");

    let name = format!("TEST_ROLE_{}", uuid::Uuid::new_v4().simple());
    let (cs, cb) = req_json(
        "POST",
        "/api/admin/roles",
        Some(&admin),
        Some(json!({
            "name": name,
            "dataScope": "facility:*",
            "fieldAllowlist": ["gov_id"],
            "permissionCodes": ["lost_found.edit_draft"]
        })),
        None,
    );
    assert_eq!(cs, 201, "{}", cb);
    let role_id = cb["id"].as_str().unwrap().to_string();
    assert_eq!(cb["dataScope"].as_str(), Some("facility:*"));
    assert_eq!(cb["fieldAllowlist"][0].as_str(), Some("gov_id"));

    let (ls, lb) = req_json("GET", "/api/admin/roles", Some(&admin), None, None);
    assert_eq!(ls, 200);
    let found = lb["roles"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["id"].as_str() == Some(&role_id));
    assert!(found);

    let (us, ub) = req_json(
        "PUT",
        &format!("/api/admin/roles/{}", role_id),
        Some(&admin),
        Some(json!({
            "name": name,
            "dataScope": "facility:*",
            "fieldAllowlist": ["gov_id", "private_notes"],
            "permissionCodes": ["lost_found.edit_draft", "lost_found.review"]
        })),
        None,
    );
    assert_eq!(us, 200, "{}", ub);
    assert_eq!(ub["fieldAllowlist"].as_array().unwrap().len(), 2);

    let (ds, _) = req_json(
        "DELETE",
        &format!("/api/admin/roles/{}", role_id),
        Some(&admin),
        None,
        None,
    );
    assert_eq!(ds, 200);
}

#[test]
fn admin_facility_crud_roundtrip() {
    wait_for_service();
    let admin = login("test_admin", "TestAdminPassword123");
    let code = format!("TF{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);

    let (cs, cb) = req_json(
        "POST",
        "/api/admin/facilities",
        Some(&admin),
        Some(json!({ "name": "Test Facility", "code": code })),
        None,
    );
    assert_eq!(cs, 201, "{}", cb);
    let fid = cb["id"].as_str().unwrap().to_string();
    assert_eq!(cb["code"].as_str(), Some(code.as_str()));
    assert_eq!(cb["isActive"].as_bool(), Some(true));

    let (us, ub) = req_json(
        "PUT",
        &format!("/api/admin/facilities/{}", fid),
        Some(&admin),
        Some(json!({ "name": "Renamed Facility", "code": code, "isActive": true })),
        None,
    );
    assert_eq!(us, 200, "{}", ub);
    assert_eq!(ub["name"].as_str(), Some("Renamed Facility"));

    let (ds, _) = req_json(
        "DELETE",
        &format!("/api/admin/facilities/{}", fid),
        Some(&admin),
        None,
        None,
    );
    assert_eq!(ds, 200);
    let (_, after) = req_json("GET", "/api/admin/facilities", Some(&admin), None, None);
    let found = after["facilities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"].as_str() == Some(&fid));
    assert_eq!(found.unwrap()["isActive"].as_bool(), Some(false));
}

#[test]
fn permissions_catalog_listed() {
    wait_for_service();
    let admin = login("test_admin", "TestAdminPassword123");
    let (s, body) = req_json("GET", "/api/admin/permissions", Some(&admin), None, None);
    assert_eq!(s, 200);
    let codes: Vec<String> = body["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["code"].as_str().unwrap().to_string())
        .collect();
    for expected in [
        "system.admin",
        "lost_found.edit_draft",
        "lost_found.review",
        "assets.write",
        "assets.transition",
        "volunteers.write",
        "packages.write",
        "notifications.admin",
    ] {
        assert!(codes.contains(&expected.to_string()), "missing permission {}", expected);
    }
}

#[test]
fn audit_log_list_returns_recent_entries() {
    wait_for_service();
    let admin = login("test_admin", "TestAdminPassword123");
    let (s, body) = req_json(
        "GET",
        "/api/admin/audit/logs?limit=50",
        Some(&admin),
        None,
        None,
    );
    assert_eq!(s, 200);
    assert!(body["logs"].is_array());
    assert!(body["count"].as_u64().unwrap() > 0);
    let entry = &body["logs"][0];
    for key in ["id", "entityType", "entityId", "action", "createdAt"] {
        assert!(entry.get(key).is_some(), "audit entry missing {}: {}", key, entry);
    }
}
