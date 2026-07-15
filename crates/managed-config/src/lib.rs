//! Read MDM-managed configuration values from OS-native policy stores.
//!
//! On macOS, reads from managed preferences (CFPreferences) under the `dev.kiro.cli` domain.
//! On Windows, reads from `HKLM\SOFTWARE\Policies\Kiro\CLI`.
//! On Linux, returns `None` (no OS-native managed config surface).

cfg_if::cfg_if! {
    if #[cfg(target_os = "macos")] {
        mod macos;
        use macos as platform;
    } else if #[cfg(target_os = "windows")] {
        mod windows;
        use windows as platform;
    } else {
        mod noop;
        use noop as platform;
    }
}

/// The managed key holding the update/download base URL.
const KEY_UPDATE_BASE_URL: &str = "update.baseUrl";

/// A managed configuration value read from the OS policy store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedValue {
    /// The string value.
    pub value: String,
    /// Whether the OS reports this value as forced (non-overridable by the user).
    pub forced: bool,
}

/// Read the managed `update.baseUrl` value, if set by enterprise policy.
///
/// Returns `Some` only if the value is present AND forced (i.e., set by MDM/GPO
/// and not overridable by the user). A value that exists but is not forced is
/// treated as absent — this prevents spoofing via user-level preferences on macOS.
pub fn managed_base_url() -> Option<String> {
    let val = platform::read_managed_string(KEY_UPDATE_BASE_URL)?;
    if val.forced { Some(val.value) } else { None }
}

/// Returns `true` if MDM enforcement should apply.
///
/// In release builds, enforcement is active whenever a forced managed value is
/// present. In debug builds, enforcement is disabled so developers can override
/// via env vars even on managed machines.
pub fn enforcement_active() -> bool {
    if cfg!(debug_assertions) {
        false
    } else {
        managed_base_url().is_some()
    }
}

#[cfg(test)]
mod tests;
