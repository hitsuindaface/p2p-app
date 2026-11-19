use libp2p::identity::PublicKey;
use sha2::{Digest, Sha512};

const DOMAIN: &[u8] = b"p2pmsg-fingerprint-v1";

fn raw_ed25519(pk: &PublicKey) -> [u8; 32] {
    pk.clone()
        .try_into_ed25519()
        .expect("identity keys are always Ed25519")
        .to_bytes()
}

/// 30 digits (six groups of five) for one party.
fn fingerprint_digits(raw: &[u8; 32]) -> String {
    let hash = Sha512::new().chain_update(DOMAIN).chain_update(raw).finalize();
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
pub fn safety_number(a: &PublicKey, b: &PublicKey) -> String {
    let (ra, rb) = (raw_ed25519(a), raw_ed25519(b));
    let (first, second) = if ra <= rb { (ra, rb) } else { (rb, ra) };
    format!("{} {}", fingerprint_digits(&first), fingerprint_digits(&second))
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair;

    #[test]
    fn symmetric() {
        let (a, b) = (Keypair::generate_ed25519(), Keypair::generate_ed25519());
        assert_eq!(safety_number(&a.public(), &b.public()),
                   safety_number(&b.public(), &a.public()));
    }

    #[test]
    fn differs_for_different_peers() {
        let (a, b, c) = (Keypair::generate_ed25519(), Keypair::generate_ed25519(),
                         Keypair::generate_ed25519());
        assert_ne!(safety_number(&a.public(), &b.public()),
                   safety_number(&a.public(), &c.public()));
    }
}