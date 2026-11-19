//! Contacts: who this node knows, and the rules for adding people.
//!
//! There are no accounts on a server. A user *is* their key pair, and their
//! `PeerId` is derived from the public key. A contact is another `PeerId` that
//! the user has agreed to talk to.
//!
//! Flow (spec section 4):
//!
//! ```text
//! Alice                                       Bob
//!   send_request(bob)        -> OutgoingPending
//!   ---- ContactRequest { name, note } ---->  receive_request(alice) -> IncomingPending
//!                                             respond(alice, true)   -> Trusted
//!   <--- ContactResponse { accepted } ------
//!   receive_response(bob)    -> Trusted
//! ```
//!
//! This module holds no networking. `ContactBook` is a state machine: each
//! method updates the book and returns what the network layer should do next.

mod book;
mod types;

pub use book::{ContactBook, Incoming, Outgoing, ResponseOutcome};
pub use types::{
    Access, Contact, ContactError, ContactRequest, ContactResponse, ContactState,
    MAX_NAME_BYTES, MAX_NOTE_BYTES, PROTOCOL_VERSION,
};
