# Test strategy

**Owner:** testing and DevOps lead · **Last updated:** 27 November 2026

This page says who tests what, where each kind of test runs, when a pull request
is done, and which test proves each success criterion (C1-C4) and each threat
(T1-T12 in [crypto/threat-model.md](crypto/threat-model.md)).

## 1. Test layers

Every module owner writes unit tests for their own code. The testing lead writes
the tests that cross modules and the tests that run in the lab. The whole team
runs the trials on real machines.

| Layer | What it checks | Who writes it | Where it runs | How to run it |
| --- | --- | --- | --- | --- |
| Unit tests | One function or module, in a `#[cfg(test)] mod tests` inside the source file | Module owner | Every PR, on Windows, Linux and macOS | `cargo test` |
| Property tests | A rule that must hold for any input, with random inputs from `proptest` | Module owner, testing lead | Every PR | `cargo test` |
| Integration tests | Two real nodes in one test process, files in `tests/` | Testing lead | Every PR. Tests that need the network are `#[ignore]`d and run by hand | `cargo test` and `cargo test -- --ignored` |
| Lab tests | Fresh nodes in Docker containers on a private network, with `tc netem` to slow or break the link | Testing lead | Nightly on GitHub Actions, and by hand | `bash testlab/smoke.sh` (see [testlab/README.md](../testlab/README.md)) |
| Fuzzing | Random bytes into the parsers; the only rule is "no crash" | Testing lead, with Alexander | 60 s on every PR, 1 hour nightly (from phase 5, not set up yet) | `cargo +nightly fuzz run <target>` |
| Manual real-world tests | Three laptops (Windows, macOS, Linux) on one LAN, peers in two different homes, and the user test | Whole team | Planned dates: 5-7 Dec, 23 Dec-3 Jan, 4-8 Jan | A written protocol per test; results go in `docs/results-*.md` |

`cargo xtask check` runs format, lint and all tests that are not ignored. CI runs
the same command ([`.github/workflows/ci.yml`](../.github/workflows/ci.yml)).
The nightly lab is [`.github/workflows/lab.yml`](../.github/workflows/lab.yml).
Scheduled runs only use `main`, so the lab always tests reviewed code.

## 2. When a pull request is done

- New code has tests. A bug fix has a test that failed before the fix.
- `cargo xtask check` passes on your machine.
- CI is green on all three systems.
- One teammate has approved it.

## 3. Test names

A test that proves a success criterion or a threat starts with its ID in lower
case: `c1_two_nodes_find_each_other_and_chat`, `t7_decode_blob_never_panics`.
Then `cargo test c1_` runs everything for criterion 1, and a search for `t7_`
finds the evidence for threat T7. Older tests that match a row in the table
below by meaning keep their names; the table lists them.

## 4. How each criterion is read

A run is one try. A criterion passes when enough runs pass.

- **C1.** A run passes when both nodes start from empty data, each finds the
  other within 5 s, and the median send-to-acknowledgement time of 50 messages
  is under 200 ms. At least 19 of 20 runs must pass.
- **C2.** A run passes when a 1 GB file arrives with the same hash, at least half
  as fast as `iperf3` measured on the same link. 10 of 10 runs. Resume: kill the
  receiver at a random moment; the first chunk asked for after the restart is the
  one after the last verified chunk, and the final hash matches. 10 of 10 runs.
  The lab limits the link speed with `netem rate`; the report states which speed.
- **C3.** 0 of 100 canary strings found in the packet capture or the files on
  disk. The same test against a plain-text build must find 100 of 100. A
  man-in-the-middle with another key is detected in 20 of 20 tries. One hour of
  fuzzing without a crash.
- **C4.** Two peers in two different homes connect within 30 s in at least 8 of
  10 tries. Direct and relayed connections are counted separately. Five users
  each add a contact, verify it and send a file in under 3 minutes, and their
  average SUS score is 68 or higher.

## 5. Traceability

Status: **covered** means an automated test proves it today. **partly** means
some of it is tested. **missing** means no test yet. **manual** means only a
person can test it.

Note: the crypto module is tested on its own, but it is not wired into the chat
yet (the chat still sends the 1024-byte blob from `networking::protocol`). Rows
marked covered for T4 and T5 are covered at unit level only, until the envelope
is used by the node.

