// Rollout framework for gradual feature gating by segment and release channel.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{
    Deserialize,
    Serialize,
};
use sha2::{
    Digest,
    Sha256,
};
use typeshare::typeshare;
use uuid::Uuid;

const AMZN_START_URL: &str = "https://amzn.awsapps.com/start";

pub const TREATMENT: &str = "TREATMENT";
pub const CONTROL: &str = "CONTROL";

#[typeshare]
#[derive(Debug, Clone, Copy, Serialize, strum::IntoStaticStr, strum::EnumIter)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum Feature {
    Tui,
    Voice,
    Lite,
    Kas,
    /// Remote/cloud sandbox sessions (`--cloud` / `--repo` flags).
    RemoteSandbox,
    V2NonInteractive,
    /// ICECAP infrastructure-safety gate. Internal-only via `rollout.json`
    /// (`segment: internal`). The launcher exports the decision as
    /// `KIRO_INFRA_SAFETY_ROLLOUT_ENABLED` so the TUI advertises the
    /// `infrastructureSafety` capability and honors the `infraSafetyMonitor` /
    /// `infraSafetyEnforce` settings only for users in the cohort.
    InfraSafety,
    Memory,
    /// KAS dynamic workflows and the TUI workflow management surfaces.
    /// Dark-shipped at 0% until the feature is ready to ramp.
    Workflows,
    /// Code-to-Spec Explore agent and analysis pipeline. Internal nightly only;
    /// launcher exports `KIRO_C2S_ROLLOUT_ENABLED` and the TUI honors
    /// `chat.enableC2s` + shows the Explore agent only when enabled.
    C2s,
    #[cfg(test)]
    #[typeshare(skip)]
    Test,
    #[cfg(test)]
    #[typeshare(skip)]
    TestInternalOnly,
    #[cfg(test)]
    #[typeshare(skip)]
    TestNightlyOnly,
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

/// Which release channel the experiment targets.
#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    /// Experiment applies to all channels (default).
    #[default]
    All,
    /// Experiment applies only to nightly builds.
    Nightly,
    /// Experiment applies only to stable builds.
    Stable,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeatureRollout {
    /// What this experiment is testing.
    #[serde(default)]
    pub description: String,
    /// Percentage of eligible users that get TREATMENT (0-100). The rest get CONTROL.
    pub treatment_percent: u8,
    #[serde(default)]
    pub segment: Segment,
    /// Which release channel this experiment targets.
    #[serde(default)]
    pub channel: Channel,
}

/// Gradual rollout configuration baked into the binary at compile time.
///
/// Edit `crates/chat-cli/rollout.json` to change rollout percentages.
/// Each feature has an independent rollout percentage. The user's client_id
/// is hashed with the feature name as salt so the same user may be in
/// different cohorts for different features.
///
/// `percent` controls what fraction of eligible users get TREATMENT.
/// The remainder get CONTROL. Users outside the segment or channel are not
/// in the experiment at all (`variation()` returns `None`).
///
/// Initialized once at startup via `Rollout::init()`, then accessed
/// anywhere via `rollout().variation()` or `rollout().is_enabled()`.
#[derive(Debug, Clone)]
pub struct Rollout {
    features: HashMap<String, FeatureRollout>,
    client_id: Option<Uuid>,
    is_internal: bool,
    is_nightly: bool,
    /// True when the running binary is installed from the toolbox `insider`
    /// channel. Used as an additional, treat-as-TREATMENT signal for
    /// `Feature::Lite` only — see `variation`. Detected from the install
    /// path because the insider release flow ships binaries with
    /// `CARGO_PKG_VERSION = 0.0.0-dev`, so the standard nightly version-string
    /// detection cannot see them.
    is_insider_toolbox: bool,
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

/// True when `start_url` is the internal Amazon IdC start URL.
fn is_amzn_start_url(start_url: Option<&str>) -> bool {
    start_url.map(str::trim) == Some(AMZN_START_URL)
}

/// Resolve the start URL used for rollout segment detection.
pub fn resolve_segment_start_url(token_start_url: Option<String>, db_start_url: Option<String>) -> Option<String> {
    token_start_url.or(db_start_url)
}

impl Rollout {
    /// Initialize the global rollout instance. Call once at startup after resolving client_id.
    pub fn init(client_id: Option<Uuid>, start_url: Option<String>) {
        // E2E tests and local debug builds exercise rollout-gated features
        // without a real release channel/client cohort. Match the v2 rollout
        // helper so the Rust launcher can still make a server-authoritative
        // decision before spawning the TUI.
        if std::env::var("KIRO_TEST_MODE").is_ok() || cfg!(debug_assertions) {
            Self::init_for_tests_enable_all();
            return;
        }

        let features = serde_json::from_str::<HashMap<String, FeatureRollout>>(EMBEDDED_CONFIG).unwrap_or_default();
        let is_internal = is_amzn_start_url(start_url.as_deref());
        let is_nightly = env!("CARGO_PKG_VERSION").contains("-nightly");
        let is_insider_toolbox = detect_insider_toolbox();
        let _ = INSTANCE.set(Rollout {
            features,
            client_id,
            is_internal,
            is_nightly,
            is_insider_toolbox,
        });
    }

