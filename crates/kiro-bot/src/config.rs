//! Configuration loading for kiro-bot instances.
//!
//! Each instance is defined by a `config.toml` and optional `secrets.toml` stored
//! under `~/.config/kiro-bot/<name>/`. The config schema is intentionally kept
//! identical to the original botctl format for zero-friction migration.

use std::collections::HashMap;
use std::path::{
    Path,
    PathBuf,
};

use anyhow::{
    Context,
    Result,
};
use serde::Deserialize;

pub use crate::engine::acp::ApprovalPolicy;

/// Root directory for all kiro-bot instances: `~/.kiro/bots/`.
pub fn base_dir() -> Result<PathBuf> {
    Ok(dirs::home_dir().context("no home dir")?.join(".kiro").join("bots"))
}

/// Config directory for a specific instance: `~/.kiro/bots/<name>/`.
pub fn config_dir(name: &str) -> Result<PathBuf> {
    Ok(base_dir()?.join(name))
}

/// State directory for a specific instance: `~/.kiro/bots/<name>/state/`.
pub fn state_dir(name: &str) -> Result<PathBuf> {
    Ok(base_dir()?.join(name).join("state"))
}

/// List all installed instance names by scanning the config directory.
pub fn all_instance_names() -> Result<Vec<String>> {
    let dir = base_dir()?;
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.path().join("config.toml").exists() {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    Ok(names)
}

// ---------------------------------------------------------------------------
// Config structs
// ---------------------------------------------------------------------------

/// Top-level instance configuration, deserialized from `config.toml`.
#[derive(Debug, Deserialize)]
pub struct Config {
    pub name: String,
    pub working_directory: Option<String>,
    pub frontend: FrontendConfig,
    pub agent: AgentConfig,
    pub authorization: Option<AuthzConfig>,
    pub users: Option<HashMap<String, String>>,
    #[serde(default)]
    pub response_policies: Vec<ResponsePolicyEntry>,
}

/// Frontend-specific configuration.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FrontendConfig {
    Slack {
        bot_name: String,
        bot_member_id: Option<String>,
        conversation_history: Option<u16>,
    },
    Cron {
        /// The prompt to send to the agent.
        prompt: String,
        /// Where to send the output.
        #[serde(default)]
        output: CronOutput,
        /// Run interval: "30s", "5m", "1h". Mutually exclusive with `schedule`.
        every: Option<String>,
        /// Cron expression: "0 15 * * *". Mutually exclusive with `every`.
        schedule: Option<String>,
        /// Start time (ISO 8601 UTC). Default: now.
        start_at: Option<String>,
        /// Stop time (ISO 8601 UTC). Default: never.
        stop_at: Option<String>,
    },
}

/// Output destination for cron frontend.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CronOutput {
    /// Print to stdout (default).
    #[default]
    Stdout,
    /// Post to a Slack channel using the bot token from secrets.
    Slack { channel: String },
}

/// Agent (ACP backend) configuration.
#[derive(Debug, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_acp_command")]
    pub command: String,
    #[serde(default = "default_model")]
    pub model: String,
    pub default_mode: Option<String>,
    #[serde(default = "default_mcp_wait_ms")]
    pub mcp_wait_ms: u64,
    #[serde(default = "default_max_workers")]
    pub max_workers: usize,
    #[serde(default = "default_idle_timeout_secs")]
    pub idle_timeout_secs: u64,
    #[serde(default)]
    pub approval_policy: ApprovalPolicy,
}

fn default_acp_command() -> String {
    "kiro-cli acp".into()
}
fn default_model() -> String {
    "claude-opus-4.6-1m".into()
}
fn default_mcp_wait_ms() -> u64 {
    2000
}
fn default_max_workers() -> usize {
    5
}
fn default_idle_timeout_secs() -> u64 {
    300
}

/// Cedar authorization configuration.
#[derive(Debug, Deserialize)]
pub struct AuthzConfig {
    pub cedar_policy_file: String,
    pub cedar_template_values: Option<String>,
    pub cedar_entities_file: Option<String>,
}

/// A single response policy entry from `[[response_policies]]`.
#[derive(Debug, Deserialize)]
pub struct ResponsePolicyEntry {
    pub conversation: String,
    pub trigger: TomlTrigger,
    pub reply: TomlReply,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TomlTrigger {
    Always,
    DirectedOnly,
    ThreadOnly,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TomlReply {
    Inline,
    Thread,
}

// ---------------------------------------------------------------------------
// Secrets
// ---------------------------------------------------------------------------

/// Frontend-specific secrets loaded from `secrets.toml`.
#[derive(Debug)]
pub enum Secrets {
    Slack(SlackSecrets),
}

#[derive(Debug, Deserialize)]
pub struct SlackSecrets {
    pub bot_token: String,
    pub app_token: String,
}

#[derive(Deserialize)]
struct RawSecrets {
    slack: Option<SlackSecrets>,
}

// ---------------------------------------------------------------------------
// Load functions
// ---------------------------------------------------------------------------

/// Load and parse `config.toml` from an instance directory.
pub fn load_config(instance_dir: &Path) -> Result<Config> {
    let content = std::fs::read_to_string(instance_dir.join("config.toml")).context("failed to read config.toml")?;
    toml::from_str(&content).context("failed to parse config.toml")
}

/// Load secrets and validate they match the frontend type in config.
pub fn load_secrets(instance_dir: &Path) -> Result<Secrets> {
    let cfg = load_config(instance_dir)?;
    let content = std::fs::read_to_string(instance_dir.join("secrets.toml")).context("failed to read secrets.toml")?;
    let raw: RawSecrets = toml::from_str(&content).context("failed to parse secrets.toml")?;
    match cfg.frontend {
        FrontendConfig::Slack { .. } | FrontendConfig::Cron { .. } => raw
            .slack
            .map(Secrets::Slack)
            .context("missing [slack] section in secrets.toml"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(toml: &str) -> Config {
        toml::from_str(toml).unwrap()
    }

    const BASE: &str = r#"
name = "test"
[frontend]
type = "slack"
bot_name = "Bot"
[agent]
"#;

    #[test]
    fn conversation_history_present() {
        let cfg = parse(&BASE.replace("[agent]", "conversation_history = 10\n[agent]"));
        let FrontendConfig::Slack {
            conversation_history, ..
        } = cfg.frontend
        else {
            panic!("expected Slack frontend");
        };
        assert_eq!(conversation_history, Some(10));
    }

    #[test]
    fn conversation_history_absent() {
        let cfg = parse(BASE);
        let FrontendConfig::Slack {
            conversation_history, ..
        } = cfg.frontend
        else {
            panic!("expected Slack frontend");
        };
        assert_eq!(conversation_history, None);
    }

    #[test]
    fn working_directory_present() {
        let cfg = parse(&BASE.replace(
            "name = \"test\"",
            "name = \"test\"\nworking_directory = \"/tmp/myproject\"",
        ));
        assert_eq!(cfg.working_directory.as_deref(), Some("/tmp/myproject"));
    }

    #[test]
    fn working_directory_absent() {
        let cfg = parse(BASE);
        assert_eq!(cfg.working_directory, None);
    }
}
