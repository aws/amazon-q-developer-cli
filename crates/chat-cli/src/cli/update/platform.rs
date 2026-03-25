//! Platform detection for the update system.
//!
//! This module provides functionality to detect the current operating system
//! and map it to the appropriate manifest key for downloading installers.

use super::UpdateError;

/// Supported platforms for the Kiro CLI
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Linux,
    MacOS,
    Windows,
}

impl Platform {
    /// Detect the current platform based on the operating system.
    ///
    /// Returns an error if the platform is not supported.
    pub fn detect() -> Result<Self, UpdateError> {
        Self::from_os_str(std::env::consts::OS)
    }

    /// Parse a platform from an OS string.
    ///
    /// This is separated from `detect()` to allow testing with arbitrary OS strings.
    pub(crate) fn from_os_str(os: &str) -> Result<Self, UpdateError> {
        match os {
            "linux" => Ok(Platform::Linux),
            "macos" => Ok(Platform::MacOS),
            "windows" => Ok(Platform::Windows),
            other => Err(UpdateError::UnsupportedPlatform(other.to_string())),
        }
    }

    /// Get the manifest key for this platform.
    ///
    /// This key is used to look up platform-specific download information
    /// in the version manifest.
    pub fn manifest_key(&self) -> &'static str {
        match self {
            Platform::Linux => "linux",
            Platform::MacOS => "macos",
            Platform::Windows => "windows",
        }
    }

    /// Get the OS name as used in the artifact manifest.
    pub fn os_name(&self) -> &'static str {
        self.manifest_key()
    }

    /// Get the CPU architecture string for the current platform.
    pub fn architecture() -> &'static str {
        std::env::consts::ARCH
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Unit tests for manifest_key() returning correct strings
    #[test]
    fn test_manifest_key_linux() {
        assert_eq!(Platform::Linux.manifest_key(), "linux");
    }

    #[test]
    fn test_manifest_key_macos() {
        assert_eq!(Platform::MacOS.manifest_key(), "macos");
    }

    #[test]
    fn test_manifest_key_windows() {
        assert_eq!(Platform::Windows.manifest_key(), "windows");
    }

    // Unit tests for platform detection from OS strings
    #[test]
    fn test_from_os_str_linux() {
        let platform = Platform::from_os_str("linux").unwrap();
        assert_eq!(platform, Platform::Linux);
    }

    #[test]
    fn test_from_os_str_macos() {
        let platform = Platform::from_os_str("macos").unwrap();
        assert_eq!(platform, Platform::MacOS);
    }

    #[test]
    fn test_from_os_str_windows() {
        let platform = Platform::from_os_str("windows").unwrap();
        assert_eq!(platform, Platform::Windows);
    }

    // Test error case for unsupported platform
    #[test]
    fn test_from_os_str_unsupported() {
        let result = Platform::from_os_str("freebsd");
        assert!(result.is_err());
        match result {
            Err(UpdateError::UnsupportedPlatform(os)) => {
                assert_eq!(os, "freebsd");
            },
            _ => panic!("Expected UnsupportedPlatform error"),
        }
    }

    #[test]
    fn test_from_os_str_unsupported_unknown() {
        let result = Platform::from_os_str("unknown_os");
        assert!(result.is_err());
        match result {
            Err(UpdateError::UnsupportedPlatform(os)) => {
                assert_eq!(os, "unknown_os");
            },
            _ => panic!("Expected UnsupportedPlatform error"),
        }
    }

    // Test that detect() works on the current platform
    #[test]
    fn test_detect_current_platform() {
        // This test verifies that detect() works on the current platform
        // It should succeed on Linux, macOS, or Windows
        let result = Platform::detect();

        #[cfg(target_os = "linux")]
        assert_eq!(result.unwrap(), Platform::Linux);

        #[cfg(target_os = "macos")]
        assert_eq!(result.unwrap(), Platform::MacOS);

        #[cfg(target_os = "windows")]
        assert_eq!(result.unwrap(), Platform::Windows);
    }

    // Test round-trip: from_os_str -> manifest_key should return the same string
    #[test]
    fn test_manifest_key_roundtrip() {
        for os in &["linux", "macos", "windows"] {
            let platform = Platform::from_os_str(os).unwrap();
            assert_eq!(platform.manifest_key(), *os);
        }
    }

    // Test Platform enum derives
    #[test]
    fn test_platform_clone() {
        let platform = Platform::Linux;
        let cloned = platform;
        assert_eq!(platform, cloned);
    }

    #[test]
    fn test_platform_debug() {
        let platform = Platform::MacOS;
        let debug_str = format!("{:?}", platform);
        assert_eq!(debug_str, "MacOS");
    }
}
