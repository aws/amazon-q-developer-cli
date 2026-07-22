// Rollout framework — mirrors crates/chat-cli/src/rollout.rs
// Uses the same rollout.json configuration.
#![allow(dead_code)]

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

/// Known rollout features.
#[derive(Debug, Clone, Copy, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum Feature {
    Tui,
    Voice,
    Lite,
    V2NonInteractive,
}

/// Which user segment the experiment targets.
#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Segment {
    #[default]
    All,
    Internal,
}

/// Which release channel the experiment targets.
#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    #[default]
    All,
    Nightly,
    Stable,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeatureRollout {
    #[serde(default)]
    pub description: String,
    pub treatment_percent: u8,
    #[serde(default)]
    pub segment: Segment,
    #[serde(default)]
    pub channel: Channel,
}

#[derive(Debug, Clone)]
pub struct Rollout {
    features: HashMap<String, FeatureRollout>,
    client_id: Option<Uuid>,
    is_internal: bool,
    is_nightly: bool,
}

// Share the same rollout.json as chat-cli
const EMBEDDED_CONFIG: &str = include_str!("../../chat-cli/rollout.json");

static INSTANCE: OnceLock<Rollout> = OnceLock::new();

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
    /// Initialize the global rollout instance. Call once at startup.
    pub fn init(client_id: Option<Uuid>, start_url: Option<String>) {
        // In test mode or debug builds, enable everything unconditionally.
        if std::env::var("KIRO_TEST_MODE").is_ok() || cfg!(debug_assertions) {
            Self::init_for_tests_enable_all();
            return;
        }
        let features = serde_json::from_str::<HashMap<String, FeatureRollout>>(EMBEDDED_CONFIG).unwrap_or_default();
        let is_internal = start_url.as_deref().map(str::trim) == Some(AMZN_START_URL)
            || std::env::var("KIRO_ROLLOUT_FORCE_INTERNAL").is_ok();
        let is_nightly =
            env!("CARGO_PKG_VERSION").contains("-nightly") || std::env::var("KIRO_ROLLOUT_FORCE_NIGHTLY").is_ok();
        let _ = INSTANCE.set(Rollout {
            features,
            client_id,
            is_internal,
            is_nightly,
        });
    }

    fn variation_impl(&self, feature: Feature) -> Option<&'static str> {
        let config = self.features.get(<&str>::from(feature))?;
        if config.segment == Segment::Internal && !self.is_internal {
            return None;
        }
        match config.channel {
            Channel::Nightly if !self.is_nightly => return None,
            Channel::Stable if self.is_nightly => return None,
            _ => {},
        }
        let id = self.client_id?;
        if in_rollout(<&str>::from(feature), id, config.treatment_percent) {
            Some(TREATMENT)
        } else {
            Some(CONTROL)
        }
    }

    pub fn variation(feature: Feature) -> Option<&'static str> {
        INSTANCE.get()?.variation_impl(feature)
    }

    /// Returns true if the user gets TREATMENT for this feature.
    pub fn is_enabled(feature: Feature) -> bool {
        Self::variation(feature) == Some(TREATMENT)
    }

    /// Test helper: force the global rollout to allow every feature
    /// regardless of segment/channel/percent. Idempotent — safe to call
    /// multiple times. Has no effect if a real `init()` already ran.
    ///
    /// Used by integration tests that exercise rollout-gated features
    /// (like `/goal`) end-to-end. Safe to leave callable in non-test
    /// builds because `INSTANCE` is a `OnceLock`: real `init()` runs at
    /// startup and wins, so this becomes a no-op.
    #[doc(hidden)]
    pub fn init_for_tests_enable_all() {
        // INSTANCE is OnceLock — first set wins; ignore subsequent attempts.
        if INSTANCE.get().is_some() {
            return;
        }
        let mut features = HashMap::new();
        for name in ["tui", "voice", "lite"] {
            features.insert(name.to_string(), FeatureRollout {
                description: "test-enabled".to_string(),
                treatment_percent: 100,
                segment: Segment::All,
                channel: Channel::All,
            });
        }
        let _ = INSTANCE.set(Rollout {
            features,
            client_id: Some(Uuid::nil()),
            is_internal: true,
            is_nightly: true,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rollout(is_internal: bool, is_nightly: bool) -> Rollout {
        Rollout {
            features: serde_json::from_str(EMBEDDED_CONFIG).unwrap(),
            client_id: Some(Uuid::from_u128(1)),
            is_internal,
            is_nightly,
        }
    }

    #[test]
    fn lite_requires_internal_stable() {
        for (is_internal, is_nightly, expected) in [
            (false, false, false),
            (false, true, false),
            (true, false, true),
            (true, true, false),
        ] {
            let rollout = rollout(is_internal, is_nightly);
            assert_eq!(
                rollout.variation_impl(Feature::Lite) == Some(TREATMENT),
                expected,
                "lite enabled={expected} for internal={is_internal}, nightly={is_nightly}"
            );
        }
    }
}
