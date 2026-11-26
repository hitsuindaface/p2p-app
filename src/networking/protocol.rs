use std::io;

use libp2p::StreamProtocol;
use serde::{Deserialize, Serialize};

pub const CHAT_PROTOCOL: StreamProtocol = StreamProtocol::new("/p2penis/chat/1");
pub const BLOB_SIZE: usize = 1024;
pub const MAX_PAYLOAD: usize = BLOB_SIZE - 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub username: Option<String>,
    pub blob: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse;

// Blob layout: u16 payload length, payload, zero padding up to BLOB_SIZE.
pub fn encode_blob(payload: &[u8]) -> io::Result<Vec<u8>> {
    if payload.len() > MAX_PAYLOAD {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "message too long",
        ));
    }
    let mut blob = vec![0u8; BLOB_SIZE];
    blob[..2].copy_from_slice(&(payload.len() as u16).to_be_bytes());
    blob[2..2 + payload.len()].copy_from_slice(payload);
    Ok(blob)
}

pub fn decode_blob(blob: &[u8]) -> io::Result<&[u8]> {
    let bad = || io::Error::new(io::ErrorKind::InvalidData, "bad blob");
    if blob.len() != BLOB_SIZE {
        return Err(bad());
    }
    let len = u16::from_be_bytes([blob[0], blob[1]]) as usize;
    if len > MAX_PAYLOAD {
        return Err(bad());
    }
    Ok(&blob[2..2 + len])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_roundtrip() {
        let blob = encode_blob(b"dick").unwrap();
        assert_eq!(decode_blob(&blob).unwrap(), b"dick");
        assert!(encode_blob(&[0; MAX_PAYLOAD + 1]).is_err());
        assert!(decode_blob(&blob[1..]).is_err());
    }
}
