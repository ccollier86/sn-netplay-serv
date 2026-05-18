//! Shared descriptor text validation helpers.
//!
//! These helpers enforce wire-safe IDs, titles, hashes, and short labels for
//! room descriptors. They do not know room state and they do not inspect local
//! filesystem paths beyond rejecting path-like descriptor values.

use crate::protocol::SessionDescriptorError;

const ID_MAX_LEN: usize = 96;
const TITLE_MAX_LEN: usize = 160;
const SHORT_TEXT_MAX_LEN: usize = 96;
const SHA256_HEX_LEN: usize = 64;

/// Validates a short protocol id or stable content key.
pub(crate) fn validate_id(field: &'static str, value: &str) -> Result<(), SessionDescriptorError> {
    validate_text(field, value, ID_MAX_LEN)?;

    if value.contains('/')
        || value.contains('\\')
        || value.contains("..")
        || looks_like_windows_drive_path(value)
    {
        return Err(SessionDescriptorError { field });
    }

    Ok(())
}

/// Validates an optional short protocol id.
pub(crate) fn validate_optional_id(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), SessionDescriptorError> {
    match value {
        Some(value) => validate_id(field, value),
        None => Ok(()),
    }
}

/// Validates a user-facing game title.
pub(crate) fn validate_title(value: &str) -> Result<(), SessionDescriptorError> {
    validate_text("title", value, TITLE_MAX_LEN)
}

/// Validates an optional short display string.
pub(crate) fn validate_optional_short_text(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), SessionDescriptorError> {
    match value {
        Some(value) => validate_text(field, value, SHORT_TEXT_MAX_LEN),
        None => Ok(()),
    }
}

/// Validates a lowercase or uppercase SHA-256 hex digest.
pub(crate) fn validate_sha256(
    field: &'static str,
    value: &str,
) -> Result<(), SessionDescriptorError> {
    if value.len() == SHA256_HEX_LEN && value.chars().all(|candidate| candidate.is_ascii_hexdigit())
    {
        Ok(())
    } else {
        Err(SessionDescriptorError { field })
    }
}

/// Validates an optional SHA-256 hex digest.
pub(crate) fn validate_optional_sha256(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), SessionDescriptorError> {
    match value {
        Some(value) => validate_sha256(field, value),
        None => Ok(()),
    }
}

fn validate_text(
    field: &'static str,
    value: &str,
    max_len: usize,
) -> Result<(), SessionDescriptorError> {
    if value.trim().is_empty()
        || value.len() > max_len
        || value != value.trim()
        || value.chars().any(char::is_control)
    {
        Err(SessionDescriptorError { field })
    } else {
        Ok(())
    }
}

fn looks_like_windows_drive_path(value: &str) -> bool {
    let bytes = value.as_bytes();

    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}
