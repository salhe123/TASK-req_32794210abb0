mod common;

use common::{login, req_json, wait_for_service};
use serde_json::json;

#[test]
fn subscription_put_then_get_matches_and_persists_across_logins() {
    wait_for_service();
    let token = login("test_asset", "TestAssetPassword123");

    let (ps, pb) = req_json(
        "PUT",
        "/api/notifications/subscriptions",
        Some(&token),
        Some(json!({
            "subscriptions": [
                { "eventKind": "submission", "enabled": false },
                { "eventKind": "review", "enabled": true },
                { "eventKind": "change", "enabled": true }
            ]
        })),
        None,
    );
    assert_eq!(ps, 200, "{}", pb);

    let (gs, gb) = req_json(
        "GET",
        "/api/notifications/subscriptions",
        Some(&token),
        None,
        None,
    );
    assert_eq!(gs, 200);
    let subs = gb["subscriptions"].as_array().unwrap();
    let by_kind: std::collections::HashMap<_, _> = subs
        .iter()
        .map(|s| (s["eventKind"].as_str().unwrap(), s["enabled"].as_bool().unwrap()))
        .collect();
    assert_eq!(by_kind.get("submission").copied(), Some(false));
    assert_eq!(by_kind.get("review").copied(), Some(true));
    assert_eq!(by_kind.get("change").copied(), Some(true));

    // New login → same preferences.
    let token2 = login("test_asset", "TestAssetPassword123");
    let (_, gb2) = req_json(
        "GET",
        "/api/notifications/subscriptions",
        Some(&token2),
        None,
        None,
    );
    let subs2 = gb2["subscriptions"].as_array().unwrap();
    let sub = subs2
        .iter()
        .find(|s| s["eventKind"].as_str() == Some("submission"))
        .unwrap();
    assert_eq!(sub["enabled"].as_bool(), Some(false));

    // Reset to defaults so other tests are unaffected.
    let (_, _) = req_json(
        "PUT",
        "/api/notifications/subscriptions",
        Some(&token2),
        Some(json!({
            "subscriptions": [
                { "eventKind": "submission", "enabled": true },
                { "eventKind": "review", "enabled": true },
                { "eventKind": "change", "enabled": true }
            ]
        })),
        None,
    );
}
