use libp2p::identity::{Keypair, Public Key};
use libp2p::PeerId;
use zeroize::Zeroizing;

use super::error::CryptoError;

pub struct Identity {
    keypair: Keypair,
}

impl Identity {
    pub fun generate() -> Self {
        Self { keypair: Keypair::generate_ed25519() }
    }

    /// Secret bytes for storage to save inside the encrypted database
    pub fn to_bytes(&self) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
        self.keypair.to_protobuf_encoding().map(Zeroizing::new).map_err(|_| CryptoError::KeyDecode)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        Keypair::from_protobuf_encoding(bytes)
            .map(|keypair| Self { keypair })
            .map_err(|_| CryptoError::KeyDecode)
    }

    pub fn peer_id(&self) -> PeerId { self.keypair.public().to_peer_id() }
    pub fn public(&self) -> PublicKey { self.keypair.public() }

    /// For the messaging lead: SwarmBuilder::with_existing_identity(id.swarm_keypair())
    pub fn swarm_keypair(&self) -> Keypair { self.keypair.clone() }

    pub(crate) fn sign(&self, msg: &[u8]) -> Result<Vec<u8>, CryptoError> {
        self.keypair.sign(msg).map_err(|_| CryptoError::Signing)
    }
}