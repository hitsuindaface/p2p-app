// Temporary terminal frontend for testing the networking library; replaced by the real UI later.

use std::io;
use std::sync::{Arc, Mutex};

use libp2p::identity::Keypair;
use p2p_app::{Config, Event, Identity, Node, Peer, default_relays};
use tokio::io::{AsyncBufReadExt, BufReader};

const HELP: &str = "commands: list | send <index> <text> | quit (or Ctrl+C)";

// Usage: p2p-app [relay multiaddr]...  (no arguments: the default relays)
pub async fn run() -> io::Result<()> {
    let mut relays: Vec<_> = std::env::args()
        .skip(1)
        .map(|a| a.parse())
        .collect::<Result<_, _>>()
        .map_err(io::Error::other)?;
    if relays.is_empty() {
        relays = default_relays();
    }
    let cfg = Config {
        identity: placeholder_identity(),
        // Any free port: several copies on one machine would otherwise share a port.
        tcp_port: 0,
        lan_discovery: true,
        relays,
    };
    let (node, mut events) = Node::start(cfg)?;
    println!("{} ({})\n{HELP}", node.me().name(), node.me().id);

    // Filled by the event task as search results arrive; send looks up indexes here.
    let peers: Arc<Mutex<Vec<Peer>>> = Arc::default();
    let found = peers.clone();
    tokio::spawn(async move {
        while let Some(ev) = events.recv().await {
            print_event(ev, &found);
        }
    });

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    loop {
        // Ctrl+C and quit end the program the same way.
        let line = tokio::select! {
            line = lines.next_line() => line?,
            _ = tokio::signal::ctrl_c() => None,
        };
        let Some(line) = line else { break };
        let line = line.trim();
        let (cmd, rest) = line.split_once(' ').unwrap_or((line, ""));
        match cmd {
            "" => {}
            "list" => list(&node, &peers),
            "send" => {
                let peer = rest
                    .split_once(' ')
                    .unwrap_or((rest, ""))
                    .0
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| peers.lock().unwrap().get(i).cloned());
                send(&node, peer, rest).await
            }
            "quit" => break,
            _ => println!("{HELP}"),
        }
    }
    Ok(())
}

fn list(node: &Node, peers: &Mutex<Vec<Peer>>) {
    peers.lock().unwrap().clear();
    match node.search() {
        Ok(()) => println!("searching..."),
        Err(e) => println!("list failed: {e}"),
    }
}

async fn send(node: &Node, peer: Option<Peer>, args: &str) {
    let text = args.split_once(' ').map_or("", |(_, t)| t);
    let Some(peer) = peer else {
        return println!("usage: send <index> <text>, index from list");
    };
    match node.send(peer.id, text.as_bytes()).await {
        Ok(()) => println!(
            "{} (you) -> {}: {text}",
            node.me().name(),
            clean(&peer.name())
        ),
        Err(e) => println!("not sent: {e}"),
    }
}

fn print_event(ev: Event, peers: &Mutex<Vec<Peer>>) {
    match ev {
        Event::PeerFound(p) => {
            let mut peers = peers.lock().unwrap();
            let place = if p.lan { "lan" } else { "internet" };
            println!(
                "  [{}] {} ({place}) {}",
                peers.len(),
                clean(&p.name()),
                p.id
            );
            peers.push(p);
        }
        Event::SearchDone => match peers.lock().unwrap().len() {
            0 => println!("nobody found"),
            n => println!("search done, {n} found"),
        },
        Event::Message { from, data } => {
            println!(
                "{}: {}",
                clean(&from.name()),
                clean(&String::from_utf8_lossy(&data))
            )
        }
    }
}

// Peers control these strings; drop control characters so they cannot send terminal escapes.
fn clean(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

// TODO(crypto): load the saved key file with Identity::decode instead.
fn placeholder_identity() -> Identity {
    Identity {
        keypair: Keypair::generate_ed25519(),
        username: None,
    }
}
