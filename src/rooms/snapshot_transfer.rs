//! In-progress snapshot transfer accounting.
//!
//! The relay does not store complete snapshots, but it tracks chunk order,
//! total bytes, and a running checksum so malformed or abusive transfers are
//! rejected before they reach another Desktop client.

use crate::protocol::{SnapshotChunk, SnapshotLimits, SnapshotManifest};
use crate::rooms::RoomError;
use sha2::{Digest, Sha256};

/// Running validation state for one host snapshot transfer.
#[derive(Clone, Debug)]
pub struct SnapshotTransferState {
    next_chunk_index: u32,
    received_bytes: u64,
    hasher: Sha256,
}

impl SnapshotTransferState {
    /// Creates an empty transfer state.
    pub fn new() -> Self {
        Self {
            next_chunk_index: 0,
            received_bytes: 0,
            hasher: Sha256::new(),
        }
    }

    /// Validates and records one chunk.
    pub fn accept_chunk(
        &mut self,
        chunk: &SnapshotChunk,
        limits: SnapshotLimits,
    ) -> Result<(), RoomError> {
        chunk
            .validate(limits)
            .map_err(|_| RoomError::SnapshotInvalid)?;

        if chunk.index != self.next_chunk_index {
            return Err(RoomError::SnapshotInvalid);
        }

        let next_total = self
            .received_bytes
            .checked_add(chunk.bytes.len() as u64)
            .ok_or(RoomError::SnapshotInvalid)?;

        if next_total > limits.max_total_bytes {
            return Err(RoomError::SnapshotInvalid);
        }

        self.hasher.update(&chunk.bytes);
        self.received_bytes = next_total;
        self.next_chunk_index = self
            .next_chunk_index
            .checked_add(1)
            .ok_or(RoomError::SnapshotInvalid)?;

        Ok(())
    }

    /// Validates the completion manifest against bytes relayed so far.
    pub fn complete(
        &self,
        manifest: &SnapshotManifest,
        limits: SnapshotLimits,
    ) -> Result<(), RoomError> {
        manifest
            .validate(limits)
            .map_err(|_| RoomError::SnapshotInvalid)?;

        if manifest.total_bytes != self.received_bytes {
            return Err(RoomError::SnapshotInvalid);
        }

        let checksum = self.hasher.clone().finalize();
        let actual = format!("{checksum:x}");
        if actual != manifest.sha256 {
            return Err(RoomError::SnapshotInvalid);
        }

        Ok(())
    }
}

impl Default for SnapshotTransferState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::SnapshotTransferState;
    use crate::protocol::{SnapshotChunk, SnapshotLimits, SnapshotManifest};
    use sha2::{Digest, Sha256};

    #[test]
    fn accepts_ordered_chunks_with_matching_manifest() {
        let mut transfer = SnapshotTransferState::new();

        transfer
            .accept_chunk(
                &SnapshotChunk {
                    index: 0,
                    bytes: vec![1, 2],
                },
                SnapshotLimits::default(),
            )
            .expect("chunk 0");
        transfer
            .accept_chunk(
                &SnapshotChunk {
                    index: 1,
                    bytes: vec![3],
                },
                SnapshotLimits::default(),
            )
            .expect("chunk 1");

        assert!(
            transfer
                .complete(&manifest(&[1, 2, 3]), SnapshotLimits::default())
                .is_ok()
        );
    }

    #[test]
    fn rejects_out_of_order_chunks() {
        let mut transfer = SnapshotTransferState::new();

        assert!(
            transfer
                .accept_chunk(
                    &SnapshotChunk {
                        index: 1,
                        bytes: vec![1],
                    },
                    SnapshotLimits::default(),
                )
                .is_err()
        );
    }

    #[test]
    fn rejects_manifest_mismatch() {
        let mut transfer = SnapshotTransferState::new();
        transfer
            .accept_chunk(
                &SnapshotChunk {
                    index: 0,
                    bytes: vec![1],
                },
                SnapshotLimits::default(),
            )
            .expect("chunk");

        assert!(
            transfer
                .complete(&manifest(&[2]), SnapshotLimits::default())
                .is_err()
        );
    }

    fn manifest(bytes: &[u8]) -> SnapshotManifest {
        SnapshotManifest {
            total_bytes: bytes.len() as u64,
            sha256: format!("{:x}", Sha256::digest(bytes)),
        }
    }
}
