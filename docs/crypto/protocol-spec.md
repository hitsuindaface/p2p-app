# Protocol Specification

**Owner:** Alexander Liberman · **Version:** 0.1 (draft) · **Last updated:** 18 November 2026
Status: Describes the Meeting 3 scope (LAN only). Threat IDs (T1-T12) refer to `threat-model.md`.

## 1. Overview

Each user has a long-term Ed25519 identity key. Peers find each other on the LAN with mDNS, connect over TCP, and authenticate and encrypt the connection with Noise (via libp2p). On top of that channel, three application protocols carry contact requests, chat messages and file transfers. Every chat message is wrapped in a signed envelope.

| Layer | Choice |
| --- | --- |
| Identity | Ed25519 key pair; PeerId = libp2p identity derived from the public key |
| Discovery | mDNS (`libp2p-mdns`) |
| Transport | TCP |
| Secure channel | Noise XX (`/noise`), X255119 + ChaCha20-Poly1305 + SHA-256 |
| Multiplexing | Yamux |
| Application protocols | `/p2pmsg/contact/1.0.0`, `/p2pmsg/chat/1.0.0`, `p2pmsg/file/1.0.0` |
| Serialization | CBOR (`serde` + `ciborium`), canonical encoding |

QUIC is not used in v0.1 (libp2p QUIC uses TLS 1.3 instead of Noise, which would change the analysis). (TODO: Ask)

## 2. Identity

- Generated on first launch with the OS random number generator.
- The private key never leaves the device and is stored encrypted (Section 7).
- A contact is identified by their PeerId. A new key is therefore a new, unknown peer, never a silent update to an existing contact.

## 3. Connection gating (T3, T6)

After the Noise hanshake, the receiving node looks up the remote PeerId:

| Remote peer | Allowed protocols |
| --- | --- |
| Trusted contact | contact, chat, file |
| Unknown | contact only |
| Blocked | none; connection closed |

If an unknown peer's contact request uses the display name of an existing contact, the client shows a warning ("this may be someone pretending to be Alice").

## 4. Contact requests: `/p2pmsg/contact/1.0.0`

Request: `{ version: 1, display_name: string (≤ 64 bytes), note: string (≤ 256 bytes) }`
Response: `{ accepted: bool }`

The user must accept a request manually. Accepting stores the PeerId as a contact with `verified = false` until safety numbers are compared.

## 5. Safety numbers (T2)

For each party:
    `fp = SHA-512("p2pmsg-fingerprint-v1" || public_key_bytes)[0..30]`
Split `fp` into six 5-byte chunks; each chunk, read as a big-endian integer mod 100000, gives a 5-digit group, so each party has 30 digits. The safety number is the two 30-digit strings contatenated, the one from the lexicographically smaller public key first, so both sides display the same 60 digits. The QR code encodes `0x01 || fp_a || fp_b`.

Marking a contact verified sets `verified = true` locally.

## 6. Chat messages: `/p2pmsg/chat/1.0.0`

Request/response protocol: the sender sends one `Envelope`, the receiver replies with an `Ack`.

### Envelope

| Field | Type | Notes |
| --- | --- | --- |
| `version` | u8 | 1 |
| `msg_id` | 16 bytes | random |
| `sender` | PeerId bytes | |
| `recipient` | PeerId bytes | prevents a message being forwarded to someone else |
| `timestamp` | u64 | Unix milliseconds; for display only, not trusted |
| `kind` | enum | `Text`, `FileOffer`, `Receipt` |
| `body` | bytes | ≤ 64 KiB |
| `signature` | 64 bytes | Ed25519 |

Signature input: `"p2pmsg-envelope-v1" || CBOR(all fields except signature)`.

Signing is extra to Noise: it gives proof of authorship that survives storage and export (verifiable abuse reports) and prepares for later relayed or offline delivery.

### Receiver checks, in order (T4, T5, T7)

1. Raw message ≤ 72 KiB, otherwise drop before encoding
2. Decodes as a valid CBOR `Envelope`.
3. `version == 1`.
4. `sender` equals the authenticated remote PeerId of this connection.
5. `recipient` equals our own PeerId.
6. Signature verifies against `sender`'s public key.
7. `msg_id` not seen before from this sender.

Failure at any step: drop the message, log the step number (never the content), and reply `Ack { msg_id, status: Rejected }` if a `msg_id` could not be read. Success: store, then reply `Ack { msg_id, status: Delivered }`.

## 7. File transfer: `/p2pmsg/file/1.0.0` (T8)

1. The sender sends a `FileOffer` envelope over a chat protocol:
    `{ file_id: 16 bytes, name, size, chunksize 262144, file_hash: BLAKE3(file) }`.
2. The receiver accepts or declines in the UI. On accept, it requests chunks over the file protocol: `ChunkRequest { file_id, index }` -> `ChunkResponse { file_id, index, data, hash }`, with up to 8 requests in a flight.
3. The receiver checks `BLAKE3(data) == hash` for each chunk, writes it to a temporary file and records the index as complete.
4. When all chunks are present, the receiver checks `BLAKE3(file) == file_hash`; only then is the file moved to its destination.
5. Resume: after a disconnect, the receiver requests only the missing indices.

File names are sanitized (no path separators, no `..`) before writing to disk.

## 8. Local storage keys (T9, T10, T12)

    `db_key = Argon2id(passphrase, salt, m = 64 MiB, t = 3, p = 1, output = 32 bytes)`

- The salt (16 random bytes) and parameters are stored unencrypted in `config.json`.
- `db_key` is the raw SQLCipher key. The identity private key is stored inside the encrypted database.
- Keys are wiped from memory with `zeroize` when no longer needed.

## 9. Limits

| Item | Limit |
| --- | --- |
| Envelope body | 64 KiB |
| Raw message before decoding | 72 KiB |
| Display name / note | 64 / 256 bytes |
| File chunk | 256 KiB |
| Chunk requests in flight | 8 |
| Pending contact requests per peer | 1 |

## 10. Versioning

Protocol IDs carry a version (`/1.0.0`). A breaking change gets a new ID; both versions can run side by side during a transition. Envelopes with an unknown version are rejected.

## 11. Open questions

- Should `Receipt` messages be signed envelopes or plain acks?
- Do we need clock-skew limits on `timestamp`, or is display-only enough?
- Chunk hashes in v0.1 come from the sender at transfer time; should the offer include a full hash list (or use BLAKE3 verfied streaming via `bao`) instead?
