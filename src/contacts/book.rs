use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use libp2p::PeerId;

use super::types::{
    Access, Contact, ContactError, ContactRequest, ContactResponse, ContactState, validate_name,
};

/// What to do after the user asks to add someone.
#[derive(Debug, PartialEq, Eq)]
pub enum Outgoing {
    /// Send a `ContactRequest` to the peer.
    SendRequest,
    /// They had already asked us, so this counts as accepting. Send
    /// `ContactResponse { accepted: true }` instead of a new request.
    AcceptTheirs,
}

/// What to do after a `ContactRequest` arrives.
#[derive(Debug, PartialEq, Eq)]
pub enum Incoming {
    /// New request. Show it to the user and wait for `respond`.
    /// `name_collision` is true when a trusted contact already uses this name,
    /// so the UI can warn that someone may be pretending to be them.
    AskUser { name_collision: bool },
    /// We had sent them a request too. Both are now trusted; reply accepted.
    BothAsked,
    /// They are already a contact (for example our earlier answer was lost).
    /// Reply accepted again; nothing changes.
    AlreadyContact,
}

/// What happened after a `ContactResponse` arrived.
#[derive(Debug, PartialEq, Eq)]
pub enum ResponseOutcome {
    Accepted,
    Declined,
}

/// All contacts of this node, in memory. Moves into the encrypted database later.
#[derive(Debug, Default)]
pub struct ContactBook {
    contacts: HashMap<PeerId, Contact>,
}

