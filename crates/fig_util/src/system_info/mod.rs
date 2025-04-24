pub mod linux;
#[cfg(target_os = "windows")]
pub mod windows;

use std::borrow::Cow;
use std::sync::OnceLock;

use cfg_if::cfg_if;
use fig_os_shim::Env;
use serde::{
    Deserialize,
    Serialize,
};
use sha2::{
    Digest,
    Sha256,
};

use crate::Error;
use crate::env_var::Q_PARENT;
use crate::manifest::is_minimal;

/// The support level for different platforms
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupportLevel {
    /// A fully supported platform
    Supported,
    /// Supported, but with a caveat
    SupportedWithCaveat { info: Cow<'static, str> },
    /// A platform that is currently in development
    InDevelopment { info: Option<Cow<'static, str>> },
    /// A platform that is not supported
    Unsupported,
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
        os_release: Option<linux::OsRelease>,
    },
    Windows {
        name: String,
        build: u32,
    },
    FreeBsd {
        version: String,
    },
}

impl OSVersion {
    pub fn support_level(&self) -> SupportLevel {
        match self {
            OSVersion::MacOS { major, minor, .. } => {
                // Minimum supported macOS version is 10.14.0
                if *major > 10 || (*major == 10 && *minor >= 14) {
                    SupportLevel::Supported
                } else {
                    SupportLevel::Unsupported
                }
            },
            OSVersion::Linux { .. } => match (is_remote(), is_minimal()) {
                (true, true) => SupportLevel::Supported,
                (false, true) => SupportLevel::SupportedWithCaveat {
                    info: "Autocomplete is not yet available on Linux, but other products should work as expected."
                        .into(),
                },
                (_, _) => SupportLevel::Supported,
            },
            OSVersion::Windows { build, .. } => match build {
                // Only Windows 11 is fully supported at the moment
                build if *build >= 22000 => SupportLevel::Supported,
                // Windows 10 development has known issues
                build if *build >= 10240 => SupportLevel::InDevelopment {
                    info: Some(
                        "Since support for Windows 10 is still in progress,\
Autocomplete only works in Git Bash with the default prompt.\
Please upgrade to Windows 11 or wait for a fix while we work this issue out."
                            .into(),
                    ),
                },
                // Earlier versions of Windows are not supported
                _ => SupportLevel::Unsupported,
            },
            OSVersion::FreeBsd { .. } => SupportLevel::InDevelopment { info: None },
        }
    }

    pub fn user_readable(&self) -> Vec<String> {
        match self {
            OSVersion::Linux {
                kernel_version,
                os_release,
            } => {
                let mut v = vec![format!("kernel: {kernel_version}")];

                if let Some(os_release) = os_release {
                    if let Some(name) = &os_release.name {
                        v.push(format!("distro: {name}"));
                    }

                    if let Some(version) = &os_release.version {
                        v.push(format!("distro-version: {version}"));
                    } else if let Some(version) = &os_release.version_id {
                        v.push(format!("distro-version: {version}"));
                    }

                    if let Some(variant) = &os_release.variant {
                        v.push(format!("distro-variant: {variant}"));
                    } else if let Some(variant) = &os_release.variant_id {
                        v.push(format!("distro-variant: {variant}"));
                    }

                    if let Some(build) = &os_release.build_id {
                        v.push(format!("distro-build: {build}"));
                    }
                }

                v
            },
            other => vec![format!("{other}")],
        }
    }
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
    OS_VERSION.get_or_init(|| {
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
                use nix::sys::utsname::uname;

                let kernel_version = uname().ok()?.release().to_string_lossy().into();
                let os_release = linux::get_os_release().cloned();

                Some(OSVersion::Linux {
                    kernel_version,
                    os_release,
                })
            } else if #[cfg(target_os = "windows")] {
                use winreg::enums::HKEY_LOCAL_MACHINE;
                use winreg::RegKey;

                let rkey = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion").ok()?;
                let build: String = rkey.get_value("CurrentBuild").ok()?;

                Some(OSVersion::Windows {
                    name: rkey.get_value("ProductName").ok()?,
                    build: build.parse::<u32>().ok()?,
                })
            } else if #[cfg(target_os = "freebsd")] {
                use nix::sys::utsname::uname;

                let version = uname().ok()?.release().to_string_lossy().into();

                Some(OSVersion::FreeBsd {
                    version,
                })

            }
        }
    }).as_ref()
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
    in_ssh() || in_cloudshell() || in_wsl() || std::env::var_os("Q_FAKE_IS_REMOTE").is_some()
}

