mod common;

use common::{login, req_json, wait_for_service};
use serde_json::json;

#[test]
fn template_create_update_delete_roundtrip() {
    wait_for_service();
    let notif = login("test_notif", "TestNotifPassword123");
    let code = format!("tpl.test.{}", uuid::Uuid::new_v4().simple());

    let (cs, cb) = req_json(
        "POST",
        "/api/notifications/templates",
        Some(&notif),
        Some(json!({
            "code": code,
            "subject": "Item {{ item.title }}",
            "body": "Status {{ item.status }} for {{ actor.displayName }}"
        })),
        None,
    );
    assert_eq!(cs, 201, "{}", cb);
    let tid = cb["id"].as_str().unwrap().to_string();
    assert_eq!(cb["code"].as_str(), Some(code.as_str()));

    let (us, ub) = req_json(
        "PUT",
        &format!("/api/notifications/templates/{}", tid),
        Some(&notif),
        Some(json!({
            "code": code,
            "subject": "Updated {{ item.title }}",
            "body": "Body",
            "isActive": false
        })),
        None,
    );
    assert_eq!(us, 200, "{}", ub);
    assert_eq!(ub["subject"].as_str(), Some("Updated {{ item.title }}"));
    assert_eq!(ub["isActive"].as_bool(), Some(false));

    let (ls, lb) = req_json(
        "GET",
        "/api/notifications/templates",
        Some(&notif),
        None,
        None,
    );
    assert_eq!(ls, 200);
    let found = lb["templates"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["id"].as_str() == Some(&tid));
    assert!(found);

    let (ds, _) = req_json(
        "DELETE",
        &format!("/api/notifications/templates/{}", tid),
        Some(&notif),
        None,
        None,
    );
    assert_eq!(ds, 200);
}

#[test]
fn template_update_with_disallowed_variable_rejected() {
    wait_for_service();
    let notif = login("test_notif", "TestNotifPassword123");
    let code = format!("tpl.bad.{}", uuid::Uuid::new_v4().simple());
    let (cs, cb) = req_json(
        "POST",
        "/api/notifications/templates",
        Some(&notif),
        Some(json!({
            "code": code,
            "subject": "ok {{ item.title }}",
            "body": "ok"
        })),
        None,
    );
    assert_eq!(cs, 201, "{}", cb);
    let tid = cb["id"].as_str().unwrap().to_string();

    let (us, body) = req_json(
        "PUT",
        &format!("/api/notifications/templates/{}", tid),
        Some(&notif),
        Some(json!({
            "code": code,
            "subject": "{{ forbidden.variable }}",
            "body": "x"
        })),
        None,
    );
    assert_eq!(us, 400);
    assert_eq!(body["error"].as_str(), Some("validation_failed"));
    assert_eq!(body["details"]["variable"].as_str(), Some("forbidden.variable"));
}
