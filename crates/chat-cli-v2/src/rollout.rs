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
        let features = serde_json::from_str::<HashMap<String, FeatureRollout>>(EMBEDDED_CONFIG).unwrap_or_default();
        let is_internal = start_url.as_deref().map(str::trim) == Some(AMZN_START_URL);
        let is_nightly = env!("CARGO_PKG_VERSION").contains("-nightly");
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
}
