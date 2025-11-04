#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "windows")]
pub mod windows;

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use cfg_if::cfg_if;
use serde::{
    Deserialize,
    Serialize,
};

use crate::os::Env;
use crate::util::env_var::is_remote_fake;

/// Fields for OS release information
/// Fields from <https://www.man7.org/linux/man-pages/man5/os-release.5.html>
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsRelease {
    pub id: Option<String>,

    pub name: Option<String>,
    pub pretty_name: Option<String>,

    pub version_id: Option<String>,
    pub version: Option<String>,

    pub build_id: Option<String>,

    pub variant_id: Option<String>,
    pub variant: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OSVersion {
    MacOS {
        major: i32,
        minor: i32,
        patch: Option<i32>,
        build: String,
    },
    Linux {
        kernel_version: String,
        #[serde(flatten)]
        os_release: Option<OsRelease>,
    },
    Windows {
        name: String,
        build: u32,
    },
    FreeBsd {
        version: String,
    },
}

impl std::fmt::Display for OSVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OSVersion::MacOS {
                major,
                minor,
                patch,
                build,
            } => {
                let patch = patch.unwrap_or(0);
                write!(f, "macOS {major}.{minor}.{patch} ({build})")
            },
            OSVersion::Linux {
                kernel_version,
                os_release,
            } => match os_release
                .as_ref()
                .and_then(|r| r.pretty_name.as_ref().or(r.name.as_ref()))
            {
                Some(distro_name) => write!(f, "Linux {kernel_version} - {distro_name}"),
                None => write!(f, "Linux {kernel_version}"),
            },
            OSVersion::Windows { name, build } => write!(f, "{name} (or newer) - build {build}"),
            OSVersion::FreeBsd { version } => write!(f, "FreeBSD {version}"),
        }
    }
}

pub fn os_version() -> Option<&'static OSVersion> {
    static OS_VERSION: OnceLock<Option<OSVersion>> = OnceLock::new();
    OS_VERSION
        .get_or_init(|| {
            cfg_if! {
                if #[cfg(target_os = "macos")] {
                    use std::process::Command;
                    use regex::Regex;

                    let version_info = Command::new("sw_vers")
                        .output()
                        .ok()?;

                    let version_info: String = String::from_utf8_lossy(&version_info.stdout).trim().into();

                    let version_regex = Regex::new(r"ProductVersion:\s*(\S+)").unwrap();
                    let build_regex = Regex::new(r"BuildVersion:\s*(\S+)").unwrap();

                    let version: String = version_regex
                        .captures(&version_info)
                        .and_then(|c| c.get(1))
                        .map(|v| v.as_str().into())?;

                    let major = version
                        .split('.')
                        .next()?
                        .parse().ok()?;

                    let minor = version
                        .split('.')
                        .nth(1)?
                        .parse().ok()?;

                    let patch = version.split('.').nth(2).and_then(|p| p.parse().ok());

                    let build = build_regex
                        .captures(&version_info)
                        .and_then(|c| c.get(1))?
                        .as_str()
                        .into();

                    Some(OSVersion::MacOS {
                        major,
                        minor,
                        patch,
                        build,
                    })
                } else if #[cfg(target_os = "linux")] {
                    linux::get_os_version()
                } else if #[cfg(target_os = "windows")] {
                    windows::get_os_version()
                } else if #[cfg(target_os = "freebsd")] {
                    use nix::sys::utsname::uname;

                    let version = uname().ok()?.release().to_string_lossy().into();

                    Some(OSVersion::FreeBsd {
                        version,
                    })
                }
            }
        })
        .as_ref()
}

pub fn in_ssh() -> bool {
    static IN_SSH: OnceLock<bool> = OnceLock::new();
    *IN_SSH.get_or_init(|| Env::new().in_ssh())
}

/// Test if the program is running under WSL
pub fn in_wsl() -> bool {
    cfg_if! {
        if #[cfg(target_os = "linux")] {
            static IN_WSL: OnceLock<bool> = OnceLock::new();
            *IN_WSL.get_or_init(|| {
                if let Ok(b) = std::fs::read("/proc/sys/kernel/osrelease") {
                    if let Ok(s) = std::str::from_utf8(&b) {
                        let a = s.to_ascii_lowercase();
                        return a.contains("microsoft") || a.contains("wsl");
                    }
                }
                false
            })
        } else {
            false
        }
    }
}

/// Is the calling binary running on a remote instance
pub fn is_remote() -> bool {
    // TODO(chay): Add detection for inside docker container
    in_ssh() || in_wsl() || is_remote_fake()
}

pub fn in_codespaces() -> bool {
    static IN_CODESPACES: OnceLock<bool> = OnceLock::new();
    *IN_CODESPACES.get_or_init(crate::util::env_var::in_codespaces)
}

pub fn in_ci() -> bool {
    static IN_CI: OnceLock<bool> = OnceLock::new();
    *IN_CI.get_or_init(crate::util::env_var::in_ci)
}

pub fn is_mwinit_available() -> bool {
    static MWINIT_AVAILABLE: OnceLock<bool> = OnceLock::new();
    *MWINIT_AVAILABLE.get_or_init(|| which_mwinit().is_some())
}

pub fn which_mwinit() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let cmd = ("where.exe", "mwinit");

    #[cfg(not(target_os = "windows"))]
    let cmd = ("which", "mwinit");

    let output = Command::new(cmd.0).arg(cmd.1).output().ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let path = stdout.lines().next()?.trim();
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}
