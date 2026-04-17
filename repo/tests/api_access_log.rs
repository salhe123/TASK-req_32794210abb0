mod common;

use common::{get_facility_id, login, req_json, wait_for_service};

#[test]
fn access_log_captures_every_required_field() {
    wait_for_service();
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let rid = uuid::Uuid::new_v4().to_string();

    // Make a request that carries request_id, user_id (via bearer), facility_id (via query).
    let (s, _) = req_json(
        "GET",
        &format!("/api/assets?facilityId={}", facility),
        Some(&admin),
        None,
        Some(&rid),
    );
    assert_eq!(s, 200);

    let (s2, body) = req_json(
        "GET",
        "/api/__diag/access-log?limit=50",
        Some(&admin),
        None,
        None,
    );
    assert_eq!(s2, 200, "{}", body);
    let records = body["records"].as_array().unwrap();
    let matching = records
        .iter()
        .find(|r| {
            r["request_id"].as_str() == Some(&rid)
                && r["path"].as_str().unwrap_or("").starts_with("/api/assets")
        })
        .expect("captured access log for our request");

    // Verify the required field set from plan §10.
    for key in [
        "request_id",
        "user_id",
        "facility_id",
        "method",
        "path",
        "status",
        "duration_ms",
    ] {
        assert!(
            matching.get(key).is_some(),
            "access log missing field {}: {}",
            key,
            matching
        );
    }
    assert!(matching["user_id"].is_string(), "user_id populated: {}", matching);
    assert_eq!(
        matching["facility_id"].as_str(),
        Some(facility.as_str()),
        "facility_id captured from query string"
    );
    assert_eq!(matching["method"].as_str(), Some("GET"));
    assert_eq!(matching["status"].as_u64(), Some(200));
    assert!(matching["duration_ms"].is_number());
}

#[test]
fn access_log_records_4xx_when_unauthenticated() {
    wait_for_service();
    let admin = login("test_admin", "TestAdminPassword123");
    let rid = uuid::Uuid::new_v4().to_string();
    let (s, _) = req_json("GET", "/api/auth/session", None, None, Some(&rid));
    assert_eq!(s, 401);
    let (_, body) = req_json("GET", "/api/__diag/access-log?limit=80", Some(&admin), None, None);
    let records = body["records"].as_array().unwrap();
    let entry = records
        .iter()
        .find(|r| r["request_id"].as_str() == Some(&rid))
        .expect("captured unauth request");
    assert_eq!(entry["status"].as_u64(), Some(401));
    assert!(entry["user_id"].is_null());
}
