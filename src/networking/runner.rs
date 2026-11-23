use std::collections::{HashMap, HashSet};
use std::io;
use std::net::Ipv4Addr;
use std::time::Duration;

use futures::StreamExt;
use libp2p::kad::{self, GetProvidersOk, QueryResult};
use libp2p::multiaddr::Protocol;
use libp2p::request_response::{self, OutboundRequestId};
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::{ConnectionId, SwarmEvent};
use libp2p::{Multiaddr, PeerId, Swarm, identify, mdns};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

use crate::networking::identity::valid_username;
use crate::networking::node::{Command, Event, Peer};
use crate::networking::protocol::{ChatRequest, ChatResponse, decode_blob, encode_blob};
use crate::networking::swarm::{Behaviour, BehaviourEvent, current_hour, provider_key};

// A search ends after this even if DHT queries are still running.
const SEARCH_WAIT: Duration = Duration::from_secs(10);
const RELAY_RETRY: Duration = Duration::from_secs(30);

pub(super) fn spawn(
    swarm: Swarm<Behaviour>,
    username: Option<String>,
    relays: Vec<Multiaddr>,
) -> (
    mpsc::UnboundedSender<Command>,
    mpsc::UnboundedReceiver<Event>,
) {
    let (events, events_rx) = mpsc::unbounded_channel();
    let (commands, commands_rx) = mpsc::unbounded_channel();
    let runner = Runner {
        swarm,
        username,
        events,
        relays,
        relay_dials: HashMap::new(),
        relays_up: HashMap::new(),
        names: HashMap::new(),
        lan: HashSet::new(),
        hour: current_hour(),
        search: None,
        sends: HashMap::new(),
        dial_errors: HashMap::new(),
    };
    tokio::spawn(runner.run(commands_rx));
    (commands, events_rx)
}

struct Search {
    queries: HashSet<kad::QueryId>,
    seen: HashSet<PeerId>,
    deadline: Instant,
}

// Owns the swarm and drives it; Node talks to it through commands.
struct Runner {
    swarm: Swarm<Behaviour>,
    username: Option<String>,
    events: mpsc::UnboundedSender<Event>,
    relays: Vec<Multiaddr>,
    relay_dials: HashMap<ConnectionId, Multiaddr>,
    relays_up: HashMap<PeerId, Multiaddr>,
    names: HashMap<PeerId, String>,
    lan: HashSet<PeerId>,
    hour: u64,
    search: Option<Search>,
    sends: HashMap<OutboundRequestId, (PeerId, oneshot::Sender<io::Result<()>>)>,
    // Last reason a dial to a peer failed; request_response only reports "failed to dial".
    dial_errors: HashMap<PeerId, String>,
}

