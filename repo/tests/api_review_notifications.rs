mod common;

use common::{get_facility_id, login, req_json, wait_for_service};
use serde_json::{json, Value};

fn create_and_submit(desk: &str, facility: &str, title: &str) -> String {
    let (_, created) = req_json(
        "POST",
        "/api/lost-found/items",
        Some(desk),
        Some(json!({
            "facilityId": facility,
            "title": title,
            "category": "other",
            "tags": [],
            "eventDate": "07/04/2026",
            "eventTime": "2:00 PM",
            "locationText": "door"
        })),
        None,
    );
    let id = created["id"].as_str().unwrap().to_string();
    let (s, _) = req_json(
        "POST",
        &format!("/api/lost-found/items/{}/submit", id),
        Some(desk),
        None,
        None,
    );
    assert_eq!(s, 200);
    id
}

fn inbox_subjects(token: &str) -> Vec<String> {
    let (_, body) = req_json("GET", "/api/notifications/inbox", Some(token), None, None);
    body["inbox"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["subject"].as_str().unwrap().to_string())
        .collect()
}

fn inbox_items(token: &str) -> Vec<Value> {
    let (_, body) = req_json("GET", "/api/notifications/inbox", Some(token), None, None);
    body["inbox"].as_array().unwrap().clone()
}

#[test]
fn approve_delivers_review_notification_to_submitter() {
    wait_for_service();
    let desk = login("test_desk", "TestDeskPassword123");
    let reviewer = login("test_review", "TestReviewPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");

    let title = format!("APPROVE-NOTIFY-{}", uuid::Uuid::new_v4());
    let id = create_and_submit(&desk, &facility, &title);

    let (s, _) = req_json(
        "POST",
        &format!("/api/lost-found/items/{}/approve", id),
        Some(&reviewer),
        None,
        None,
    );
    assert_eq!(s, 200);

    let subs = inbox_subjects(&desk);
    assert!(
        subs.iter().any(|s| s.contains("approved") && s.contains(&title)),
        "desk inbox should contain an approval notification with the item title. inbox={:?}",
        subs
    );

    let items = inbox_items(&desk);
    let matching = items
        .iter()
        .find(|n| {
            n["eventKind"].as_str() == Some("review")
                && n["subject"].as_str().unwrap_or("").contains(&title)
        })
        .expect("review notification on desk inbox");
    assert_eq!(matching["payload"]["item"]["status"].as_str(), Some("PUBLISHED"));
}

#[test]
fn bounce_delivers_review_notification_with_reason_to_submitter() {
    wait_for_service();
    let desk = login("test_desk", "TestDeskPassword123");
    let reviewer = login("test_review", "TestReviewPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");

    let title = format!("BOUNCE-NOTIFY-{}", uuid::Uuid::new_v4());
    let id = create_and_submit(&desk, &facility, &title);

    let (s, _) = req_json(
        "POST",
        &format!("/api/lost-found/items/{}/bounce", id),
        Some(&reviewer),
        Some(json!({ "reason": "Blurry photo" })),
        None,
    );
    assert_eq!(s, 200);

    let items = inbox_items(&desk);
    let n = items
        .iter()
        .find(|n| {
            n["eventKind"].as_str() == Some("review")
                && n["subject"].as_str().unwrap_or("").contains(&title)
        })
        .expect("bounce notification on desk inbox");
    assert!(
        n["body"].as_str().unwrap_or("").contains("Blurry photo"),
        "bounce body should carry the reason: {}",
        n
    );
    assert_eq!(
        n["payload"]["item"]["bounceReason"].as_str(),
        Some("Blurry photo")
    );
}
