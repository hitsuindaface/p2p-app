use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Version carried in every `ContactRequest`.
pub const PROTOCOL_VERSION: u8 = 1;
/// Spec section 9.
pub const MAX_NAME_BYTES: usize = 64;
/// Spec section 9.
pub const MAX_NOTE_BYTES: usize = 256;

/// One entry in the contact book.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    #[serde(with = "peer_id_str")]
    pub peer_id: PeerId,
    /// The name the peer gave us. `None` until they tell us (for example while
    /// our own request is still pending). It is a label, not proof of identity.
    pub display_name: Option<String>,
    /// The note from their contact request. Only kept while the request is pending.
    pub note: Option<String>,
    pub state: ContactState,
    /// True after the user compared safety numbers and marked the contact verified.
    pub verified: bool,
    /// Unix milliseconds when the entry was created.
    pub added_at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactState {
    /// We sent a request and wait for their answer.
    OutgoingPending,
    /// They sent a request and wait for the user to answer.
    IncomingPending,
    /// Both sides agreed. Chat and files are allowed.
    Trusted,
    /// Everything from this peer is refused and the connection is closed.
    Blocked,
}

/// What a peer may do on a connection (spec section 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    /// Contact, chat and file protocols.
    Full,
    /// Contact requests only.
    ContactOnly,
    /// Nothing. Close the connection.
    Denied,
}

/// Sent on `/p2pmsg/contact/1.0.0`.
///
/// It deliberately has no public key field: the peer's key comes from the
/// Noise-authenticated connection, not from what the peer claims.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactRequest {
    pub version: u8,
    pub display_name: String,
    pub note: String,
}

/// Answer to a `ContactRequest`.
///
/// `display_name` is not in spec v0.1. Without it, the person who sent the
/// request never learns the other side's name. Agree on this with Alexander.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactResponse {
    pub accepted: bool,
    pub display_name: Option<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ContactError {
    #[error("display name is empty")]
    EmptyName,
    #[error("display name is longer than 64 bytes")]
    NameTooLong,
    #[error("note is longer than 256 bytes")]
    NoteTooLong,
    #[error("text contains control characters")]
    ControlCharacters,
    #[error("unsupported contact protocol version {0}")]
    UnsupportedVersion(u8),
    #[error("a contact request with this peer is already pending")]
    AlreadyPending,
    #[error("this peer is already a contact")]
    AlreadyContact,
    #[error("this peer is blocked")]
    Blocked,
    #[error("there is no contact request from this peer to answer")]
    NoIncomingRequest,
    #[error("no contact request was sent to this peer")]
    NoOutgoingRequest,
    #[error("this peer is not a contact")]
    NotAContact,
}

impl ContactRequest {
    /// Builds a request to send, checking the limits first.
    pub fn new(display_name: &str, note: &str) -> Result<Self, ContactError> {
        let req = ContactRequest {
            version: PROTOCOL_VERSION,
            display_name: display_name.trim().to_owned(),
            note: note.trim().to_owned(),
        };
        req.validate()?;
        Ok(req)
    }

    /// Checks a request received from the network.
    pub fn validate(&self) -> Result<(), ContactError> {
        if self.version != PROTOCOL_VERSION {
            return Err(ContactError::UnsupportedVersion(self.version));
        }
        validate_name(&self.display_name)?;
        validate_note(&self.note)
    }
}

pub(crate) fn validate_name(name: &str) -> Result<(), ContactError> {
    if name.trim().is_empty() {
        return Err(ContactError::EmptyName);
    }
    if name.len() > MAX_NAME_BYTES {
        return Err(ContactError::NameTooLong);
    }
    // A name with a newline or other control character could fake extra lines in the UI.
    if name.chars().any(char::is_control) {
        return Err(ContactError::ControlCharacters);
    }
    Ok(())
}

fn validate_note(note: &str) -> Result<(), ContactError> {
    if note.len() > MAX_NOTE_BYTES {
        return Err(ContactError::NoteTooLong);
    }
    if note.chars().any(|c| c.is_control() && c != '\n') {
        return Err(ContactError::ControlCharacters);
    }
    Ok(())
}

/// Serializes a `PeerId` as its base58 string, e.g. `12D3KooW...`.
/// Readable in JSON for the UI and the database, and needs no extra libp2p feature.
mod peer_id_str {
    use libp2p::PeerId;
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    pub fn serialize<S: Serializer>(peer: &PeerId, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(peer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<PeerId, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(D::Error::custom)
    }
}
