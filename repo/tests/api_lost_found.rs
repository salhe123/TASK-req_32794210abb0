mod common;

use base64::Engine;
use common::{get_facility_id, login, req_json, wait_for_service};
use serde_json::{json, Value};

fn tiny_png() -> String {
    // Build a valid 1x1 RGB PNG at runtime so we never rely on a hand-typed
    // byte table whose CRCs can drift.
    let mut buf = std::io::Cursor::new(Vec::new());
    let img = image::ImageBuffer::<image::Rgb<u8>, _>::from_pixel(1, 1, image::Rgb([255, 0, 0]));
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut buf, image::ImageFormat::Png)
        .expect("encode tiny png");
    base64::engine::general_purpose::STANDARD.encode(buf.into_inner())
}

fn create_draft(token: &str, facility_id: &str) -> Value {
    let (status, body) = req_json(
        "POST",
        "/api/lost-found/items",
        Some(token),
        Some(json!({
            "facilityId": facility_id,
            "title": "Red Umbrella",
            "description": "Left by the north gate",
            "category": "lost",
            "tags": ["red", "rain"],
            "eventDate": "07/04/2026",
            "eventTime": "3:15 PM",
            "locationText": "North gate bench"
        })),
        None,
    );
    assert_eq!(status, 201, "create draft failed: body={}", body);
    assert_eq!(body["status"].as_str(), Some("DRAFT"));
    assert_eq!(body["title"].as_str(), Some("Red Umbrella"));
    body
}

#[test]
fn full_workflow_submit_approve_unpublish_republish_and_audit() {
    wait_for_service();
    let desk = login("test_desk", "TestDeskPassword123");
    let review = login("test_review", "TestReviewPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");

    let draft = create_draft(&desk, &facility);
    let id = draft["id"].as_str().unwrap().to_string();

    let (s1, _) = req_json(
        "POST",
        &format!("/api/lost-found/items/{}/submit", id),
        Some(&desk),
        None,
        None,
    );
    assert_eq!(s1, 200);
    let (_, after_submit) = req_json(
        "GET",
        &format!("/api/lost-found/items/{}", id),
        Some(&desk),
        None,
        None,
    );
    assert_eq!(after_submit["status"].as_str(), Some("IN_REVIEW"));

    let (s2, _) = req_json(
        "POST",
        &format!("/api/lost-found/items/{}/approve", id),
        Some(&review),
        None,
        None,
    );
    assert_eq!(s2, 200);
    let (_, after_approve) = req_json(
        "GET",
        &format!("/api/lost-found/items/{}", id),
        Some(&review),
        None,
        None,
    );
    assert_eq!(after_approve["status"].as_str(), Some("PUBLISHED"));

    let (s3, _) = req_json(
        "POST",
        &format!("/api/lost-found/items/{}/unpublish", id),
        Some(&review),
        None,
        None,
    );
    assert_eq!(s3, 200);
    let (s4, _) = req_json(
        "POST",
        &format!("/api/lost-found/items/{}/republish", id),
        Some(&review),
        None,
        None,
    );
    assert_eq!(s4, 200);

    let (s5, hist) = req_json(
        "GET",
        &format!("/api/lost-found/items/{}/history", id),
        Some(&review),
        None,
        None,
    );
    assert_eq!(s5, 200);
    let actions: Vec<String> = hist["history"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["action"].as_str().unwrap().to_string())
        .collect();
    for expected in ["create", "submit", "approve", "unpublish", "republish"] {
        assert!(
            actions.contains(&expected.to_string()),
            "missing audit action {} in {:?}",
            expected,
            actions
        );
    }
}

#[test]
fn bounce_requires_reason_and_returns_to_draft() {
    wait_for_service();
    let desk = login("test_desk", "TestDeskPassword123");
    let review = login("test_review", "TestReviewPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");

    let draft = create_draft(&desk, &facility);
    let id = draft["id"].as_str().unwrap().to_string();
    let (_, _) = req_json(
        "POST",
        &format!("/api/lost-found/items/{}/submit", id),
        Some(&desk),
        None,
        None,
    );

    // missing reason
    let (status, body) = req_json(
        "POST",
        &format!("/api/lost-found/items/{}/bounce", id),
        Some(&review),
        Some(json!({ "reason": "" })),
        None,
    );
    assert_eq!(status, 400);
    assert_eq!(body["error"].as_str(), Some("validation_failed"));

    let (s2, body2) = req_json(
        "POST",
        &format!("/api/lost-found/items/{}/bounce", id),
        Some(&review),
        Some(json!({ "reason": "Need more detail" })),
        None,
    );
    assert_eq!(s2, 200);
    assert_eq!(body2["status"].as_str(), Some("DRAFT"));
    assert_eq!(body2["bounceReason"].as_str(), Some("Need more detail"));
}

