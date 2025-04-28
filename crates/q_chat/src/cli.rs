use clap::Parser;

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
