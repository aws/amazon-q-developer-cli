//! Version comparison functionality for the update command.
//!
//! Uses semantic versioning to compare the current installed version
//! against the latest available version from the manifest.

use std::cmp::Ordering;

use semver::Version;

use super::UpdateError;

/// Compares semantic version strings.
pub struct VersionComparator;

impl VersionComparator {
    /// Compare current version against latest version.
    ///
    /// Returns `Ordering::Less` if an update is available (current < latest),
    /// `Ordering::Equal` if versions match, and `Ordering::Greater` if current
    /// is newer than latest.
    ///
    /// # Errors
    ///
    /// Returns `UpdateError::InvalidVersion` if either version string is not
    /// a valid semantic version.
    pub fn compare(current: &str, latest: &str) -> Result<Ordering, UpdateError> {
        let current_ver =
            Version::parse(current).map_err(|_parse_err| UpdateError::InvalidVersion(current.to_string()))?;
        let latest_ver =
            Version::parse(latest).map_err(|_parse_err| UpdateError::InvalidVersion(latest.to_string()))?;

        Ok(current_ver.cmp(&latest_ver))
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use super::*;

    // =========================================================================
    // Unit Tests for Version Comparison Edge Cases
    // Requirements: 5.1, 5.4
    // =========================================================================

    #[test]
    fn test_compare_less_than() {
        assert_eq!(VersionComparator::compare("1.0.0", "1.0.1").unwrap(), Ordering::Less);
        assert_eq!(VersionComparator::compare("1.0.0", "1.1.0").unwrap(), Ordering::Less);
        assert_eq!(VersionComparator::compare("1.0.0", "2.0.0").unwrap(), Ordering::Less);
    }

    #[test]
    fn test_compare_greater_than() {
        assert_eq!(VersionComparator::compare("2.0.0", "1.9.9").unwrap(), Ordering::Greater);
        assert_eq!(VersionComparator::compare("1.1.0", "1.0.9").unwrap(), Ordering::Greater);
        assert_eq!(VersionComparator::compare("1.0.1", "1.0.0").unwrap(), Ordering::Greater);
    }

    #[test]
    fn test_compare_equal() {
        assert_eq!(VersionComparator::compare("1.0.0", "1.0.0").unwrap(), Ordering::Equal);
        assert_eq!(VersionComparator::compare("2.5.10", "2.5.10").unwrap(), Ordering::Equal);
    }

    #[test]
    fn test_major_version_precedence() {
        // Major version takes precedence over minor and patch
        assert_eq!(VersionComparator::compare("1.99.99", "2.0.0").unwrap(), Ordering::Less);
        assert_eq!(
            VersionComparator::compare("2.0.0", "1.99.99").unwrap(),
            Ordering::Greater
        );
    }

    #[test]
    fn test_minor_version_precedence() {
        // Minor version takes precedence over patch
        assert_eq!(VersionComparator::compare("1.0.99", "1.1.0").unwrap(), Ordering::Less);
        assert_eq!(
            VersionComparator::compare("1.1.0", "1.0.99").unwrap(),
            Ordering::Greater
        );
    }

    #[test]
    fn test_invalid_version_current() {
        let result = VersionComparator::compare("not-a-version", "1.0.0");
        assert!(matches!(result, Err(UpdateError::InvalidVersion(v)) if v == "not-a-version"));
    }

    #[test]
    fn test_invalid_version_latest() {
        let result = VersionComparator::compare("1.0.0", "invalid");
        assert!(matches!(result, Err(UpdateError::InvalidVersion(v)) if v == "invalid"));
    }

    #[test]
    fn test_invalid_version_empty() {
        let result = VersionComparator::compare("", "1.0.0");
        assert!(matches!(result, Err(UpdateError::InvalidVersion(v)) if v.is_empty()));
    }

    #[test]
    fn test_invalid_version_partial() {
        // Partial versions are invalid in strict semver
        let result = VersionComparator::compare("1.0", "1.0.0");
        assert!(matches!(result, Err(UpdateError::InvalidVersion(v)) if v == "1.0"));
    }
}
