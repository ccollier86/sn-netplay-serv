//! Server-side fast-input release buffer.
//!
//! Accepted `SBI2` records wait here until the authoritative relay frame reaches
//! them. The buffer preserves each record's encoded bytes for zero-copy fanout.

use crate::protocol::FastInputFrame;
use crate::rooms::ConnectionId;
use std::collections::BTreeMap;

/// Ready-to-emit fast-input record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BufferedFastInputFrame {
    /// Connection that supplied the record.
    pub source: ConnectionId,
    /// Validated fast-input record.
    pub frame: FastInputFrame,
}

/// Holds accepted future fast-input records until canonical-frame release.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct FastInputRelayBuffer {
    frames_by_number: BTreeMap<u64, Vec<BufferedFastInputFrame>>,
}

impl FastInputRelayBuffer {
    /// Stores one accepted record for later canonical-frame release.
    pub fn push(&mut self, source: ConnectionId, frame: FastInputFrame) {
        self.frames_by_number
            .entry(frame.frame)
            .or_default()
            .push(BufferedFastInputFrame { source, frame });
    }

    /// Drains exactly one released frame for the active epochs.
    pub fn drain_frame(
        &mut self,
        frame: u64,
        room_epoch: u64,
        session_epoch: u64,
    ) -> Vec<BufferedFastInputFrame> {
        self.frames_by_number
            .remove(&frame)
            .unwrap_or_default()
            .into_iter()
            .filter(|buffered| {
                buffered.frame.room_epoch == room_epoch
                    && buffered.frame.session_epoch == session_epoch
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::FastInputRelayBuffer;
    use crate::protocol::{decode_fast_input_batch, encode_fast_input_frame};
    use crate::rooms::{ConnectionId, PlayerIndex};

    #[test]
    fn drains_only_active_epoch_records() {
        let source = ConnectionId::new();
        let active = fast_frame(7, 9, 2);
        let stale = fast_frame(6, 9, 2);
        let mut buffer = FastInputRelayBuffer::default();

        buffer.push(source, active.clone());
        buffer.push(source, stale);

        let drained = buffer.drain_frame(2, 7, 9);

        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].frame, active);
    }

    fn fast_frame(
        room_epoch: u64,
        session_epoch: u64,
        frame: u64,
    ) -> crate::protocol::FastInputFrame {
        let payload =
            encode_fast_input_frame(room_epoch, session_epoch, PlayerIndex::ONE, frame, &[0])
                .expect("encoded");
        decode_fast_input_batch(payload)
            .expect("decoded")
            .frames
            .into_iter()
            .next()
            .expect("frame")
    }
}
