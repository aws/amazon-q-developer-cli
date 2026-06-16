use serde::{
    Deserialize,
    Serialize,
};

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
    InstallMethod::Unknown
}
