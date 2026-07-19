//! Parser for metadata-service protected-client authorization responses.
//!
//! The backend may return a direct authorization response or a billing-style
//! entitlement object. This module normalizes those shapes into one verified
//! identity without exposing transport details to room code.

use crate::auth::{AuthError, ClientKind, VerifiedLicense};
use serde_json::Value;

const RECORD_KEYS: &[&str] = &[
    "account",
    "billing",
    "customer",
    "entitlement",
    "license",
    "purchase",
    "stripe",
];

/// Parses a backend response into a verified netplay identity.
pub fn parse_verified_license(
    value: Value,
    expected_client_kind: ClientKind,
    installation_id: &str,
    feature: &str,
) -> Result<VerifiedLicense, AuthError> {
    let records = collect_records(&value);

    if records.is_empty() {
        return Err(AuthError::InvalidAuthorityResponse);
    }

    if read_bool(&records, &["valid", "authorized", "allowed"]) == Some(false) {
        return Err(AuthError::Unauthorized);
    }

    if let Some(client_kind) = read_client_kind(&records)?
        && client_kind != expected_client_kind
    {
        return Err(AuthError::InvalidAuthorityResponse);
    }

    let features = collect_features(&records);
    let tier = read_string(&records, &["tier", "accessStatus", "status", "entitlement"])
        .unwrap_or_else(|| "unknown".to_string());
    let has_premium = read_bool(&records, &["hasPremium", "isPremium", "premiumActive"])
        .unwrap_or(false)
        || matches!(
            tier.to_ascii_lowercase().as_str(),
            "lifetime" | "paid" | "premium" | "pro"
        );
    let trial_active = read_bool(&records, &["isTrialActive", "trialActive"]).unwrap_or(false)
        || tier.eq_ignore_ascii_case("trial")
        || read_future_date(&records, &["trialEndsAt", "trialExpiresAt"]);
    let subject_id = read_string(
        &records,
        &[
            "subjectId",
            "subject_id",
            "licenseId",
            "license_id",
            "accountId",
            "account_id",
            "installationId",
            "installation_id",
        ],
    )
    .unwrap_or_else(|| installation_id.to_string());
    let verified = VerifiedLicense::with_entitlement(
        expected_client_kind,
        installation_id,
        subject_id,
        tier,
        features,
        has_premium,
        trial_active,
    );

    if read_bool(&records, &["authorized", "allowed"]) == Some(true)
        || verified.allows_feature_or_active_access(feature)
    {
        return Ok(verified);
    }

    Err(AuthError::EntitlementRequired)
}

fn collect_records(value: &Value) -> Vec<&serde_json::Map<String, Value>> {
    let mut records = Vec::new();

    if let Some(root) = value.as_object() {
        records.push(root);

        for key in RECORD_KEYS {
            if let Some(record) = root.get(*key).and_then(Value::as_object) {
                records.push(record);
            }
        }
    }

    records
}

fn collect_features(records: &[&serde_json::Map<String, Value>]) -> Vec<String> {
    let mut features = Vec::new();

    for record in records {
        if let Some(feature_array) = record.get("features").and_then(Value::as_array) {
            for feature in feature_array {
                if let Some(feature) = feature.as_str().filter(|value| !value.trim().is_empty()) {
                    features.push(feature.trim().to_string());
                }
            }
        }

        if let Some(feature_map) = record.get("features").and_then(Value::as_object) {
            for (feature, enabled) in feature_map {
                if enabled.as_bool() == Some(true) {
                    features.push(feature.to_string());
                }
            }
        }
    }

    features.sort();
    features.dedup();
    features
}

fn read_bool(records: &[&serde_json::Map<String, Value>], keys: &[&str]) -> Option<bool> {
    records
        .iter()
        .flat_map(|record| keys.iter().filter_map(|key| record.get(*key)))
        .find_map(Value::as_bool)
}

fn read_string(records: &[&serde_json::Map<String, Value>], keys: &[&str]) -> Option<String> {
    records
        .iter()
        .flat_map(|record| keys.iter().filter_map(|key| record.get(*key)))
        .find_map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
        })
}

fn read_client_kind(
    records: &[&serde_json::Map<String, Value>],
) -> Result<Option<ClientKind>, AuthError> {
    read_string(records, &["clientKind", "client_kind"])
        .map(|value| ClientKind::parse(&value).map_err(|_| AuthError::InvalidAuthorityResponse))
        .transpose()
}

