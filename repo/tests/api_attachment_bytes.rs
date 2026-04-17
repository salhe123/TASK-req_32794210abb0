mod common;

use base64::Engine;
use common::{get_facility_id, login, req_json, wait_for_service};
use serde_json::json;

fn create_draft(token: &str, facility_id: &str) -> String {
    let (s, body) = req_json(
        "POST",
        "/api/lost-found/items",
        Some(token),
        Some(json!({
            "facilityId": facility_id,
            "title": "Big upload",
            "category": "found",
            "tags": [],
        })),
        None,
    );
    assert_eq!(s, 201, "{}", body);
    body["id"].as_str().unwrap().to_string()
}

fn pdf_blob_b64(mb: usize, filler: u8) -> String {
    // Valid-ish PDF prologue; body is padded with a non-zero filler byte so
    // each test blob is unique enough to bypass the SHA dedup path.
    let mut bytes = b"%PDF-1.4\n".to_vec();
    bytes.resize(mb * 1024 * 1024, filler);
    bytes.extend_from_slice(b"\n%%EOF\n");
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[test]
fn second_large_pdf_trips_byte_limit_with_limit_bytes_detail() {
    wait_for_service();
    let desk = login("test_desk", "TestDeskPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let id = create_draft(&desk, &facility);

    // First upload: ~15 MB PDF, well under the 25 MB aggregate cap.
    let (s1, b1) = req_json(
        "POST",
        &format!("/api/lost-found/items/{}/attachments", id),
        Some(&desk),
        Some(json!({
            "filename": "first.pdf",
            "contentType": "application/pdf",
            "dataBase64": pdf_blob_b64(15, 0xA5),
        })),
        None,
    );
    assert_eq!(s1, 201, "{}", b1);
    assert!((b1["sizeBytes"].as_i64().unwrap() as f64) > 14.0 * 1024.0 * 1024.0);

    // Second upload: another ~15 MB PDF → aggregate 30 MB, must 413.
    let (s2, b2) = req_json(
        "POST",
        &format!("/api/lost-found/items/{}/attachments", id),
        Some(&desk),
        Some(json!({
            "filename": "second.pdf",
            "contentType": "application/pdf",
            "dataBase64": pdf_blob_b64(15, 0x5A),
        })),
        None,
    );
    assert_eq!(s2, 413, "{}", b2);
    assert_eq!(b2["error"].as_str(), Some("attachment_limit_exceeded"));
    assert_eq!(b2["details"]["limit"].as_str(), Some("bytes"));
}
