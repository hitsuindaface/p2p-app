//! Two real nodes in one test process.
//!
//! The node always joins the public IPFS DHT, so these tests are ignored in CI.
//! Run them by hand on a machine whose network allows mDNS:
//!
//! ```text
//! cargo test --test two_nodes -- --ignored --nocapture
//! ```

mod common;

use std::time::{Duration, Instant};

// Generous limits for one run. The C1 limits (5 s discovery, 200 ms median
// latency, 19 of 20 runs) are judged over many runs in the lab.
const FIND_LIMIT: Duration = Duration::from_secs(10);
const MESSAGE_LIMIT: Duration = Duration::from_secs(10);

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a LAN-only mode in networking::Config (no DHT bootstrap, no relays)"]
async fn c1_two_nodes_find_each_other_and_chat() {
    let (a, mut a_events) = common::start_node();
    let (b, mut b_events) = common::start_node();
    let (a_id, b_id) = (a.me().id, b.me().id);

    let started = Instant::now();
    let (b_seen_by_a, a_seen_by_b) = tokio::join!(
        common::find_lan_peer(&a, &mut a_events, b_id, FIND_LIMIT),
        common::find_lan_peer(&b, &mut b_events, a_id, FIND_LIMIT),
    );
    let discovery = started.elapsed();
    assert!(
        b_seen_by_a.is_some(),
        "A did not find B on the LAN within {FIND_LIMIT:?}"
    );
    assert!(
        a_seen_by_b.is_some(),
        "B did not find A on the LAN within {FIND_LIMIT:?}"
    );

    let sent = Instant::now();
    tokio::time::timeout(MESSAGE_LIMIT, a.send(b_id, b"hello"))
        .await
        .expect("no acknowledgement from B in time")
        .expect("send from A to B failed");
    let acked = sent.elapsed();

    let left = MESSAGE_LIMIT.saturating_sub(sent.elapsed());
    let data = common::wait_for_message(&mut b_events, a_id, left).await;
    assert_eq!(
        data.as_deref(),
        Some(&b"hello"[..]),
        "B did not get \"hello\" from A within {MESSAGE_LIMIT:?} of sending"
    );

    println!(
        "c1: both found each other in {} ms, send to acknowledgement {} ms",
        discovery.as_millis(),
        acked.as_millis()
    );
}
