use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;

use crate::networking::discovery::lan;
use crate::networking::protocol::{BLOB_SIZE, Blob, NodeId, decode_blob, decode_hello, encode_blob, encode_hello};

pub const DEFAULT_TCP_PORT: u16 = 47801;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

pub struct Config {
    pub tcp_port: u16,
    pub discovery_port: u16,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            tcp_port: DEFAULT_TCP_PORT,
            discovery_port: lan::DEFAULT_PORT,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Event {
    Connected(NodeId, SocketAddr),
    Disconnected(NodeId),
    Message { from: NodeId, data: Vec<u8> },
}

struct Conn {
    peer: NodeId,
    addr: SocketAddr,
    tx: UnboundedSender<Blob>,
    reader: JoinHandle<()>,
    generation: u64,
}

struct Inner {
    id: NodeId,
    tcp_port: u16,
    discovery_port: u16,
    peers: Mutex<Vec<(NodeId, SocketAddr)>>,
    conn: Mutex<Option<Conn>>,
    events: UnboundedSender<Event>,
    next_generation: AtomicU64,
}

#[derive(Clone)]
pub struct Node {
    inner: Arc<Inner>,
}

impl Node {
    pub async fn start(cfg: Config) -> io::Result<(Node, UnboundedReceiver<Event>)> {
        let listener = match TcpListener::bind((Ipv4Addr::UNSPECIFIED, cfg.tcp_port)).await {
            Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
                TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).await?
            }
            r => r?,
        };
        let (events, rx) = mpsc::unbounded_channel();
        let node = Node {
            inner: Arc::new(Inner {
                id: NodeId::generate(),
                tcp_port: listener.local_addr()?.port(),
                discovery_port: cfg.discovery_port,
                peers: Mutex::new(Vec::new()),
                conn: Mutex::new(None),
                events,
                next_generation: AtomicU64::new(1),
            }),
        };

        tokio::spawn(node.clone().accept_loop(listener));
        tokio::spawn(lan::respond(node.id(), node.tcp_port(), cfg.discovery_port));
        Ok((node, rx))
    }

    pub fn id(&self) -> NodeId {
        self.inner.id
    }

    pub fn tcp_port(&self) -> u16 {
        self.inner.tcp_port
    }

    pub async fn list(&self) -> io::Result<Vec<(NodeId, SocketAddr)>> {
        let found = lan::query(self.inner.id, self.inner.discovery_port).await?;
        *self.inner.peers.lock().unwrap() = found.clone();
        Ok(found)
    }

    pub fn peers(&self) -> Vec<(NodeId, SocketAddr)> {
        self.inner.peers.lock().unwrap().clone()
    }

    pub fn connected(&self) -> Option<(NodeId, SocketAddr)> {
        let conn = self.inner.conn.lock().unwrap();
        conn.as_ref().filter(|c| !c.tx.is_closed()).map(|c| (c.peer, c.addr))
    }

    pub async fn connect(&self, addr: SocketAddr) -> io::Result<NodeId> {
        self.disconnect();
        let stream = TcpStream::connect(addr).await?;
        self.setup_conn(stream).await
    }

    pub fn disconnect(&self) {
        let old = self.inner.conn.lock().unwrap().take();
        if let Some(c) = old {
            c.reader.abort();
            self.emit(Event::Disconnected(c.peer));
        }
    }

    pub fn send(&self, data: &[u8]) -> io::Result<()> {
        // TODO(crypto): encrypt blob here.
        let blob = encode_blob(data)?;
        let conn = self.inner.conn.lock().unwrap();
        let conn = conn
            .as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "not connected"))?;
        conn.tx
            .send(blob)
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "connection closed"))
    }

    fn emit(&self, event: Event) {
        let _ = self.inner.events.send(event);
    }

    async fn accept_loop(self, listener: TcpListener) {
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    if self.connected().is_some() {
                        continue;
                    }
                    let node = self.clone();
                    tokio::spawn(async move {
                        if let Err(e) = node.setup_conn(stream).await {
                            eprintln!("[node] incoming {addr} failed: {e}");
                        }
                    });
                }
                Err(e) => eprintln!("[node] accept failed: {e}"),
            }
        }
    }

    async fn setup_conn(&self, stream: TcpStream) -> io::Result<NodeId> {
        let addr = stream.peer_addr()?;
        let (mut rd, mut wr) = stream.into_split();

        // TODO(crypto): key exchange goes next to hello.
        let peer = tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
            wr.write_all(&encode_blob(&encode_hello(self.inner.id))?).await?;
            let mut blob = [0u8; BLOB_SIZE];
            rd.read_exact(&mut blob).await?;
            decode_hello(decode_blob(&blob)?)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "not a p2penis peer"))
        })
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "handshake timed out"))??;

        let (tx, mut rx) = mpsc::unbounded_channel::<Blob>();
        tokio::spawn(async move {
            while let Some(blob) = rx.recv().await {
                if wr.write_all(&blob).await.is_err() {
                    break;
                }
            }
        });

        let generation = self.inner.next_generation.fetch_add(1, Ordering::Relaxed);
        {
            let mut conn = self.inner.conn.lock().unwrap();
            if conn.as_ref().is_some_and(|c| !c.tx.is_closed()) {
                return Err(io::Error::other("already connected"));
            }
            let node = self.clone();
            let reader = tokio::spawn(async move {
                let mut blob = [0u8; BLOB_SIZE];
                while rd.read_exact(&mut blob).await.is_ok() {
                    // TODO(crypto): decrypt blob here.
                    match decode_blob(&blob) {
                        Ok(data) => node.emit(Event::Message { from: peer, data: data.to_vec() }),
                        Err(_) => break,
                    }
                }
                node.drop_conn(generation);
            });
            *conn = Some(Conn { peer, addr, tx, reader, generation });
        }
        self.emit(Event::Connected(peer, addr));
        Ok(peer)
    }

    fn drop_conn(&self, generation: u64) {
        let mut conn = self.inner.conn.lock().unwrap();
        if conn.as_ref().is_some_and(|c| c.generation == generation) {
            let c = conn.take().unwrap();
            self.emit(Event::Disconnected(c.peer));
        }
    }
}
