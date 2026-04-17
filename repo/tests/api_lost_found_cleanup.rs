mod common;

use base64::Engine;
use common::{get_facility_id, login, req_json, wait_for_service};
use serde_json::json;

fn tiny_png_b64() -> String {
    let mut buf = std::io::Cursor::new(Vec::new());
    let img = image::ImageBuffer::<image::Rgb<u8>, _>::from_pixel(1, 1, image::Rgb([0, 255, 0]));
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut buf, image::ImageFormat::Png)
        .expect("encode tiny png");
    base64::engine::general_purpose::STANDARD.encode(buf.into_inner())
}

fn new_draft(token: &str, facility_id: &str) -> String {
    let (_, body) = req_json(
        "POST",
        "/api/lost-found/items",
        Some(token),
        Some(json!({
            "facilityId": facility_id,
            "title": "Cleanup draft",
            "category": "other",
            "tags": []
        })),
        None,
    );
    body["id"].as_str().unwrap().to_string()
}

#[test]
fn delete_attachment_removes_from_listing() {
    wait_for_service();
    let desk = login("test_desk", "TestDeskPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let item = new_draft(&desk, &facility);

    let (us, ub) = req_json(
        "POST",
        &format!("/api/lost-found/items/{}/attachments", item),
        Some(&desk),
        Some(json!({
            "filename": "x.png",
            "contentType": "image/png",
            "dataBase64": tiny_png_b64()
        })),
        None,
    );
    assert_eq!(us, 201, "{}", ub);
    let att_id = ub["id"].as_str().unwrap().to_string();

    let (ds, db) = req_json(
        "DELETE",
        &format!("/api/lost-found/items/{}/attachments/{}", item, att_id),
        Some(&desk),
        None,
        None,
    );
    assert_eq!(ds, 200, "{}", db);

    let (_, after) = req_json(
        "GET",
        &format!("/api/lost-found/items/{}/attachments", item),
        Some(&desk),
        None,
        None,
    );
    let present = after["attachments"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["id"].as_str() == Some(&att_id));
    assert!(!present);
}

#[test]
fn soft_delete_lost_found_item_hides_from_default_list() {
    wait_for_service();
    let desk = login("test_desk", "TestDeskPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let item = new_draft(&desk, &facility);

    let (s, _) = req_json(
        "DELETE",
        &format!("/api/lost-found/items/{}", item),
        Some(&desk),
        None,
        None,
    );
    assert_eq!(s, 200);

    let (_, listing) = req_json(
        "GET",
        &format!("/api/lost-found/items?facilityId={}", facility),
        Some(&desk),
        None,
        None,
    );
    let found = listing["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["id"].as_str() == Some(&item));
    assert!(!found, "soft-deleted item must be hidden by default");

    let (_, listing_all) = req_json(
        "GET",
        &format!("/api/lost-found/items?facilityId={}&includeDeleted=true", facility),
        Some(&desk),
        None,
        None,
    );
    let visible = listing_all["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["id"].as_str() == Some(&item));
    assert!(visible, "includeDeleted=true must include soft-deleted items");
}

#[test]
fn update_outside_draft_blocked_with_invalid_transition() {
    wait_for_service();
    let desk = login("test_desk", "TestDeskPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let id = new_draft(&desk, &facility);
    let (_, _) = req_json(
        "PUT",
        &format!("/api/lost-found/items/{}", id),
        Some(&desk),
        Some(json!({
            "eventDate": "01/02/2026",
            "eventTime": "9:00 AM",
            "locationText": "x"
        })),
        None,
    );
    let (_, _) = req_json(
        "POST",
        &format!("/api/lost-found/items/{}/submit", id),
        Some(&desk),
        None,
        None,
    );
    let (s, body) = req_json(
        "PUT",
        &format!("/api/lost-found/items/{}", id),
        Some(&desk),
        Some(json!({ "title": "nope" })),
        None,
    );
    assert_eq!(s, 409);
    assert_eq!(body["error"].as_str(), Some("invalid_transition"));
}
