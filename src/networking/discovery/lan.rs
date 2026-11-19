use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
use tokio::time::{Instant, timeout_at};

use crate::networking::protocol::{Discovery, NodeId};

pub const DEFAULT_PORT: u16 = 47800;
const WAIT: Duration = Duration::from_secs(1);

pub async fn respond(id: NodeId, tcp_port: u16, port: u16) {
    let sock = match bind_shared(port) {
        Ok(s) => s,
        Err(e) => {
            return eprintln!(
                "[lan] cannot listen on udp {port} ({e}), others cannot find this node"
            );
        }
    };
    let reply = Discovery::Announce(id, tcp_port).encode();
    let mut buf = [0u8; 64];
    loop {
        match sock.recv_from(&mut buf).await {
            Ok((len, src)) => {
                if let Some(Discovery::Query(from)) = Discovery::decode(&buf[..len])
                    && from != id
                    && let Err(e) = sock.send_to(&reply, src).await
                {
                    eprintln!("[lan] reply to {src} failed: {e}");
                }
            }
            Err(e) => eprintln!("[lan] recv failed: {e}"),
        }
    }
}

// Lets several instances on one machine listen on the same port; each gets every broadcast.
fn bind_shared(port: u16) -> io::Result<UdpSocket> {
    let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    sock.set_reuse_address(true)?;
    #[cfg(unix)]
    sock.set_reuse_port(true)?;
    sock.set_nonblocking(true)?;
    sock.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)).into())?;
    UdpSocket::from_std(sock.into())
}

pub async fn query(own: NodeId, port: u16) -> io::Result<Vec<(NodeId, SocketAddr)>> {
    let sock = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await?;
    sock.set_broadcast(true)?;
    sock.send_to(&Discovery::Query(own).encode(), (Ipv4Addr::BROADCAST, port))
        .await?;

    let mut found: Vec<(NodeId, SocketAddr)> = Vec::new();
    let mut buf = [0u8; 64];
    let deadline = Instant::now() + WAIT;
    loop {
        let (len, src) = match timeout_at(deadline, sock.recv_from(&mut buf)).await {
            Err(_) => break,
            Ok(Err(_)) => continue,
            Ok(Ok(r)) => r,
        };
        if let Some(Discovery::Announce(id, tcp_port)) = Discovery::decode(&buf[..len])
            && id != own
            && !found.iter().any(|(f, _)| *f == id)
        {
            found.push((id, SocketAddr::new(src.ip(), tcp_port)));
        }
    }
    Ok(found)
}
