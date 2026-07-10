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
    /// True when the running binary is installed from the toolbox `insider`
    /// channel. Used as an additional, treat-as-TREATMENT signal for
    /// `Feature::Lite` only — see `variation_impl`. Detected from the install
    /// path because the insider release flow ships binaries with
    /// `CARGO_PKG_VERSION = 0.0.0-dev`, so the standard nightly version-string
    /// detection cannot see them.
    is_insider_toolbox: bool,
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

/// True when `exe` looks like `~/.toolbox/tools/kiro-cli/<X>-insider/.../kiro-cli`.
///
/// Toolbox installs at `~/.toolbox/tools/<tool>/<version>/...`. Custom channels
/// suffix the version dir with the channel name (`2.5.0-insider`,
/// `2.5.0-beta`); stable does not. Checking the suffix on the version
/// component is therefore a reliable runtime signal for "running from the
/// insider channel" — independent of `CARGO_PKG_VERSION`, which the insider
/// release flow does not bake correctly today.
///
/// Pulled out as a free function so unit tests can exercise the path-parsing
/// without needing to relocate a real binary.
fn path_is_insider_toolbox(exe: &std::path::Path) -> bool {
    let mut comps = exe.components();
    // `any` short-circuits, leaving the iterator positioned just after the
    // `.toolbox` component (same as `position`) so `nth(2)` below still lands
    // on <version>.
    if !comps.by_ref().any(|c| c.as_os_str() == ".toolbox") {
        return false;
    }
    // After ".toolbox" we expect: tools / <tool> / <version> / ...
    // We only care about <version>, which is `.toolbox` + 3.
    let version_comp = match comps.nth(2) {
        Some(c) => c,
        None => return false,
    };
    version_comp.as_os_str().to_string_lossy().ends_with("-insider")
}

