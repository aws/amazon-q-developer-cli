use std::fmt;

use cfg_if::cfg_if;
use eyre::Result;
use serde::{
    Deserialize,
    Serialize,
};

/// The support level for different platforms
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupportLevel {
    /// A fully supported platform
    Supported,
    /// A platform that is currently in development
    InDevelopment,
    /// A platform that is not supported
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OSVersion {
    MacOS {
        major: i32,
        minor: i32,
        patch: Option<i32>,
        build: String,
    },
    Linux {
        kernel_version: String,
        distribution: Option<String>,
        release: Option<String>,
    },
    Windows {
        version: String,
    },
}

impl fmt::Display for OSVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OSVersion::MacOS {
                major,
                minor,
                patch,
                build,
            } => {
                let patch = patch.unwrap_or(0);
                f.write_str(&format!("macOS {major}.{minor}.{patch} ({build})",))
            },
            OSVersion::Linux { kernel_version, .. } => f.write_str(&format!("Linux {kernel_version}")),
            OSVersion::Windows { version } => f.write_str(&format!("Windows {version}")),
        }
    }
}

impl From<OSVersion> for String {
    fn from(os: OSVersion) -> Self {
        format!("{os}")
    }
}

impl OSVersion {
    pub fn new() -> Result<OSVersion> {
        cfg_if! {
            if #[cfg(target_os = "macos")] {
                use std::process::Command;
                use regex::Regex;
                use eyre::{ContextCompat, WrapErr};

                let version_info = Command::new("sw_vers")
                    .output()
                    .with_context(|| "Could not get macOS version")?;

                let version_info: String = String::from_utf8_lossy(&version_info.stdout).trim().into();

                let version_regex = Regex::new(r#"ProductVersion:\s*(\S+)"#).unwrap();
                let build_regex = Regex::new(r#"BuildVersion:\s*(\S+)"#).unwrap();

                let version: String = version_regex
                    .captures(&version_info)
                    .and_then(|c| c.get(1))
                    .map(|v| v.as_str().into())
                    .context("Invalid version")?;

                let major = version
                    .split('.')
                    .next()
                    .context("Invalid version")?
                    .parse()?;

                let minor = version
                    .split('.')
                    .nth(1)
                    .context("Invalid version")?
                    .parse()?;

                let patch = version.split('.').nth(2).and_then(|p| p.parse().ok());

                let build = build_regex
                    .captures(&version_info)
                    .and_then(|c| c.get(1))
                    .context("Invalid version")?
                    .as_str()
                    .into();

                Ok(OSVersion::MacOS {
                    major,
                    minor,
                    patch,
                    build,
                })
            } else if #[cfg(target_os = "linux")] {
                use nix::sys::utsname::uname;
                // use regex::Regex;

                let uname = uname()?;
                let kernel_version = uname.release().to_string_lossy().into();

                // let version_info = Command::new("lsb_release")
                //     .arg("-a")
                //     .output()
                //     .with_context(|| "Could not get Linux version")?;

                // let version_info: String = String::from_utf8_lossy(&version_info.stdout).trim().into();

                // let distribution_regex = Regex::new(r#"Distributor ID:\s*(\S+)"#).unwrap();
                // let kernel_regex = Regex::new(r#"Description:\s*(\S+)"#).unwrap();

                // let flavor = distribution_regex
                //     .captures(&version_info)
                //     .and_then(|c| c.get(1))
                //     .map(|v| v.as_str().into())
                //     .context("Invalid version")?;

                // let kernel_version = kernel_regex
                //     .captures(&version_info)
                //     .and_then(|c| c.get(1))
                //     .map(|v| v.as_str().into())
                //     .context("Invalid version")?;

                Ok(OSVersion::Linux {
                    kernel_version,
                    distribution: None,
                    release: None,
                })
            } else if #[cfg(target_os = "windows")] {
                use std::process::Command;

                use eyre::WrapErr;

                Ok(OSVersion::Windows {
                    version: String::from_utf8_lossy(&Command::new("systeminfo")
                        .arg("/FO")
                        .arg("CSV")
                        .output()
                        .context("Could not get windows version")?.stdout)
                        .split_once('\n')
                        .unwrap()
                        .1
                        .split(',')
                        .nth(2)
                        .unwrap()
                        .trim_matches('"')
                        .to_owned()
                })
            } else {
                Err(eyre::eyre!("Unsupported platform"))
            }
        }
    }

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
            OSVersion::Linux { .. } => SupportLevel::InDevelopment,
            OSVersion::Windows { .. } => SupportLevel::InDevelopment,
        }
    }
}
