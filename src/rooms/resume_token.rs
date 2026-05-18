//! Resume token generation and storage helpers.
//!
//! Tokens are sent to clients only once on successful join. The room stores a
//! hash so internal room snapshots and debug endpoints never expose a reusable
//! reconnect credential.

use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Opaque token returned to a client for reconnecting to the same player slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResumeToken(String);

impl ResumeToken {
    /// Wraps a generated token string.
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// Returns the token value that should be sent to the client once.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Returns a one-way hash suitable for room storage and comparison.
    pub fn hash(&self) -> ResumeTokenHash {
        hash_resume_token(&self.0)
    }
}

/// Hashed resume token stored in a room slot.
pub type ResumeTokenHash = String;

/// Generates opaque resume tokens.
pub trait ResumeTokenGenerator: Send + Sync {
    /// Creates a token with enough entropy for live reconnect authorization.
    fn generate(&self) -> ResumeToken;
}

/// UUID-backed generator used by the relay process.
#[derive(Default)]
pub struct UuidResumeTokenGenerator;

impl ResumeTokenGenerator for UuidResumeTokenGenerator {
    fn generate(&self) -> ResumeToken {
        ResumeToken::new(format!(
            "{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        ))
    }
}

/// Hashes a token supplied by a reconnecting client.
pub fn hash_resume_token(value: &str) -> ResumeTokenHash {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
