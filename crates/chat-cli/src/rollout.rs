// Rollout framework is intentionally kept for future feature gating even though
// `variation()` / `is_enabled()` have no callers after the TUI rollout completed.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;
use sha2::{
    Digest,
    Sha256,
};
use uuid::Uuid;

const AMZN_START_URL: &str = "https://amzn.awsapps.com/start";

pub const TREATMENT: &str = "TREATMENT";
pub const CONTROL: &str = "CONTROL";

/// Known rollout features. Add new variants here when adding entries to `rollout.json`.
#[derive(Debug, Clone, Copy, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum Feature {
    Tui,
    #[cfg(test)]
    Test,
    #[cfg(test)]
    TestInternalOnly,
}

/// Which user segment the experiment targets.
#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Segment {
    /// Experiment applies to all users (default).
    #[default]
    All,
    /// Experiment applies only to internal (Amazon) users.
    Internal,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeatureRollout {
    /// What this experiment is testing.
    #[serde(default)]
    #[allow(dead_code)]
    pub description: String,
    /// Percentage of eligible users that get TREATMENT (0-100). The rest get CONTROL.
    pub treatment_percent: u8,
    #[serde(default)]
    pub segment: Segment,
}

/// Gradual rollout configuration baked into the binary at compile time.
///
/// Edit `crates/chat-cli/rollout.json` to change rollout percentages.
/// Each feature has an independent rollout percentage. The user's client_id
/// is hashed with the feature name as salt so the same user may be in
/// different cohorts for different features.
///
/// `percent` controls what fraction of eligible users get TREATMENT.
/// The remainder get CONTROL. Users outside the segment are not in
/// the experiment at all (`variation()` returns `None`).
///
///
/// Initialized once at startup via `Rollout::init()`, then accessed
/// anywhere via `Rollout::variation()` or `Rollout::is_enabled()`.
#[derive(Debug, Clone)]
pub struct Rollout {
    features: HashMap<String, FeatureRollout>,
    client_id: Option<Uuid>,
    is_internal: bool,
}

const EMBEDDED_CONFIG: &str = include_str!("../rollout.json");

static INSTANCE: OnceLock<Rollout> = OnceLock::new();

/// Pure function: is `client_id` in the rollout bucket for `feature` at `percent`?
fn in_rollout(feature: &str, client_id: Uuid, percent: u8) -> bool {
    let percent = percent.min(100);
    let mut hasher = Sha256::new();
    hasher.update(feature.as_bytes());
    hasher.update(client_id.as_bytes());
    let hash = hasher.finalize();
    let bucket = u64::from_le_bytes(hash[..8].try_into().unwrap()) % 100;
    bucket < percent as u64
}

impl Rollout {
    /// Initialize the global rollout instance. Call once at startup after resolving client_id.
    pub fn init(client_id: Option<Uuid>, start_url: Option<String>) {
        let features = serde_json::from_str::<HashMap<String, FeatureRollout>>(EMBEDDED_CONFIG).unwrap_or_default();
        let is_internal = start_url.as_deref().map(str::trim) == Some(AMZN_START_URL);
        let _ = INSTANCE.set(Rollout {
            features,
            client_id,
            is_internal,
        });
    }

    /// Returns the variation for the current user in the given experiment.
    ///
    /// - `Some(TREATMENT)` — user is in the experiment and gets the new behavior
    /// - `Some(CONTROL)` — user is in the experiment but gets the default behavior (for
    ///   measurement)
    /// - `None` — user is not in the experiment (wrong segment, no client_id, or feature not
    ///   configured)
    pub fn variation(feature: Feature) -> Option<&'static str> {
        INSTANCE.get()?.variation_impl(feature)
    }

    /// Instance method: testable without OnceLock.
    fn variation_impl(&self, feature: Feature) -> Option<&'static str> {
        let config = self.features.get(<&str>::from(feature))?;
        if config.segment == Segment::Internal && !self.is_internal {
            return None;
        }
        let id = self.client_id?;
        if in_rollout(<&str>::from(feature), id, config.treatment_percent) {
            Some(TREATMENT)
        } else {
            Some(CONTROL)
        }
    }

    /// Convenience: returns true if the user gets TREATMENT for this feature.
    pub fn is_enabled(feature: Feature) -> bool {
        Self::variation(feature) == Some(TREATMENT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_config_parses() {
        let features: HashMap<String, FeatureRollout> = serde_json::from_str(EMBEDDED_CONFIG).unwrap();
        assert!(features.contains_key(<&str>::from(Feature::Tui)));
        assert!(features.contains_key(<&str>::from(Feature::Test)));
        assert!(features.contains_key(<&str>::from(Feature::TestInternalOnly)));
    }

    #[test]
    fn test_external_user_sees_all_segment_but_not_internal() {
        let rollout = Rollout {
            features: serde_json::from_str(EMBEDDED_CONFIG).unwrap(),
            client_id: Some(Uuid::from_u128(1)),
            is_internal: false,
        };

        // test has segment=all, treatment_percent=100 → TREATMENT
        assert_eq!(rollout.variation_impl(Feature::Test), Some(TREATMENT));

        // test_internal_only has segment=internal → None for external user
        assert_eq!(rollout.variation_impl(Feature::TestInternalOnly), None);
    }

    #[test]
    fn test_internal_user_sees_both_segments() {
        let rollout = Rollout {
            features: serde_json::from_str(EMBEDDED_CONFIG).unwrap(),
            client_id: Some(Uuid::from_u128(1)),
            is_internal: true,
        };

        assert_eq!(rollout.variation_impl(Feature::Test), Some(TREATMENT));
        assert_eq!(rollout.variation_impl(Feature::TestInternalOnly), Some(TREATMENT));
    }

    #[test]
    fn test_0_percent_never_enables() {
        for i in 0..100u128 {
            assert!(!in_rollout("test", Uuid::from_u128(i), 0));
        }
    }

    #[test]
    fn test_100_percent_always_enables() {
        for i in 0..100u128 {
            assert!(in_rollout("test", Uuid::from_u128(i), 100));
        }
    }

    #[test]
    fn test_known_uuid_bucket_boundary() {
        // UUID 550e8400-... hashes to bucket 16 for feature "test"
        let id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        assert!(!in_rollout("test", id, 16)); // bucket 16 is NOT < 16
        assert!(in_rollout("test", id, 17)); // bucket 16 IS < 17
    }
}
