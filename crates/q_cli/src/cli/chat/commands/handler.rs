use std::future::Future;
use std::pin::Pin;

use eyre::Result;
use fig_os_shim::Context;

use crate::cli::chat::QueuedTool;
pub use crate::cli::chat::conversation_state::ChatState;

/// Trait for command handlers
pub trait CommandHandler: Send + Sync {
    /// Returns the name of the command
    fn name(&self) -> &'static str;

    /// Returns a description of the command
    fn description(&self) -> &'static str;

    /// Returns usage information for the command
    fn usage(&self) -> &'static str;

    /// Returns detailed help text for the command
    fn help(&self) -> String;

    /// Execute the command with the given arguments
    ///
    /// This method is async to allow for operations that require async/await,
    /// such as file system operations or network requests.
    fn execute<'a>(
        &'a self,
        args: Vec<&'a str>,
        ctx: &'a Context,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState>> + Send + 'a>>;

    /// Check if this command requires confirmation before execution
    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        true // Most commands require confirmation by default
    }

    /// Parse arguments for this command
    ///
    /// This method takes a vector of string slices and returns a vector of string slices.
    /// The lifetime of the returned slices must be the same as the lifetime of the input slices.
    fn parse_args<'a>(&self, args: Vec<&'a str>) -> Result<Vec<&'a str>> {
        Ok(args)
    }
}
