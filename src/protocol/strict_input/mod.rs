//! Protocol v5 strict controller-input lane messages.
//!
//! Envelope integers use network byte order. The ten-byte Retropad payload is
//! opaque here and keeps its separately specified little-endian byte layout.

mod codec_common;
mod error;
mod frame_clock;
mod input_batch;
mod input_cursor;

/// Fixed controller payload codec required by protocol v5.
pub const V5_INPUT_CODEC_ID: &str = "shadowboy-retropad-v1-le";
/// Cross-platform prediction semantics required by protocol v5.
pub const V5_INPUT_PREDICTOR_ID: &str = "shadowboy-retropad-predictor-v1";

pub use error::StrictInputCodecError;
pub use frame_clock::{
    AcceptedInputCursor, HostFrameOpen, ServerFrameReleaseV5, decode_host_frame_open,
    decode_server_frame_release_v5, encode_host_frame_open, encode_server_frame_release_v5,
};
pub use input_batch::{
    RetropadInputPayload, StrictInputBatch, decode_strict_input_batch, encode_strict_input_batch,
};
pub use input_cursor::{
    InputCursorAck, InputCursorNack, InputCursorNackReason, InputCursorResponse,
    decode_input_cursor_ack, decode_input_cursor_nack, encode_input_cursor_ack,
    encode_input_cursor_nack,
};

#[cfg(test)]
mod predictor_tests;
#[cfg(test)]
mod tests;