impl Runner {
    async fn run(mut self, mut commands: mpsc::UnboundedReceiver<Command>) {
        let mut relay_tick = tokio::time::interval(RELAY_RETRY);
        let mut search_tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = relay_tick.tick() => self.dial_relays(),
                _ = search_tick.tick() => {
                    self.end_search_if_done();
                    self.rotate_key();
                }
                cmd = commands.recv() => match cmd {
                    Some(cmd) => self.command(cmd),
                    None => {
                        // Local only: DHT servers keep our announcement until it expires.
                        self.swarm.behaviour_mut().kad.stop_providing(&provider_key(self.hour));
                        return;
                    }
                },
                ev = self.swarm.select_next_some() => self.swarm_event(ev),
            }
        }
    }

    fn dial_relays(&mut self) {
        for addr in &self.relays {
            if self
                .relays_up
                .values()
                .chain(self.relay_dials.values())
                .any(|a| a == addr)
            {
                continue;
            }
            let opts = DialOpts::unknown_peer_id().address(addr.clone()).build();
            let id = opts.connection_id();
            if self.swarm.dial(opts).is_ok() {
                self.relay_dials.insert(id, addr.clone());
            }
        }
    }

    fn rotate_key(&mut self) {
        let hour = current_hour();
        if hour == self.hour {
            return;
        }
        let kad = &mut self.swarm.behaviour_mut().kad;
        kad.stop_providing(&provider_key(self.hour));
        let _ = kad.start_providing(provider_key(hour));
        self.hour = hour;
    }

    fn command(&mut self, cmd: Command) {
        match cmd {
            Command::Search => {
                let kad = &mut self.swarm.behaviour_mut().kad;
                if let Some(old) = self.search.take() {
                    for id in old.queries {
                        if let Some(mut q) = kad.query_mut(&id) {
                            q.finish();
                        }
                    }
                }
                let queries = [self.hour, self.hour - 1]
                    .into_iter()
                    .map(|hour| kad.get_providers(provider_key(hour)))
                    .collect();
                self.search = Some(Search {
                    queries,
                    seen: HashSet::new(),
                    deadline: Instant::now() + SEARCH_WAIT,
                });
                let lan: Vec<_> = self.lan.iter().copied().collect();
                self.found(lan);
            }
            Command::Send { to, data, reply } => {
                // TODO(crypto): encrypt blob here.
                let blob = match encode_blob(&data) {
                    Ok(b) => b,
                    Err(e) => return drop(reply.send(Err(e))),
                };
                // Peers behind NAT are reachable through any relay we share; DCUtR then upgrades to direct.
                for (relay, addr) in &self.relays_up {
                    let circuit = addr
                        .clone()
                        .with(Protocol::P2p(*relay))
                        .with(Protocol::P2pCircuit);
                    self.swarm.add_peer_address(to, circuit);
                }
                let request = ChatRequest {
                    username: self.username.clone(),
                    blob,
                };
                let id = self.swarm.behaviour_mut().chat.send_request(&to, request);
                self.sends.insert(id, (to, reply));
            }
        }
    }

    fn swarm_event(&mut self, event: SwarmEvent<BehaviourEvent>) {
        match event {
            SwarmEvent::ConnectionEstablished {
                peer_id,
                connection_id,
                ..
            } => {
                if let Some(addr) = self.relay_dials.remove(&connection_id) {
                    let circuit = addr
                        .clone()
                        .with(Protocol::P2p(peer_id))
                        .with(Protocol::P2pCircuit);
                    if let Err(e) = self.swarm.listen_on(circuit) {
                        eprintln!("[node] relay reservation at {addr} failed: {e}");
                    }
                    self.relays_up.insert(peer_id, addr);
                }
            }
            SwarmEvent::NewListenAddr { address, .. }
                if address.iter().any(|p| p == Protocol::P2pCircuit) =>
            {
                // DHT servers drop provider records without addresses, so announce the relay circuit and provide again.
                self.swarm.add_external_address(address);
                let _ = self
                    .swarm
                    .behaviour_mut()
                    .kad
                    .start_providing(provider_key(self.hour));
            }
            SwarmEvent::OutgoingConnectionError {
                connection_id,
                peer_id,
                error,
            } => {
                if let Some(addr) = self.relay_dials.remove(&connection_id) {
                    eprintln!("[node] relay {addr} unreachable: {error}");
                } else if let Some(peer) = peer_id
                    // Only peers we are sending to; DHT lookups dial many peers that fail.
                    && self.sends.values().any(|(to, _)| *to == peer)
                {
                    self.dial_errors
                        .insert(peer, dial_reason(&error.to_string()));
                }
            }
            SwarmEvent::ListenerClosed {
                addresses, reason, ..
            } => {
                // A lost or refused relay reservation: forget the relay so the retry tick dials it again.
                for addr in addresses
                    .iter()
                    .filter(|a| a.iter().any(|p| p == Protocol::P2pCircuit))
                {
                    if let Err(e) = &reason {
                        eprintln!("[node] relay reservation {addr} closed: {e}");
                    }
                    let relay = addr.iter().find_map(|p| match p {
                        Protocol::P2p(id) => Some(id),
                        _ => None,
                    });
                    if let Some(relay) = relay {
                        self.relays_up.remove(&relay);
                    }
                }
            }
            SwarmEvent::ConnectionClosed {
                peer_id,
                num_established: 0,
                ..
            } => {
                self.relays_up.remove(&peer_id);
            }
            SwarmEvent::Behaviour(BehaviourEvent::Mdns(mdns::Event::Discovered(found))) => {
                for (peer, addr) in found {
                    self.swarm.add_peer_address(peer, addr);
                    if self.lan.insert(peer) {
                        self.found([peer]);
                    }
                }
            }
            SwarmEvent::Behaviour(BehaviourEvent::Mdns(mdns::Event::Expired(gone))) => {
                for (peer, _) in gone {
                    self.lan.remove(&peer);
                }
            }
            SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received {
                peer_id,
                info,
                ..
            })) => {
                // Feed DHT servers we meet into the routing table. Only public addresses, so a peer
                // cannot make us dial hosts on our own network; a few per peer is plenty.
                if info.protocols.contains(&kad::PROTOCOL_NAME) {
                    for addr in info.listen_addrs.into_iter().filter(is_public).take(8) {
                        self.swarm.behaviour_mut().kad.add_address(&peer_id, addr);
                    }
                }
            }
            SwarmEvent::Behaviour(BehaviourEvent::Kad(kad::Event::OutboundQueryProgressed {
                id,
                result,
                step,
                ..
            })) => {
                if let QueryResult::GetProviders(Ok(GetProvidersOk::FoundProviders {
                    providers,
                    ..
                })) = result
                    && self
                        .search
                        .as_ref()
                        .is_some_and(|s| s.queries.contains(&id))
                {
                    self.found(providers);
                }
                if step.last
                    && let Some(search) = &mut self.search
                {
                    search.queries.remove(&id);
                }
                self.end_search_if_done();
            }
            SwarmEvent::Behaviour(BehaviourEvent::Chat(event)) => self.chat_event(event),
            _ => {}
        }
    }

    // Reports peers of the running search that were not reported yet.
    fn found(&mut self, peers: impl IntoIterator<Item = PeerId>) {
        let own = *self.swarm.local_peer_id();
        let Some(search) = &mut self.search else {
            return;
        };
        for id in peers {
            if id == own || self.relays_up.contains_key(&id) || !search.seen.insert(id) {
                continue;
            }
            let _ = self.events.send(Event::PeerFound(Peer {
                id,
                username: self.names.get(&id).cloned(),
                lan: self.lan.contains(&id),
            }));
        }
    }

    fn end_search_if_done(&mut self) {
        if self
            .search
            .as_ref()
            .is_some_and(|s| s.queries.is_empty() || s.deadline <= Instant::now())
        {
            self.search = None;
            let _ = self.events.send(Event::SearchDone);
        }
    }

    fn chat_event(&mut self, event: request_response::Event<ChatRequest, ChatResponse>) {
        match event {
            request_response::Event::Message { peer, message, .. } => match message {
                request_response::Message::Request {
                    request, channel, ..
                } => {
                    let _ = self
                        .swarm
                        .behaviour_mut()
                        .chat
                        .send_response(channel, ChatResponse);
                    // TODO(crypto): decrypt blob here.
                    let Ok(data) = decode_blob(&request.blob) else {
                        return;
                    };
                    let username = Some(request.username).filter(valid_username).flatten();
                    if let Some(name) = &username {
                        self.names.insert(peer, name.clone());
                    }
                    let from = Peer {
                        id: peer,
                        username,
                        lan: self.lan.contains(&peer),
                    };
                    let _ = self.events.send(Event::Message {
                        from,
                        data: data.to_vec(),
                    });
                }
                request_response::Message::Response { request_id, .. } => {
                    self.dial_errors.remove(&peer);
                    if let Some((_, reply)) = self.sends.remove(&request_id) {
                        let _ = reply.send(Ok(()));
                    }
                }
            },
            request_response::Event::OutboundFailure {
                peer,
                request_id,
                error,
                ..
            } => {
                if let Some((_, reply)) = self.sends.remove(&request_id) {
                    let reason = match (&error, self.dial_errors.remove(&peer)) {
                        (request_response::OutboundFailure::DialFailure, Some(r)) => r,
                        _ => error.to_string(),
                    };
                    let _ = reply.send(Err(io::Error::other(reason)));
                }
            }
            _ => {}
        }
    }
}

