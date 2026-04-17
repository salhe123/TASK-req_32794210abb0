mod common;

use common::{get_facility_id, login, req_json, wait_for_service};
use serde_json::json;

fn entity_actions(token: &str, entity_type: &str, entity_id: &str) -> Vec<String> {
    let (s, body) = req_json(
        "GET",
        &format!(
            "/api/admin/audit/logs?entityType={}&entityId={}&limit=200",
            entity_type, entity_id
        ),
        Some(token),
        None,
        None,
    );
    assert_eq!(s, 200, "audit list: {}", body);
    body["logs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["action"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn volunteer_mutations_produce_audit_rows() {
    wait_for_service();
    let vol_user = login("test_vol", "TestVolPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");

    let (_, created) = req_json(
        "POST",
        "/api/volunteers",
        Some(&vol_user),
        Some(json!({
            "facilityId": facility,
            "fullName": format!("Audit-Vol-{}", uuid::Uuid::new_v4()),
        })),
        None,
    );
    let vid = created["id"].as_str().unwrap().to_string();

    let (_, _) = req_json(
        "PUT",
        &format!("/api/volunteers/{}", vid),
        Some(&vol_user),
        Some(json!({ "fullName": "Updated" })),
        None,
    );
    let (_, _) = req_json(
        "DELETE",
        &format!("/api/volunteers/{}", vid),
        Some(&vol_user),
        None,
        None,
    );

    let actions = entity_actions(&admin, "volunteer", &vid);
    for expected in ["create", "update", "deactivate"] {
        assert!(
            actions.contains(&expected.to_string()),
            "volunteer audit missing {:?}: {:?}",
            expected,
            actions
        );
    }
}

#[test]
fn package_update_and_delete_emit_audit_rows() {
    wait_for_service();
    let pkg_user = login("test_pkg", "TestPkgPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let (_, pkg) = req_json(
        "POST",
        "/api/packages",
        Some(&pkg_user),
        Some(json!({
            "facilityId": facility,
            "name": format!("Audit-Pkg-{}", uuid::Uuid::new_v4()),
            "basePrice": "5.00"
        })),
        None,
    );
    let pid = pkg["id"].as_str().unwrap().to_string();

    let (_, _) = req_json(
        "PUT",
        &format!("/api/packages/{}", pid),
        Some(&pkg_user),
        Some(json!({ "basePrice": "7.50" })),
        None,
    );
    let (_, _) = req_json(
        "DELETE",
        &format!("/api/packages/{}", pid),
        Some(&pkg_user),
        None,
        None,
    );

    let actions = entity_actions(&admin, "package", &pid);
    for expected in ["create", "update", "delete"] {
        assert!(
            actions.contains(&expected.to_string()),
            "package audit missing {:?}: {:?}",
            expected,
            actions
        );
    }
}

#[test]
fn qualification_create_and_delete_emit_audit_rows() {
    wait_for_service();
    let vol_user = login("test_vol", "TestVolPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");

    let (_, v) = req_json(
        "POST",
        "/api/volunteers",
        Some(&vol_user),
        Some(json!({
            "facilityId": facility,
            "fullName": format!("Q-Vol-{}", uuid::Uuid::new_v4()),
        })),
        None,
    );
    let vid = v["id"].as_str().unwrap().to_string();
    let issued = chrono::Utc::now()
        .date_naive()
        .format("%m/%d/%Y")
        .to_string();
    let (_, q) = req_json(
        "POST",
        &format!("/api/volunteers/{}/qualifications", vid),
        Some(&vol_user),
        Some(json!({
            "kind": "Safety",
            "issuer": "OSHA",
            "issuedOn": issued,
        })),
        None,
    );
    let qid = q["id"].as_str().unwrap().to_string();
    let (_, _) = req_json(
        "DELETE",
        &format!("/api/volunteers/{}/qualifications/{}", vid, qid),
        Some(&vol_user),
        None,
        None,
    );

    let actions = entity_actions(&admin, "qualification", &qid);
    for expected in ["create", "delete"] {
        assert!(
            actions.contains(&expected.to_string()),
            "qualification audit missing {}: {:?}",
            expected,
            actions
        );
    }
}

#[test]
fn template_crud_emits_audit_rows() {
    wait_for_service();
    let notif = login("test_notif", "TestNotifPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let code = format!("tpl.audit.{}", uuid::Uuid::new_v4().simple());
    let (_, t) = req_json(
        "POST",
        "/api/notifications/templates",
        Some(&notif),
        Some(json!({
            "code": code,
            "subject": "Subj {{ item.title }}",
            "body": "Body",
        })),
        None,
    );
    let tid = t["id"].as_str().unwrap().to_string();
    let (_, _) = req_json(
        "PUT",
        &format!("/api/notifications/templates/{}", tid),
        Some(&notif),
        Some(json!({
            "code": code,
            "subject": "Subj updated",
            "body": "Body",
            "isActive": true,
        })),
        None,
    );
    let (_, _) = req_json(
        "DELETE",
        &format!("/api/notifications/templates/{}", tid),
        Some(&notif),
        None,
        None,
    );

    let actions = entity_actions(&admin, "notification_template", &tid);
    for expected in ["create", "update", "delete"] {
        assert!(
            actions.contains(&expected.to_string()),
            "template audit missing {}: {:?}",
            expected,
            actions
        );
    }
}

#[test]
fn admin_user_and_facility_and_role_mutations_audited() {
    wait_for_service();
    let admin = login("test_admin", "TestAdminPassword123");

    // user
    let (_, u) = req_json(
        "POST",
        "/api/admin/users",
        Some(&admin),
        Some(json!({
            "username": format!("audit_user_{}", uuid::Uuid::new_v4().simple()),
            "password": "AuditPassword1234",
            "displayName": "Audit",
        })),
        None,
    );
    let uid = u["id"].as_str().unwrap().to_string();
    let (_, _) = req_json(
        "PUT",
        &format!("/api/admin/users/{}", uid),
        Some(&admin),
        Some(json!({ "displayName": "Audit2" })),
        None,
    );
    let (_, _) = req_json(
        "PUT",
        &format!("/api/admin/users/{}/unlock", uid),
        Some(&admin),
        None,
        None,
    );
    let (_, _) = req_json(
        "POST",
        &format!("/api/admin/users/{}/reset-password", uid),
        Some(&admin),
        Some(json!({ "newPassword": "ResetPassword12345" })),
        None,
    );
    let user_actions = entity_actions(&admin, "user", &uid);
    for expected in ["create", "update", "unlock", "reset_password"] {
        assert!(
            user_actions.contains(&expected.to_string()),
            "user audit missing {}: {:?}",
            expected,
            user_actions
        );
    }

    // facility
    let code = format!("AUD{}", &uuid::Uuid::new_v4().simple().to_string()[..7]);
    let (_, f) = req_json(
        "POST",
        "/api/admin/facilities",
        Some(&admin),
        Some(json!({ "name": "audit fac", "code": code })),
        None,
    );
    let fid = f["id"].as_str().unwrap().to_string();
    let (_, _) = req_json(
        "PUT",
        &format!("/api/admin/facilities/{}", fid),
        Some(&admin),
        Some(json!({ "name": "renamed", "code": code, "isActive": true })),
        None,
    );
    let (_, _) = req_json(
        "DELETE",
        &format!("/api/admin/facilities/{}", fid),
        Some(&admin),
        None,
        None,
    );
    let facility_actions = entity_actions(&admin, "facility", &fid);
    for expected in ["create", "update", "deactivate"] {
        assert!(
            facility_actions.contains(&expected.to_string()),
            "facility audit missing {}: {:?}",
            expected,
            facility_actions
        );
    }

    // role
    let (_, r) = req_json(
        "POST",
        "/api/admin/roles",
        Some(&admin),
        Some(json!({
            "name": format!("AUDROLE_{}", uuid::Uuid::new_v4().simple()),
            "dataScope": "facility:*",
            "fieldAllowlist": [],
            "permissionCodes": []
        })),
        None,
    );
    let rid = r["id"].as_str().unwrap().to_string();
    let (_, _) = req_json(
        "DELETE",
        &format!("/api/admin/roles/{}", rid),
        Some(&admin),
        None,
        None,
    );
    let role_actions = entity_actions(&admin, "role", &rid);
    for expected in ["create", "delete"] {
        assert!(
            role_actions.contains(&expected.to_string()),
            "role audit missing {}: {:?}",
            expected,
            role_actions
        );
    }
}
