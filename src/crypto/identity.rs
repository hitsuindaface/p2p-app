use libp2p::PeerId;
use libp2p::identity::{Keypair, PublicKey};
use zeroize::Zeroizing;

use super::error::CryptoError;

pub struct Identity {
    keypair: Keypair,
}

impl Identity {
    pub fn generate() -> Self {
        Self {
            keypair: Keypair::generate_ed25519(),
        }
    }

    /// Secret bytes for storage to save inside the encrypted database
    pub fn to_bytes(&self) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
        self.keypair
            .to_protobuf_encoding()
            .map(Zeroizing::new)
            .map_err(|_| CryptoError::KeyDecode)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        let keypair = Keypair::from_protobuf_encoding(bytes).map_err(|_| CryptoError::KeyDecode)?;
        if keypair.public().try_into_ed25519().is_err() {
            return Err(CryptoError::KeyDecode);
        }
        Ok(Self { keypair })
    }

    pub fn peer_id(&self) -> PeerId {
        self.keypair.public().to_peer_id()
    }
    pub fn public(&self) -> PublicKey {
        self.keypair.public()
    }

    /// For the messaging lead: SwarmBuilder::with_existing_identity(id.swarm_keypair())
    pub fn swarm_keypair(&self) -> Keypair {
        self.keypair.clone()
    }

    pub(crate) fn sign(&self, msg: &[u8]) -> Result<Vec<u8>, CryptoError> {
        self.keypair.sign(msg).map_err(|_| CryptoError::Signing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_and_load_gives_same_identity() {
        let id = Identity::generate();
        let bytes = id.to_bytes().unwrap();
        let loaded = Identity::from_bytes(&bytes).unwrap();
        assert_eq!(id.peer_id(), loaded.peer_id());
    }

    #[test]
    fn loaded_identity_can_still_sign() {
        // a wrong load could give the right public key, but wrong secret key
        let loaded = Identity::from_bytes(&Identity::generate().to_bytes().unwrap()).unwrap();
        let sig = loaded.sign(b"hello").unwrap();
        assert!(loaded.public().verify(b"hello", &sig));
    }

    #[test]
    fn two_identities_differ() {
        assert_ne!(
            Identity::generate().peer_id(),
            Identity::generate().peer_id()
        );
    }

    #[test]
    fn garbage_bytes_are_rejected() {
        for bad in [&[][..], &[0u8; 10][..], &[0xff; 100][..]] {
            assert!(matches!(
                Identity::from_bytes(bad),
                Err(CryptoError::KeyDecode)
            ));
        }
    }

    #[test]
    fn signature_fails_for_wrong_message_or_wrong_key() {
        let (a, b) = (Identity::generate(), Identity::generate());
        let sig = a.sign(b"hello").unwrap();
        assert!(!a.public().verify(b"hellp", &sig)); // tampered message
        assert!(!b.public().verify(b"hello", &sig)); // someone else's key
    }

    #[test]
    fn swarm_key_pair_is_the_same_identity() {
        // if this fails, envelope check 4 rejects very message
        let id = Identity::generate();
        assert_eq!(id.swarm_keypair().public().to_peer_id(), id.peer_id());
    }

    #[test]
    fn peer_id_matches_public_key() {
        let id = Identity::generate();
        assert_eq!(id.public().to_peer_id(), id.peer_id());
    }
}
