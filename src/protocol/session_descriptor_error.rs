//! Shared session descriptor validation errors.
//!
//! Descriptor validation modules return this compact field-level error. It is
//! safe to surface at the HTTP edge because it contains only stable field names
//! and never includes user secrets, paths, or ROM bytes.

/// Descriptor validation failure.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("invalid netplay session descriptor field {field}")]
pub struct SessionDescriptorError {
    /// Invalid field name in camelCase JSON form.
    pub field: &'static str,
}
