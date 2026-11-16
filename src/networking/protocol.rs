use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAGIC: &[u8; 7] = b"p2penis";
pub const BLOB_SIZE: usize = 1024;
pub const MAX_PAYLOAD: usize = BLOB_SIZE - 2;
pub const NODE_ID_LEN: usize = 16;

pub type Blob = [u8; BLOB_SIZE];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub [u8; NODE_ID_LEN]);

impl NodeId {
    // TODO(crypto): derive from public key instead of random bytes.
    pub fn generate() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let mut h = blake3::Hasher::new();
        h.update(&nanos.to_le_bytes());
        h.update(&std::process::id().to_le_bytes());
        let mut id = [0u8; NODE_ID_LEN];
        id.copy_from_slice(&h.finalize().as_bytes()[..NODE_ID_LEN]);
        NodeId(id)
    }

    pub fn short(&self) -> String {
        self.0[..4].iter().map(|b| format!("{b:02x}")).collect()
    }
}

const QUERY: u8 = b'?';
const ANNOUNCE: u8 = b'!';

// UDP discovery: MAGIC + kind + id (+ tcp port for Announce).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Discovery {
    Query(NodeId),
    Announce(NodeId, u16),
}

impl Discovery {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = MAGIC.to_vec();
        match self {
            Discovery::Query(id) => {
                b.push(QUERY);
                b.extend_from_slice(&id.0);
            }
            Discovery::Announce(id, tcp_port) => {
                b.push(ANNOUNCE);
                b.extend_from_slice(&id.0);
                b.extend_from_slice(&tcp_port.to_be_bytes());
            }
        }
        b
    }

    pub fn decode(buf: &[u8]) -> Option<Self> {
        let (&kind, rest) = buf.strip_prefix(MAGIC)?.split_first()?;
        let (id, rest) = rest.split_at_checked(NODE_ID_LEN)?;
        let id = NodeId(id.try_into().ok()?);
        match (kind, rest.len()) {
            (QUERY, 0) => Some(Discovery::Query(id)),
            (ANNOUNCE, 2) => Some(Discovery::Announce(id, u16::from_be_bytes([rest[0], rest[1]]))),
            _ => None,
        }
    }
}

// TODO(crypto): key exchange goes next to hello.
pub fn encode_hello(id: NodeId) -> Vec<u8> {
    [MAGIC.as_slice(), &id.0].concat()
}

pub fn decode_hello(buf: &[u8]) -> Option<NodeId> {
    let id = buf.strip_prefix(MAGIC)?;
    Some(NodeId(id.try_into().ok()?))
}

// Blob layout: u16 payload length, payload, zero padding up to BLOB_SIZE.
pub fn encode_blob(payload: &[u8]) -> io::Result<Blob> {
    if payload.len() > MAX_PAYLOAD {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "message too long"));
    }
    let mut blob = [0u8; BLOB_SIZE];
    blob[..2].copy_from_slice(&(payload.len() as u16).to_be_bytes());
    blob[2..2 + payload.len()].copy_from_slice(payload);
    Ok(blob)
}

pub fn decode_blob(blob: &Blob) -> io::Result<&[u8]> {
    let len = u16::from_be_bytes([blob[0], blob[1]]) as usize;
    if len > MAX_PAYLOAD {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bad blob length"));
    }
    Ok(&blob[2..2 + len])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_roundtrip() {
        let id = NodeId::generate();
        for msg in [Discovery::Query(id), Discovery::Announce(id, 1234)] {
            assert_eq!(Discovery::decode(&msg.encode()), Some(msg));
        }
        assert_eq!(Discovery::decode(b"garbage"), None);
    }

    #[test]
    fn blob_roundtrip() {
        let blob = encode_blob("dick".as_bytes()).unwrap();
        assert_eq!(decode_blob(&blob).unwrap(), "dick".as_bytes());
        assert!(encode_blob(&[0; MAX_PAYLOAD + 1]).is_err());
    }
}