impl ContactBook {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, peer: &PeerId) -> Option<&Contact> {
        self.contacts.get(peer)
    }

    /// Every entry, sorted by name so the UI gets a stable order.
    pub fn list(&self) -> Vec<Contact> {
        let mut all: Vec<Contact> = self.contacts.values().cloned().collect();
        all.sort_by(|a, b| {
            a.display_name
                .cmp(&b.display_name)
                .then_with(|| a.peer_id.to_bytes().cmp(&b.peer_id.to_bytes()))
        });
        all
    }

    /// Requests from other people that wait for the user's answer.
    pub fn pending_incoming(&self) -> Vec<Contact> {
        self.list()
            .into_iter()
            .filter(|c| c.state == ContactState::IncomingPending)
            .collect()
    }

    /// The connection rule from spec section 3. The network layer asks this
    /// when a connection opens and before dispatching every incoming message.
    pub fn access(&self, peer: &PeerId) -> Access {
        match self.contacts.get(peer).map(|c| c.state) {
            Some(ContactState::Trusted) => Access::Full,
            Some(ContactState::Blocked) => Access::Denied,
            Some(ContactState::OutgoingPending | ContactState::IncomingPending) | None => {
                Access::ContactOnly
            }
        }
    }

    /// The user wants to add `peer`. The caller builds and sends the
    /// `ContactRequest` (with `ContactRequest::new`) if this returns `SendRequest`.
    pub fn send_request(&mut self, peer: PeerId) -> Result<Outgoing, ContactError> {
        match self.contacts.get_mut(&peer) {
            None => {
                self.contacts.insert(peer, new_entry(peer, ContactState::OutgoingPending));
                Ok(Outgoing::SendRequest)
            }
            Some(c) => match c.state {
                ContactState::OutgoingPending => Err(ContactError::AlreadyPending),
                ContactState::Trusted => Err(ContactError::AlreadyContact),
                ContactState::Blocked => Err(ContactError::Blocked),
                ContactState::IncomingPending => {
                    c.state = ContactState::Trusted;
                    c.note = None;
                    Ok(Outgoing::AcceptTheirs)
                }
            },
        }
    }

    /// A `ContactRequest` arrived from `peer`, the Noise-authenticated sender.
    ///
    /// On `Err`, the caller sends no response. In particular a second request
    /// while one is pending is ignored (spec section 9: one pending request per
    /// peer), and a blocked peer is disconnected.
    pub fn receive_request(
        &mut self,
        peer: PeerId,
        req: ContactRequest,
    ) -> Result<Incoming, ContactError> {
        req.validate()?;

        let state = self.contacts.get(&peer).map(|c| c.state);
        match state {
            Some(ContactState::Blocked) => Err(ContactError::Blocked),
            Some(ContactState::IncomingPending) => Err(ContactError::AlreadyPending),
            Some(ContactState::Trusted) => Ok(Incoming::AlreadyContact),
            Some(ContactState::OutgoingPending) => {
                let c = self.contacts.get_mut(&peer).expect("state was read above");
                c.state = ContactState::Trusted;
                c.display_name = Some(req.display_name);
                Ok(Incoming::BothAsked)
            }
            None => {
                let name_collision = self.name_in_use(&req.display_name, &peer);
                let mut c = new_entry(peer, ContactState::IncomingPending);
                c.display_name = Some(req.display_name);
                c.note = (!req.note.is_empty()).then_some(req.note);
                self.contacts.insert(peer, c);
                Ok(Incoming::AskUser { name_collision })
            }
        }
    }

    /// The user answered a pending request from `peer`. On success the caller
    /// sends `ContactResponse { accepted, display_name }`.
    pub fn respond(&mut self, peer: PeerId, accept: bool) -> Result<(), ContactError> {
        match self.contacts.get_mut(&peer) {
            Some(c) if c.state == ContactState::IncomingPending => {
                if accept {
                    c.state = ContactState::Trusted;
                    c.note = None;
                } else {
                    self.contacts.remove(&peer);
                }
                Ok(())
            }
            _ => Err(ContactError::NoIncomingRequest),
        }
    }

    /// A `ContactResponse` arrived from `peer` for a request we sent.
    pub fn receive_response(
        &mut self,
        peer: PeerId,
        resp: ContactResponse,
    ) -> Result<ResponseOutcome, ContactError> {
        let Some(c) = self.contacts.get_mut(&peer) else {
            return Err(ContactError::NoOutgoingRequest);
        };
        if c.state != ContactState::OutgoingPending {
            return Err(ContactError::NoOutgoingRequest);
        }
        if !resp.accepted {
            self.contacts.remove(&peer);
            return Ok(ResponseOutcome::Declined);
        }
        if let Some(name) = &resp.display_name {
            validate_name(name)?;
        }
        c.state = ContactState::Trusted;
        c.display_name = resp.display_name;
        Ok(ResponseOutcome::Accepted)
    }

    /// Blocks `peer`, whether or not it was known. The caller disconnects it.
    pub fn block(&mut self, peer: PeerId) {
        let c = self
            .contacts
            .entry(peer)
            .or_insert_with(|| new_entry(peer, ContactState::Blocked));
        c.state = ContactState::Blocked;
        c.note = None;
        c.verified = false;
    }

    /// Forgets `peer` entirely, including a block.
    pub fn remove(&mut self, peer: &PeerId) -> Option<Contact> {
        self.contacts.remove(peer)
    }

    /// The user compared safety numbers with a trusted contact and they matched.
    pub fn mark_verified(&mut self, peer: &PeerId) -> Result<(), ContactError> {
        match self.contacts.get_mut(peer) {
            Some(c) if c.state == ContactState::Trusted => {
                c.verified = true;
                Ok(())
            }
            _ => Err(ContactError::NotAContact),
        }
    }

    /// True if a trusted contact other than `except` already uses `name`.
    fn name_in_use(&self, name: &str, except: &PeerId) -> bool {
        let wanted = normalize(name);
        self.contacts.values().any(|c| {
            c.peer_id != *except
                && c.state == ContactState::Trusted
                && c.display_name.as_deref().is_some_and(|n| normalize(n) == wanted)
        })
    }
}

fn new_entry(peer: PeerId, state: ContactState) -> Contact {
    Contact {
        peer_id: peer,
        display_name: None,
        note: None,
        state,
        verified: false,
        added_at: now_ms(),
    }
}

