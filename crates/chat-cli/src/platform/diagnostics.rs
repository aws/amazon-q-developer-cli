#![allow(clippy::ref_option_ref)]
use std::collections::BTreeMap;

use serde::Serialize;
use sysinfo::{
    CpuRefreshKind,
    MemoryRefreshKind,
    RefreshKind,
};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::platform::Context;
use crate::telemetry::InstallMethod;
use crate::util::consts::build::HASH;
use crate::util::system_info::{
    OSVersion,
    os_version,
};

fn serialize_display<D, S>(display: D, serializer: S) -> Result<S::Ok, S::Error>
where
    D: std::fmt::Display,
    S: serde::Serializer,
{
    serializer.serialize_str(&display.to_string())
}

fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct BuildDetails {
    pub version: String,
    pub hash: Option<&'static str>,
    pub date: Option<String>,
}

impl BuildDetails {
    pub fn new() -> BuildDetails {
        let date = crate::util::consts::build::DATETIME
            .and_then(|input| OffsetDateTime::parse(input, &Rfc3339).ok())
            .and_then(|time| {
                let rfc3339 = time.format(&Rfc3339).ok()?;
                let duration = OffsetDateTime::now_utc() - time;
                Some(format!("{rfc3339} ({duration:.0} ago)"))
            });

        BuildDetails {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            hash: HASH,
            date,
        }
    }
}

fn serialize_os_version<S>(version: &Option<&OSVersion>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match version {
        Some(version) => match version {
            OSVersion::Linux { .. } => version.serialize(serializer),
            other => serializer.serialize_str(&other.to_string()),
        },
        None => serializer.serialize_none(),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct SystemInfo {
    #[serde(serialize_with = "serialize_os_version")]
    pub os: Option<&'static OSVersion>,
    pub chip: Option<String>,
    pub total_cores: Option<usize>,
    pub memory: Option<String>,
}

impl SystemInfo {
    fn new() -> SystemInfo {
        let system = sysinfo::System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );

        let mut hardware_info = SystemInfo {
            os: os_version(),
            chip: None,
            total_cores: system.physical_core_count(),
            memory: Some(format!("{:0.2} GB", system.total_memory() as f32 / 2.0_f32.powi(30))),
        };

        if let Some(processor) = system.cpus().first() {
            hardware_info.chip = Some(processor.brand().into());
        }

        hardware_info
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct EnvVarDiagnostic {
    pub env_vars: BTreeMap<String, String>,
}

impl EnvVarDiagnostic {
    fn new() -> EnvVarDiagnostic {
        let env_vars = std::env::vars()
            .filter(|(key, _)| {
                let fig_var = crate::util::env_var::ALL.contains(&key.as_str());
                let other_var = [
                    // General env vars
                    "SHELL",
                    "DISPLAY",
                    "PATH",
                    "TERM",
                    "ZDOTDIR",
                    // Linux vars
                    "XDG_CURRENT_DESKTOP",
                    "XDG_SESSION_DESKTOP",
                    "XDG_SESSION_TYPE",
                    "GLFW_IM_MODULE",
                    "GTK_IM_MODULE",
                    "QT_IM_MODULE",
                    "XMODIFIERS",
                    // Macos vars
                    "__CFBundleIdentifier",
                ]
                .contains(&key.as_str());

                fig_var || other_var
            })
            .map(|(key, value)| {
                // sanitize username from values
                let username = format!("/{}", whoami::username());
                (key, value.replace(&username, "/USER"))
            })
            .collect();

        EnvVarDiagnostic { env_vars }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct CurrentEnvironment {
    pub cwd: Option<String>,
    pub cli_path: Option<String>,
    #[serde(serialize_with = "serialize_display")]
    pub install_method: InstallMethod,
    #[serde(skip_serializing_if = "is_false")]
    pub in_cloudshell: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub in_ssh: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub in_ci: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub in_wsl: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub in_codespaces: bool,
}

impl CurrentEnvironment {
    async fn new() -> CurrentEnvironment {
        let ctx = Context::new();

        let username = format!("/{}", whoami::username());

        let cwd = ctx
            .env()
            .current_dir()
            .ok()
            .map(|path| path.to_string_lossy().replace(&username, "/USER"));

        let cli_path = ctx
            .env()
            .current_dir()
            .ok()
            .map(|path| path.to_string_lossy().replace(&username, "/USER"));

        let install_method = crate::telemetry::get_install_method();

        let in_cloudshell = crate::util::system_info::in_cloudshell();
        let in_ssh = crate::util::system_info::in_ssh();
        let in_ci = crate::util::system_info::in_ci();
        let in_wsl = crate::util::system_info::in_wsl();
        let in_codespaces = crate::util::system_info::in_codespaces();

        CurrentEnvironment {
            cwd,
            cli_path,
            install_method,
            in_cloudshell,
            in_ssh,
            in_ci,
            in_wsl,
            in_codespaces,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Diagnostics {
    #[serde(rename = "q-details")]
    pub build_details: BuildDetails,
    pub system_info: SystemInfo,
    pub environment: CurrentEnvironment,
    #[serde(flatten)]
    pub environment_variables: EnvVarDiagnostic,
}

impl Diagnostics {
    pub async fn new() -> Diagnostics {
        Diagnostics {
            build_details: BuildDetails::new(),
            system_info: SystemInfo::new(),
            environment: CurrentEnvironment::new().await,
            environment_variables: EnvVarDiagnostic::new(),
        }
    }

    pub fn user_readable(&self) -> Result<String, toml::ser::Error> {
        toml::to_string(&self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_diagnostics_user_readable() {
        let diagnostics = Diagnostics::new().await;
        let toml = diagnostics.user_readable().unwrap();
        assert!(!toml.is_empty());
    }
}