| ID | What proves it | Tests that exist today | Status |
| --- | --- | --- | --- |
| C1 | Lab script: 20 fresh container pairs, discovery time and median latency of 50 messages. Then three real laptops. | `two_nodes::c1_two_nodes_find_each_other_and_chat` (ignored until the LAN-only mode exists), `testlab/smoke.sh` (one run, nightly) | partly |
| C2 | Lab script: 10 transfers of 1 GB under `netem rate`, hash check, `iperf3` baseline, random kill and resume. | None. File transfer is not written yet. | missing |
| C3 | Leak test (C3 canaries, see T1, T9), man-in-the-middle container (T2, T3), 1 hour `cargo-fuzz` (T7). | See T1, T2, T3, T7, T9. Property tests stand in for fuzzing until `cargo-fuzz` is set up. | partly |
| C4 | Home-to-home trials (10 tries per pair, direct and relay counted apart), user test with SUS. | None. Lab NAT simulation is optional and is not evidence for C4. | manual |
| T1 | Packet capture during a session holds none of 100 canary strings. | None. | missing |
| T2 | MiM peer with a substituted key gives different safety numbers. Users compare safety numbers. | `safety_number::tests::symmetric`, `safety_number::tests::differs_for_different_peers`, `safety_number::tests::matches_reference_vector`, `safety_number::tests::rsa_key_is_an_error_not_a_panic`. No MiM test yet. | partly |
| T3 | Node refuses a known contact whose key changed. Envelope sender must match the connection's PeerId. | `envelope::tests::check4_rejects_wrong_sender`, `envelope::tests::check4_rejects_garbage_sender_key`, `envelope::tests::check4_rejects_rsa_sender`, `crypto::identity::tests::peer_id_matches_public_key`. Pinned-key check is not written yet. | partly |
| T4 | A changed envelope is rejected. | `envelope::tests::check6_rejects_tampered_body`, `envelope::tests::check6_rejects_tampered_signature`, `envelope::tests::check6_rejects_signature_by_another_key`, `envelope::tests::check6_rejects_signature_without_domain_separator`, `envelope::tests::any_single_byte_change_is_rejected` | covered (unit) |
| T5 | A replayed envelope is rejected. | `envelope::tests::check7_rejects_replay_with_its_msg_id`, `envelope::tests::rejected_message_does_not_mark_msg_id_as_seen`, `envelope::tests::same_msg_id_from_different_senders_is_not_a_replay`, `envelope::tests::msg_ids_are_unique` | covered (unit) |
| T6 | Unknown peers can only send contact requests. | `gate::tests::unknown_peers_can_only_send_contact_requests`, `gate::tests::blocked_peers_can_use_nothing`, `gate::tests::trusted_peers_can_use_everything`. The gate is not used by the node yet, so no end-to-end test. | partly |
| T7 | Malformed input is rejected without a crash; 1 hour of fuzzing. | `envelope::tests::check1_rejects_wire_over_limit`, `envelope::tests::check2_*` (3 tests), `envelope::tests::check3_rejects_wrong_version`, `envelope::tests::check5_*` (2 tests), `envelope::tests::checks_run_in_spec_order`, `envelope::tests::open_never_panics`, `protocol_props::t7_*` (3 tests), `networking::protocol::tests::blob_roundtrip`, `crypto::identity::tests::garbage_bytes_are_rejected`, `networking::identity::tests::decode`. No `cargo-fuzz` yet. | partly |
| T8 | A corrupted or swapped chunk is never written to disk. | None. File transfer is not written yet. | missing |
| T9 | Canary strings are not found in the database file. | `kdf::tests::matches_reference_vector` (the database key). The database is not written yet. | missing |
| T10 | The identity key is stored only inside the encrypted database and wiped after use. | `crypto::identity::tests::save_and_load_gives_same_identity`, `crypto::identity::tests::loaded_identity_can_still_sign`, `crypto::identity::tests::garbage_bytes_are_rejected`. Encrypted storage is not written yet. | partly |
| T11 | No secrets or message bodies in logs. Checked in code review; later the leak test also searches the node's log. | None. | manual |
| T12 | Argon2id with expensive parameters; minimum passphrase length. | `kdf::tests::matches_reference_vector` pins Argon2id with m = 64 MiB, t = 3, p = 1. No minimum length check yet. | partly |

Test paths are relative to the crate: `envelope::tests::...` is
`crypto::envelope::tests::...`, and `two_nodes::` and `protocol_props::` are
files in `tests/`. List every test with `cargo test -- --list`.
