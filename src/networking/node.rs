use std::io;

use libp2p::{Multiaddr, PeerId};
use tokio::sync::{mpsc, oneshot};

use crate::networking::identity::{Identity, display_name, valid_username};
use crate::networking::{runner, swarm};

// Relays the app uses unless told otherwise. Add, remove or move relays here.
const DEFAULT_RELAYS: &[&str] = &["/dns4/p2p.miro.zip/tcp/47802"];

pub fn default_relays() -> Vec<Multiaddr> {
    DEFAULT_RELAYS
        .iter()
        .filter_map(|a| a.parse().ok())
        .collect()
}

pub struct Config {
    // TODO(crypto): signed envelopes on top of Noise.
    pub identity: Identity,
    pub tcp_port: u16,
    pub lan_discovery: bool,
    // Relay addresses without /p2p; the relay's PeerId is learned on connect.
    pub relays: Vec<Multiaddr>,
}

#[derive(Clone, Debug)]
pub struct Peer {
    pub id: PeerId,
    pub username: Option<String>,
    // Found on the local network (mDNS) rather than only through the DHT.
    pub lan: bool,
}

impl Peer {
    pub fn name(&self) -> String {
        display_name(&self.username, &self.id)
    }
}

#[derive(Clone, Debug)]
pub enum Event {
    Message { from: Peer, data: Vec<u8> },
    // A peer found by the current search: LAN peers come first, DHT peers as they arrive.
    PeerFound(Peer),
    SearchDone,
}

pub(super) enum Command {
    Search,
    Send {
        to: PeerId,
        data: Vec<u8>,
        reply: oneshot::Sender<io::Result<()>>,
    },
}

// Handle to a running node. Cheap to clone; all clones talk to the same background task.
#[derive(Clone)]
pub struct Node {
    me: Peer,
    commands: mpsc::UnboundedSender<Command>,
}

impl Node {
    pub fn start(cfg: Config) -> io::Result<(Node, mpsc::UnboundedReceiver<Event>)> {
        let Identity { keypair, username } = cfg.identity;
        if !valid_username(&username) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "username longer than 32 bytes",
            ));
        }
        let swarm = swarm::build(keypair, cfg.tcp_port, cfg.lan_discovery)?;
        let me = Peer {
            id: *swarm.local_peer_id(),
            username: username.clone(),
            lan: true,
        };
        let (commands, events) = runner::spawn(swarm, username, cfg.relays);
        Ok((Node { me, commands }, events))
    }

    pub fn me(&self) -> &Peer {
        &self.me
    }

    // Starts a search and returns at once. Results arrive as Event::PeerFound, then
    // Event::SearchDone after at most about 10 seconds. A new search replaces a running one.
    pub fn search(&self) -> io::Result<()> {
        self.command(Command::Search)
    }

    pub async fn send(&self, to: PeerId, data: &[u8]) -> io::Result<()> {
        let (reply, rx) = oneshot::channel();
        self.command(Command::Send {
            to,
            data: data.to_vec(),
            reply,
        })?;
        rx.await.map_err(io::Error::other)?
    }

    fn command(&self, c: Command) -> io::Result<()> {
        self.commands
            .send(c)
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "node stopped"))
    }
}
