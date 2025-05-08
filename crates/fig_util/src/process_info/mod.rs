use std::path::PathBuf;
use std::{
    fmt,
    str,
};

use cfg_if::cfg_if;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(target_os = "macos")]
mod macos;
// #[cfg(target_os = "macos")]
// pub use macos::*;

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "freebsd")]
mod freebsd;
#[cfg(target_os = "freebsd")]
pub use self::freebsd::*;

macro_rules! pid_decl {
    ($typ:ty) => {
        #[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
        #[repr(transparent)]
        pub struct Pid(pub(crate) $typ);

        impl From<$typ> for Pid {
            fn from(v: $typ) -> Self {
                Self(v)
            }
        }
        impl From<Pid> for $typ {
            fn from(v: Pid) -> Self {
                v.0
            }
        }
        impl str::FromStr for Pid {
            type Err = <$typ as str::FromStr>::Err;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(<$typ>::from_str(s)?))
            }
        }
        impl fmt::Display for Pid {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

cfg_if! {
    if #[cfg(unix)] {
        use nix::libc::pid_t;

        pid_decl!(pid_t);

        impl From<nix::unistd::Pid> for Pid {
            fn from(pid: nix::unistd::Pid) -> Self {
                Pid(pid.as_raw())
            }
        }

        impl From<Pid> for nix::unistd::Pid {
            fn from(pid: Pid) -> Self {
                nix::unistd::Pid::from_raw(pid.0)
            }
        }
    } else if #[cfg(windows)] {
        pid_decl!(u32);
    }
}

pub trait PidExt {
    fn current() -> Self;
    fn parent(&self) -> Option<Pid>;
    fn exe(&self) -> Option<PathBuf>;
}

pub fn get_parent_process_exe() -> Option<PathBuf> {
    let mut pid = Pid::current();
    loop {
        pid = pid.parent()?;
        match pid.exe() {
            // We ignore toolbox-exec since we never want to know if that is the parent process
            Some(pid) if pid.file_name().is_some_and(|s| s == "toolbox-exec") => {},
            other => return other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_name() {
        let process_pid = Pid::current();
        let parent_pid = process_pid.parent().unwrap();
        let parent_exe = parent_pid.exe().unwrap();
        let parent_name = parent_exe.file_name().unwrap().to_str().unwrap();

        assert!(parent_name.contains("cargo"));
    }

    #[test]
    fn test_get_parent_process_exe() {
        get_parent_process_exe();
    }
}
