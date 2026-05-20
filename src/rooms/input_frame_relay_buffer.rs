//! Server-side input frame release buffer.
//!
//! The relay accepts bounded future input for prediction clients, but it should
//! only publish frames once the room's canonical frame reaches them.

use crate::protocol::{InputFrame, InputFrameBatch};
use crate::rooms::ConnectionId;
use std::collections::BTreeMap;

/// Ready-to-emit input frames grouped by source socket and player slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BufferedInputFrameBatch {
    pub source: ConnectionId,
    pub batch: InputFrameBatch,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BufferedInputFrame {
    source: ConnectionId,
    input: InputFrame,
}

/// Holds accepted future input until the authoritative room frame catches up.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct InputFrameRelayBuffer {
    frames_by_number: BTreeMap<u64, Vec<BufferedInputFrame>>,
}

impl InputFrameRelayBuffer {
    /// Stores one accepted frame for later canonical-frame release.
    pub fn push(&mut self, source: ConnectionId, input: InputFrame) {
        self.frames_by_number
            .entry(input.frame)
            .or_default()
            .push(BufferedInputFrame { source, input });
    }

    /// Drains exactly one released frame.
    pub fn drain_frame(
        &mut self,
        frame: u64,
        room_epoch: u64,
        session_epoch: u64,
    ) -> Vec<BufferedInputFrameBatch> {
        let mut batches = Vec::new();

        for buffered in self.frames_by_number.remove(&frame).unwrap_or_default() {
            push_to_batch(
                &mut batches,
                buffered.source,
                room_epoch,
                session_epoch,
                buffered.input,
            );
        }

        batches
    }

    /// Drops all buffered input that belongs to a previous sync epoch.
    pub fn clear(&mut self) {
        self.frames_by_number.clear();
    }
}

fn push_to_batch(
    batches: &mut Vec<BufferedInputFrameBatch>,
    source: ConnectionId,
    room_epoch: u64,
    session_epoch: u64,
    input: InputFrame,
) {
    if let Some(existing) = batches
        .iter_mut()
        .find(|batch| batch.source == source && batch.batch.player_index == input.player_index)
    {
        existing.batch.frames.push(input);
        return;
    }

    let player_index = input.player_index;
    batches.push(BufferedInputFrameBatch {
        source,
        batch: InputFrameBatch {
            frames: vec![input],
            player_index,
            room_epoch,
            session_epoch,
        },
    });
}

#[cfg(test)]
mod tests {
    use super::InputFrameRelayBuffer;
    use crate::protocol::InputFrame;
    use crate::rooms::{ConnectionId, PlayerIndex};

    #[test]
    fn drains_only_released_frame() {
        let mut buffer = InputFrameRelayBuffer::default();
        let source = ConnectionId::new();

        buffer.push(source, input(PlayerIndex::ONE, 2));
        buffer.push(source, input(PlayerIndex::ONE, 3));

        assert!(buffer.drain_frame(1, 7, 9).is_empty());

        let drained = buffer.drain_frame(2, 7, 9);

        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].batch.frames[0].frame, 2);
        assert_eq!(drained[0].batch.room_epoch, 7);
        assert_eq!(drained[0].batch.session_epoch, 9);
        assert_eq!(buffer.drain_frame(3, 7, 9)[0].batch.frames[0].frame, 3);
    }

    fn input(player_index: PlayerIndex, frame: u64) -> InputFrame {
        InputFrame {
            frame,
            payload: vec![0],
            player_index,
        }
    }
}