fn dial_reason(error: &str) -> String {
    if error.contains("no reservation") {
        "peer is offline or not connected to our relay".into()
    } else {
        format!("could not connect: {error}")
    }
}

// Only plain public IPs: DNS names could resolve to a private address.
fn is_public(addr: &Multiaddr) -> bool {
    match addr.iter().next() {
        Some(Protocol::Ip4(ip)) => is_public_v4(ip),
        Some(Protocol::Ip6(ip)) => match ip.to_ipv4() {
            // IPv4-mapped or -compatible addresses are checked as IPv4.
            Some(v4) => is_public_v4(v4),
            None => {
                let first = ip.segments()[0];
                !(ip.is_loopback()
                    || ip.is_unspecified()
                    || ip.is_multicast()
                    || ip.is_unique_local()
                    || ip.is_unicast_link_local()
                    || first & 0xffc0 == 0xfec0 // site-local
                    || first == 0x2002 // 6to4 embeds IPv4
                    || ip.segments()[..6] == [0x64, 0xff9b, 0, 0, 0, 0]) // NAT64 embeds IPv4
            }
        },
        _ => false,
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_multicast()
        || a == 0
        || a >= 240
        || a == 100 && (b & 0xc0) == 64 // shared/CGNAT, also Tailscale
        || a == 198 && (b & 0xfe) == 18) // benchmarking
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_addresses() {
        for (addr, public) in [
            ("/ip4/8.8.8.8/tcp/4001", true),
            ("/ip6/2001:4860:4860::8888/tcp/4001", true),
            ("/dns4/example.com/tcp/4001", false),
            ("/ip6/::ffff:192.168.0.1/tcp/4001", false),
            ("/ip6/2002:c0a8:1::1/tcp/4001", false),
            ("/ip4/198.18.0.1/tcp/4001", false),
            ("/ip4/240.0.0.1/tcp/4001", false),
            ("/ip4/192.168.0.1/tcp/4001", false),
            ("/ip4/127.0.0.1/tcp/4001", false),
            ("/ip4/100.64.0.1/tcp/4001", false),
            ("/ip6/::1/tcp/4001", false),
            ("/ip6/fd00::1/tcp/4001", false),
        ] {
            assert_eq!(is_public(&addr.parse().unwrap()), public, "{addr}");
        }
    }
}
