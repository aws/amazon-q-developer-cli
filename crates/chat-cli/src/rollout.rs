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

use crate::util::consts::env_var::KIRO_ROLLOUT_FORCE_INTERNAL;

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
    /// `/tangent` named side-conversations. Generally available via
    /// `rollout.json`; the launcher includes `tangent` in
    /// `KIRO_ENABLED_FEATURES` and the TUI registers the `/tangent` command
    /// only when enabled (treatment_percent is the kill-switch).
    Tangent,
    /// Remote changelog feed fetch on stable builds (prod URL). Nightly
    /// builds always fetch from gamma (unconditional); this gate extends
    /// the same fetch behavior to stable builds reading the prod feed.
    /// Note the rollout `channel: stable` only excludes nightly, so this
    /// gate also reports enabled on rc/feature builds — the feed code's own
    /// channel match is what keeps those from fetching.
    RemoteChangelog,
    /// Auto-upgrade of V2-only agent configs to the universal format when
    /// launching the V3/KAS engine. Internal nightly only for now; the launcher
    /// only runs the migration prompt/scan when this is enabled.
    AutoAgentUpgrade,
    /// Cloud config: the consolidated `/config` panel and cloud/local source
    /// labels on config listings (`/mcp` Source column), and the launcher
    /// passing the stage-default cloud-config BFF endpoint to KAS for local
    /// V3 sessions. Turning this off hides the UI and stops the launcher
    /// supplying that stage default, so KAS makes no cloud-config sync calls
    /// (steering/agents/skills/hooks) unless the user explicitly sets a
    /// trusted-host endpoint override, which applies regardless of the
    /// rollout. Ramped to all internal users on every channel; external
    /// users stay dark.
    CloudConfig,
    /// Session dashboard: the full-screen `/sessions` browser, the `--sessions`
    /// launch flag, and (on v3) routing `--resume-picker` into the dashboard.
    /// Nightly-only preview (`channel: nightly` in `rollout.json`); the launcher
    /// includes `session_dashboard` in `KIRO_ENABLED_FEATURES` and both the Rust
    /// launch gate and the TUI honor it.
    SessionDashboard,
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
        // Test/dev eligibility override: makes this process internal-eligible
        // for EVERY internal-segment feature (and supplies a deterministic
        // bucketing identity when the store has no telemetry id). Narrower
        // than KIRO_TEST_MODE only in that rollout.json's channel and
        // treatment_percent still apply per feature.
        let force_internal = std::env::var(KIRO_ROLLOUT_FORCE_INTERNAL).is_ok();
        let is_internal = is_amzn_start_url(start_url.as_deref()) || force_internal;
        let client_id = client_id.or_else(|| force_internal.then(Uuid::nil));
        let is_nightly = crate::util::channel::channel() == crate::util::channel::Channel::Nightly;
        let _ = INSTANCE.set(Rollout {
            features,
            client_id,
            is_internal,
            is_nightly,
        });
    }

    /// Returns the variation for this rollout state in the given experiment.
    ///
    /// - `Some(TREATMENT)` — gets the new behavior
    /// - `Some(CONTROL)` — in the experiment but gets the default behavior
    /// - `None` — not in the experiment (wrong segment/channel, feature absent, or no client_id at
    ///   a partial percent)
    pub fn variation(&self, feature: Feature) -> Option<&'static str> {
        let config = self.features.get(<&str>::from(feature))?;
        if config.segment == Segment::Internal && !self.is_internal {
            return None;
        }
        match config.channel {
            Channel::Nightly if !self.is_nightly => return None,
            Channel::Stable if self.is_nightly => return None,
            _ => {},
        }
        // Fully ramped features need no bucketing id: every bucket is
        // treatment. This is what lets users without a persisted client id
        // (telemetry opted out) receive features at GA.
        if config.treatment_percent >= 100 {
            return Some(TREATMENT);
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
        assert!(features.contains_key(<&str>::from(Feature::Lite)));
        assert!(features.contains_key(<&str>::from(Feature::Workflows)));
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
        };
        let json = serde_json::to_string(&r.enabled_features()).unwrap();
        assert!(json.contains("\"voice\""), "voice should be enabled: {json}");
        assert!(
            json.contains("\"lite\""),
            "lite should be enabled for internal (any channel): {json}"
        );
        assert!(
            json.contains("\"remote_sandbox\""),
            "remote_sandbox should be enabled for all users: {json}"
        );
        assert!(
            json.contains("\"workflows\""),
            "workflows should be enabled for internal nightly: {json}"
        );
        assert!(
            json.contains("\"memory\""),
            "memory should be enabled for internal nightly: {json}"
        );
        assert!(
            json.contains("\"cloud_config\""),
            "cloud_config should be enabled for internal nightly: {json}"
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
    fn test_remote_sandbox_enabled_for_all_users_any_channel() {
        let features: HashMap<String, FeatureRollout> = serde_json::from_str(EMBEDDED_CONFIG).unwrap();
        assert!(
            features.contains_key(<&str>::from(Feature::RemoteSandbox)),
            "remote_sandbox must be declared in rollout.json"
        );

        // Ramped to every user on every channel (segment: all, 100%).
        // treatment_percent in rollout.json is the kill-switch: dialing it to
        // 0 re-darkens `--cloud`/`--repo` in the next release.
        for (is_internal, is_nightly) in [(false, false), (false, true), (true, false), (true, true)] {
            let r = Rollout::new_for_test(is_internal, is_nightly);
            assert!(
                r.is_enabled(Feature::RemoteSandbox),
                "remote_sandbox must be enabled for internal={is_internal}, nightly={is_nightly}"
            );
        }
    }

    #[test]
    fn test_remote_changelog_enabled_for_all_stable() {
        let features: HashMap<String, FeatureRollout> = serde_json::from_str(EMBEDDED_CONFIG).unwrap();
        assert!(
            features.contains_key(<&str>::from(Feature::RemoteChangelog)),
            "remote_changelog must be declared in rollout.json"
        );

        // GA: every stable user, internal or external. Nightly stays outside
        // the gate (it fetches gamma unconditionally, and the rollout's
        // channel: stable excludes it).
        for (is_internal, is_nightly, expected) in [
            (false, false, true),
            (false, true, false),
            (true, false, true),
            (true, true, false),
        ] {
            let r = Rollout::new_for_test(is_internal, is_nightly);
            assert_eq!(
                r.is_enabled(Feature::RemoteChangelog),
                expected,
                "remote_changelog enabled={expected} for internal={is_internal}, nightly={is_nightly}"
            );
        }
    }

    #[test]
    fn test_cloud_config_enabled_for_all_internal_any_channel() {
        let features: HashMap<String, FeatureRollout> = serde_json::from_str(EMBEDDED_CONFIG).unwrap();
        assert!(
            features.contains_key(<&str>::from(Feature::CloudConfig)),
            "cloud_config must be declared in rollout.json"
        );

        // Ramped to all internal users on every channel. External users must
        // stay dark on any channel — that is the guarantee that keeps the
        // /config surface and the /mcp Source column invisible to live
        // customers.
        for (is_internal, is_nightly, expected) in [
            (false, false, false),
            (false, true, false),
            (true, false, true),
            (true, true, true),
        ] {
            let r = Rollout::new_for_test(is_internal, is_nightly);
            assert_eq!(
                r.is_enabled(Feature::CloudConfig),
                expected,
                "cloud_config enabled={expected} for internal={is_internal}, nightly={is_nightly}"
            );
        }
    }

    #[test]
    fn test_cloud_config_kill_switch_darkens_internal() {
        // Dialing treatment_percent to 0 in a follow-up release must
        // re-darken the feature for the ramped (internal) cohort — that is
        // the kill-switch contract for the ramp. Starts from the shipped
        // config so the segment/channel under test cannot drift from it.
        let mut features: HashMap<String, FeatureRollout> = serde_json::from_str(EMBEDDED_CONFIG).unwrap();
        features
            .get_mut(<&str>::from(Feature::CloudConfig))
            .expect("cloud_config must be declared in rollout.json")
            .treatment_percent = 0;
        let r = Rollout {
            features,
            client_id: Some(Uuid::from_u128(1)),
            is_internal: true,
            is_nightly: false,
        };
        // With a client id: bucketed into the experiment but gets CONTROL.
        assert!(!r.is_enabled(Feature::CloudConfig));
        assert_eq!(r.variation(Feature::CloudConfig), Some(CONTROL));
        // Without a client id (telemetry opted out): no bucketing is
        // possible at a partial percent, so the user is out entirely.
        let no_id = Rollout { client_id: None, ..r };
        assert!(!no_id.is_enabled(Feature::CloudConfig));
        assert_eq!(no_id.variation(Feature::CloudConfig), None);
    }

    #[test]
    fn test_cloud_config_fully_ramped_needs_no_client_id() {
        // Internal users with telemetry opted out (no persisted client id)
        // must still receive the feature at 100%: bucketing only matters for
        // partial ramps.
        let r = Rollout {
            client_id: None,
            ..Rollout::new_for_test(true, false)
        };
        assert_eq!(r.variation(Feature::CloudConfig), Some(TREATMENT));
    }

    #[test]
    fn test_fully_ramped_features_need_no_client_id() {
        // Users without a persisted client id (telemetry opted out) must
        // still receive features at 100%: bucketing is only meaningful for
        // partial ramps. remote_changelog at GA is the motivating case.
        let r = Rollout {
            client_id: None,
            ..Rollout::new_for_test(false, false)
        };
        assert_eq!(r.variation(Feature::RemoteChangelog), Some(TREATMENT));
        assert_eq!(r.variation(Feature::Tangent), Some(TREATMENT));
    }

    #[test]
    fn test_partial_ramp_stays_dark_without_client_id() {
        // Partial percentages cannot bucket without an id, so those users
        // stay out of the experiment entirely (None, not CONTROL).
        let mut features = HashMap::new();
        features.insert("test".to_string(), FeatureRollout {
            description: String::new(),
            treatment_percent: 50,
            segment: Segment::All,
            channel: Channel::All,
        });
        let no_id = Rollout {
            features: features.clone(),
            client_id: None,
            is_internal: true,
            is_nightly: true,
        };
        assert_eq!(no_id.variation(Feature::Test), None);
        // With an id the same state is in the experiment.
        let with_id = Rollout {
            client_id: Some(Uuid::from_u128(1)),
            ..no_id
        };
        assert!(with_id.variation(Feature::Test).is_some());
    }

    #[test]
    fn test_workflows_enabled_for_internal_nightly() {
        let features: HashMap<String, FeatureRollout> = serde_json::from_str(EMBEDDED_CONFIG).unwrap();
        assert!(
            features.contains_key(<&str>::from(Feature::Workflows)),
            "workflows must be declared in rollout.json"
        );

        // Enabled for internal nightly only.
        for (is_internal, is_nightly, expected) in [
            (false, false, false),
            (false, true, false),
            (true, false, false),
            (true, true, true),
        ] {
            let rollout = Rollout::new_for_test(is_internal, is_nightly);
            assert_eq!(
                rollout.is_enabled(Feature::Workflows),
                expected,
                "workflows enabled={expected} for internal={is_internal}, nightly={is_nightly}"
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
    fn test_auto_agent_upgrade_enabled_only_for_internal_nightly() {
        let features: HashMap<String, FeatureRollout> = serde_json::from_str(EMBEDDED_CONFIG).unwrap();
        assert!(
            features.contains_key(<&str>::from(Feature::AutoAgentUpgrade)),
            "auto_agent_upgrade must be declared in rollout.json"
        );

        // segment=internal, channel=nightly: only internal nightly users get it;
        // external (any channel) and stable builds stay dark.
        for (is_internal, is_nightly, expected) in [
            (false, false, false),
            (false, true, false),
            (true, false, false),
            (true, true, true),
        ] {
            let r = Rollout::new_for_test(is_internal, is_nightly);
            assert_eq!(
                r.is_enabled(Feature::AutoAgentUpgrade),
                expected,
                "auto_agent_upgrade enabled={expected} for internal={is_internal}, nightly={is_nightly}"
            );
        }
    }

    #[test]
    fn test_session_dashboard_enabled_only_on_nightly() {
        let features: HashMap<String, FeatureRollout> = serde_json::from_str(EMBEDDED_CONFIG).unwrap();
        assert!(
            features.contains_key(<&str>::from(Feature::SessionDashboard)),
            "session_dashboard must be declared in rollout.json"
        );

        // channel=nightly, segment=all: every nightly user (internal or not)
        // gets it; stable/rc builds stay dark.
        for (is_internal, is_nightly, expected) in [
            (false, false, false),
            (false, true, true),
            (true, false, false),
            (true, true, true),
        ] {
            let r = Rollout::new_for_test(is_internal, is_nightly);
            assert_eq!(
                r.is_enabled(Feature::SessionDashboard),
                expected,
                "session_dashboard enabled={expected} for internal={is_internal}, nightly={is_nightly}"
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
        };
        assert_eq!(rollout.variation(Feature::TestNightlyOnly), Some(TREATMENT));

        // Stable build, internal user → does NOT see nightly-only feature
        let rollout_stable = Rollout {
            features: serde_json::from_str(EMBEDDED_CONFIG).unwrap(),
            client_id: Some(Uuid::from_u128(1)),
            is_internal: true,
            is_nightly: false,
        };
        assert_eq!(rollout_stable.variation(Feature::TestNightlyOnly), None);
    }

    #[test]
    fn test_voice_enabled_for_all_segments_and_channels() {
        // Voice is GA (segment=all, 100%): every combination of segment and
        // channel gets TREATMENT.
        for (is_internal, is_nightly) in [(false, false), (false, true), (true, false), (true, true)] {
            let rollout = Rollout::new_for_test(is_internal, is_nightly);
            assert_eq!(
                rollout.variation(Feature::Voice),
                Some(TREATMENT),
                "voice should be enabled for internal={is_internal}, nightly={is_nightly}"
            );
        }
    }

    #[test]
    fn test_lite_requires_internal_any_channel() {
        for (is_internal, is_nightly, expected) in [
            (false, false, false),
            (false, true, false),
            (true, false, true),
            (true, true, true),
        ] {
            let rollout = Rollout::new_for_test(is_internal, is_nightly);
            assert_eq!(
                rollout.is_enabled(Feature::Lite),
                expected,
                "lite enabled={expected} for internal={is_internal}, nightly={is_nightly}"
            );
        }
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
