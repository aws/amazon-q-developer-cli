use std::collections::HashMap;

use clap::{
    Args,
    Parser,
    Subcommand,
    ValueEnum,
};

#[derive(Debug, Clone, PartialEq, Eq, Default, Parser)]
pub struct Chat {
    /// (Deprecated, use --trust-all-tools) Enabling this flag allows the model to execute
    /// all commands without first accepting them.
    #[arg(short, long, hide = true)]
    pub accept_all: bool,
    /// Print the first response to STDOUT without interactive mode. This will fail if the
    /// prompt requests permissions to use a tool, unless --trust-all-tools is also used.
    #[arg(long)]
    pub no_interactive: bool,
    /// Resumes the previous conversation from this directory.
    #[arg(short, long)]
    pub resume: bool,
    /// The first question to ask
    pub input: Option<String>,
    /// Context profile to use
    #[arg(long = "profile")]
    pub profile: Option<String>,
    /// Allows the model to use any tool to run commands without asking for confirmation.
    #[arg(long)]
    pub trust_all_tools: bool,
    /// Trust only this set of tools. Example: trust some tools:
    /// '--trust-tools=fs_read,fs_write', trust no tools: '--trust-tools='
    #[arg(long, value_delimiter = ',', value_name = "TOOL_NAMES")]
    pub trust_tools: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum Mcp {
    /// Add or replace a configured server
    Add(McpAdd),
    /// Remove a server from the MCP configuration
    #[command(alias = "rm")]
    Remove(McpRemove),
    /// List configured servers
    List(McpList),
    /// Import a server configuration from another file
    Import(McpImport),
    /// Get the status of a configured server
    Status(McpStatus),
    /// Set whether a profile should exclusively use its own MCP servers
    UseProfileServersOnly(McpUseProfileServersOnly),
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct McpAdd {
    /// Name for the server
    #[arg(long)]
    pub name: String,
    /// The command used to launch the server
    #[arg(long)]
    pub command: String,
    /// Where to add the server to.
    #[arg(long, value_enum)]
    pub scope: Option<Scope>,
    /// Profile to add the MCP server to
    #[arg(long)]
    pub profile: Option<String>,
    /// Environment variables to use when launching the server
    #[arg(long, value_parser = parse_env_vars)]
    pub env: Vec<HashMap<String, String>>,
    /// Server launch timeout, in milliseconds
    #[arg(long)]
    pub timeout: Option<u64>,
    /// Overwrite an existing server with the same name
    #[arg(long, default_value_t = false)]
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct McpRemove {
    #[arg(long)]
    pub name: String,
    #[arg(long, value_enum)]
    pub scope: Option<Scope>,
    /// Profile to remove the MCP server from
    #[arg(long)]
    pub profile: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct McpList {
    #[arg(value_enum)]
    pub scope: Option<Scope>,
    /// Profile to list MCP servers for
    #[arg(long)]
    pub profile: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct McpImport {
    #[arg(long)]
    pub file: String,
    #[arg(value_enum)]
    pub scope: Option<Scope>,
    /// Profile to import MCP servers to
    #[arg(long)]
    pub profile: Option<String>,
    /// Overwrite an existing server with the same name
    #[arg(long, default_value_t = false)]
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct McpStatus {
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub profile: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct McpUseProfileServersOnly {
    #[arg(long, help = "Profile name")]
    pub profile: String,
    #[arg(long, default_value_t = false)]
    pub value: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum Scope {
    Workspace,
    Global,
    Profile,
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Scope::Workspace => write!(f, "workspace"),
            Scope::Global => write!(f, "global"),
            Scope::Profile => write!(f, "profile"),
        }
    }
}

#[derive(Debug)]
struct EnvVarParseError(String);

impl std::fmt::Display for EnvVarParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Failed to parse environment variables: {}", self.0)
    }
}

impl std::error::Error for EnvVarParseError {}

fn parse_env_vars(arg: &str) -> Result<HashMap<String, String>, EnvVarParseError> {
    let mut vars = HashMap::new();

    for pair in arg.split(",") {
        match pair.split_once('=') {
            Some((key, value)) => {
                vars.insert(key.trim().to_string(), value.trim().to_string());
            },
            None => {
                return Err(EnvVarParseError(format!(
                    "Invalid environment variable '{}'. Expected 'name=value'",
                    pair
                )));
            },
        }
    }

    Ok(vars)
}
