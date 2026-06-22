//! Clock-sample protocol values for scheduled netplay starts.
//!
//! These messages ride on the existing room control WebSocket. They do not
//! create a second transport connection, own room state, or drive frame release.

use serde::{Deserialize, Serialize};

/// Client-originated clock ping used for diagnostics and compatibility.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClockSyncPing {
    /// Client-generated sample id echoed by the server.
    pub sample_id: String,
    /// Client monotonic send timestamp in milliseconds.
    pub client_send_time_ms: u64,
}

/// Server response to a client-originated clock ping.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClockSyncPong {
    /// Client-generated sample id echoed by the server.
    pub sample_id: String,
    /// Client monotonic send timestamp supplied by the ping.
    pub client_send_time_ms: u64,
    /// Server monotonic receive timestamp in milliseconds.
    pub server_receive_time_ms: u64,
    /// Server monotonic send timestamp in milliseconds.
    pub server_send_time_ms: u64,
}

/// Server request for a short startup clock-sample burst.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClockSyncSampleRequest {
    /// Server-generated id tying replies to this request.
    pub request_id: String,
    /// Number of samples requested from each v2 client.
    pub requested_sample_count: u8,
    /// Server monotonic send timestamp in milliseconds.
    pub server_send_time_ms: u64,
}

/// Client response for one server-requested clock sample.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClockSyncSample {
    /// Request id supplied by `ClockSyncSampleRequest`.
    pub request_id: String,
    /// Zero-based sample index for this request.
    pub sample_index: u8,
    /// Server send timestamp echoed from the request.
    pub server_send_time_ms: u64,
    /// Client monotonic receive timestamp in milliseconds.
    pub client_receive_time_ms: u64,
    /// Client monotonic send timestamp in milliseconds.
    pub client_send_time_ms: u64,
}

/// Client-side estimate produced from ping/pong timing.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClockSyncEstimate {
    /// Sample id used to correlate the estimate.
    pub sample_id: String,
    /// Symmetric round-trip estimate in milliseconds.
    pub round_trip_ms: u64,
    /// Estimated server minus client clock offset in milliseconds.
    pub server_time_offset_ms: i64,
    /// Conservative uncertainty budget in milliseconds.
    pub uncertainty_ms: u64,
}