#[test]
fn submit_rejects_without_required_fields() {
    wait_for_service();
    let desk = login("test_desk", "TestDeskPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");

    // create without eventDate/eventTime
    let (status, body) = req_json(
        "POST",
        "/api/lost-found/items",
        Some(&desk),
        Some(json!({
            "facilityId": facility,
            "title": "Lost Keys",
            "category": "lost",
            "tags": []
        })),
        None,
    );
    assert_eq!(status, 201, "create: {}", body);
    let id = body["id"].as_str().unwrap().to_string();
    let (s, body) = req_json(
        "POST",
        &format!("/api/lost-found/items/{}/submit", id),
        Some(&desk),
        None,
        None,
    );
    assert_eq!(s, 400);
    assert_eq!(body["error"].as_str(), Some("validation_failed"));
    assert_eq!(body["details"]["field"].as_str(), Some("eventDate"));
}

#[test]
fn attachment_dedup_per_facility_and_413_on_count() {
    wait_for_service();
    let desk = login("test_desk", "TestDeskPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");

    let draft = create_draft(&desk, &facility);
    let id = draft["id"].as_str().unwrap().to_string();

    let payload = json!({ "filename": "a.png", "contentType": "image/png", "dataBase64": tiny_png() });
    let (s1, b1) = req_json(
        "POST",
        &format!("/api/lost-found/items/{}/attachments", id),
        Some(&desk),
        Some(payload.clone()),
        None,
    );
    assert_eq!(s1, 201, "first: {}", b1);
    assert_eq!(b1["deduplicated"].as_bool(), Some(false));
    let sha_first = b1["sha256"].as_str().unwrap().to_string();

    let (s2, b2) = req_json(
        "POST",
        &format!("/api/lost-found/items/{}/attachments", id),
        Some(&desk),
        Some(payload.clone()),
        None,
    );
    assert_eq!(s2, 201);
    assert_eq!(b2["deduplicated"].as_bool(), Some(true));
    assert_eq!(b2["sha256"].as_str().unwrap(), sha_first);

    // push past 10 files
    for _ in 0..8 {
        let (s, _) = req_json(
            "POST",
            &format!("/api/lost-found/items/{}/attachments", id),
            Some(&desk),
            Some(payload.clone()),
            None,
        );
        assert_eq!(s, 201);
    }
    // we now have 10; the 11th must fail
    let (s11, b11) = req_json(
        "POST",
        &format!("/api/lost-found/items/{}/attachments", id),
        Some(&desk),
        Some(payload.clone()),
        None,
    );
    assert_eq!(s11, 413);
    assert_eq!(b11["error"].as_str(), Some("attachment_limit_exceeded"));
    assert_eq!(b11["details"]["limit"].as_str(), Some("files"));
}

#[test]
fn cannot_delete_attachment_through_a_different_items_route() {
    // Audit round 3 / High #2: the delete service now binds the attachment
    // to (parent_type, parent_id) as well as facility_id, so an authorized
    // caller in the same facility cannot delete item A's attachment by
    // routing the request through item B.
    wait_for_service();
    let desk = login("test_desk", "TestDeskPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");

    // Two independent lost-found items in the same facility.
    let a = create_draft(&desk, &facility);
    let b = create_draft(&desk, &facility);
    let a_id = a["id"].as_str().unwrap().to_string();
    let b_id = b["id"].as_str().unwrap().to_string();

    // Upload an attachment to A.
    let payload = json!({
        "filename": "evidence.png",
        "contentType": "image/png",
        "dataBase64": tiny_png()
    });
    let (s, att) = req_json(
        "POST",
        &format!("/api/lost-found/items/{}/attachments", a_id),
        Some(&desk),
        Some(payload),
        None,
    );
    assert_eq!(s, 201, "upload: {}", att);
    let att_id = att["id"].as_str().unwrap().to_string();

    // Try to delete A's attachment by routing through item B. Must refuse.
    let (status, body) = req_json(
        "DELETE",
        &format!("/api/lost-found/items/{}/attachments/{}", b_id, att_id),
        Some(&desk),
        None,
        None,
    );
    assert_eq!(status, 404, "cross-parent delete must not succeed: {}", body);

    // And the attachment must still be listed under A.
    let (_, list) = req_json(
        "GET",
        &format!("/api/lost-found/items/{}/attachments", a_id),
        Some(&desk),
        None,
        None,
    );
    let still_there = list["attachments"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x["id"].as_str() == Some(&att_id));
    assert!(still_there, "A's attachment must still exist after a routed-through-B delete attempt");
}
