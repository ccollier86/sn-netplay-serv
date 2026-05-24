//! Sanitized peer-close details for WebSocket diagnostics.
//!
//! Close reasons are client supplied, so they are compacted and bounded before
//! entering relay debug events.

use axum::extract::ws::CloseFrame;

/// Describes a peer close frame without exposing unbounded client text.
pub(super) fn peer_close_detail(frame: Option<CloseFrame>) -> String {
    match frame {
        Some(frame) => {
            let reason = sanitize_reason(&frame.reason.to_string());
            format!("peer close frame code={} reason={reason}", frame.code)
        }
        None => "peer stream ended without close frame".to_string(),
    }
}

/// Describes a socket receive error without exposing unbounded transport text.
pub(super) fn peer_error_detail(error: &axum::Error) -> String {
    format!(
        "socket receive error {}",
        sanitize_reason(&error.to_string())
    )
}

fn sanitize_reason(reason: &str) -> String {
    let compact = reason
        .replace(['\n', '\r', '\t'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    if compact.is_empty() {
        return "unspecified".to_string();
    }

    compact.chars().take(160).collect()
}

#[cfg(test)]
mod tests {
    use super::sanitize_reason;

    #[test]
    fn compacts_close_reason() {
        assert_eq!(
            sanitize_reason("netplay\nclosed\tbad input"),
            "netplay closed bad input"
        );
    }
}
