//! Chat module - provides ChatArgs for TUI launch and legacy utilities.
//!
//! The actual chat functionality is handled by the TUI (launched via Bun)
//! which communicates with the ACP agent backend.

pub mod legacy;

use std::process::ExitCode;

use clap::{
    Args,
    ValueEnum,
};
use eyre::Result;

use crate::os::Os;

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum WrapMode {
    Always,
    Never,
    Auto,
}

/// Arguments for the chat command.
///
/// Note: The actual chat is handled by the TUI launcher in Cli::run().
/// This struct only defines the CLI arguments.
#[derive(Debug, Clone, PartialEq, Eq, Default, Args)]
pub struct ChatArgs {
    /// Resume the most recent conversation from this directory.
    #[arg(short, long)]
    pub resume: bool,
    /// Interactively select a conversation to resume from this directory.
    #[arg(long, conflicts_with = "resume")]
    pub resume_picker: bool,
    /// Context profile to use
    #[arg(long = "agent", alias = "profile")]
    pub agent: Option<String>,
    /// Current model to use
    #[arg(long = "model")]
    pub model: Option<String>,
    /// Allows the model to use any tool to run commands without asking for confirmation.
    #[arg(short = 'a', long)]
    pub trust_all_tools: bool,
    /// Trust only this set of tools.
    #[arg(long, value_delimiter = ',', value_name = "TOOL_NAMES")]
    pub trust_tools: Option<Vec<String>>,
    /// Whether the command should run without expecting user input
    #[arg(long, alias = "non-interactive")]
    pub no_interactive: bool,
    /// List all saved chat sessions for the current directory.
    #[arg(short = 'l', long)]
    pub list_sessions: bool,
    /// Delete a saved chat session by ID.
    #[arg(short = 'd', long, value_name = "SESSION_ID")]
    pub delete_session: Option<String>,
    /// Use the legacy terminal UI instead of the embedded TUI
    #[arg(long)]
    pub legacy_mode: bool,
    /// The first question to ask
    pub input: Option<String>,
    /// Control line wrapping behavior (default: auto-detect)
    #[arg(short = 'w', long, value_enum)]
    pub wrap: Option<WrapMode>,
}

impl ChatArgs {
    /// This should never be called - Chat is handled by TUI launcher in Cli::run()
    pub async fn execute(self, _os: &mut Os) -> Result<ExitCode> {
        unreachable!("Chat command is handled by TUI launcher, not execute()")
    }
}
