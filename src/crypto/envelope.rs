use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

use libp2p::{PeerId, identity::PublicKey};
use serde::{Deserialize, Serialize};

use super::error::{CryptoError, RejectReason};
use super::identity::Identity;

const DOMAIN: &[u8] = b"p2pmsg-envelope-v1";
const VERSION: u8 = 1;
pub const MAX_BODY: usize = 64 * 1024;
pub const MAX_WIRE: usize = 72 * 1024;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    FileOffer,
    Receipt,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Payload {
    pub version: u8,
    pub msg_id: [u8; 16],
    #[serde(with = "serde_bytes")]
    pub sender_key: Vec<u8>, // protobuf-encoded PublicKey
    #[serde(with = "serde_bytes")]
    pub recipient: Vec<u8>, // PeerId bytes
    pub timestamp: u64, // display only
    pub kind: Kind,
    #[serde(with = "serde_bytes")]
    pub body: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    #[serde(with = "serde_bytes")]
    payload: Vec<u8>, // CBOR(Payload), exactly as signed
    #[serde(with = "serde_bytes")]
    signature: Vec<u8>,
}

fn signing_input(payload: &[u8]) -> Vec<u8> {
    [DOMAIN, payload].concat()
}

fn to_cbor<T: Serialize>(v: &T) -> Result<Vec<u8>, CryptoError> {
    let mut out = Vec::new();
    ciborium::into_writer(v, &mut out).map_err(|_| CryptoError::Encode)?;
    Ok(out)
}

// Build, sign and encode an envelope. Returns the bytes to send.
pub fn seal(
    me: &Identity,
    recipient: &PeerId,
    kind: Kind,
    body: Vec<u8>,
) -> Result<Vec<u8>, CryptoError> {
    if body.len() > MAX_BODY {
        return Err(CryptoError::BodyTooLarge);
    }

    let payload = Payload {
        version: VERSION,
        msg_id: rand::random(),
        sender_key: me.public().encode_protobuf(),
        recipient: recipient.to_bytes(),
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        kind,
        body,
    };

    let payload = to_cbor(&payload)?;
    let signature = me.sign(&signing_input(&payload))?;
    to_cbor(&Envelope { payload, signature })
}

/// Why `open` rejected a message. `msg_id` is set whenever the payload could be
/// decoded, so the caller can reply `Ack { msg_id, status: Rejected }` (spec §6).
/// The id is not authenticated if the signature check had not passed yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rejected {
    pub reason: RejectReason,
    pub msg_id: Option<[u8; 16]>,
}

impl From<RejectReason> for Rejected {
    fn from(reason: RejectReason) -> Self {
        Self {
            reason,
            msg_id: None,
        }
    }
}

// The seven receive checks (spec §6), in order.
pub fn open(
    wire: &[u8],
    remote: &PeerId, // authenticated by Noise
    me: &PeerId,
    seen: &mut HashSet<(PeerId, [u8; 16])>,
) -> Result<Payload, Rejected> {
    let (env, p) = decode(wire)?;
    let msg_id = p.msg_id;
    verify(&env, &p, remote, me, seen).map_err(|reason| Rejected {
        reason,
        msg_id: Some(msg_id),
    })?;
    Ok(p)
}

// Checks 1-2. Fails before a msg_id can be read, except for an oversized body.
fn decode(wire: &[u8]) -> Result<(Envelope, Payload), Rejected> {
    // 1. size before any parsing
    if wire.len() > MAX_WIRE {
        return Err(RejectReason::TooLarge.into());
    }

    // 2. decode
    let env: Envelope = ciborium::from_reader(wire).map_err(|_| RejectReason::Malformed)?;
    let p: Payload =
        ciborium::from_reader(env.payload.as_slice()).map_err(|_| RejectReason::Malformed)?;
    if p.body.len() > MAX_BODY {
        return Err(Rejected {
            reason: RejectReason::TooLarge,
            msg_id: Some(p.msg_id),
        });
    }
    Ok((env, p))
}

