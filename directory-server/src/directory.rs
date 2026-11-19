use std::collections::HashMap;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use tokio::net::UdpSocket;

use crate::protocol::{Discovery, PublicKey, Record};

pub const DEFAULT_PORT: u16 = 47802;
const REGISTER_INTERVAL: Duration = Duration::from_secs(20);
const RECORD_TTL: Duration = Duration::from_secs(60);
const MAX_RECORDS: usize = 10_000;

// Client side: keep this node's record fresh on every server.
pub async fn register(record: Record, servers: Vec<SocketAddr>) {
    if servers.is_empty() {
        return;
    }
    let sock = match UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await {
        Ok(s) => s,
        Err(e) => return eprintln!("[directory] cannot open udp socket: {e}"),
    };
    let msg = Discovery::Announce(record).encode();
    let mut tick = tokio::time::interval(REGISTER_INTERVAL);
    loop {
        tick.tick().await;
        for server in &servers {
            if let Err(e) = sock.send_to(&msg, server).await {
                eprintln!("[directory] register at {server} failed: {e}");
            }
        }
    }
}

// Server side: remember announced records with the sender's IP and hand them out on query.
pub async fn serve(port: u16) -> io::Result<()> {
    let sock = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port)).await?;
    let mut records: HashMap<PublicKey, (Record, Ipv4Addr, Instant)> = HashMap::new();
    let mut buf = [0u8; 512];
    loop {
        let Ok((len, SocketAddr::V4(src))) = sock.recv_from(&mut buf).await else {
            continue;
        };
        match Discovery::decode(&buf[..len]) {
            Some(Discovery::Announce(record)) => {
                records.retain(|_, (_, _, seen)| seen.elapsed() < RECORD_TTL);
                if records.len() < MAX_RECORDS || records.contains_key(&record.public_key) {
                    records.insert(record.public_key, (record, *src.ip(), Instant::now()));
                }
            }
            Some(Discovery::Query(from)) => {
                for (record, ip, seen) in records.values() {
                    if seen.elapsed() < RECORD_TTL && record.public_key.id() != from {
                        let reply = Discovery::Peer(record.clone(), *ip).encode();
                        let _ = sock.send_to(&reply, src).await;
                    }
                }
            }
            _ => {}
        }
    }
}
