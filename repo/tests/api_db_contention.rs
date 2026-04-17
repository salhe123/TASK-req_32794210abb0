mod common;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use common::{client, get_facility_id, login, wait_for_service};
use serde_json::json;

/// 24 threads race to create the same `(facility_id, asset_label)` pair.
/// The UNIQUE constraint must let exactly one win; the rest must return
/// `409 duplicate_asset_label`. This stresses the DB contention path and
/// verifies we never leak the real Diesel error envelope.
#[test]
fn unique_label_contention_produces_one_winner() {
    wait_for_service();
    let token = login("test_asset", "TestAssetPassword123");
    let admin = login("test_admin", "TestAdminPassword123");
    let facility = get_facility_id(&admin, "DEFAULT");
    let label = format!("CONTEND-{}", uuid::Uuid::new_v4());

    let winners = Arc::new(AtomicU64::new(0));
    let duplicates = Arc::new(AtomicU64::new(0));
    let others = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::new();
    for _ in 0..24 {
        let t = token.clone();
        let f = facility.clone();
        let l = label.clone();
        let w = winners.clone();
        let d = duplicates.clone();
        let o = others.clone();
        handles.push(std::thread::spawn(move || {
            let c = client();
            let r = c
                .post(format!("{}/api/assets", common::base_url()))
                .bearer_auth(&t)
                .json(&json!({
                    "facilityId": f,
                    "assetLabel": l,
                    "name": "contention",
                }))
                .send()
                .expect("send");
            let status = r.status().as_u16();
            let body: serde_json::Value = r.json().unwrap_or(serde_json::Value::Null);
            match status {
                201 => {
                    w.fetch_add(1, Ordering::Relaxed);
                }
                409 => {
                    assert_eq!(body["error"].as_str(), Some("duplicate_asset_label"));
                    d.fetch_add(1, Ordering::Relaxed);
                }
                _ => {
                    eprintln!("unexpected status {} body={}", status, body);
                    o.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    let w = winners.load(Ordering::Relaxed);
    let d = duplicates.load(Ordering::Relaxed);
    let o = others.load(Ordering::Relaxed);
    assert_eq!(w, 1, "exactly one creator must win: winners={} dups={} other={}", w, d, o);
    assert_eq!(
        w + d,
        24,
        "every request must end in either 201 or 409; winners={} dups={} other={}",
        w,
        d,
        o
    );
}