fn read_future_date(records: &[&serde_json::Map<String, Value>], keys: &[&str]) -> bool {
    records
        .iter()
        .flat_map(|record| keys.iter().filter_map(|key| record.get(*key)))
        .any(|value| match value {
            Value::Number(number) => number
                .as_i64()
                .is_some_and(|timestamp| normalize_timestamp_ms(timestamp) > current_epoch_ms()),
            Value::String(value) => {
                parse_date_ms(value).is_some_and(|timestamp| timestamp > current_epoch_ms())
            }
            _ => false,
        })
}

fn parse_date_ms(value: &str) -> Option<i64> {
    let trimmed = value.trim();

    if let Ok(timestamp) = trimmed.parse::<i64>() {
        return Some(normalize_timestamp_ms(timestamp));
    }

    chrono::DateTime::parse_from_rfc3339(trimmed)
        .ok()
        .map(|timestamp| timestamp.timestamp_millis())
}

fn normalize_timestamp_ms(timestamp: i64) -> i64 {
    if timestamp < 10_000_000_000 {
        timestamp * 1_000
    } else {
        timestamp
    }
}

fn current_epoch_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::parse_verified_license;
    use crate::auth::{AuthError, ClientKind};
    use serde_json::json;

    #[test]
    fn accepts_premium_billing_shape() {
        let verified = parse_verified_license(
            json!({
                "accessStatus": "premium",
                "licenseId": "lic_1",
                "features": {
                    "cheats": true
                }
            }),
            ClientKind::Desktop,
            "install-1",
            "netplay",
        )
        .expect("verified");

        assert_eq!(verified.subject_id, "lic_1");
        assert!(verified.has_premium);
    }

    #[test]
    fn accepts_feature_specific_authorization() {
        let verified = parse_verified_license(
            json!({
                "valid": true,
                "subjectId": "install-1",
                "features": {
                    "netplay": true
                }
            }),
            ClientKind::Desktop,
            "install-1",
            "netplay",
        )
        .expect("verified");

        assert!(verified.features.iter().any(|feature| feature == "netplay"));
    }

    #[test]
    fn accepts_explicit_authorized_response() {
        let verified = parse_verified_license(
            json!({
                "authorized": true,
                "subjectId": "install-1",
                "tier": "authenticated"
            }),
            ClientKind::Android,
            "install-1",
            "netplay",
        )
        .expect("verified");

        assert_eq!(verified.subject_id, "install-1");
        assert_eq!(verified.client_kind, ClientKind::Android);
    }

    #[test]
    fn accepts_android_eligible_client_response() {
        let verified = parse_verified_license(
            json!({
                "authorized": true,
                "clientKind": "android",
                "installationId": "inst_123",
                "subjectId": "inst_123",
                "tier": "authenticated",
                "features": {
                    "netplay": true
                }
            }),
            ClientKind::Android,
            "inst_123",
            "netplay",
        )
        .expect("verified");

        assert_eq!(verified.client_kind, ClientKind::Android);
        assert_eq!(verified.identity_key(), "android:inst_123");
    }

    #[test]
    fn accepts_ios_eligible_client_response() {
        let verified = parse_verified_license(
            json!({
                "authorized": true,
                "clientKind": "ios",
                "installationId": "ios_inst_123",
                "subjectId": "ios_inst_123",
                "tier": "authenticated",
                "features": {
                    "netplay": true
                }
            }),
            ClientKind::Ios,
            "ios_inst_123",
            "netplay",
        )
        .expect("verified");

        assert_eq!(verified.client_kind, ClientKind::Ios);
        assert_eq!(verified.identity_key(), "ios:ios_inst_123");
    }

    #[test]
    fn rejects_mismatched_client_kind_response() {
        let result = parse_verified_license(
            json!({
                "authorized": true,
                "clientKind": "desktop",
                "subjectId": "subject-1"
            }),
            ClientKind::Android,
            "install-1",
            "netplay",
        );

        assert!(matches!(result, Err(AuthError::InvalidAuthorityResponse)));
    }

    #[test]
    fn rejects_expired_entitlement_without_feature() {
        let result = parse_verified_license(
            json!({
                "valid": true,
                "accessStatus": "expired",
                "features": {
                    "netplay": false
                }
            }),
            ClientKind::Desktop,
            "install-1",
            "netplay",
        );

        assert!(matches!(result, Err(AuthError::EntitlementRequired)));
    }

    #[test]
    fn rejects_invalid_token_response() {
        let result = parse_verified_license(
            json!({
                "valid": false
            }),
            ClientKind::Desktop,
            "install-1",
            "netplay",
        );

        assert!(matches!(result, Err(AuthError::Unauthorized)));
    }
}
