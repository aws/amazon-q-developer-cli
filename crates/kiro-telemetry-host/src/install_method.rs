use std::process::Command;
use std::sync::OnceLock;

use serde::{
    Deserialize,
    Serialize,
};

const TOOLBOX_VERSION_FAILURE: &str = "failed to determine CLI version";

static INSTALL_METHOD: OnceLock<InstallMethod> = OnceLock::new();

fn detect_install_method(brew_cask_name: &str, cli_name: &str) -> InstallMethod {
    if let Ok(output) = Command::new("brew").args(["list", brew_cask_name, "-1"]).output()
        && output.status.success()
    {
        return InstallMethod::Brew;
    }

    if let Ok(current_exe) = std::env::current_exe()
        && current_exe.components().any(|c| c.as_os_str() == ".toolbox")
    {
        let version = toolbox_version(cli_name).unwrap_or_else(|| TOOLBOX_VERSION_FAILURE.to_string());
        return InstallMethod::Toolbox(version);
    }

    InstallMethod::Unknown
}

fn toolbox_version(cli_name: &str) -> Option<String> {
    let output = Command::new("toolbox").args(["list", "--installed"]).output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .find(|line| line.starts_with(cli_name))
        .and_then(|line| line.split_whitespace().nth(1))
        .map(|v| v.to_string())
}

/// The method used to install the CLI
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstallMethod {
    Brew,
    Toolbox(String),
    Unknown,
}

impl std::fmt::Display for InstallMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallMethod::Brew => f.write_str("brew"),
            InstallMethod::Toolbox(v) if v.is_empty() => f.write_str("toolbox"),
            InstallMethod::Toolbox(v) => write!(f, "toolbox ({v})"),
            InstallMethod::Unknown => f.write_str("unknown"),
        }
    }
}

/// Detect (and cache) the install method for the running CLI.
///
/// `brew_cask_name` is checked via `brew list` and `cli_name` is matched against
/// the leading column of `toolbox list --installed`. The result is memoised on
/// first call; subsequent calls ignore the parameters.
pub fn get_install_method(brew_cask_name: &str, cli_name: &str) -> InstallMethod {
    INSTALL_METHOD
        .get_or_init(|| detect_install_method(brew_cask_name, cli_name))
        .clone()
}
