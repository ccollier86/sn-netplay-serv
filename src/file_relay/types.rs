//! DTOs for the trusted file relay service.
//!
//! These mirror `sb-file-relay-serv` without coupling the netplay relay to that
//! server's crate.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// Payload kind sent through the file relay.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileRelayTransferKind {
    /// Temporary ROM payload.
    Rom,
    /// Serialized save-state payload.
    SaveState,
}

/// Service-created transfer request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFileRelayTransferRequest {
    /// Netplay room or lobby id that owns this transfer.
    pub room_id: String,
    /// Sender player id or slot id.
    pub sender_player_id: String,
    /// Receiver player id or slot id.
    pub receiver_player_id: String,
    /// Payload kind.
    pub kind: FileRelayTransferKind,
    /// Expected SHA-256 of the complete payload.
    pub sha256: String,
    /// Expected byte size of the complete payload.
    pub size_bytes: u64,
    /// Optional shorter lifetime than the relay default.
    pub expires_in_seconds: Option<u64>,
}

/// File relay transfer grant returned by the trusted relay.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFileRelayTransferResponse {
    /// Generated transfer id.
    pub transfer_id: String,
    /// Bytes per chunk.
    pub chunk_size_bytes: u64,
    /// Expected chunk count.
    pub chunk_count: u64,
    /// Opaque upload token for the sender.
    pub upload_token: String,
    /// Opaque download token for the receiver.
    pub download_token: String,
    /// Transfer expiry timestamp.
    #[serde(deserialize_with = "deserialize_relay_expires_at")]
    pub expires_at: String,
}

fn deserialize_relay_expires_at<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::String(value) if !value.trim().is_empty() => Ok(value),
        Value::Array(parts) => {
            deserialize_legacy_time_array(&parts).map_err(serde::de::Error::custom)
        }
        _ => Err(serde::de::Error::custom(
            "expiresAt must be a non-empty RFC3339 string",
        )),
    }
}

fn deserialize_legacy_time_array(parts: &[Value]) -> Result<String, String> {
    if parts.len() != 9 {
        return Err("legacy expiresAt array must contain 9 values".to_string());
    }

    let year = array_i32(parts, 0, "year")?;
    let ordinal = array_u32(parts, 1, "ordinal day")?;
    let hour = array_u32(parts, 2, "hour")?;
    let minute = array_u32(parts, 3, "minute")?;
    let second = array_u32(parts, 4, "second")?;
    let nanosecond = array_u32(parts, 5, "nanosecond")?;
    let offset_hour = array_i32(parts, 6, "offset hour")?;
    let offset_minute = array_i32(parts, 7, "offset minute")?;
    let offset_second = array_i32(parts, 8, "offset second")?;

    if ordinal == 0 || ordinal > days_in_year(year) {
        return Err("legacy expiresAt ordinal day is out of range".to_string());
    }
    if hour > 23 || minute > 59 || second > 60 || nanosecond > 999_999_999 {
        return Err("legacy expiresAt time is out of range".to_string());
    }
    if offset_hour.abs() > 23 || offset_minute.abs() > 59 || offset_second != 0 {
        return Err("legacy expiresAt offset is out of range".to_string());
    }

    let (month, day) = month_day_from_ordinal(year, ordinal)?;
    let fraction = if nanosecond == 0 {
        String::new()
    } else {
        format!(".{:09}", nanosecond)
            .trim_end_matches('0')
            .to_string()
    };
    let offset = if offset_hour == 0 && offset_minute == 0 {
        "Z".to_string()
    } else {
        let sign = if offset_hour < 0 || offset_minute < 0 {
            '-'
        } else {
            '+'
        };
        format!(
            "{}{:02}:{:02}",
            sign,
            offset_hour.abs(),
            offset_minute.abs()
        )
    };

    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}{fraction}{offset}"
    ))
}

fn array_i32(parts: &[Value], index: usize, field: &str) -> Result<i32, String> {
    parts[index]
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| format!("legacy expiresAt {field} must be an integer"))
}

fn array_u32(parts: &[Value], index: usize, field: &str) -> Result<u32, String> {
    parts[index]
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| format!("legacy expiresAt {field} must be an unsigned integer"))
}

fn month_day_from_ordinal(year: i32, ordinal: u32) -> Result<(u32, u32), String> {
    let month_lengths = [
        31,
        if is_leap_year(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut remaining = ordinal;
    for (index, days) in month_lengths.iter().enumerate() {
        if remaining <= *days {
            return Ok(((index + 1) as u32, remaining));
        }
        remaining -= *days;
    }
    Err("legacy expiresAt ordinal day is out of range".to_string())
}

fn days_in_year(year: i32) -> u32 {
    if is_leap_year(year) { 366 } else { 365 }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::CreateFileRelayTransferResponse;
    use serde_json::json;

    #[test]
    fn deserializes_rfc3339_expiry_strings() {
        let response: CreateFileRelayTransferResponse = serde_json::from_value(json!({
            "transferId": "tx_test",
            "chunkSizeBytes": 1024,
            "chunkCount": 1,
            "uploadToken": "upload",
            "downloadToken": "download",
            "expiresAt": "2026-05-30T21:08:16Z"
        }))
        .expect("response");

        assert_eq!(response.expires_at, "2026-05-30T21:08:16Z");
    }

    #[test]
    fn deserializes_legacy_time_array_expiry() {
        let response: CreateFileRelayTransferResponse = serde_json::from_value(json!({
            "transferId": "tx_test",
            "chunkSizeBytes": 1048576,
            "chunkCount": 1,
            "uploadToken": "upload",
            "downloadToken": "download",
            "expiresAt": [2026, 150, 21, 8, 16, 783642669, 0, 0, 0]
        }))
        .expect("legacy response");

        assert_eq!(response.expires_at, "2026-05-30T21:08:16.783642669Z");
    }
}
