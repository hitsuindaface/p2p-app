//! Helpers shared by the integration tests. A test file uses them with `mod common;`.

// Each test file compiles this module on its own and uses only some of the helpers.
#![allow(dead_code)]

use std::time::Duration;

use libp2p::PeerId;
use libp2p::identity::Keypair;
use p2p_app::{Config, Event, Identity, Node, Peer};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::{Instant, sleep, timeout_at};

pub type Events = UnboundedReceiver<Event>;

/// Starts a node with a fresh identity, any free TCP port, LAN discovery on and
/// no relays. Call it inside a Tokio runtime, for example from `#[tokio::test]`.
pub fn start_node() -> (Node, Events) {
    let cfg = Config {
        identity: Identity {
            keypair: Keypair::generate_ed25519(),
            username: None,
        },
        tcp_port: 0,
        lan_discovery: true,
        relays: vec![],
    };
    Node::start(cfg).expect("node should start")
}

/// Reads events until `pick` returns `Some`, and returns that value. Other events
/// are skipped. Returns `None` if `limit` runs out or the node stops.
pub async fn wait_for<T>(
    events: &mut Events,
    limit: Duration,
    mut pick: impl FnMut(&Event) -> Option<T>,
) -> Option<T> {
    let deadline = Instant::now() + limit;
    loop {
        match timeout_at(deadline, events.recv()).await {
            Ok(Some(event)) => {
                if let Some(value) = pick(&event) {
                    return Some(value);
                }
            }
            Ok(None) | Err(_) => return None,
        }
    }
}

/// Waits for a chat message from `from` and returns its data.
pub async fn wait_for_message(
    events: &mut Events,
    from: PeerId,
    limit: Duration,
) -> Option<Vec<u8>> {
    wait_for(events, limit, |event| match event {
        Event::Message { from: peer, data } if peer.id == from => Some(data.clone()),
        _ => None,
    })
    .await
}

/// Searches until `target` is reported as a LAN peer, starting a new search each
/// time one ends. Returns `None` if `limit` runs out first.
pub async fn find_lan_peer(
    node: &Node,
    events: &mut Events,
    target: PeerId,
    limit: Duration,
) -> Option<Peer> {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        node.search().expect("node should be running");
        let left = deadline.saturating_duration_since(Instant::now());
        let result = wait_for(events, left, |event| match event {
            Event::PeerFound(peer) if peer.id == target && peer.lan => Some(Some(peer.clone())),
            Event::SearchDone => Some(None),
            _ => None,
        })
        .await;
        match result {
            Some(Some(peer)) => return Some(peer),
            // The search ended without the peer: wait a moment for mDNS, then search again.
            Some(None) => sleep(Duration::from_millis(250)).await,
            None => return None,
        }
    }
    None
}
