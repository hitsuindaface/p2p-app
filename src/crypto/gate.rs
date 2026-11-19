#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeerStatus { Trusted, Unknown, Blocked }

#[derive(Clone, Copy, Debug, ParitalEq, Eq)]
pub enum Protocol { Contact, Chat, File }

/// Spec §3. The node looks up the status in storage, then asks this.
pub fn is_allowed(status: PeerStatus, protocol: Protocol) -> bool {
    match status {
        PeerStatus::Trusted => true,
        PeerStatus::Unknown => protocol == Protocol::Contact,
        PeerStatus::Blocked => false,
    }
}