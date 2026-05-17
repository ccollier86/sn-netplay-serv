//! Invite-code generation and normalization.
//!
//! Invite codes are short user-facing identifiers. The room registry stores a
//! normalized code so Desktop can accept pasted codes with hyphens or spaces.

use crate::rooms::RoomError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const INVITE_CODE_LEN: usize = 6;
const INVITE_ALPHABET: &[u8] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";

/// Normalized invite code for a room.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InviteCode(String);

impl InviteCode {
    /// Parses a user-entered invite code into its canonical uppercase form.
    pub fn parse(value: impl AsRef<str>) -> Result<Self, RoomError> {
        let normalized = value
            .as_ref()
            .chars()
            .filter(|character| !character.is_ascii_whitespace() && *character != '-')
            .map(|character| character.to_ascii_uppercase())
            .collect::<String>();

        if normalized.len() != INVITE_CODE_LEN
            || !normalized
                .bytes()
                .all(|byte| INVITE_ALPHABET.contains(&byte))
        {
            return Err(RoomError::InvalidInviteCode);
        }

        Ok(Self(normalized))
    }

    /// Returns the registry key form with no separators.
    pub fn normalized(&self) -> &str {
        &self.0
    }

    /// Returns the user-facing form with a hyphen.
    pub fn display(&self) -> String {
        format!("{}-{}", &self.0[..4], &self.0[4..])
    }
}

/// Generates invite codes for new rooms.
pub trait InviteCodeGenerator: Send + Sync {
    /// Returns a normalized invite code.
    fn generate(&self) -> InviteCode;
}

/// Invite-code generator backed by random UUID bytes.
#[derive(Default)]
pub struct UuidInviteCodeGenerator;

impl InviteCodeGenerator for UuidInviteCodeGenerator {
    fn generate(&self) -> InviteCode {
        let bytes = *Uuid::new_v4().as_bytes();
        let code = bytes
            .iter()
            .take(INVITE_CODE_LEN)
            .map(|byte| {
                let index = usize::from(*byte) % INVITE_ALPHABET.len();
                INVITE_ALPHABET[index] as char
            })
            .collect::<String>();

        InviteCode(code)
    }
}

#[cfg(test)]
mod tests {
    use super::{INVITE_CODE_LEN, InviteCode, InviteCodeGenerator, UuidInviteCodeGenerator};

    #[test]
    fn parse_accepts_hyphenated_lowercase_codes() {
        let code = InviteCode::parse("ab23-cd").expect("code");

        assert_eq!(code.normalized(), "AB23CD");
        assert_eq!(code.display(), "AB23-CD");
    }

    #[test]
    fn parse_rejects_wrong_length_codes() {
        assert!(InviteCode::parse("ABC").is_err());
    }

    #[test]
    fn generator_returns_normalized_codes() {
        let generator = UuidInviteCodeGenerator;
        let code = generator.generate();

        assert_eq!(code.normalized().len(), INVITE_CODE_LEN);
        assert!(!code.normalized().contains('-'));
    }
}
