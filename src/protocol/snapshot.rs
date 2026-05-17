//! Save-state snapshot validation helpers.
//!
//! Snapshot bytes are untrusted relay payloads. This module validates declared
//! sizes and checksums but does not persist data.

use crate::limits::{MAX_SNAPSHOT_BYTES, MAX_SNAPSHOT_CHUNK_BYTES};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Snapshot size limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotLimits {
    /// Maximum bytes accepted for one chunk.
    pub max_chunk_bytes: usize,
    /// Maximum bytes accepted for the whole snapshot.
    pub max_total_bytes: u64,
}

impl Default for SnapshotLimits {
    fn default() -> Self {
        Self {
            max_chunk_bytes: MAX_SNAPSHOT_CHUNK_BYTES,
            max_total_bytes: MAX_SNAPSHOT_BYTES,
        }
    }
}

/// Metadata for one complete sync snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotManifest {
    /// Total snapshot byte count.
    pub total_bytes: u64,
    /// Lowercase hexadecimal SHA-256 checksum.
    pub sha256: String,
}

impl SnapshotManifest {
    /// Validates declared snapshot metadata before bytes are relayed.
    pub fn validate(&self, limits: SnapshotLimits) -> Result<(), SnapshotError> {
        if self.total_bytes > limits.max_total_bytes {
            return Err(SnapshotError::TotalTooLarge);
        }

        if !is_sha256_hex(&self.sha256) {
            return Err(SnapshotError::InvalidChecksum);
        }

        Ok(())
    }

    /// Validates a completed snapshot byte buffer against size and checksum.
    pub fn validate_bytes(
        &self,
        bytes: &[u8],
        limits: SnapshotLimits,
    ) -> Result<(), SnapshotError> {
        self.validate(limits)?;

        if bytes.len() as u64 != self.total_bytes {
            return Err(SnapshotError::SizeMismatch);
        }

        let checksum = Sha256::digest(bytes);
        let actual = format!("{checksum:x}");
        if actual != self.sha256 {
            return Err(SnapshotError::ChecksumMismatch);
        }

        Ok(())
    }
}

/// One chunk of a save-state sync snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotChunk {
    /// Zero-based chunk index.
    pub index: u32,
    /// Chunk bytes.
    pub bytes: Vec<u8>,
}

impl SnapshotChunk {
    /// Validates the chunk size against relay limits.
    pub fn validate(&self, limits: SnapshotLimits) -> Result<(), SnapshotError> {
        if self.bytes.len() > limits.max_chunk_bytes {
            Err(SnapshotError::ChunkTooLarge)
        } else {
            Ok(())
        }
    }
}

/// Snapshot validation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum SnapshotError {
    /// One chunk exceeded the configured max chunk size.
    #[error("snapshot chunk is too large")]
    ChunkTooLarge,
    /// Snapshot manifest declared too many total bytes.
    #[error("snapshot total size is too large")]
    TotalTooLarge,
    /// Completed byte count does not match the manifest.
    #[error("snapshot size does not match manifest")]
    SizeMismatch,
    /// Completed checksum does not match the manifest.
    #[error("snapshot checksum does not match manifest")]
    ChecksumMismatch,
    /// Manifest checksum is not a hexadecimal SHA-256 digest.
    #[error("snapshot checksum format is invalid")]
    InvalidChecksum,
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::{SnapshotChunk, SnapshotError, SnapshotLimits, SnapshotManifest};

    #[test]
    fn chunk_rejects_large_payloads() {
        let chunk = SnapshotChunk {
            index: 0,
            bytes: vec![0; 3],
        };
        let limits = SnapshotLimits {
            max_chunk_bytes: 2,
            max_total_bytes: 10,
        };

        assert_eq!(chunk.validate(limits), Err(SnapshotError::ChunkTooLarge));
    }

    #[test]
    fn manifest_rejects_checksum_mismatch() {
        let manifest = SnapshotManifest {
            total_bytes: 3,
            sha256: "0".repeat(64),
        };
        let limits = SnapshotLimits {
            max_chunk_bytes: 10,
            max_total_bytes: 10,
        };

        assert_eq!(
            manifest.validate_bytes(&[1, 2, 3], limits),
            Err(SnapshotError::ChecksumMismatch)
        );
    }

    #[test]
    fn manifest_rejects_invalid_checksum_format() {
        let manifest = SnapshotManifest {
            total_bytes: 3,
            sha256: "bad".to_string(),
        };

        assert_eq!(
            manifest.validate(SnapshotLimits::default()),
            Err(SnapshotError::InvalidChecksum)
        );
    }
}
