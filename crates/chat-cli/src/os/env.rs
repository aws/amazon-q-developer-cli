use std::collections::HashMap;
use std::env::{
    self,
    VarError,
};
use std::ffi::{
    OsStr,
    OsString,
};
use std::io;
use std::path::PathBuf;
use std::sync::{
    Arc,
    Mutex,
};

use crate::os::ACTIVE_USER_HOME;

#[derive(Debug, Clone)]
pub struct Env(inner::Inner);

mod inner {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{
        Arc,
        Mutex,
    };

    #[derive(Debug, Clone)]
    pub(super) enum Inner {
        Real,
        Fake(Arc<Mutex<Fake>>),
    }

    #[derive(Debug, Clone)]
    pub(super) struct Fake {
        pub vars: HashMap<String, String>,
        pub cwd: PathBuf,
        pub current_exe: PathBuf,
    }
}

impl Env {
    pub fn new() -> Self {
        if cfg!(test) {
            match cfg!(windows) {
                true => Env::from_slice(&[
                    ("USERPROFILE", ACTIVE_USER_HOME),
                    ("USERNAME", "testuser"),
                    ("PATH", ""),
                ]),
                false => Env::from_slice(&[("HOME", ACTIVE_USER_HOME), ("USER", "testuser"), ("PATH", "")]),
            }
        } else {
            Env(inner::Inner::Real)
        }
    }

    /// Create a fake process environment from a slice of tuples.
    pub fn from_slice(vars: &[(&str, &str)]) -> Self {
        use inner::Inner;
        let map: HashMap<_, _> = vars.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
        Self(Inner::Fake(Arc::new(Mutex::new(inner::Fake {
            vars: map,
            cwd: PathBuf::from("/"),
            current_exe: PathBuf::from("/current_exe"),
        }))))
    }

    pub fn get<K: AsRef<str>>(&self, key: K) -> Result<String, VarError> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => env::var(key.as_ref()),
            Inner::Fake(fake) => fake
                .lock()
                .unwrap()
                .vars
                .get(key.as_ref())
                .cloned()
                .ok_or(VarError::NotPresent),
        }
    }

    pub fn get_os<K: AsRef<OsStr>>(&self, key: K) -> Option<OsString> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => env::var_os(key.as_ref()),
            Inner::Fake(fake) => fake
                .lock()
                .unwrap()
                .vars
                .get(key.as_ref().to_str()?)
                .cloned()
                .map(OsString::from),
        }
    }

    /// Sets the environment variable `key` to the value `value` for the currently running
    /// process.
    ///
    /// # Safety
    ///
    /// See [std::env::set_var] for the safety requirements.
    pub unsafe fn set_var(&self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) {
        unsafe {
            use inner::Inner;
            match &self.0 {
                Inner::Real => std::env::set_var(key, value),
                Inner::Fake(fake) => {
                    fake.lock().unwrap().vars.insert(
                        key.as_ref().to_str().expect("key must be valid str").to_string(),
                        value.as_ref().to_str().expect("key must be valid str").to_string(),
                    );
                },
            }
        }
    }

    pub fn home(&self) -> Option<PathBuf> {
        match &self.0 {
            inner::Inner::Real => dirs::home_dir(),
            inner::Inner::Fake(fake) => fake.lock().unwrap().vars.get("HOME").map(PathBuf::from),
        }
    }

    pub fn current_dir(&self) -> Result<PathBuf, io::Error> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => std::env::current_dir(),
            Inner::Fake(fake) => Ok(fake.lock().unwrap().cwd.clone()),
        }
    }

    pub fn set_current_dir_for_test(&self, path: PathBuf) {
        use inner::Inner;
        if let Inner::Fake(fake) = &self.0 {
            fake.lock().unwrap().cwd = path;
        }
    }

    pub fn current_exe(&self) -> Result<PathBuf, io::Error> {
        use inner::Inner;
        match &self.0 {
            Inner::Real => std::env::current_exe(),
            Inner::Fake(fake) => Ok(fake.lock().unwrap().current_exe.clone()),
        }
    }

    pub fn in_ssh(&self) -> bool {
        self.get("SSH_CLIENT").is_ok() || self.get("SSH_CONNECTION").is_ok() || self.get("SSH_TTY").is_ok()
    }

    pub fn in_codespaces(&self) -> bool {
        self.get_os("CODESPACES").is_some() || self.get_os("KIRO_CODESPACES").is_some()
    }

    pub fn in_ci(&self) -> bool {
        self.get_os("CI").is_some() || self.get_os("KIRO_CI").is_some()
    }

    /// Whether or not the current executable is run from an AppImage.
    ///
    /// See: https://docs.appimage.org/packaging-guide/environment-variables.html
    pub fn in_appimage(&self) -> bool {
        self.get_os("APPIMAGE").is_some()
    }
}

impl Default for Env {
    fn default() -> Self {
        Env::new()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn test_get() {
        let env = Env::new();
        assert!(env.get("PATH").is_ok());
        assert!(env.get_os("PATH").is_some());
        assert!(env.get("NON_EXISTENT").is_err());

        let env = Env::from_slice(&[("HOME", "/home/user"), ("PATH", "/bin:/usr/bin")]);
        assert_eq!(env.home().unwrap(), Path::new("/home/user"));
        assert_eq!(env.get("PATH").unwrap(), "/bin:/usr/bin");
        assert!(env.get_os("PATH").is_some());
        assert!(env.get("NON_EXISTENT").is_err());
    }

    #[test]
    fn test_in_envs() {
        let env = Env::from_slice(&[]);
        assert!(!env.in_ssh());

        let env = Env::from_slice(&[("SSH_CLIENT", "1")]);
        assert!(env.in_ssh());

        let env = Env::from_slice(&[("APPIMAGE", "/tmp/.mount-asdf/usr")]);
        assert!(env.in_appimage());
    }

    #[test]
    fn test_default_current_dir() {
        let env = Env::from_slice(&[]);
        assert_eq!(env.current_dir().unwrap(), PathBuf::from("/"));
    }
}