// Checks 3-7, once the msg_id is known.
fn verify(
    env: &Envelope,
    p: &Payload,
    remote: &PeerId,
    me: &PeerId,
    seen: &mut HashSet<(PeerId, [u8; 16])>,
) -> Result<(), RejectReason> {
    // 3. version
    if p.version != VERSION {
        return Err(RejectReason::BadVersion);
    }

    // 4. the key in the envelope belongs to the peer on this connection
    let key = PublicKey::try_decode_protobuf(&p.sender_key).map_err(|_| RejectReason::Malformed)?;
    if key.to_peer_id() != *remote {
        return Err(RejectReason::WrongSender);
    }
    // identities are Ed25519 only (spec §2), even if libp2p accepts RSA
    if key.clone().try_into_ed25519().is_err() {
        return Err(RejectReason::Malformed);
    }

    // 5. addressed to us
    if PeerId::from_bytes(&p.recipient).ok().as_ref() != Some(me) {
        return Err(RejectReason::WrongRecipient);
    }

    // 6. signature over the exact received bytes
    if !key.verify(&signing_input(&env.payload), &env.signature) {
        return Err(RejectReason::BadSignature);
    }

    // 7. replay
    if !seen.insert((*remote, p.msg_id)) {
        return Err(RejectReason::Replay);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair;
    use proptest::prelude::*;

    // Helpers

    fn open_as(
        wire: &[u8],
        from: &Identity,
        to: &Identity,
        seen: &mut HashSet<(PeerId, [u8; 16])>,
    ) -> Result<Payload, Rejected> {
        open(wire, &from.peer_id(), &to.peer_id(), seen)
    }

    fn payload(from: &Identity, to: &Identity, body: &[u8]) -> Payload {
        Payload {
            version: VERSION,
            msg_id: rand::random(),
            sender_key: from.public().encode_protobuf(),
            recipient: to.peer_id().to_bytes(),
            timestamp: 0,
            kind: Kind::Text,
            body: body.to_vec(),
        }
    }

    /// Encode and sign any payload, including ones `seal` refuses to build.
    fn sign_raw(signer: &Identity, p: &Payload, domain: &[u8]) -> Vec<u8> {
        let payload = to_cbor(p).unwrap();
        let signature = signer.sign(&[domain, &payload[..]].concat()).unwrap();
        to_cbor(&Envelope { payload, signature }).unwrap()
    }

    fn signed(signer: &Identity, p: &Payload) -> Vec<u8> {
        sign_raw(signer, p, DOMAIN)
    }

    fn decode_envelope(wire: &[u8]) -> (Envelope, Payload) {
        let env: Envelope = ciborium::from_reader(wire).unwrap();
        let p: Payload = ciborium::from_reader(env.payload.as_slice()).unwrap();
        (env, p)
    }

    fn rejected(reason: RejectReason, msg_id: [u8; 16]) -> Rejected {
        Rejected {
            reason,
            msg_id: Some(msg_id),
        }
    }

    fn rsa_keypair() -> Keypair {
        // test-only key (rsa feature is enabled for dev builds only)
        Keypair::rsa_from_pkcs8(&mut include_bytes!("test_rsa_key.der").to_vec()).unwrap()
    }

    // Happy path

    #[test]
    fn seal_then_open_roundtrip() {
        let (alice, bob) = (Identity::generate(), Identity::generate());
        let wire = seal(&alice, &bob.peer_id(), Kind::Text, b"hello".to_vec()).unwrap();
        let p = open_as(&wire, &alice, &bob, &mut HashSet::new()).unwrap();
        assert_eq!(p.version, VERSION);
        assert_eq!(p.kind, Kind::Text);
        assert_eq!(p.body, b"hello");
        let sender = PublicKey::try_decode_protobuf(&p.sender_key).unwrap();
        assert_eq!(sender.to_peer_id(), alice.peer_id());
    }

    #[test]
    fn every_kind_roundtrips() {
        let (alice, bob) = (Identity::generate(), Identity::generate());
        let mut seen = HashSet::new();
        for kind in [Kind::Text, Kind::FileOffer, Kind::Receipt] {
            let wire = seal(&alice, &bob.peer_id(), kind, vec![1, 2, 3]).unwrap();
            assert_eq!(open_as(&wire, &alice, &bob, &mut seen).unwrap().kind, kind);
        }
    }

    #[test]
    fn empty_body_is_ok() {
        let (alice, bob) = (Identity::generate(), Identity::generate());
        let wire = seal(&alice, &bob.peer_id(), Kind::Text, vec![]).unwrap();
        assert!(
            open_as(&wire, &alice, &bob, &mut HashSet::new())
                .unwrap()
                .body
                .is_empty()
        );
    }

    #[test]
    fn max_body_fits_on_wire() {
        // random bytes are the worst case if bodies are encoded as CBOR arrays
        let (alice, bob) = (Identity::generate(), Identity::generate());
        let body: Vec<u8> = (0..MAX_BODY).map(|_| rand::random()).collect();
        let wire = seal(&alice, &bob.peer_id(), Kind::Text, body.clone()).unwrap();
        assert!(wire.len() <= MAX_WIRE, "wire is {} bytes", wire.len());
        assert_eq!(
            open_as(&wire, &alice, &bob, &mut HashSet::new())
                .unwrap()
                .body,
            body
        );
    }

    #[test]
    fn msg_ids_are_unique() {
        let (alice, bob) = (Identity::generate(), Identity::generate());
        let mut seen = HashSet::new();
        let a = seal(&alice, &bob.peer_id(), Kind::Text, b"same".to_vec()).unwrap();
        let b = seal(&alice, &bob.peer_id(), Kind::Text, b"same".to_vec()).unwrap();
        let (pa, pb) = (
            open_as(&a, &alice, &bob, &mut seen).unwrap(),
            open_as(&b, &alice, &bob, &mut seen).unwrap(),
        );
        assert_ne!(pa.msg_id, pb.msg_id);
    }

    // Sender side

    #[test]
    fn seal_rejects_oversized_body() {
        let (alice, bob) = (Identity::generate(), Identity::generate());
        let res = seal(&alice, &bob.peer_id(), Kind::Text, vec![0; MAX_BODY + 1]);
        assert!(matches!(res, Err(CryptoError::BodyTooLarge)));
    }

    // One test per receive check

    #[test]
    fn check1_rejects_wire_over_limit() {
        let (alice, bob) = (Identity::generate(), Identity::generate());
        let r = open_as(&vec![0; MAX_WIRE + 1], &alice, &bob, &mut HashSet::new()).unwrap_err();
        assert_eq!(r, RejectReason::TooLarge.into());
    }

    #[test]
    fn check2_rejects_garbage() {
        let (alice, bob) = (Identity::generate(), Identity::generate());
        let valid = seal(&alice, &bob.peer_id(), Kind::Text, b"hi".to_vec()).unwrap();
        let random: Vec<u8> = (0..200).map(|_| rand::random()).collect();
        for bad in [&[][..], &random[..], &valid[..valid.len() - 1]] {
            let r = open_as(bad, &alice, &bob, &mut HashSet::new()).unwrap_err();
            assert_eq!(r, RejectReason::Malformed.into());
        }
    }

    #[test]
    fn check2_rejects_payload_that_is_not_a_payload() {
        let (alice, bob) = (Identity::generate(), Identity::generate());
        let wire = to_cbor(&Envelope {
            payload: b"not cbor".to_vec(),
            signature: vec![0; 64],
        })
        .unwrap();
        let r = open_as(&wire, &alice, &bob, &mut HashSet::new()).unwrap_err();
        assert_eq!(r, RejectReason::Malformed.into());
    }

    #[test]
    fn check2_rejects_oversized_body_with_its_msg_id() {
        let (alice, bob) = (Identity::generate(), Identity::generate());
        let p = payload(&alice, &bob, &vec![0; MAX_BODY + 1]);
        let r = open_as(&signed(&alice, &p), &alice, &bob, &mut HashSet::new()).unwrap_err();
        assert_eq!(r, rejected(RejectReason::TooLarge, p.msg_id));
    }

    #[test]
    fn check3_rejects_wrong_version() {
        let (alice, bob) = (Identity::generate(), Identity::generate());
        let p = Payload {
            version: 2,
            ..payload(&alice, &bob, b"hi")
        };
        let r = open_as(&signed(&alice, &p), &alice, &bob, &mut HashSet::new()).unwrap_err();
        assert_eq!(r, rejected(RejectReason::BadVersion, p.msg_id));
    }

    #[test]
    fn check4_rejects_wrong_sender() {
        let (alice, bob, carol) = (
            Identity::generate(),
            Identity::generate(),
            Identity::generate(),
        );
        let wire = seal(&alice, &bob.peer_id(), Kind::Text, b"hi".to_vec()).unwrap();
        let r = open_as(&wire, &carol, &bob, &mut HashSet::new()).unwrap_err();
        assert_eq!(r.reason, RejectReason::WrongSender);
    }

    #[test]
    fn check4_rejects_garbage_sender_key() {
        let (alice, bob) = (Identity::generate(), Identity::generate());
        let p = Payload {
            sender_key: vec![1, 2, 3],
            ..payload(&alice, &bob, b"hi")
        };
        let r = open_as(&signed(&alice, &p), &alice, &bob, &mut HashSet::new()).unwrap_err();
        assert_eq!(r, rejected(RejectReason::Malformed, p.msg_id));
    }

    #[test]
    fn check4_rejects_rsa_sender() {
        // correctly signed and from the right peer, but identities must be Ed25519
        let (rsa, bob) = (rsa_keypair(), Identity::generate());
        let p = Payload {
            sender_key: rsa.public().encode_protobuf(),
            ..payload(&bob, &bob, b"hi")
        };
        let payload = to_cbor(&p).unwrap();
        let signature = rsa.sign(&signing_input(&payload)).unwrap();
        let wire = to_cbor(&Envelope { payload, signature }).unwrap();
        let r = open(
            &wire,
            &rsa.public().to_peer_id(),
            &bob.peer_id(),
            &mut HashSet::new(),
        )
        .unwrap_err();
        assert_eq!(r, rejected(RejectReason::Malformed, p.msg_id));
    }

    #[test]
    fn check5_rejects_message_for_someone_else() {
        // Alice's message to Carol, forwarded to Bob
        let (alice, bob, carol) = (
            Identity::generate(),
            Identity::generate(),
            Identity::generate(),
        );
        let wire = seal(&alice, &carol.peer_id(), Kind::Text, b"hi".to_vec()).unwrap();
        let r = open_as(&wire, &alice, &bob, &mut HashSet::new()).unwrap_err();
        assert_eq!(r.reason, RejectReason::WrongRecipient);
    }

    #[test]
    fn check5_rejects_garbage_recipient() {
        let (alice, bob) = (Identity::generate(), Identity::generate());
        let p = Payload {
            recipient: vec![1, 2, 3],
            ..payload(&alice, &bob, b"hi")
        };
        let r = open_as(&signed(&alice, &p), &alice, &bob, &mut HashSet::new()).unwrap_err();
        assert_eq!(r, rejected(RejectReason::WrongRecipient, p.msg_id));
    }

    #[test]
    fn check6_rejects_tampered_body() {
        let (alice, bob) = (Identity::generate(), Identity::generate());
        let wire = seal(&alice, &bob.peer_id(), Kind::Text, b"pay 10".to_vec()).unwrap();
        let (env, mut p) = decode_envelope(&wire);
        p.body = b"pay 99".to_vec();
        let forged = to_cbor(&Envelope {
            payload: to_cbor(&p).unwrap(),
            signature: env.signature,
        })
        .unwrap();
        let r = open_as(&forged, &alice, &bob, &mut HashSet::new()).unwrap_err();
        assert_eq!(r, rejected(RejectReason::BadSignature, p.msg_id));
    }

    #[test]
    fn check6_rejects_tampered_signature() {
        let (alice, bob) = (Identity::generate(), Identity::generate());
        let wire = seal(&alice, &bob.peer_id(), Kind::Text, b"hi".to_vec()).unwrap();
        let (mut env, _) = decode_envelope(&wire);
        env.signature[0] ^= 1;
        let r = open_as(&to_cbor(&env).unwrap(), &alice, &bob, &mut HashSet::new()).unwrap_err();
        assert_eq!(r.reason, RejectReason::BadSignature);
    }

    #[test]
    fn check6_rejects_signature_by_another_key() {
        // Carol signs a payload that claims to be from Alice
        let (alice, bob, carol) = (
            Identity::generate(),
            Identity::generate(),
            Identity::generate(),
        );
        let p = payload(&alice, &bob, b"hi");
        let r = open_as(&signed(&carol, &p), &alice, &bob, &mut HashSet::new()).unwrap_err();
        assert_eq!(r, rejected(RejectReason::BadSignature, p.msg_id));
    }

    #[test]
    fn check6_rejects_signature_without_domain_separator() {
        let (alice, bob) = (Identity::generate(), Identity::generate());
        let p = payload(&alice, &bob, b"hi");
        for domain in [&b""[..], b"p2pmsg-envelope-v2"] {
            let r = open_as(
                &sign_raw(&alice, &p, domain),
                &alice,
                &bob,
                &mut HashSet::new(),
            )
            .unwrap_err();
            assert_eq!(r, rejected(RejectReason::BadSignature, p.msg_id));
        }
    }

    #[test]
    fn check7_rejects_replay_with_its_msg_id() {
        let (alice, bob) = (Identity::generate(), Identity::generate());
        let mut seen = HashSet::new();
        let wire = seal(&alice, &bob.peer_id(), Kind::Text, b"hi".to_vec()).unwrap();
        let first = open_as(&wire, &alice, &bob, &mut seen).unwrap();
        let r = open_as(&wire, &alice, &bob, &mut seen).unwrap_err();
        assert_eq!(r, rejected(RejectReason::Replay, first.msg_id));
    }

    // Ordering and state

    #[test]
    fn rejected_message_does_not_mark_msg_id_as_seen() {
        // otherwise a forged copy could block the real message
        let (alice, bob) = (Identity::generate(), Identity::generate());
        let mut seen = HashSet::new();
        let wire = seal(&alice, &bob.peer_id(), Kind::Text, b"hi".to_vec()).unwrap();
        let (mut env, _) = decode_envelope(&wire);
        env.signature[0] ^= 1;
        assert!(open_as(&to_cbor(&env).unwrap(), &alice, &bob, &mut seen).is_err());
        assert!(open_as(&wire, &alice, &bob, &mut seen).is_ok());
    }

    #[test]
    fn same_msg_id_from_different_senders_is_not_a_replay() {
        let (alice, bob, carol) = (
            Identity::generate(),
            Identity::generate(),
            Identity::generate(),
        );
        let mut seen = HashSet::new();
        let from_alice = Payload {
            msg_id: [9; 16],
            ..payload(&alice, &bob, b"hi")
        };
        let from_carol = Payload {
            msg_id: [9; 16],
            ..payload(&carol, &bob, b"hi")
        };
        assert!(open_as(&signed(&alice, &from_alice), &alice, &bob, &mut seen).is_ok());
        assert!(open_as(&signed(&carol, &from_carol), &carol, &bob, &mut seen).is_ok());
    }

    #[test]
    fn checks_run_in_spec_order() {
        // wrong sender AND bad signature: check 4 must win over check 6
        let (alice, bob, carol) = (
            Identity::generate(),
            Identity::generate(),
            Identity::generate(),
        );
        let p = payload(&alice, &bob, b"hi");
        let r = open_as(&signed(&carol, &p), &carol, &bob, &mut HashSet::new()).unwrap_err();
        assert_eq!(r.reason, RejectReason::WrongSender);
    }

    // Properties

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn any_body_roundtrips(body in prop::collection::vec(any::<u8>(), 0..4096)) {
            let (alice, bob) = (Identity::generate(), Identity::generate());
            let wire = seal(&alice, &bob.peer_id(), Kind::Text, body.clone()).unwrap();
            prop_assert_eq!(open_as(&wire, &alice, &bob, &mut HashSet::new()).unwrap().body, body);
        }

        #[test]
        fn open_never_panics(wire in prop::collection::vec(any::<u8>(), 0..4096)) {
            let (alice, bob) = (Identity::generate(), Identity::generate());
            let _ = open_as(&wire, &alice, &bob, &mut HashSet::new());
        }

        #[test]
        fn any_single_byte_change_is_rejected(pos in any::<prop::sample::Index>(), mask in 1u8..=255) {
            let (alice, bob) = (Identity::generate(), Identity::generate());
            let mut wire = seal(&alice, &bob.peer_id(), Kind::Text, b"hello bob".to_vec()).unwrap();
            let i = pos.index(wire.len());
            wire[i] ^= mask;
            prop_assert!(open_as(&wire, &alice, &bob, &mut HashSet::new()).is_err());
        }
    }
}
