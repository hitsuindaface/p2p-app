//! Property tests for the 1024-byte chat blob in `networking::protocol`.
//! Only the public API is used, the same way the rest of the app sees it.

use p2p_app::networking::protocol::{BLOB_SIZE, MAX_PAYLOAD, decode_blob, encode_blob};
use proptest::prelude::*;

proptest! {
    // T7: bytes from a peer must never crash the decoder, whatever their length.
    #[test]
    fn t7_decode_blob_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..4096)) {
        let _ = decode_blob(&bytes);
    }

    // T7: random input of the right length reaches the length check. A decoded
    // payload is never longer than MAX_PAYLOAD and is the length the header says.
    #[test]
    fn t7_decode_blob_of_blob_size_respects_the_length_header(
        bytes in prop::collection::vec(any::<u8>(), BLOB_SIZE)
    ) {
        let header = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
        match decode_blob(&bytes) {
            Ok(payload) => {
                prop_assert!(payload.len() <= MAX_PAYLOAD);
                prop_assert_eq!(payload.len(), header);
                prop_assert_eq!(payload, &bytes[2..2 + header]);
            }
            Err(_) => prop_assert!(header > MAX_PAYLOAD),
        }
    }

    // T7: the sender refuses anything over the size limit.
    #[test]
    fn t7_encode_blob_rejects_payloads_over_max_payload(
        payload in prop::collection::vec(any::<u8>(), MAX_PAYLOAD + 1..4096)
    ) {
        prop_assert!(encode_blob(&payload).is_err());
    }

    #[test]
    fn encode_then_decode_returns_the_payload(
        payload in prop::collection::vec(any::<u8>(), 0..=MAX_PAYLOAD)
    ) {
        let blob = encode_blob(&payload).unwrap();
        prop_assert_eq!(blob.len(), BLOB_SIZE);
        prop_assert_eq!(decode_blob(&blob).unwrap(), payload.as_slice());
    }
}