/// Determines if we have an IPC path to a Desktop app from a remote environment
pub fn has_parent() -> bool {
    static HAS_PARENT: OnceLock<bool> = OnceLock::new();
    *HAS_PARENT.get_or_init(|| std::env::var_os(Q_PARENT).is_some())
}

/// This true if the env var `AWS_EXECUTION_ENV=CloudShell`
pub fn in_cloudshell() -> bool {
    static IN_CLOUDSHELL: OnceLock<bool> = OnceLock::new();
    *IN_CLOUDSHELL.get_or_init(|| Env::new().in_cloudshell())
}

pub fn in_codespaces() -> bool {
    static IN_CODESPACES: OnceLock<bool> = OnceLock::new();
    *IN_CODESPACES
        .get_or_init(|| std::env::var_os("CODESPACES").is_some() || std::env::var_os("Q_CODESPACES").is_some())
}

pub fn in_ci() -> bool {
    static IN_CI: OnceLock<bool> = OnceLock::new();
    *IN_CI.get_or_init(|| std::env::var_os("CI").is_some() || std::env::var_os("Q_CI").is_some())
}

#[cfg(target_os = "macos")]
fn raw_system_id() -> Result<String, Error> {
    let output = std::process::Command::new("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()?;

    let output = String::from_utf8_lossy(&output.stdout);

    let machine_id: String = output
        .lines()
        .find(|line| line.contains("IOPlatformUUID"))
        .ok_or(Error::HwidNotFound)?
        .split('=')
        .nth(1)
        .ok_or(Error::HwidNotFound)?
        .trim()
        .trim_start_matches('"')
        .trim_end_matches('"')
        .into();

    Ok(machine_id)
}

#[cfg(target_os = "linux")]
fn raw_system_id() -> Result<String, Error> {
    for path in ["/var/lib/dbus/machine-id", "/etc/machine-id"] {
        if std::path::Path::new(path).exists() {
            return Ok(std::fs::read_to_string(path)?);
        }
    }
    Err(Error::HwidNotFound)
}

#[cfg(target_os = "windows")]
fn raw_system_id() -> Result<String, Error> {
    use winreg::RegKey;
    use winreg::enums::HKEY_LOCAL_MACHINE;

    let rkey = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\Microsoft\Cryptography")?;
    let id: String = rkey.get_value("MachineGuid")?;

    Ok(id)
}

#[cfg(target_os = "freebsd")]
fn raw_system_id() -> Result<String, Error> {
    Err(Error::HwidNotFound)
}

pub fn get_system_id() -> Option<&'static str> {
    static SYSTEM_ID: OnceLock<Option<String>> = OnceLock::new();
    SYSTEM_ID
        .get_or_init(|| {
            let hwid = raw_system_id().ok()?;
            let mut hasher = Sha256::new();
            hasher.update(hwid);
            Some(format!("{:x}", hasher.finalize()))
        })
        .as_deref()
}

pub fn get_platform() -> &'static str {
    if let Some(over_ride) = option_env!("Q_OVERRIDE_PLATFORM") {
        over_ride
    } else {
        std::env::consts::OS
    }
}

pub fn get_arch() -> &'static str {
    if let Some(over_ride) = option_env!("Q_OVERRIDE_ARCH") {
        over_ride
    } else {
        std::env::consts::ARCH
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_system_id() {
        let id = get_system_id();
        assert!(id.is_some());
        assert_eq!(id.unwrap().len(), 64);
    }
}