fn normalize(name: &str) -> String {
    name.trim().to_lowercase()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contacts::types::{MAX_NAME_BYTES, MAX_NOTE_BYTES};
    use libp2p::identity::Keypair;

    /// A fixed PeerId per number, from a real Ed25519 key.
    fn peer(n: u8) -> PeerId {
        Keypair::ed25519_from_bytes([n; 32])
            .expect("any 32 bytes are a valid Ed25519 secret key")
            .public()
            .to_peer_id()
    }

    fn request(name: &str) -> ContactRequest {
        ContactRequest::new(name, "hej").unwrap()
    }

    fn accept(name: &str) -> ContactResponse {
        ContactResponse { accepted: true, display_name: Some(name.into()) }
    }

    fn decline() -> ContactResponse {
        ContactResponse { accepted: false, display_name: None }
    }

    #[test]
    fn request_then_accept_makes_both_trusted() {
        let (alice, bob) = (peer(1), peer(2));
        let (mut alice_book, mut bob_book) = (ContactBook::new(), ContactBook::new());

        assert_eq!(alice_book.send_request(bob), Ok(Outgoing::SendRequest));
        assert_eq!(alice_book.access(&bob), Access::ContactOnly);

        let got = bob_book.receive_request(alice, request("Alice"));
        assert_eq!(got, Ok(Incoming::AskUser { name_collision: false }));
        assert_eq!(bob_book.pending_incoming().len(), 1);
        assert_eq!(bob_book.get(&alice).unwrap().note.as_deref(), Some("hej"));

        bob_book.respond(alice, true).unwrap();
        assert_eq!(bob_book.access(&alice), Access::Full);
        assert_eq!(bob_book.get(&alice).unwrap().note, None);

        let got = alice_book.receive_response(bob, accept("Bob"));
        assert_eq!(got, Ok(ResponseOutcome::Accepted));
        assert_eq!(alice_book.access(&bob), Access::Full);
        assert_eq!(alice_book.get(&bob).unwrap().display_name.as_deref(), Some("Bob"));
        assert!(!alice_book.get(&bob).unwrap().verified);
    }

    #[test]
    fn decline_removes_on_both_sides() {
        let (alice, bob) = (peer(1), peer(2));
        let (mut alice_book, mut bob_book) = (ContactBook::new(), ContactBook::new());

        alice_book.send_request(bob).unwrap();
        bob_book.receive_request(alice, request("Alice")).unwrap();
        bob_book.respond(alice, false).unwrap();
        assert!(bob_book.get(&alice).is_none());

        assert_eq!(alice_book.receive_response(bob, decline()), Ok(ResponseOutcome::Declined));
        assert!(alice_book.get(&bob).is_none());
        // Alice may ask again later.
        assert_eq!(alice_book.send_request(bob), Ok(Outgoing::SendRequest));
    }

    #[test]
    fn only_one_pending_request_per_peer() {
        let mut book = ContactBook::new();
        book.receive_request(peer(1), request("Alice")).unwrap();
        assert_eq!(
            book.receive_request(peer(1), request("Alice again")),
            Err(ContactError::AlreadyPending)
        );
        // The first request is unchanged.
        assert_eq!(book.get(&peer(1)).unwrap().display_name.as_deref(), Some("Alice"));
    }

    #[test]
    fn sending_twice_is_refused() {
        let mut book = ContactBook::new();
        book.send_request(peer(2)).unwrap();
        assert_eq!(book.send_request(peer(2)), Err(ContactError::AlreadyPending));
    }

    #[test]
    fn both_ask_at_once() {
        let mut book = ContactBook::new();
        book.send_request(peer(2)).unwrap();
        assert_eq!(book.receive_request(peer(2), request("Bob")), Ok(Incoming::BothAsked));
        assert_eq!(book.access(&peer(2)), Access::Full);
        assert_eq!(book.get(&peer(2)).unwrap().display_name.as_deref(), Some("Bob"));
    }

    #[test]
    fn adding_someone_who_already_asked_accepts_them() {
        let mut book = ContactBook::new();
        book.receive_request(peer(2), request("Bob")).unwrap();
        assert_eq!(book.send_request(peer(2)), Ok(Outgoing::AcceptTheirs));
        assert_eq!(book.access(&peer(2)), Access::Full);
    }

    #[test]
    fn repeated_request_from_contact_is_answered_again() {
        let mut book = ContactBook::new();
        book.receive_request(peer(1), request("Alice")).unwrap();
        book.respond(peer(1), true).unwrap();
        assert_eq!(book.receive_request(peer(1), request("Alice")), Ok(Incoming::AlreadyContact));
    }

    #[test]
    fn blocked_peer_is_denied_everything() {
        let mut book = ContactBook::new();
        book.block(peer(3));
        assert_eq!(book.access(&peer(3)), Access::Denied);
        assert_eq!(book.receive_request(peer(3), request("Eve")), Err(ContactError::Blocked));
        assert_eq!(book.send_request(peer(3)), Err(ContactError::Blocked));

        book.remove(&peer(3));
        assert_eq!(book.access(&peer(3)), Access::ContactOnly);
    }

    #[test]
    fn blocking_a_contact_clears_verified() {
        let mut book = ContactBook::new();
        book.receive_request(peer(1), request("Alice")).unwrap();
        book.respond(peer(1), true).unwrap();
        book.mark_verified(&peer(1)).unwrap();
        book.block(peer(1));
        assert!(!book.get(&peer(1)).unwrap().verified);
        assert_eq!(book.access(&peer(1)), Access::Denied);
    }

    #[test]
    fn same_name_as_a_contact_is_flagged() {
        let mut book = ContactBook::new();
        book.receive_request(peer(1), request("Alice")).unwrap();
        book.respond(peer(1), true).unwrap();

        let got = book.receive_request(peer(3), request("  alice "));
        assert_eq!(got, Ok(Incoming::AskUser { name_collision: true }));
    }

    #[test]
    fn same_name_as_a_pending_request_is_not_flagged() {
        let mut book = ContactBook::new();
        book.receive_request(peer(1), request("Alice")).unwrap();
        let got = book.receive_request(peer(3), request("Alice"));
        assert_eq!(got, Ok(Incoming::AskUser { name_collision: false }));
    }

    #[test]
    fn limits_are_enforced() {
        let long_name = "a".repeat(MAX_NAME_BYTES + 1);
        let long_note = "a".repeat(MAX_NOTE_BYTES + 1);
        assert_eq!(ContactRequest::new(&long_name, ""), Err(ContactError::NameTooLong));
        assert_eq!(ContactRequest::new("Alice", &long_note), Err(ContactError::NoteTooLong));
        assert_eq!(ContactRequest::new("   ", ""), Err(ContactError::EmptyName));
        assert_eq!(ContactRequest::new("Al\nice", ""), Err(ContactError::ControlCharacters));
        assert!(ContactRequest::new(&"a".repeat(MAX_NAME_BYTES), "line 1\nline 2").is_ok());
        // Bytes, not characters: 33 two-byte letters are 66 bytes.
        assert_eq!(ContactRequest::new(&"å".repeat(33), ""), Err(ContactError::NameTooLong));
    }

    #[test]
    fn invalid_request_from_network_is_rejected_and_not_stored() {
        let mut book = ContactBook::new();
        let raw = ContactRequest { version: 1, display_name: "x".repeat(65), note: String::new() };
        assert_eq!(book.receive_request(peer(1), raw), Err(ContactError::NameTooLong));

        let raw = ContactRequest { version: 2, display_name: "Alice".into(), note: String::new() };
        assert_eq!(book.receive_request(peer(1), raw), Err(ContactError::UnsupportedVersion(2)));
        assert!(book.get(&peer(1)).is_none());
    }

    #[test]
    fn response_without_request_is_rejected() {
        let mut book = ContactBook::new();
        assert_eq!(
            book.receive_response(peer(2), accept("Bob")),
            Err(ContactError::NoOutgoingRequest)
        );
        assert!(book.get(&peer(2)).is_none());
    }

    #[test]
    fn answering_without_request_is_refused() {
        let mut book = ContactBook::new();
        assert_eq!(book.respond(peer(1), true), Err(ContactError::NoIncomingRequest));
    }

    #[test]
    fn only_trusted_contacts_can_be_verified() {
        let mut book = ContactBook::new();
        book.send_request(peer(2)).unwrap();
        assert_eq!(book.mark_verified(&peer(2)), Err(ContactError::NotAContact));
        assert_eq!(book.mark_verified(&peer(9)), Err(ContactError::NotAContact));
    }

    #[test]
    fn contact_round_trips_through_json() {
        let mut book = ContactBook::new();
        book.receive_request(peer(1), request("Alice")).unwrap();
        let contact = book.get(&peer(1)).unwrap().clone();

        let json = serde_json::to_string(&contact).unwrap();
        assert!(json.contains(&peer(1).to_string()), "PeerId is stored as a readable string");
        assert!(json.contains("\"incoming_pending\""));
        let back: Contact = serde_json::from_str(&json).unwrap();
        assert_eq!(back, contact);
    }
}
