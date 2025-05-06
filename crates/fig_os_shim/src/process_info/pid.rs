use std::path::PathBuf;
use std::sync::Weak;
use std::{fmt, str};

use cfg_if::cfg_if;

// Platform-specific implementations
#[cfg(target_os = "linux")]
use super::linux::{cmdline, current, exe, parent};
#[cfg(target_os = "macos")]
use super::macos::{cmdline, current, exe, parent};
#[cfg(windows)]
use super::windows::{cmdline, current, exe, parent};
use crate::Context;

#[derive(Default, Debug, Clone)]
pub struct FakePid {
    pub pid: u32,
    pub parent: Option<Box<Pid>>,
    pub exe: Option<PathBuf>,
    pub cmdline: Option<String>,
}

impl std::fmt::Display for FakePid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", &self)
    }
}

/// Represents a currently running process. [Pid] should be used as the interface to accessing
/// process-related information, e.g. the current process executable or parent processes.
#[derive(Debug, Clone)]
pub enum Pid {
    Real(Weak<Context>, RawPid),
    Fake(FakePid),
}

impl std::fmt::Display for Pid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Pid::Real(_, raw) => write!(f, "{}", raw),
            Pid::Fake(fake) => write!(f, "{}", fake),
        }
    }
}

macro_rules! pid_decl {
    ($typ:ty) => {
        /// Wrapper around the platform's actual process id type.
        #[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
        #[repr(transparent)]
        pub struct RawPid(pub(crate) $typ);

        impl From<$typ> for RawPid {
            fn from(v: $typ) -> Self {
                Self(v)
            }
        }
        impl From<RawPid> for $typ {
            fn from(v: RawPid) -> Self {
                v.0
            }
        }
        impl str::FromStr for RawPid {
            type Err = <$typ as str::FromStr>::Err;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(<$typ>::from_str(s)?))
            }
        }
        impl fmt::Display for RawPid {
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

        impl RawPid {
            pub fn as_u32(&self) -> u32 {
                self.0 as _
            }
        }
        impl From<nix::unistd::Pid> for RawPid {
            fn from(pid: nix::unistd::Pid) -> Self {
                RawPid(pid.as_raw())
            }
        }

        impl From<RawPid> for nix::unistd::Pid {
            fn from(pid: RawPid) -> Self {
                nix::unistd::Pid::from_raw(pid.0)
            }
        }
    } else if #[cfg(windows)] {
        pid_decl!(u32);

        impl RawPid {
            pub fn as_u32(&self) -> u32 {
                self.0
            }
        }
    }
}

impl Pid {
    pub fn current(ctx: Weak<Context>) -> Self {
        current(ctx.clone())
    }

    pub fn new_fake(fake: FakePid) -> Self {
        Self::Fake(fake)
    }

    /// Returns the parent process of this process, if it exists.
    pub fn parent(&self) -> Option<Box<Pid>> {
        match self {
            Pid::Real(ctx, raw) => parent(ctx.clone(), raw),
            Pid::Fake(fake) => fake.parent.clone(),
        }
    }

    /// Returns the path to the executable file that launched this process if it exists.
    pub fn exe(&self) -> Option<PathBuf> {
        match self {
            Pid::Real(ctx, raw) => exe(ctx.clone(), raw),
            Pid::Fake(fake) => fake.exe.clone(),
        }
    }

    /// Returns the command that originally started this process. Optional since not all platforms
    /// (e.g. Mac) support this.
    pub fn cmdline(&self) -> Option<String> {
        match self {
            Pid::Real(ctx, raw) => cmdline(ctx.clone(), raw),
            Pid::Fake(fake) => fake.cmdline.clone(),
        }
    }

    /// Returns the process id.
    pub fn as_u32(&self) -> u32 {
        match self {
            Pid::Real(_, raw) => raw.as_u32(),
            Pid::Fake(fake) => fake.pid,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn parent_name() {
        let ctx = Context::new();
        let process_pid = Pid::current(Arc::downgrade(&ctx));
        let parent_pid = process_pid.parent().unwrap();
        let parent_exe = parent_pid.exe().unwrap();
        let parent_name = parent_exe.file_name().unwrap().to_str().unwrap().to_lowercase();

        // On Windows, the parent process might be cargo.exe, or it could be another process
        // like runner.exe or pwsh.exe depending on how the test is run
        #[cfg(windows)]
        {
            // Check for common parent processes when running tests on Windows
            assert!(
                parent_name.contains("cargo")
                    || parent_name.contains("runner")
                    || parent_name.contains("pwsh")
                    || parent_name.contains("cmd")
                    || parent_name.contains("powershell"),
                "Unexpected parent process name: {}",
                parent_name
            );
        }

        #[cfg(not(windows))]
        {
            assert!(
                parent_name.contains("cargo"),
                "Unexpected parent process name: {}",
                parent_name
            );
        }
    }
}
