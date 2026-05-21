//! Client platforms represented in room/session protocol metadata.
//!
//! This is separate from auth parsing so room views can expose the host
//! platform without depending on HTTP headers.

use serde::{Deserialize, Serialize};

/// ShadowBoy app family that created a room.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NetplayClientKind {
    /// ShadowBoy Desktop.
    Desktop,
    /// ShadowBoy Android.
    Android,
}

#[cfg(test)]
mod tests {
    use super::NetplayClientKind;
    use serde_json::json;

    #[test]
    fn serializes_canonical_contract_values() {
        assert_eq!(
            serde_json::to_value(NetplayClientKind::Desktop).expect("json"),
            json!("desktop")
        );
        assert_eq!(
            serde_json::to_value(NetplayClientKind::Android).expect("json"),
            json!("android")
        );
    }
}
