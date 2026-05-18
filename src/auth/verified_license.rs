//! Verified client identity returned by the license authority.
//!
//! This module stores only the stable subject data needed by rooms. It does not
//! retain raw client tokens.

use crate::auth::ClientKind;

/// Client subject allowed to use netplay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedLicense {
    /// Platform family tied to the verified protected-client session.
    pub client_kind: ClientKind,
    /// Install id tied to the verified protected-client session.
    pub installation_id: String,
    /// Stable license, install, or account subject id.
    pub subject_id: String,
    /// Billing or entitlement tier returned by the license authority.
    pub tier: String,
    /// Enabled features for the verified subject.
    pub features: Vec<String>,
    /// Whether the backend considers this install premium.
    pub has_premium: bool,
    /// Whether the backend considers this install in an active trial.
    pub trial_active: bool,
}

impl VerifiedLicense {
    /// Creates a verified license identity for tests and simple fixtures.
    pub fn new(
        subject_id: impl Into<String>,
        tier: impl Into<String>,
        features: Vec<String>,
    ) -> Self {
        let subject_id = subject_id.into();
        let tier = tier.into();
        let normalized_tier = tier.to_ascii_lowercase();
        let has_premium = matches!(
            normalized_tier.as_str(),
            "lifetime" | "paid" | "premium" | "pro"
        );
        let trial_active = normalized_tier == "trial";

        Self {
            client_kind: ClientKind::Desktop,
            installation_id: subject_id.clone(),
            subject_id,
            tier,
            features,
            has_premium,
            trial_active,
        }
    }

    /// Creates a verified identity from backend entitlement details.
    pub fn with_entitlement(
        client_kind: ClientKind,
        installation_id: impl Into<String>,
        subject_id: impl Into<String>,
        tier: impl Into<String>,
        features: Vec<String>,
        has_premium: bool,
        trial_active: bool,
    ) -> Self {
        Self {
            client_kind,
            installation_id: installation_id.into(),
            subject_id: subject_id.into(),
            tier: tier.into(),
            features,
            has_premium,
            trial_active,
        }
    }

    /// Returns whether this verified subject includes active access for `feature`.
    pub fn allows_feature_or_active_access(&self, feature: &str) -> bool {
        self.has_premium
            || self.trial_active
            || self.features.iter().any(|candidate| candidate == feature)
    }

    /// Returns a collision-resistant key for room ownership comparisons.
    pub fn identity_key(&self) -> String {
        format!(
            "{}:{}",
            self.client_kind.identity_namespace(),
            self.subject_id
        )
    }
}
