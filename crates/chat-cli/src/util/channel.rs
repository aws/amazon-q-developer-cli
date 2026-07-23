//! Single owner of "which version am I" and "which release channel am I".
//!
//! Several subsystems gate behavior on the release channel (changelog feed
//! fetching, feature rollouts). They must all agree, and they must all honor
//! `KIRO_VERSION_OVERRIDE` so version-gated behavior is testable against a
//! real binary without a release build.

use crate::util::consts::env_var::KIRO_VERSION_OVERRIDE;

/// Release channel, derived from the effective version's prerelease
/// component (`2.13.2` → Stable, `2.13.2-nightly.3` → Nightly,
/// `2.13.2-rc.1` → Rc, `2.13.2-fix-foo.1` → Other("fix-foo")).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Channel {
    Stable,
    Nightly,
    Rc,
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
        Channel::Stable
    } else if pre.starts_with("nightly") {
        Channel::Nightly
    } else if pre.starts_with("rc") {
        Channel::Rc
    } else {
        Channel::Other(pre.split('.').next().unwrap_or(pre).to_string())
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
            ("99.99.99-dev", Channel::Other("dev".to_string())),
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
