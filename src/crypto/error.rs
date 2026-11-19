use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("key derivation failed")]
    Kdf,
    #[error("invalid key encoding")]
    KeyDecode,
    #[error("signing failed")]
    Signing,
    #[error("message body too large")]
    BodyTooLarge,
    #[error("encoding failed")]
    Encode,
}

/// Which receive check failed (spec §6). Log this, never the content.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RejectReason {
    #[error("1: too large")]            TooLarge,
    #[error("2: malformed")]            Malformed,
    #[error("3: unsupported version")]  BadVersion,
    #[error("4: sender mismatch")]      WrongSender,
    #[error("5: not for us")]           WrongRecipient,
    #[error("6: bad signature")]        BadSignature,
    #[error("7: replay")]               Replay,
}