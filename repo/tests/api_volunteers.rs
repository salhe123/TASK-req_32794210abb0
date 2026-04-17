mod common;

use chrono::{Duration, Utc};
use common::{get_facility_id, login, req_json, wait_for_service};
use serde_json::json;

fn create_volunteer(token: &str, facility_id: &str, name: &str) -> String {
    // Helper used by tests that don't care about sensitive-field writes.
    // govId/privateNotes are left off so the default VOLUNTEER_COORDINATOR
    // role (no field allowlist) is allowed to create.
    let (s, body) = req_json(
        "POST",
        "/api/volunteers",
        Some(token),
        Some(json!({
            "facilityId": facility_id,
            "fullName": name,
            "contactEmail": "a@example.com",
        })),
        None,
    );
    assert_eq!(s, 201, "{}", body);
    body["id"].as_str().unwrap().to_string()
}

#[test]
fn expiring_within_days_filter_matches_and_triggers_notification() {
    wait_for_service();
    let vol_user = login("test_vol", "TestVolPassword123");
    let vol_admin = login("test_vol_full", "TestVolFullPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let notif = login("test_notif", "TestNotifPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");

    let name = format!("Expiring-{}", uuid::Uuid::new_v4());
    let vid = create_volunteer(&vol_user, &facility, &name);

    let expires = (Utc::now().date_naive() + Duration::days(10))
        .format("%m/%d/%Y")
        .to_string();
    let issued = Utc::now()
        .date_naive()
        .format("%m/%d/%Y")
        .to_string();

    let (_, outbox_before) = req_json(
        "GET",
        "/api/notifications/outbox",
        Some(&notif),
        None,
        None,
    );
    let before_count = outbox_before["count"].as_u64().unwrap();

    // Certificate writes require the sensitive-field allowlist.
    let (qs, qb) = req_json(
        "POST",
        &format!("/api/volunteers/{}/qualifications", vid),
        Some(&vol_admin),
        Some(json!({
            "kind": "CPR",
            "issuer": "Red Cross",
            "certificate": "CERT-ABC-123456",
            "issuedOn": issued,
            "expiresOn": expires,
        })),
        None,
    );
    assert_eq!(qs, 201, "{}", qb);
    assert_eq!(qb["expiresOn"].as_str(), Some(expires.as_str()));

    let (_, outbox_after) = req_json(
        "GET",
        "/api/notifications/outbox",
        Some(&notif),
        None,
        None,
    );
    let after_count = outbox_after["count"].as_u64().unwrap();
    assert!(
        after_count > before_count,
        "expiring qualification should enqueue an outbox row (before={}, after={})",
        before_count,
        after_count
    );

    let (ls, lb) = req_json(
        "GET",
        "/api/volunteers?expiringWithinDays=30",
        Some(&vol_user),
        None,
        None,
    );
    assert_eq!(ls, 200);
    let volunteers = lb["volunteers"].as_array().unwrap();
    assert!(
        volunteers.iter().any(|v| v["id"].as_str() == Some(&vid)),
        "expiringWithinDays=30 list should include the volunteer: {}",
        lb
    );
}

#[test]
fn expiring_filter_excludes_volunteers_with_no_expiring_quals() {
    wait_for_service();
    let vol_user = login("test_vol", "TestVolPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let vid = create_volunteer(&vol_user, &facility, &format!("NoExp-{}", uuid::Uuid::new_v4()));
    let (s, b) = req_json(
        "GET",
        "/api/volunteers?expiringWithinDays=30",
        Some(&vol_user),
        None,
        None,
    );
    assert_eq!(s, 200);
    let found = b["volunteers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["id"].as_str() == Some(&vid));
    assert!(!found, "volunteer with no expiring quals must be absent");
}

#[test]
fn certificate_masked_by_default_and_full_with_allowlist() {
    wait_for_service();
    let vol_user = login("test_vol", "TestVolPassword123");
    let vol_admin = login("test_vol_full", "TestVolFullPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");

    let vid = create_volunteer(&vol_user, &facility, &format!("Cert-{}", uuid::Uuid::new_v4()));
    let issued = Utc::now().date_naive().format("%m/%d/%Y").to_string();
    // Writing a certificate requires the sensitive-field allowlist, so we
    // post the qualification as vol_admin; the later read-back as vol_user
    // is the mask check.
    let (qs, qb) = req_json(
        "POST",
        &format!("/api/volunteers/{}/qualifications", vid),
        Some(&vol_admin),
        Some(json!({
            "kind": "First Aid",
            "issuer": "Red Cross",
            "certificate": "CRT-SECRET-999999",
            "issuedOn": issued,
        })),
        None,
    );
    assert_eq!(qs, 201, "{}", qb);
    assert_eq!(qb["certificate"].as_str(), Some("CRT-SECRET-999999"));

    let (_, masked) = req_json(
        "GET",
        &format!("/api/volunteers/{}/qualifications", vid),
        Some(&vol_user),
        None,
        None,
    );
    let masked_cert = masked["qualifications"]
        .as_array()
        .unwrap()
        .iter()
        .find(|q| q["kind"].as_str() == Some("First Aid"))
        .expect("qual present")["certificate"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(masked_cert.ends_with("9999"));
    assert!(masked_cert.contains('*'));

    let (_, full) = req_json(
        "GET",
        &format!("/api/volunteers/{}/qualifications", vid),
        Some(&vol_admin),
        None,
        None,
    );
    let quals = full["qualifications"].as_array().unwrap();
    let q = quals
        .iter()
        .find(|q| q["kind"].as_str() == Some("First Aid"))
        .expect("qual present");
    assert_eq!(q["certificate"].as_str(), Some("CRT-SECRET-999999"));
}

#[test]
fn volunteer_update_and_soft_delete() {
    wait_for_service();
    let vol_user = login("test_vol", "TestVolPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let vid = create_volunteer(&vol_user, &facility, &format!("Upd-{}", uuid::Uuid::new_v4()));

    let (us, ub) = req_json(
        "PUT",
        &format!("/api/volunteers/{}", vid),
        Some(&vol_user),
        Some(json!({ "fullName": "Updated Name", "isActive": true })),
        None,
    );
    assert_eq!(us, 200, "{}", ub);
    assert_eq!(ub["fullName"].as_str(), Some("Updated Name"));
    assert_eq!(ub["isActive"].as_bool(), Some(true));

    let (ds, db_) = req_json(
        "DELETE",
        &format!("/api/volunteers/{}", vid),
        Some(&vol_user),
        None,
        None,
    );
    assert_eq!(ds, 200, "{}", db_);
    let (_, after) = req_json(
        "GET",
        &format!("/api/volunteers/{}", vid),
        Some(&vol_user),
        None,
        None,
    );
    assert_eq!(after["isActive"].as_bool(), Some(false));
}

#[test]
fn qualification_delete_removes_row() {
    wait_for_service();
    let vol_user = login("test_vol", "TestVolPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let vid = create_volunteer(&vol_user, &facility, &format!("DelQ-{}", uuid::Uuid::new_v4()));
    let issued = Utc::now().date_naive().format("%m/%d/%Y").to_string();
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
    let (s, body) = req_json(
        "DELETE",
        &format!("/api/volunteers/{}/qualifications/{}", vid, qid),
        Some(&vol_user),
        None,
        None,
    );
    assert_eq!(s, 200, "{}", body);
    let (_, listing) = req_json(
        "GET",
        &format!("/api/volunteers/{}/qualifications", vid),
        Some(&vol_user),
        None,
        None,
    );
    let present = listing["qualifications"]
        .as_array()
        .unwrap()
        .iter()
        .any(|q| q["id"].as_str() == Some(&qid));
    assert!(!present);
}
