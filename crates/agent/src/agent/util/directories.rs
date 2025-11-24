use std::env;
use std::path::{
    Path,
    PathBuf,
};
use std::sync::OnceLock;

use tracing::warn;

use super::error::{
    ErrorContext as _,
    UtilError,
};
use crate::agent::util::consts::env_var::CLI_DATA_DIR;

const DATA_DIR_NAME: &str = "amazon-q";

type Result<T, E = UtilError> = std::result::Result<T, E>;

enum Subpath<'a> {
    Same(&'a str),
    Diff {
        q_cli_variant: &'a str,
        kiro_cli_variant: &'a str,
    },
}

impl<'a> From<&'a str> for Subpath<'a> {
    fn from(s: &'a str) -> Self {
        Subpath::Same(s)
    }
}

pub fn home_dir() -> Result<PathBuf, UtilError> {
    dirs::home_dir().ok_or(UtilError::MissingHomeDir)
}

/// Path to the local data directory.
pub fn data_dir() -> Result<PathBuf> {
    static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

    if let Some(p) = DATA_DIR.get() {
        return Ok(p.clone());
    }

    let p = if let Ok(p) = env::var(CLI_DATA_DIR) {
        warn!(?p, "Using override env var for data directory");
        PathBuf::from(p)
    } else {
        dirs::data_local_dir()
            .ok_or(UtilError::MissingDataLocalDir)?
            .join(DATA_DIR_NAME)
    };

    DATA_DIR.set(p.clone()).expect("Setting the data directory cannot fail");

    Ok(p)
}

pub fn database_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("data.sqlite3"))
}

pub fn settings_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("settings.json"))
}

/// Relative path to the settings JSON schema file
pub fn settings_schema_path(base: impl AsRef<Path>) -> PathBuf {
    base.as_ref().join("settings_schema.json")
}

fn resolve_migrated_path(is_global: bool, subpath: Subpath<'_>) -> Result<PathBuf> {
    let (kiro_base, amazonq_base) = if is_global {
        let home = home_dir()?;
        (home.join(".kiro"), home.join(".aws/amazonq"))
    } else {
        let cwd = env::current_dir().context("unable to get the current directory")?;
        (cwd.join(".kiro"), cwd.join(".amazonq"))
    };

    let scope = if is_global { "global" } else { "workspace" };

    match (kiro_base.exists(), amazonq_base.exists()) {
        (true, false) => {
            warn!("Using .kiro {} configuration", scope);
            match subpath {
                Subpath::Same(path) => Ok(kiro_base.join(path)),
                Subpath::Diff { kiro_cli_variant, .. } => Ok(kiro_base.join(kiro_cli_variant)),
            }
        },
        (false, true) => {
            warn!("Migration notice: Using .amazonq {} configs", scope);
            match subpath {
                Subpath::Same(path) => Ok(amazonq_base.join(path)),
                Subpath::Diff { q_cli_variant, .. } => Ok(amazonq_base.join(q_cli_variant)),
            }
        },
        (true, true) => {
            warn!("Both .amazonq and .kiro {} configs exist, using .amazonq", scope);
            match subpath {
                Subpath::Same(path) => Ok(amazonq_base.join(path)),
                Subpath::Diff { q_cli_variant, .. } => Ok(amazonq_base.join(q_cli_variant)),
            }
        },
        (false, false) => match subpath {
            Subpath::Same(path) => Ok(kiro_base.join(path)),
            Subpath::Diff { kiro_cli_variant, .. } => Ok(kiro_base.join(kiro_cli_variant)),
        }, // Default to kiro
    }
}

/// Path to the directory containing local agent configs.
pub fn local_agents_path() -> Result<PathBuf> {
    let subpath = Subpath::Diff {
        q_cli_variant: "cli-agents",
        kiro_cli_variant: "agents",
    };
    resolve_migrated_path(false, subpath)
}

/// Path to the directory containing global agent configs.
pub fn global_agents_path() -> Result<PathBuf> {
    let subpath = Subpath::Diff {
        q_cli_variant: "cli-agents",
        kiro_cli_variant: "agents",
    };
    resolve_migrated_path(true, subpath)
}

/// Legacy workspace MCP server config path
pub fn legacy_workspace_mcp_config_path() -> Result<PathBuf> {
    resolve_migrated_path(false, "mcp.json".into())
}

/// Legacy global MCP server config path
pub fn legacy_global_mcp_config_path() -> Result<PathBuf> {
    resolve_migrated_path(true, "mcp.json".into())
}
