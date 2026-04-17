mod common;

use common::{get_facility_id, login, req_json, wait_for_service};
use serde_json::json;

fn create_lost_found_submit(token: &str, facility_id: &str) -> String {
    let (s, body) = req_json(
        "POST",
        "/api/lost-found/items",
        Some(token),
        Some(json!({
            "facilityId": facility_id,
            "title": "Wallet",
            "category": "found",
            "tags": ["brown"],
            "eventDate": "07/04/2026",
            "eventTime": "2:00 PM",
            "locationText": "entrance"
        })),
        None,
    );
    assert_eq!(s, 201, "create: {}", body);
    let id = body["id"].as_str().unwrap().to_string();
    let (s2, _) = req_json(
        "POST",
        &format!("/api/lost-found/items/{}/submit", id),
        Some(token),
        None,
        None,
    );
    assert_eq!(s2, 200);
    id
}

#[test]
fn submit_enqueues_outbox_and_inbox_rows() {
    wait_for_service();
    let desk = login("test_desk", "TestDeskPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let notif = login("test_notif", "TestNotifPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let _id = create_lost_found_submit(&desk, &facility);

    let (s, body) = req_json("GET", "/api/notifications/outbox", Some(&notif), None, None);
    assert_eq!(s, 200);
    assert!(body["count"].as_u64().unwrap() > 0);

    let (s2, body2) = req_json("GET", "/api/notifications/inbox", Some(&desk), None, None);
    assert_eq!(s2, 200);
    assert!(body2["count"].as_u64().unwrap() > 0);
}

#[test]
fn outbox_retry_three_failures_then_dead() {
    wait_for_service();
    let desk = login("test_desk", "TestDeskPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let notif = login("test_notif", "TestNotifPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let _ = create_lost_found_submit(&desk, &facility);

    let (_, outbox) = req_json(
        "GET",
        "/api/notifications/outbox?status=PENDING",
        Some(&notif),
        None,
        None,
    );
    let arr = outbox["outbox"].as_array().unwrap();
    let id = arr.last().unwrap()["id"].as_str().unwrap().to_string();

    for _ in 0..4 {
        let (s, _) = req_json(
            "POST",
            "/api/notifications/outbox/import-results",
            Some(&notif),
            Some(json!({ "results": [{ "id": id, "status": "FAILED", "error": "offline" }] })),
            None,
        );
        assert_eq!(s, 200);
    }

    let (_, after) = req_json(
        "GET",
        "/api/notifications/outbox",
        Some(&notif),
        None,
        None,
    );
    let row = after["outbox"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"].as_str() == Some(&id))
        .expect("row still present");
    assert_eq!(row["status"].as_str(), Some("DEAD"));
    assert_eq!(row["attemptCount"].as_u64(), Some(4));
}

#[test]
fn export_then_import_ack_moves_to_sent() {
    wait_for_service();
    let desk = login("test_desk", "TestDeskPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let notif = login("test_notif", "TestNotifPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let _ = create_lost_found_submit(&desk, &facility);

    let c = common::client();
    let r = c
        .get(format!("{}/api/notifications/outbox/export", common::base_url()))
        .bearer_auth(&notif)
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    let text = r.text().unwrap();
    let last_line = text.lines().last().unwrap();
    let obj: serde_json::Value = serde_json::from_str(last_line).unwrap();
    let id = obj["id"].as_str().unwrap().to_string();

    let (s, body) = req_json(
        "POST",
        "/api/notifications/outbox/import-results",
        Some(&notif),
        Some(json!({ "results": [{ "id": id, "status": "SENT" }] })),
        None,
    );
    assert_eq!(s, 200);
    assert_eq!(body["acked"].as_u64(), Some(1));

    let (_, list) = req_json(
        "GET",
        "/api/notifications/outbox?status=SENT",
        Some(&notif),
        None,
        None,
    );
    let found = list["outbox"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["id"].as_str() == Some(&id));
    assert!(found);
}

#[test]
fn opt_out_skips_enqueue_entirely() {
    wait_for_service();
    let desk = login("test_desk", "TestDeskPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");

    let (s, _) = req_json(
        "PUT",
        "/api/notifications/subscriptions",
        Some(&desk),
        Some(json!({ "subscriptions": [{ "eventKind": "submission", "enabled": false }] })),
        None,
    );
    assert_eq!(s, 200);

    let (_, before) = req_json("GET", "/api/notifications/inbox", Some(&desk), None, None);
    let before_count = before["count"].as_u64().unwrap();
    let _ = create_lost_found_submit(&desk, &facility);
    let (_, after) = req_json("GET", "/api/notifications/inbox", Some(&desk), None, None);
    let after_count = after["count"].as_u64().unwrap();
    assert_eq!(before_count, after_count, "opt-out should not enqueue");

    // restore default-on for subsequent tests
    let (s2, _) = req_json(
        "PUT",
        "/api/notifications/subscriptions",
        Some(&desk),
        Some(json!({ "subscriptions": [{ "eventKind": "submission", "enabled": true }] })),
        None,
    );
    assert_eq!(s2, 200);
}

#[test]
fn template_with_disallowed_variable_rejected() {
    wait_for_service();
    let notif = login("test_notif", "TestNotifPassword123");
    let (s, body) = req_json(
        "POST",
        "/api/notifications/templates",
        Some(&notif),
        Some(json!({
            "code": "bad.template",
            "subject": "Hi {{ system.secret }}",
            "body": "body"
        })),
        None,
    );
    assert_eq!(s, 400);
    assert_eq!(body["error"].as_str(), Some("validation_failed"));
    assert_eq!(body["details"]["variable"].as_str(), Some("system.secret"));
}
