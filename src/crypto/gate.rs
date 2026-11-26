#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeerStatus {
    Trusted,
    Unknown,
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    Contact,
    Chat,
    File,
}

/// Spec §3. The node looks up the status in storage, then asks this.
pub fn is_allowed(status: PeerStatus, protocol: Protocol) -> bool {
    match status {
        PeerStatus::Trusted => true,
        PeerStatus::Unknown => protocol == Protocol::Contact,
        PeerStatus::Blocked => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use PeerStatus::*;
    use Protocol::*;

    #[test]
    fn trusted_peers_can_use_everything() {
        assert!(is_allowed(Trusted, Contact));
        assert!(is_allowed(Trusted, Chat));
        assert!(is_allowed(Trusted, File));
    }

    #[test]
    fn unknown_peers_can_only_send_contact_requests() {
        // T6
        assert!(is_allowed(Unknown, Contact));
        assert!(!is_allowed(Unknown, Chat));
        assert!(!is_allowed(Unknown, File));
    }

    #[test]
    fn blocked_peers_can_use_nothing() {
        assert!(!is_allowed(Blocked, Contact));
        assert!(!is_allowed(Blocked, Chat));
        assert!(!is_allowed(Blocked, File));
    }
}
