use std::io;
use std::net::SocketAddr;

use crate::{Config, Event, Node, NodeId};

pub async fn start(cfg: Config, on_event: impl Fn(Event) + Send + 'static) -> io::Result<Node> {
    let (node, mut events) = Node::start(cfg).await?;
    tokio::spawn(async move {
        while let Some(ev) = events.recv().await {
            on_event(ev);
        }
    });
    Ok(node)
}

pub async fn list(node: &Node) -> io::Result<Vec<(NodeId, SocketAddr)>> {
    node.list().await
}

pub async fn connect(node: &Node, peer_index: usize) -> io::Result<NodeId> {
    let (_, addr) = node
        .peers()
        .get(peer_index)
        .copied()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such peer, run /list"))?;
    node.connect(addr).await
}

pub fn disconnect(node: &Node) {
    node.disconnect();
}

pub fn send(node: &Node, text: &str) -> io::Result<()> {
    node.send(text.as_bytes())
}
