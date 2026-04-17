mod common;

use base64::Engine;
use common::{get_facility_id, login, req_json, wait_for_service};
use serde_json::json;

fn draft(token: &str, facility: &str) -> String {
    let (_, body) = req_json(
        "POST",
        "/api/lost-found/items",
        Some(token),
        Some(json!({
            "facilityId": facility,
            "title": "errors",
            "category": "other",
            "tags": []
        })),
        None,
    );
    body["id"].as_str().unwrap().to_string()
}

#[test]
fn unsupported_mime_rejected_with_invalid_attachment() {
    wait_for_service();
    let desk = login("test_desk", "TestDeskPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let id = draft(&desk, &facility);
    let data = base64::engine::general_purpose::STANDARD.encode(b"hello");
    let (s, body) = req_json(
        "POST",
        &format!("/api/lost-found/items/{}/attachments", id),
        Some(&desk),
        Some(json!({
            "filename": "note.txt",
            "contentType": "text/plain",
            "dataBase64": data
        })),
        None,
    );
    assert_eq!(s, 400);
    assert_eq!(body["error"].as_str(), Some("invalid_attachment"));
    assert!(body["message"].as_str().unwrap().contains("text/plain"));
}

#[test]
fn malformed_base64_rejected_with_invalid_attachment() {
    wait_for_service();
    let desk = login("test_desk", "TestDeskPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let id = draft(&desk, &facility);
    let (s, body) = req_json(
        "POST",
        &format!("/api/lost-found/items/{}/attachments", id),
        Some(&desk),
        Some(json!({
            "filename": "x.png",
            "contentType": "image/png",
            "dataBase64": "%%%%%not-base64%%%%%"
        })),
        None,
    );
    assert_eq!(s, 400);
    assert_eq!(body["error"].as_str(), Some("invalid_attachment"));
}

#[test]
fn corrupted_image_bytes_rejected_at_decode() {
    wait_for_service();
    let desk = login("test_desk", "TestDeskPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let id = draft(&desk, &facility);
    let not_an_image = base64::engine::general_purpose::STANDARD.encode(b"definitely not a png");
    let (s, body) = req_json(
        "POST",
        &format!("/api/lost-found/items/{}/attachments", id),
        Some(&desk),
        Some(json!({
            "filename": "broken.png",
            "contentType": "image/png",
            "dataBase64": not_an_image
        })),
        None,
    );
    assert_eq!(s, 400);
    assert_eq!(body["error"].as_str(), Some("invalid_attachment"));
}
