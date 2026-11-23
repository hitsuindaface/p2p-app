use std::io;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use libp2p::identity::Keypair;
use libp2p::kad::{self, RecordKey, store::MemoryStore};
use libp2p::multiaddr::Protocol;
use libp2p::request_response::{self, ProtocolSupport};
use libp2p::swarm::NetworkBehaviour;
use libp2p::swarm::behaviour::toggle::Toggle;
use libp2p::{Multiaddr, Swarm, dcutr, identify, mdns, noise, ping, relay, tcp, yamux};

use crate::networking::protocol::{CHAT_PROTOCOL, ChatRequest, ChatResponse};

const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

// Every running app announces itself in the public DHT under a key for the current hour.
// DHT servers keep announcements for 48h and cannot be told to drop them, so a list only
// looks at the last two hours and a closed app disappears within two hours.
pub(super) fn current_hour() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 3600
}

pub(super) fn provider_key(hour: u64) -> RecordKey {
    RecordKey::new(&format!("p2penis/v1/{hour}"))
}

// Public IPFS bootstrap nodes; only used to join the DHT.
const BOOTSTRAP: &[&str] = &[
    "/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
    "/dnsaddr/bootstrap.libp2p.io/p2p/QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
    "/dnsaddr/bootstrap.libp2p.io/p2p/QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb",
    "/dnsaddr/bootstrap.libp2p.io/p2p/QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt",
    "/ip4/104.131.131.82/tcp/4001/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ",
];

pub(super) use behaviour::{Behaviour, BehaviourEvent};

// The derive expands to code rustc flags as unreachable (libp2p handlers with Infallible events).
#[allow(unreachable_code)]
mod behaviour {
    use super::*;

    #[derive(NetworkBehaviour)]
    pub(in crate::networking) struct Behaviour {
        pub relay: relay::client::Behaviour,
        pub dcutr: dcutr::Behaviour,
        pub identify: identify::Behaviour,
        pub ping: ping::Behaviour,
        pub mdns: Toggle<mdns::tokio::Behaviour>,
        pub kad: kad::Behaviour<MemoryStore>,
        pub chat: request_response::cbor::Behaviour<ChatRequest, ChatResponse>,
    }
}

// Builds the swarm, starts listening, joins the DHT and announces this node.
pub(super) fn build(keypair: Keypair, tcp_port: u16, lan: bool) -> io::Result<Swarm<Behaviour>> {
    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )
        .map_err(io::Error::other)?
        .with_dns()
        .map_err(io::Error::other)?
        .with_relay_client(noise::Config::new, yamux::Config::default)
        .map_err(io::Error::other)?
        .with_behaviour(|key, relay| {
            let id = key.public().to_peer_id();
            let mdns = if lan {
                Some(mdns::tokio::Behaviour::new(mdns::Config::default(), id)?)
            } else {
                None
            };
            Ok(Behaviour {
                relay,
                dcutr: dcutr::Behaviour::new(id),
                identify: identify::Behaviour::new(identify::Config::new(
                    "/p2penis/1".into(),
                    key.public(),
                )),
                ping: ping::Behaviour::default(),
                mdns: mdns.into(),
                kad: kad::Behaviour::with_config(
                    id,
                    MemoryStore::new(id),
                    kad::Config::new(kad::PROTOCOL_NAME),
                ),
                chat: request_response::cbor::Behaviour::new(
                    [(CHAT_PROTOCOL, ProtocolSupport::Full)],
                    request_response::Config::default(),
                ),
            })
        })
        .map_err(io::Error::other)?
        .with_swarm_config(|c| c.with_idle_connection_timeout(IDLE_TIMEOUT))
        .build();

    swarm
        .listen_on(
            format!("/ip4/0.0.0.0/tcp/{tcp_port}")
                .parse()
                .map_err(io::Error::other)?,
        )
        .map_err(io::Error::other)?;
    let kad = &mut swarm.behaviour_mut().kad;
    // Stay a DHT client: as a server, IPFS nodes would route their own DHT traffic to us through the relay.
    kad.set_mode(Some(kad::Mode::Client));
    for addr in BOOTSTRAP {
        let addr: Multiaddr = addr.parse().map_err(io::Error::other)?;
        if let Some(Protocol::P2p(peer)) = addr.iter().last() {
            kad.add_address(&peer, addr);
        }
    }
    let _ = kad.bootstrap();
    kad.start_providing(provider_key(current_hour()))
        .map_err(io::Error::other)?;
    Ok(swarm)
}