fn detect_insider_toolbox() -> bool {
    std::env::current_exe()
        .ok()
        .as_deref()
        .is_some_and(path_is_insider_toolbox)
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
        let is_insider_toolbox = detect_insider_toolbox();
        let _ = INSTANCE.set(Rollout {
            features,
            client_id,
            is_internal,
            is_nightly,
            is_insider_toolbox,
        });
    }

    fn variation_impl(&self, feature: Feature) -> Option<&'static str> {
        // Lite-only escape hatch: anyone running from the toolbox `insider`
        // channel gets `Feature::Lite` regardless of segment / channel /
        // treatment_percent. The insider channel is itself a curated install
        // (only Amazonians have a working toolbox), so it acts as the gate.
        // Bypasses the rest of the rollout logic before any percent rolls.
        // Scoped via matches!() so this CANNOT affect Voice or Tui.
        if matches!(feature, Feature::Lite) && self.is_insider_toolbox {
            return Some(TREATMENT);
        }

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
        for name in ["tui", "voice"] {
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
            is_insider_toolbox: true,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rollout(is_internal: bool, is_nightly: bool, is_insider_toolbox: bool) -> Rollout {
        Rollout {
            features: serde_json::from_str(EMBEDDED_CONFIG).unwrap(),
            client_id: Some(Uuid::from_u128(1)),
            is_internal,
            is_nightly,
            is_insider_toolbox,
        }
    }

    // ── path_is_insider_toolbox: pure path parsing ────────────────────────

    #[test]
    fn path_insider_install_is_detected() {
        let p = std::path::PathBuf::from(
            "/Users/alice/.toolbox/tools/kiro-cli/2.5.0-insider/Kiro CLI.app/Contents/MacOS/kiro-cli",
        );
        assert!(path_is_insider_toolbox(&p));
    }

    #[test]
    fn path_stable_toolbox_install_is_not_insider() {
        let p =
            std::path::PathBuf::from("/Users/alice/.toolbox/tools/kiro-cli/2.5.0/Kiro CLI.app/Contents/MacOS/kiro-cli");
        assert!(!path_is_insider_toolbox(&p));
    }

    #[test]
    fn path_nightly_toolbox_install_is_not_insider() {
        let p = std::path::PathBuf::from(
            "/Users/alice/.toolbox/tools/kiro-cli/2.5.1-nightly.1-nightly/Kiro CLI.app/Contents/MacOS/kiro-cli",
        );
        assert!(!path_is_insider_toolbox(&p));
    }

    #[test]
    fn path_beta_toolbox_install_is_not_insider() {
        let p = std::path::PathBuf::from(
            "/Users/alice/.toolbox/tools/kiro-cli/2.5.0-beta/Kiro CLI.app/Contents/MacOS/kiro-cli",
        );
        assert!(!path_is_insider_toolbox(&p));
    }

    #[test]
    fn path_outside_toolbox_is_not_insider() {
        let p = std::path::PathBuf::from("/usr/local/bin/kiro-cli");
        assert!(!path_is_insider_toolbox(&p));
    }

    #[test]
    fn path_insider_substring_in_unrelated_segment_is_not_match() {
        // `insider` appearing in a parent dir but NOT as the version-component
        // suffix should not flip the gate. The check is anchored to the
        // component immediately after `.toolbox/tools/<tool>/`.
        let p = std::path::PathBuf::from("/Users/insider-fan/.toolbox/tools/kiro-cli/2.5.0/bin/kiro-cli");
        assert!(!path_is_insider_toolbox(&p));
    }

    // ── variation_impl: Feature::Lite via the insider toolbox branch ──────

    #[test]
    fn insider_toolbox_enables_lite_for_external_stable_user() {
        // The signature scenario: external user (not Amazon SSO), stable
        // channel — would normally fail the `Feature::Lite` segment+channel
        // gate. With `is_insider_toolbox = true`, lite is enabled anyway.
        let r = rollout(
            // internal=
            false, // nightly=
            false, // insider=
            true,
        );
        assert_eq!(r.variation_impl(Feature::Lite), Some(TREATMENT));
    }

    #[test]
    fn insider_toolbox_enables_lite_for_internal_nightly_user() {
        // Already eligible via the standard rollout path; the insider branch
        // simply short-circuits. Confirms it doesn't downgrade.
        let r = rollout(
            // internal=
            true, // nightly=
            true, // insider=
            true,
        );
        assert_eq!(r.variation_impl(Feature::Lite), Some(TREATMENT));
    }

    #[test]
    fn insider_toolbox_hatch_is_scoped_to_lite() {
        // Critical scope guarantee: the insider branch is gated on
        // `matches!(Feature::Lite)`, so it does NOT leak to Tui. Voice is
        // Some(TREATMENT) here via its own segment=all config (all users),
        // not the insider hatch; Tui still rejects the external user below.
        let r = rollout(
            // internal=
            false, // nightly=
            false, // insider=
            true,
        );
        assert_eq!(
            r.variation_impl(Feature::Voice),
            Some(TREATMENT),
            "voice is enabled for all users (segment=all), independent of the insider hatch"
        );
        // Tui's standard config (segment=internal, treatment_percent=50) should
        // still reject the external user.
        assert_eq!(
            r.variation_impl(Feature::Tui),
            None,
            "insider toolbox install must not affect Tui gating"
        );
    }

    #[test]
    fn non_insider_install_preserves_existing_lite_gating() {
        // External + stable + NOT insider-install → no lite (segment=internal,
        // channel=nightly both reject).
        let r = rollout(
            // internal=
            false, // nightly=
            false, // insider=
            false,
        );
        assert_eq!(r.variation_impl(Feature::Lite), None);
    }

    // NOTE: `non_insider_internal_nightly_still_works` (the config-driven
    // TREATMENT path) lands with the flag-flip PR that adds the `lite` key to
    // rollout.json. Until then there is no `lite` config entry, so the only
    // way `Feature::Lite` resolves to TREATMENT is the insider-toolbox escape
    // hatch (covered by `insider_toolbox_enables_lite_*` above).

    #[test]
    fn non_insider_internal_stable_no_lite() {
        // Internal but stable, no insider install → no `lite` config entry yet
        // → lite stays gated off.
        let r = rollout(
            // internal=
            true,  // nightly=
            false, // insider=
            false,
        );
        assert_eq!(r.variation_impl(Feature::Lite), None);
    }
}
