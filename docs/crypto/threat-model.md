# Threat Model

**Owner:** Alexander Liberman · **Last updated:** 18 November 2026 · **Status:** Draft

## 1. Scope

This document covers the P2P messenger as planned for meeting 3: peer discovery on a local network (mDNS), one-to-one encrypted chat, encrypted file transfer, and encrypted local storage. Internet connectivity (DHT, hole punching, relays) is a stretch goal and is covered only briefly in Section 7. (Ask the group)

## 2. Assets

| Asset | Why it matters |
| --- | --- |
| Message content | Confidential conversations |
| Transferred files | May contain sensitive documents |
| Identity private key | Whoever holds it can impersonate the user |
| Contact list and pinned keys | Reveals the social graph; tampering enables impersonation |
| Local message history | Readable by anyone who obtains the device |
| Metadata (who talks to whom, when, IP addresses) | Reveals relationships and habits |

## 3. Trust assumptions

- The user's device and operating system are not compromised (no malware, no root access by an attacker).
- The cryptographic libraries we use (libp2p Noise, ed25519-dalek, ChaCha20-Poly1305, Argon2, SQLCipher) are correctly implemented.
- The operating system's random number generator is secure.
- Users compare safety numbers through a channel the attacker does not control (in person, or a trusted call).
- The user chooses a reasonably strong passphrase.

## 4. Adversaries

| Adversary | Capabilities | Goal |
| --- | --- | --- |
| Passive network observer | Sees all packets on the LAN or upstream network | Read messages and files; learn who talks to whom |
| Active network attacker | Same Wi-Fi; can intercept, modify, replay, and inject packets | MiM; impersonate a contact; tamper with messages |
| Device thief | Physical access to a powered-off or locked laptop | Read history, steal the identity key |
| Former contact | Has a legitimate past relationship | Keep messaging after being blocked; harass |

## Threats and mitigations

| # | Threat | Adversary | Mitigation | Status |
| --- | --- | --- | --- | --- |
| T1 | Eavesdropping on messages or files | Passive observer | All traffic runs over the Noise channel (ChaCha20-Poly1305) | Planned |
| T2 | MiM on first contact | Active attacker | Noise authenticates peer identity keys; users verify safety numbers or a QR code | Planned |
| T3 | Impersonating a known contact | Active attacker, malicious peer | The remote PeerId must match the pinned key; on a key change the app blocks the connection and warns the user | Planned |
| T4 | Message tampering | Active attacker | Noise provides integrity; envelopes are also signed with the sender's identity key | Planned |
| T5 | Replaying old messages | Active attacker | Unique `msg_id` per message; duplicates rejected | Planned |
| T6 | Unsolicited messages or files from strangers | Malicious peer | Unknown peers can only send a contact request; nothing else is accepted until it is approved | Planned |
| T7 | Malformed input crashes or exploits the client | Malicious peer | Rust memory safety; size limits on all messages; no `unwrap()` on network input; fuzzing the parser in CI | Planned |
| T8 | Corrupted oro swapped file chunks | Active attacker, malicious peer | Every chunk verified with BLAKE3 hash before it is written to disk | Planned |
| T9 | Reading history from a stolen device | Device thief | SQLCipher database with a key derived from the passphrase via Argon2id | Planned |
| T10 | Stealing the identity key from disk | Device thief | Key stored encrypted (OS keychain or passphrase-derived key); wiped from memory after use with `zeroize` | Planned |
| T11 | Keys or plaintext leaking into logs | Any | Never log secrets or message bodies; checked in code review | Planned |
| T12 | Weak passphrase brute-forced offline | Device thief | Argon2id with deliberately expensive parameters; minimum passphrase length | Planned |

## 6. Out of scope and accepted risks

- **Metadata and anonymity:** peers and network observers can see IP addresses, connection times and traffic volume. We provide confidentiality, not anonymity, and do not route traffic over Tor.
- **Compromised device:** malware or an attacker with access to an unlocked session can read everything.
- **Forward secrecy for stored history:** if the passphrase and database are both obtained, the whole history can be read.
- **Per-message forward secrecy:** Noisie gives forward secrecy per session, not per message. The Double Ratchet is a stretch goal.
- **Denial of service:** an attacker on the LAN can flood or jam the network; we do not defend against this.
- **Users skipping verification:** if users never compare safety numbers, a first-contact MiM remains possible (tryst on first use).

## 7. Stretch goal: internet mode

Joining a public DHT adds new adversaries: Sybil and eclipse attacks on the DHT and relays that see the connection metadata. Relays only ever forward Noise-encrypted bytes, so content stays confidential, but these threats will be analysed before internet mode is enabled.

## 8. How we test this

| Threat | Test |
| --- | --- |
| T1 | Wireshark capture during a session contains none of 100 canary strings |
| T2, T3 | A MiM peer with a substituted key is rejected or produces mismatched safety numbers |
| T4, T5 | Unit tests: tampered or replayed envelopes are rejected |
| T7 | `cargo-fuzz` on the envelope parser runs for 1 hour without crashes |
| T9 | `grep` for canary strings in the database file returns nothing |

## 9. Open questions

- Where is the identity key stored: OS keychain or a passphrase-encrypted file?
- Do we require a passphrase on every start-up, or cache it in the keychain?
- What happens on a key change: block until the user re-verifies, or warn and allow?
