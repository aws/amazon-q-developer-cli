use std::process::Command;
use std::sync::LazyLock;

use serde::{
    Deserialize,
    Serialize,
};

use crate::constants::{
    BREW_CASK_NAME,
    CLI_NAME,
};

const TOOLBOX_VERSION_FAILURE: &str = "failed to determine CLI version";

static INSTALL_METHOD: LazyLock<InstallMethod> = LazyLock::new(|| {
    if let Ok(output) = Command::new("brew").args(["list", BREW_CASK_NAME, "-1"]).output()
        && output.status.success()
    {
        return InstallMethod::Brew;
    }

    if let Ok(current_exe) = std::env::current_exe()
        && current_exe.components().any(|c| c.as_os_str() == ".toolbox")
    {
        let version = toolbox_version().unwrap_or_else(|| TOOLBOX_VERSION_FAILURE.to_string());
        return InstallMethod::Toolbox(version);
    }

    InstallMethod::Unknown
});

fn toolbox_version() -> Option<String> {
    let output = Command::new("toolbox").args(["list", "--installed"]).output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .find(|line| line.starts_with(CLI_NAME))
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

pub fn get_install_method() -> InstallMethod {
    INSTALL_METHOD.clone()
}