    /// Returns the variation for this rollout state in the given experiment.
    ///
    /// - `Some(TREATMENT)` — gets the new behavior
    /// - `Some(CONTROL)` — in the experiment but gets the default behavior
    /// - `None` — not in the experiment (wrong segment/channel, no client_id, or feature absent)
    pub fn variation(&self, feature: Feature) -> Option<&'static str> {
        // Lite-only escape hatch: anyone running from the toolbox `insider`
        // channel gets `Feature::Lite` regardless of segment / channel /
        // treatment_percent. The insider channel is itself a curated install
        // (only Amazonians have a working toolbox), so it acts as the gate.
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

    /// Returns true if this rollout state gets TREATMENT for the feature.
    pub fn is_enabled(&self, feature: Feature) -> bool {
        self.variation(feature) == Some(TREATMENT)
    }

    pub fn enabled_features(&self) -> Vec<Feature> {
        use strum::IntoEnumIterator;
        Feature::iter().filter(|f| self.is_enabled(*f)).collect()
    }

    /// Test helper: force the global rollout to allow gated features
    /// regardless of segment/channel/percent. Idempotent — safe to call
    /// multiple times. Has no effect if a real `init()` already ran.
    #[doc(hidden)]
    pub fn init_for_tests_enable_all() {
        if INSTANCE.get().is_some() {
            return;
        }

        let mut features = HashMap::new();
        // Kas excluded: it flips the default engine.
        use strum::IntoEnumIterator;
        for feature in Feature::iter().filter(|f| !matches!(f, Feature::Kas)) {
            features.insert(<&str>::from(feature).to_string(), FeatureRollout {
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

    /// A rollout with no features enabled. Used as the fallback before [`init`]
    /// runs (see [`rollout`]).
    fn disabled() -> Self {
        Rollout {
            features: HashMap::new(),
            client_id: None,
            is_internal: false,
            is_nightly: false,
            is_insider_toolbox: false,
        }
    }

    /// Construct a rollout with explicit segment/channel state, reading the real
    /// embedded feature config. Lets callers exercise gating deterministically
    /// without touching the process-global instance.
    #[cfg(test)]
    pub fn new_for_test(is_internal: bool, is_nightly: bool) -> Self {
        Rollout {
            features: serde_json::from_str(EMBEDDED_CONFIG).unwrap_or_default(),
            client_id: Some(Uuid::from_u128(1)),
            is_internal,
            is_nightly,
            is_insider_toolbox: false,
        }
    }
}

/// Returns the process-global rollout instance set by [`Rollout::init`].
///
/// `init` is always called in [crate::os::Os::new], avoids reinstantiating
/// another [crate::os::Os::new].
pub fn rollout() -> &'static Rollout {
    static FALLBACK: OnceLock<Rollout> = OnceLock::new();
    INSTANCE
        .get()
        .unwrap_or_else(|| FALLBACK.get_or_init(Rollout::disabled))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_segment_start_url_prefers_token() {
        // Token's start URL wins even when the state-table value differs or is stale.
        assert_eq!(
            resolve_segment_start_url(
                Some(AMZN_START_URL.to_string()),
                Some("https://other.awsapps.com/start".into())
            ),
            Some(AMZN_START_URL.to_string())
        );
    }

    #[test]
    fn test_resolve_segment_start_url_falls_back_to_db() {
        assert_eq!(
            resolve_segment_start_url(None, Some(AMZN_START_URL.to_string())),
            Some(AMZN_START_URL.to_string())
        );
    }

    #[test]
    fn test_resolve_segment_start_url_none_when_both_absent() {
        assert_eq!(resolve_segment_start_url(None, None), None);
    }

    #[test]
    fn test_device_code_login_is_internal_from_token_only() {
        // Device-code login populates the token's start URL but not the
        // `auth.idc.start-url` state value. Internal detection must still hold.
        let start_url = resolve_segment_start_url(Some(AMZN_START_URL.to_string()), None);
        assert!(is_amzn_start_url(start_url.as_deref()));
    }

    #[test]
    fn test_is_amzn_start_url() {
        assert!(is_amzn_start_url(Some(AMZN_START_URL)));
        assert!(is_amzn_start_url(Some(&format!("  {AMZN_START_URL}  "))));
        assert!(!is_amzn_start_url(Some("https://view.awsapps.com/start")));
        assert!(!is_amzn_start_url(None));
    }

    #[test]
    fn test_embedded_config_parses() {
        let features: HashMap<String, FeatureRollout> = serde_json::from_str(EMBEDDED_CONFIG).unwrap();
        assert!(features.contains_key(<&str>::from(Feature::Tui)));
        assert!(features.contains_key(<&str>::from(Feature::Voice)));
        assert!(features.contains_key(<&str>::from(Feature::Workflows)));
        // NOTE: no `lite` key in rollout.json yet — it is added in the final
        // flag-flip PR, which keeps lite dark until then. The config-driven
        // `Feature::Lite` tests land with that PR.
        assert!(features.contains_key(<&str>::from(Feature::Test)));
        assert!(features.contains_key(<&str>::from(Feature::TestInternalOnly)));
        assert!(features.contains_key(<&str>::from(Feature::TestNightlyOnly)));
    }

    #[test]
    fn enabled_features_serializes_to_snake_case_json_array() {
        // Internal nightly user with everything at 100% except the dark
        // features; asserts the exact wire format the TUI parses from
        // KIRO_ENABLED_FEATURES.
        let r = Rollout {
            features: serde_json::from_str(EMBEDDED_CONFIG).unwrap(),
            client_id: Some(Uuid::from_u128(1)),
            is_internal: true,
            is_nightly: true,
            is_insider_toolbox: false,
        };
        let json = serde_json::to_string(&r.enabled_features()).unwrap();
        assert!(json.contains("\"voice\""), "voice should be enabled: {json}");
        assert!(json.contains("\"lite\""), "lite should be enabled: {json}");
        assert!(
            !json.contains("\"remote_sandbox\""),
            "remote_sandbox must stay dark: {json}"
        );
        assert!(!json.contains("\"workflows\""), "workflows must stay dark: {json}");
        assert!(
            json.contains("\"memory\""),
            "memory should be enabled for internal nightly: {json}"
        );
    }

    #[test]
    fn feature_names_agree_across_strum_serde_and_typeshare() {
        use strum::IntoEnumIterator;
        for f in Feature::iter() {
            let strum_name: &str = f.into();
            let serde_name = serde_json::to_string(&f).unwrap();
            assert_eq!(
                serde_name,
                format!("\"{strum_name}\""),
                "strum and serde renames drifted for {f:?}"
            );
        }
    }

    #[test]
    fn test_remote_sandbox_is_present_but_dark_in_all_real_builds() {
        // The rollout entry must exist (so it can be ramped later by editing
        // the percent + rebuilding)...
        let features: HashMap<String, FeatureRollout> = serde_json::from_str(EMBEDDED_CONFIG).unwrap();
        assert!(
            features.contains_key(<&str>::from(Feature::RemoteSandbox)),
            "remote_sandbox must be declared in rollout.json"
        );

        // ...but at treatment_percent 0 it is OFF for every real build:
        // external/stable, external/nightly, internal/stable, and even
        // internal/nightly. Only `init_for_tests_enable_all` (debug /
        // KIRO_TEST_MODE / E2E) turns it on. This is the dark-ship guarantee
        // that keeps `--cloud`/`--repo` unusable by live customers.
        for (is_internal, is_nightly) in [(false, false), (false, true), (true, false), (true, true)] {
            let r = Rollout::new_for_test(is_internal, is_nightly);
            assert!(
                !r.is_enabled(Feature::RemoteSandbox),
                "remote_sandbox must be dark for internal={is_internal}, nightly={is_nightly}"
            );
        }
    }

    #[test]
    fn test_workflows_is_present_but_dark_in_all_real_builds() {
        let features: HashMap<String, FeatureRollout> = serde_json::from_str(EMBEDDED_CONFIG).unwrap();
        assert!(
            features.contains_key(<&str>::from(Feature::Workflows)),
            "workflows must be declared in rollout.json"
        );

        for (is_internal, is_nightly) in [(false, false), (false, true), (true, false), (true, true)] {
            let rollout = Rollout::new_for_test(is_internal, is_nightly);
            assert!(
                !rollout.is_enabled(Feature::Workflows),
                "workflows must be dark for internal={is_internal}, nightly={is_nightly}"
            );
        }
    }

    #[test]
    fn test_memory_is_present_and_enabled_only_for_internal_nightly() {
        let features: HashMap<String, FeatureRollout> = serde_json::from_str(EMBEDDED_CONFIG).unwrap();
        assert!(
            features.contains_key(<&str>::from(Feature::Memory)),
            "memory must be declared in rollout.json"
        );

        for (is_internal, is_nightly, expected) in [
            (false, false, false),
            (false, true, false),
            (true, false, false),
            (true, true, true),
        ] {
            let r = Rollout::new_for_test(is_internal, is_nightly);
            assert_eq!(
                r.is_enabled(Feature::Memory),
                expected,
                "memory enabled={expected} for internal={is_internal}, nightly={is_nightly}"
            );
        }
    }

    #[test]
    fn test_external_user_sees_all_segment_but_not_internal() {
        let rollout = Rollout {
            features: serde_json::from_str(EMBEDDED_CONFIG).unwrap(),
            client_id: Some(Uuid::from_u128(1)),
            is_internal: false,
            is_nightly: true,
            is_insider_toolbox: false,
        };

        // test has segment=all, treatment_percent=100 → TREATMENT
        assert_eq!(rollout.variation(Feature::Test), Some(TREATMENT));

        // test_internal_only has segment=internal → None for external user
        assert_eq!(rollout.variation(Feature::TestInternalOnly), None);
    }

    #[test]
    fn test_internal_user_sees_both_segments() {
        let rollout = Rollout {
            features: serde_json::from_str(EMBEDDED_CONFIG).unwrap(),
            client_id: Some(Uuid::from_u128(1)),
            is_internal: true,
            is_nightly: true,
            is_insider_toolbox: false,
        };

        assert_eq!(rollout.variation(Feature::Test), Some(TREATMENT));
        assert_eq!(rollout.variation(Feature::TestInternalOnly), Some(TREATMENT));
    }

    #[test]
    fn test_nightly_channel_gate() {
        // Nightly build, internal user → sees nightly-only feature
        let rollout = Rollout {
            features: serde_json::from_str(EMBEDDED_CONFIG).unwrap(),
            client_id: Some(Uuid::from_u128(1)),
            is_internal: true,
            is_nightly: true,
            is_insider_toolbox: false,
        };
        assert_eq!(rollout.variation(Feature::TestNightlyOnly), Some(TREATMENT));

        // Stable build, internal user → does NOT see nightly-only feature
        let rollout_stable = Rollout {
            features: serde_json::from_str(EMBEDDED_CONFIG).unwrap(),
            client_id: Some(Uuid::from_u128(1)),
            is_internal: true,
            is_nightly: false,
            is_insider_toolbox: false,
        };
        assert_eq!(rollout_stable.variation(Feature::TestNightlyOnly), None);
    }

    #[test]
    fn test_voice_enabled_for_all_internal() {
        // Nightly + internal → voice enabled
        let rollout = Rollout {
            features: serde_json::from_str(EMBEDDED_CONFIG).unwrap(),
            client_id: Some(Uuid::from_u128(1)),
            is_internal: true,
            is_nightly: true,
            is_insider_toolbox: false,
        };
        assert_eq!(rollout.variation(Feature::Voice), Some(TREATMENT));

        // Stable + internal → voice enabled (no channel gate)
        let rollout_stable = Rollout {
            features: serde_json::from_str(EMBEDDED_CONFIG).unwrap(),
            client_id: Some(Uuid::from_u128(1)),
            is_internal: true,
            is_nightly: false,
            is_insider_toolbox: false,
        };
        assert_eq!(rollout_stable.variation(Feature::Voice), Some(TREATMENT));

        // Nightly + external → voice NOT enabled (segment=internal)
        let rollout_external = Rollout {
            features: serde_json::from_str(EMBEDDED_CONFIG).unwrap(),
            client_id: Some(Uuid::from_u128(1)),
            is_internal: false,
            is_nightly: true,
            is_insider_toolbox: false,
        };
        assert_eq!(rollout_external.variation(Feature::Voice), None);
    }

    // NOTE: the config-driven `test_lite_requires_internal_and_nightly` test
    // lives with the flag-flip PR that adds the `lite` key to rollout.json.
    // Until then there is no `lite` config entry, so `variation` only
    // resolves `Feature::Lite` via the insider-toolbox escape hatch (covered
    // by `insider_toolbox_enables_lite_*` above).

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

    // ── path_is_insider_toolbox: pure path parsing ────────────────────────

    #[test]
    fn path_insider_install_is_detected() {
        let p = std::path::PathBuf::from(
            "/Users/alice/.toolbox/tools/kiro-cli/2.5.0-insider/Kiro CLI.app/Contents/MacOS/kiro-cli",
        );
        assert!(path_is_insider_toolbox(&p));
    }

    #[test]
    fn path_stable_and_nightly_toolbox_installs_are_not_insider() {
        let stable =
            std::path::PathBuf::from("/Users/alice/.toolbox/tools/kiro-cli/2.5.0/Kiro CLI.app/Contents/MacOS/kiro-cli");
        let nightly = std::path::PathBuf::from(
            "/Users/alice/.toolbox/tools/kiro-cli/2.5.1-nightly.1-nightly/Kiro CLI.app/Contents/MacOS/kiro-cli",
        );
        assert!(!path_is_insider_toolbox(&stable));
        assert!(!path_is_insider_toolbox(&nightly));
    }

    #[test]
    fn path_outside_toolbox_or_insider_in_parent_dir_is_not_match() {
        let outside = std::path::PathBuf::from("/usr/local/bin/kiro-cli");
        // `insider` in a parent dir but NOT the version component must not match.
        let parent = std::path::PathBuf::from("/Users/insider-fan/.toolbox/tools/kiro-cli/2.5.0/bin/kiro-cli");
        assert!(!path_is_insider_toolbox(&outside));
        assert!(!path_is_insider_toolbox(&parent));
    }

    // ── variation: Feature::Lite via the insider toolbox branch ──────

    #[test]
    fn insider_toolbox_enables_lite_for_external_stable_user() {
        // External + stable would normally fail Lite's segment+channel gate.
        // With is_insider_toolbox = true, lite is enabled anyway.
        let r = Rollout {
            features: serde_json::from_str(EMBEDDED_CONFIG).unwrap(),
            client_id: Some(Uuid::from_u128(1)),
            is_internal: false,
            is_nightly: false,
            is_insider_toolbox: true,
        };
        assert_eq!(r.variation(Feature::Lite), Some(TREATMENT));
    }

    #[test]
    fn insider_toolbox_does_not_enable_voice_or_tui() {
        // Scope guarantee: the insider branch is gated on matches!(Feature::Lite),
        // so Voice/Tui resolution is unchanged for an external+stable user.
        let r = Rollout {
            features: serde_json::from_str(EMBEDDED_CONFIG).unwrap(),
            client_id: Some(Uuid::from_u128(1)),
            is_internal: false,
            is_nightly: false,
            is_insider_toolbox: true,
        };
        assert_eq!(r.variation(Feature::Voice), None);
        assert_eq!(r.variation(Feature::Tui), None);
    }
}
