use std::io;
use std::net::Ipv4Addr;

pub const MAGIC: &[u8; 7] = b"p2penis";
pub const BLOB_SIZE: usize = 1024;
pub const MAX_PAYLOAD: usize = BLOB_SIZE - 2;
pub const NODE_ID_LEN: usize = 16;
pub const KEY_LEN: usize = 32;
pub const MAX_USERNAME: usize = 32;

pub type Blob = [u8; BLOB_SIZE];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub [u8; NODE_ID_LEN]);

impl NodeId {
    pub fn short(&self) -> String {
        self.0[..4].iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PublicKey(pub [u8; KEY_LEN]);

impl PublicKey {
    pub fn id(&self) -> NodeId {
        let mut id = [0u8; NODE_ID_LEN];
        id.copy_from_slice(&blake3::hash(&self.0).as_bytes()[..NODE_ID_LEN]);
        NodeId(id)
    }
}

// Saved key file layout: public key, u8 username length, username, secret key.
pub struct Identity {
    pub public_key: PublicKey,
    pub username: Option<String>,
    pub secret_key: Vec<u8>,
}

impl Identity {
    pub fn decode(buf: &[u8]) -> Option<Self> {
        let (key, rest) = buf.split_at_checked(KEY_LEN)?;
        let (username, secret_key) = read_username(rest)?;
        Some(Identity {
            public_key: PublicKey(key.try_into().ok()?),
            username,
            secret_key: secret_key.to_vec(),
        })
    }
}

// Shown when a user has no username.
pub fn display_name(username: &Option<String>, key: &PublicKey) -> String {
    username.clone().unwrap_or_else(|| key.id().short())
}

// u8 length then UTF-8 bytes; length 0 means no username.
fn read_username(buf: &[u8]) -> Option<(Option<String>, &[u8])> {
    let (&len, rest) = buf.split_first()?;
    if len as usize > MAX_USERNAME {
        return None;
    }
    let (name, rest) = rest.split_at_checked(len as usize)?;
    let name = String::from_utf8(name.to_vec()).ok()?;
    Some(((!name.is_empty()).then_some(name), rest))
}

// What a user advertises. Layout: key, tcp port, u8 username length, username.
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub username: Option<String>,
    pub public_key: PublicKey,
    pub tcp_port: u16,
}

impl Record {
    fn write(&self, b: &mut Vec<u8>) {
        b.extend_from_slice(&self.public_key.0);
        b.extend_from_slice(&self.tcp_port.to_be_bytes());
        let name = self.username.as_deref().unwrap_or("");
        b.push(name.len() as u8);
        b.extend_from_slice(name.as_bytes());
    }

    fn read(buf: &[u8]) -> Option<(Record, &[u8])> {
        let (key, rest) = buf.split_at_checked(KEY_LEN)?;
        let (port, rest) = rest.split_at_checked(2)?;
        let (username, rest) = read_username(rest)?;
        let record = Record {
            username,
            public_key: PublicKey(key.try_into().ok()?),
            tcp_port: u16::from_be_bytes(port.try_into().ok()?),
        };
        Some((record, rest))
    }
}

const QUERY: u8 = b'?';
const ANNOUNCE: u8 = b'!';
const PEER: u8 = b'@';

// UDP discovery: MAGIC + kind + body.
// Query: asker id. Announce: own record. Peer: record + IPv4, a server relaying someone else.
// TODO(crypto): sign records so nobody can announce someone else's key.
#[derive(Clone, Debug, PartialEq)]
pub enum Discovery {
    Query(NodeId),
    Announce(Record),
    Peer(Record, Ipv4Addr),
}

impl Discovery {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = MAGIC.to_vec();
        match self {
            Discovery::Query(id) => {
                b.push(QUERY);
                b.extend_from_slice(&id.0);
            }
            Discovery::Announce(record) => {
                b.push(ANNOUNCE);
                record.write(&mut b);
            }
            Discovery::Peer(record, ip) => {
                b.push(PEER);
                record.write(&mut b);
                b.extend_from_slice(&ip.octets());
            }
        }
        b
    }

    pub fn decode(buf: &[u8]) -> Option<Self> {
        let (&kind, rest) = buf.strip_prefix(MAGIC)?.split_first()?;
        match kind {
            QUERY => Some(Discovery::Query(NodeId(rest.try_into().ok()?))),
            ANNOUNCE => match Record::read(rest)? {
                (record, []) => Some(Discovery::Announce(record)),
                _ => None,
            },
            PEER => {
                let (record, ip) = Record::read(rest)?;
                let ip: [u8; 4] = ip.try_into().ok()?;
                Some(Discovery::Peer(record, ip.into()))
            }
            _ => None,
        }
    }
}

// TODO(crypto): key exchange goes next to hello.
pub fn encode_hello(record: &Record) -> Vec<u8> {
    let mut b = MAGIC.to_vec();
    record.write(&mut b);
    b
}

pub fn decode_hello(buf: &[u8]) -> Option<Record> {
    match Record::read(buf.strip_prefix(MAGIC)?)? {
        (record, []) => Some(record),
        _ => None,
    }
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
        let record = Record { username: Some("alice".into()), public_key: PublicKey([7; KEY_LEN]), tcp_port: 1234 };
        let nameless = Record { username: None, ..record.clone() };
        assert_eq!(decode_hello(&encode_hello(&nameless)), Some(nameless));
        for msg in [
            Discovery::Query(record.public_key.id()),
            Discovery::Announce(record.clone()),
            Discovery::Peer(record.clone(), Ipv4Addr::new(100, 64, 0, 1)),
        ] {
            assert_eq!(Discovery::decode(&msg.encode()), Some(msg));
        }
        assert_eq!(decode_hello(&encode_hello(&record)), Some(record));
        assert_eq!(Discovery::decode(b"garbage"), None);
    }

    #[test]
    fn identity_decode() {
        let mut file = vec![9; KEY_LEN];
        file.push(3);
        file.extend_from_slice(b"bob");
        file.extend_from_slice(&[5; 64]);
        let id = Identity::decode(&file).unwrap();
        assert_eq!(id.public_key, PublicKey([9; KEY_LEN]));
        assert_eq!(id.username.as_deref(), Some("bob"));
        assert_eq!(id.secret_key, vec![5; 64]);
    }

    #[test]
    fn blob_roundtrip() {
        let blob = encode_blob("dick".as_bytes()).unwrap();
        assert_eq!(decode_blob(&blob).unwrap(), "dick".as_bytes());
        assert!(encode_blob(&[0; MAX_PAYLOAD + 1]).is_err());
    }
}
