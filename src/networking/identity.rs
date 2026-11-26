use libp2p::PeerId;
use libp2p::identity::{Keypair, ed25519};

pub const KEY_LEN: usize = 32;
pub const MAX_USERNAME: usize = 32;

// TODO(crypto): merge with the crypto module's Identity.
pub struct Identity {
    pub keypair: Keypair,
    pub username: Option<String>,
}

impl Identity {
    // Saved key file layout: public key, u8 username length, username, secret key.
    pub fn decode(buf: &[u8]) -> Option<Self> {
        let (public, rest) = buf.split_at_checked(KEY_LEN)?;
        let (&len, rest) = rest.split_first()?;
        let (name, secret) = rest.split_at_checked(len as usize)?;
        let mut secret: [u8; KEY_LEN] = secret.try_into().ok()?;
        let pair = ed25519::Keypair::from(ed25519::SecretKey::try_from_bytes(&mut secret).ok()?);
        if pair.public().to_bytes() != public {
            return None;
        }
        let name = String::from_utf8(name.to_vec()).ok()?;
        let username = (!name.is_empty()).then_some(name);
        valid_username(&username).then_some(Identity {
            keypair: pair.into(),
            username,
        })
    }
}

pub fn valid_username(username: &Option<String>) -> bool {
    username.as_ref().is_none_or(|n| n.len() <= MAX_USERNAME)
}

// Shown when a user has no username.
pub fn display_name(username: &Option<String>, id: &PeerId) -> String {
    username.clone().unwrap_or_else(|| {
        let s = id.to_base58();
        s[s.len() - 8..].to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode() {
        let pair = ed25519::Keypair::generate();
        let mut file = pair.public().to_bytes().to_vec();
        file.push(3);
        file.extend_from_slice(b"bob");
        file.extend_from_slice(pair.secret().as_ref());
        let id = Identity::decode(&file).unwrap();
        assert_eq!(id.username.as_deref(), Some("bob"));
        assert_eq!(id.keypair.public(), Keypair::from(pair).public());
        file[0] ^= 1;
        assert!(Identity::decode(&file).is_none());
    }
}
