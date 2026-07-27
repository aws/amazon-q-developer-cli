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
    managed_base_url_from(platform::read_managed_string(KEY_UPDATE_BASE_URL))
}

/// Core filter for managed values: only forced values are honored. Separated from the
/// platform read so tests exercise the same code path as [`managed_base_url`].
fn managed_base_url_from(val: Option<ManagedValue>) -> Option<String> {
    let val = val?;
    if val.forced { Some(val.value) } else { None }
}

/// Returns `true` if MDM enforcement should apply.
///
/// `forced_present` is whether a forced managed value is present, as determined by an
/// earlier [`managed_base_url`] call — passing it in avoids a second read of the OS
/// policy store that could disagree with the first.
///
/// In release builds, enforcement is active whenever a forced managed value is
/// present. In debug builds, enforcement is disabled so developers can override
/// via env vars even on managed machines.
pub fn enforcement_active(forced_present: bool) -> bool {
    enforcement_active_inner(cfg!(debug_assertions), forced_present)
}

/// Pure enforcement logic, separated so the release-build branch is testable.
fn enforcement_active_inner(debug_build: bool, forced_present: bool) -> bool {
    if debug_build { false } else { forced_present }
}

#[cfg(test)]
mod tests;
