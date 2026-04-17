mod common;

use std::time::Duration;

use common::wait_for_service;

/// The test network is created with `--internal`, which means the service,
/// the test runner, and Postgres cannot reach the internet. This test
/// exercises that invariant from inside the test container. If it ever
/// starts passing with outbound traffic, the offline guarantee has regressed.
#[test]
fn test_network_blocks_outbound_internet() {
    if std::env::var("CIVICOPS_SKIP_OFFLINE_ASSERT").ok().as_deref() == Some("1") {
        eprintln!("skipping offline-network assertion (CIVICOPS_SKIP_OFFLINE_ASSERT=1)");
        return;
    }
    wait_for_service(); // local civicops stays reachable on the internal net
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("client");
    // Several known public endpoints; all must be unreachable from --internal.
    let targets = [
        "http://1.1.1.1/",
        "http://8.8.8.8/",
        "https://example.com/",
    ];
    for t in targets {
        match client.get(t).send() {
            Ok(r) => panic!(
                "expected no outbound access, but {} returned status {}",
                t,
                r.status()
            ),
            Err(_) => {
                // expected: connection / DNS failure
            }
        }
    }
}
