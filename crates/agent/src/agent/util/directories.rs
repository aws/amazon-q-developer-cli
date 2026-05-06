use std::path::PathBuf;

use super::error::UtilError;

type Result<T, E = UtilError> = std::result::Result<T, E>;

/// Environment variable that overrides `~/.kiro` as the root for user-level
/// Kiro config data. When set, all global Kiro paths (agents, prompts,
/// steering, sessions, etc.) resolve relative to `$KIRO_HOME` instead.
pub const KIRO_HOME_ENV: &str = "KIRO_HOME";

pub fn home_dir() -> Result<PathBuf, UtilError> {
    dirs::home_dir().ok_or(UtilError::MissingHomeDir)
}

/// Root directory for user-level Kiro config data, honoring `KIRO_HOME`.
///
/// Falls back to `$HOME/.kiro` when `KIRO_HOME` is unset or empty.
pub fn kiro_home_dir() -> Result<PathBuf, UtilError> {
    if let Ok(dir) = std::env::var(KIRO_HOME_ENV)
        && !dir.is_empty()
    {
        return Ok(PathBuf::from(dir));
    }
    Ok(home_dir()?.join(".kiro"))
}

/// Like [`kiro_home_dir`] but rooted at a caller-supplied home directory.
/// Used by `SystemProvider`-based code paths that inject a fake home for
/// testing; production callers should prefer [`kiro_home_dir`].
pub fn kiro_home_dir_in(home: &std::path::Path) -> PathBuf {
    if let Ok(dir) = std::env::var(KIRO_HOME_ENV)
        && !dir.is_empty()
    {
        return PathBuf::from(dir);
    }
    home.join(".kiro")
}

#[cfg(test)]
mod tests {
    //! Tests for the `KIRO_HOME` override behavior.
    //!
    //! These mutate the process environment and therefore must not run in
    //! parallel with each other (or with other tests that read `KIRO_HOME`).
    //! The `serial_test::serial` attribute and a single `_guard` mutex ensure
    //! single-threaded execution across this module; we also restore any
    //! previously-set value on drop.

    use std::path::Path;
    use std::sync::Mutex;

    use super::*;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        key: &'static str,
        prev: Option<String>,
        // Lock released on drop. Held so ENV mutations and reads in this module
        // never race with each other even under `--test-threads` > 1.
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let prev = std::env::var(key).ok();
            // SAFETY: guarded by ENV_LOCK to serialize all KIRO_HOME writes
            // within this module.
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, prev, _lock: lock }
        }

        fn unset(key: &'static str) -> Self {
            let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let prev = std::env::var(key).ok();
            unsafe {
                std::env::remove_var(key);
            }
            Self { key, prev, _lock: lock }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                match &self.prev {
                    Some(v) => std::env::set_var(self.key, v),
                    None => std::env::remove_var(self.key),
                }
            }
        }
    }

    #[test]
    fn test_kiro_home_dir_in_fallback() {
        let _g = EnvGuard::unset(KIRO_HOME_ENV);
        let home = Path::new("/home/testuser");
        // Join uses the native separator, so comparisons here match the
        // platform's convention.
        assert_eq!(kiro_home_dir_in(home), home.join(".kiro"));
    }

    #[test]
    fn test_kiro_home_dir_in_empty_falls_back() {
        let _g = EnvGuard::set(KIRO_HOME_ENV, "");
        let home = Path::new("/home/testuser");
        assert_eq!(kiro_home_dir_in(home), home.join(".kiro"));
    }

    #[test]
    fn test_kiro_home_dir_in_override() {
        let _g = EnvGuard::set(KIRO_HOME_ENV, "/custom/kiro");
        let home = Path::new("/home/testuser");
        assert_eq!(kiro_home_dir_in(home), PathBuf::from("/custom/kiro"));
    }

    #[test]
    fn test_kiro_home_dir_override() {
        let _g = EnvGuard::set(KIRO_HOME_ENV, "/custom/kiro");
        assert_eq!(kiro_home_dir().unwrap(), PathBuf::from("/custom/kiro"));
    }
}
