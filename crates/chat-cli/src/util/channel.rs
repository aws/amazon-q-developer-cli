//! Single owner of "which version am I" and "which release channel am I".
//!
//! Several subsystems gate behavior on the release channel (changelog feed
//! fetching, feature rollouts). They must all agree, and they must all honor
//! `KIRO_VERSION_OVERRIDE` so version-gated behavior is testable against a
//! real binary without a release build.

use crate::util::consts::env_var::KIRO_VERSION_OVERRIDE;

/// Release channel, derived from the effective version's prerelease
/// component (`2.13.2` → Stable, `2.13.2-nightly.3` → Nightly,
/// `2.13.2-rc.1` → Rc, `2.13.2-fix-foo.1` → Other("fix-foo"),
/// `99.99.99-dev` → Dev).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Channel {
    Stable,
    Nightly,
    Rc,
    /// Local developer build (`-dev` prerelease). Kept distinct from
    /// `Other` so local builds do not inherit released-channel behavior
    /// such as remote feed fetching.
    Dev,
    /// Feature-branch build (`-<feature-name>.N` prerelease).
    Other(String),
}

/// Effective CLI version string: `KIRO_VERSION_OVERRIDE` when set and
/// non-empty, else the compile-time crate version.
pub fn cli_version_string() -> String {
    if let Ok(version) = std::env::var(KIRO_VERSION_OVERRIDE) {
        let version = version.trim();
        if !version.is_empty() {
            return version.to_string();
        }
    }
    env!("CARGO_PKG_VERSION").to_string()
}

/// Effective CLI version as semver. An unparseable override falls back to
/// the compile-time version; `None` only if that is unparseable too (dev
/// builds inject a well-formed placeholder, so this is effectively never).
pub fn cli_version() -> Option<semver::Version> {
    semver::Version::parse(&cli_version_string())
        .or_else(|_| semver::Version::parse(env!("CARGO_PKG_VERSION")))
        .ok()
}

/// Release channel of this process, from the effective version.
pub fn channel() -> Channel {
    let Some(version) = cli_version() else {
        return Channel::Stable;
    };
    let pre = version.pre.as_str();
    if pre.is_empty() {
        return Channel::Stable;
    }
    // Exact match on the first dot-separated prerelease identifier
    // ("nightly.3" -> "nightly"), so a feature branch named e.g. `dev-tools`
    // or `rc-tweak` classifies as Other rather than Dev/Rc.
    match pre.split('.').next().unwrap_or(pre) {
        "nightly" => Channel::Nightly,
        "rc" => Channel::Rc,
        "dev" => Channel::Dev,
        other => Channel::Other(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_from_version() {
        // Serialized with other env-mutating tests via the shared lock.
        let _guard = crate::util::paths::ENV_MUTATION_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        let cases = [
            ("2.13.2", Channel::Stable),
            ("2.13.2-nightly.3", Channel::Nightly),
            ("2.13.2-rc.1", Channel::Rc),
            ("2.13.1-fix-foo.3", Channel::Other("fix-foo".to_string())),
            ("99.99.99-dev", Channel::Dev),
            ("0.0.0-dev", Channel::Dev),
            // Feature branches whose name shares a released-channel prefix
            // must not inherit that channel's behavior.
            ("2.13.1-dev-tools.1", Channel::Other("dev-tools".to_string())),
            ("2.13.1-rc-tweak.1", Channel::Other("rc-tweak".to_string())),
            ("2.13.1-nightly-fix.1", Channel::Other("nightly-fix".to_string())),
        ];
        for (version, expected) in cases {
            unsafe { std::env::set_var(KIRO_VERSION_OVERRIDE, version) };
            assert_eq!(channel(), expected, "for version {version}");
            assert_eq!(cli_version().unwrap().to_string(), version);
        }

        // Unparseable override falls back to the compile-time version.
        unsafe { std::env::set_var(KIRO_VERSION_OVERRIDE, "not-a-version") };
        assert_eq!(cli_version().unwrap().to_string(), env!("CARGO_PKG_VERSION"));

        unsafe { std::env::remove_var(KIRO_VERSION_OVERRIDE) };
        assert_eq!(cli_version_string(), env!("CARGO_PKG_VERSION"));
    }
}
