use std::process::Command;
use std::sync::LazyLock;

use serde::{
    Deserialize,
    Serialize,
};

static INSTALL_METHOD: LazyLock<InstallMethod> = LazyLock::new(|| {
    if let Ok(output) = Command::new("brew").args(["list", "amazon-q", "-1"]).output() {
        if output.status.success() {
            return InstallMethod::Brew;
        }
    }

    if let Ok(current_exe) = std::env::current_exe() {
        if current_exe.components().any(|c| c.as_os_str() == ".toolbox") {
            return InstallMethod::Toolbox;
        }
    }

    InstallMethod::Unknown
});

/// The method of installation that Fig was installed with
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstallMethod {
    Brew,
    Toolbox,
    Unknown,
}

impl std::fmt::Display for InstallMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            InstallMethod::Brew => "brew",
            InstallMethod::Toolbox => "toolbox",
            InstallMethod::Unknown => "unknown",
        })
    }
}

pub fn get_install_method() -> InstallMethod {
    *INSTALL_METHOD
}
