use super::error::CryptoError;
use libp2p::identity::PublicKey;
use sha2::{Digest, Sha512};

const DOMAIN: &[u8] = b"p2pmsg-fingerprint-v1";

fn raw_ed25519(pk: &PublicKey) -> Result<[u8; 32], CryptoError> {
    pk.clone()
        .try_into_ed25519()
        .map(|k| k.to_bytes())
        .map_err(|_| CryptoError::KeyDecode)
}

/// 30 digits (six groups of five) for one party.
fn fingerprint_digits(raw: &[u8; 32]) -> String {
    let hash = Sha512::new()
        .chain_update(DOMAIN)
        .chain_update(raw)
        .finalize();
    hash[..30]
        .chunks(5)
        .map(|c| {
            let n = c.iter().fold(0u64, |acc, &b| (acc << 8) | b as u64);
            format!("{:05}", n % 100_000)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Same 60-digit string on both sides: smaller public key first.
pub fn safety_number(a: &PublicKey, b: &PublicKey) -> Result<String, CryptoError> {
    let (ra, rb) = (raw_ed25519(a)?, raw_ed25519(b)?);
    let (first, second) = if ra <= rb { (ra, rb) } else { (rb, ra) };
    Ok(format!(
        "{} {}",
        fingerprint_digits(&first),
        fingerprint_digits(&second)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::{Keypair, ed25519};

    #[test]
    fn symmetric() {
        let (a, b) = (Keypair::generate_ed25519(), Keypair::generate_ed25519());
        assert_eq!(
            safety_number(&a.public(), &b.public()).unwrap(),
            safety_number(&b.public(), &a.public()).unwrap()
        );
    }

    #[test]
    fn differs_for_different_peers() {
        let (a, b, c) = (
            Keypair::generate_ed25519(),
            Keypair::generate_ed25519(),
            Keypair::generate_ed25519(),
        );
        assert_ne!(
            safety_number(&a.public(), &b.public()).unwrap(),
            safety_number(&a.public(), &c.public()).unwrap()
        );
    }

    fn key_from_seed(seed: u8) -> PublicKey {
        let sk = ed25519::SecretKey::try_from_bytes([seed; 32]).unwrap();
        Keypair::from(ed25519::Keypair::from(sk)).public()
    }

    #[test]
    fn matches_reference_vector() {
        let (a, b) = (key_from_seed(1), key_from_seed(2));
        assert_eq!(
            safety_number(&a, &b).unwrap(),
            "87203 84067 04458 83833 42521 19561 54395 73280 74865 89283 21916 41374"
        );
    }

    #[test]
    fn rsa_key_is_an_error_not_a_panic() {
        // any libp2p peer can present an RSA key, so this must not panic (T7)
        let mut der = include_bytes!("test_rsa_key.der").to_vec();
        let rsa = Keypair::rsa_from_pkcs8(&mut der).unwrap().public();
        let ed = Keypair::generate_ed25519().public();
        assert!(safety_number(&rsa, &ed).is_err());
        assert!(safety_number(&ed, &rsa).is_err());
    }
}
