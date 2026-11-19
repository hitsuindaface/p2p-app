use std::io;

use futures::StreamExt;
use libp2p::multiaddr::Protocol;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{Multiaddr, identify, noise, ping, relay, tcp, yamux};

use behaviour::{Behaviour, BehaviourEvent};

// The derive expands to code rustc flags as unreachable (libp2p handlers with Infallible events).
#[allow(unreachable_code)]
mod behaviour {
    use super::*;

    #[derive(NetworkBehaviour)]
    pub(super) struct Behaviour {
        pub(super) relay: relay::Behaviour,
        pub(super) identify: identify::Behaviour,
        pub(super) ping: ping::Behaviour,
    }
}

// Usage: relay-server <public multiaddr, e.g. /dns4/p2p.miro.zip/tcp/47802> [listen port]
// The listen port defaults to the port in the public multiaddr.
#[tokio::main]
async fn main() -> io::Result<()> {
    let mut args = std::env::args().skip(1);
    // libp2p only offers the relay protocol once the node has an external address.
    let external: Multiaddr = args
        .next()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: relay-server <public multiaddr> [port]",
            )
        })?
        .parse()
        .map_err(io::Error::other)?;
    let port: u16 = match args.next() {
        Some(p) => p.parse().map_err(io::Error::other)?,
        None => external
            .iter()
            .find_map(|p| {
                if let Protocol::Tcp(port) = p {
                    Some(port)
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "public multiaddr has no /tcp port",
                )
            })?,
    };

    // A fresh key each start is fine: clients learn the relay's PeerId when they connect.
    let mut swarm = libp2p::SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )
        .map_err(io::Error::other)?
        .with_behaviour(|key| Behaviour {
            relay: relay::Behaviour::new(key.public().to_peer_id(), relay::Config::default()),
            identify: identify::Behaviour::new(identify::Config::new(
                "/p2penis-relay/1".into(),
                key.public(),
            )),
            ping: ping::Behaviour::default(),
        })
        .map_err(io::Error::other)?
        .build();

    swarm
        .listen_on(
            format!("/ip4/0.0.0.0/tcp/{port}")
                .parse()
                .map_err(io::Error::other)?,
        )
        .map_err(io::Error::other)?;
    swarm.add_external_address(external);
    println!("relay {} on tcp {port}", swarm.local_peer_id());

    loop {
        tokio::select! {
            ev = swarm.select_next_some() => {
                if let SwarmEvent::Behaviour(BehaviourEvent::Relay(e)) = ev {
                    println!("{e:?}");
                }
            }
            _ = shutdown() => return Ok(()),
        }
    }
}

// As PID 1 in a container nothing handles SIGTERM unless we do.
async fn shutdown() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut term) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = term.recv() => {}
                _ = tokio::signal::ctrl_c() => {}
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}
