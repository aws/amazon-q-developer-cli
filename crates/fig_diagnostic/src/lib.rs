#![allow(clippy::ref_option_ref)]
use std::collections::BTreeMap;

use fig_os_shim::{
    Context,
    Os,
    PlatformProvider,
};
use fig_telemetry::InstallMethod;
use fig_util::consts::build::HASH;
use fig_util::manifest::manifest;
use fig_util::system_info::{
    OSVersion,
    os_version,
};
use fig_util::{
    Shell,
    Terminal,
};
use serde::Serialize;
use sysinfo::{
    CpuRefreshKind,
    MemoryRefreshKind,
    RefreshKind,
};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

fn serialize_display<D, S>(display: D, serializer: S) -> Result<S::Ok, S::Error>
where
    D: std::fmt::Display,
    S: serde::Serializer,
{
    serializer.serialize_str(&display.to_string())
}

fn serialize_display_option<D, S>(display: &Option<D>, serializer: S) -> Result<S::Ok, S::Error>
where
    D: std::fmt::Display,
    S: serde::Serializer,
{
    match display {
        Some(display) => serializer.serialize_str(&display.to_string()),
        None => serializer.serialize_none(),
    }
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
    pub variant: String,
}

impl BuildDetails {
    pub fn new() -> BuildDetails {
        let date = fig_util::consts::build::DATETIME
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
            variant: manifest().variant.to_string(),
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
                let fig_var = fig_util::env_var::ALL.contains(&key.as_str());
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
    pub os: Os,
    pub shell_path: Option<String>,
    pub shell_version: Option<String>,
    #[serde(serialize_with = "serialize_display_option")]
    pub terminal: Option<Terminal>,
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
        use fig_util::process_info::{
            Pid,
            PidExt,
        };
        let ctx = Context::new();

        let username = format!("/{}", whoami::username());

        let shell_path = Pid::current()
            .parent()
            .and_then(|pid| pid.exe())
            .map(|p| p.to_string_lossy().replace(&username, "/USER"));
        let shell_version = Shell::current_shell_version().await.map(|(_, v)| v).ok();

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

        let os = ctx.platform().os();
        let terminal = Terminal::parent_terminal(&ctx);
        let install_method = fig_telemetry::get_install_method();

        let in_cloudshell = fig_util::system_info::in_cloudshell();
        let in_ssh = fig_util::system_info::in_ssh();
        let in_ci = fig_util::system_info::in_ci();
        let in_wsl = fig_util::system_info::in_wsl();
        let in_codespaces = fig_util::system_info::in_codespaces();

        CurrentEnvironment {
            shell_path,
            shell_version,
            cwd,
            cli_path,
            os,
            terminal,
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
