use std::future::Future;
use std::pin::Pin;

use eyre::Result;
use fig_os_shim::Context;

use crate::cli::chat::commands::CommandHandler;
use crate::cli::chat::{
    ChatState,
    QueuedTool,
};

// TODO: In the future, this help text should be generated dynamically from the command registry
// TODO: We need to support subcommands in the help text generation, e.g., /profile create
/// Comprehensive help text with formatting
pub const HELP_TEXT: &str = color_print::cstr! {"

<magenta,em>q</magenta,em> (Amazon Q Chat)

<cyan,em>Commands:</cyan,em>
<em>/clear</em>        <black!>Clear the conversation history</black!>
<em>/issue</em>        <black!>Report an issue or make a feature request</black!>
<em>/editor</em>       <black!>Open $EDITOR (defaults to vi) to compose a prompt</black!>
<em>/help</em>         <black!>Show this help dialogue</black!>
<em>/quit</em>         <black!>Quit the application</black!>
<em>/compact</em>      <black!>Summarize the conversation to free up context space</black!>
  <em>help</em>        <black!>Show help for the compact command</black!>
  <em>[prompt]</em>    <black!>Optional custom prompt to guide summarization</black!>
  <em>--summary</em>   <black!>Display the summary after compacting</black!>
<em>/tools</em>        <black!>View and manage tools and permissions</black!>
  <em>help</em>        <black!>Show an explanation for the trust command</black!>
  <em>trust</em>       <black!>Trust a specific tool for the session</black!>
  <em>untrust</em>     <black!>Revert a tool to per-request confirmation</black!>
  <em>trustall</em>    <black!>Trust all tools (equivalent to deprecated /acceptall)</black!>
  <em>reset</em>       <black!>Reset all tools to default permission levels</black!>
<em>/profile</em>      <black!>Manage profiles</black!>
  <em>help</em>        <black!>Show profile help</black!>
  <em>list</em>        <black!>List profiles</black!>
  <em>set</em>         <black!>Set the current profile</black!>
  <em>create</em>      <black!>Create a new profile</black!>
  <em>delete</em>      <black!>Delete a profile</black!>
  <em>rename</em>      <black!>Rename a profile</black!>
<em>/context</em>      <black!>Manage context files for the chat session</black!>
  <em>help</em>        <black!>Show context help</black!>
  <em>show</em>        <black!>Display current context rules configuration [--expand]</black!>
  <em>add</em>         <black!>Add file(s) to context [--global] [--force]</black!>
  <em>rm</em>          <black!>Remove file(s) from context [--global]</black!>
  <em>clear</em>       <black!>Clear all files from current context [--global]</black!>

<cyan,em>Tips:</cyan,em>
<em>!{command}</em>            <black!>Quickly execute a command in your current session</black!>
<em>Ctrl(^) + j</em>           <black!>Insert new-line to provide multi-line prompt. Alternatively, [Alt(⌥) + Enter(⏎)]</black!>

"};

/// Handler for the help command
pub struct HelpCommand;

impl HelpCommand {
    pub fn new() -> Self {
        Self
    }
}

impl CommandHandler for HelpCommand {
    fn name(&self) -> &'static str {
        "help"
    }

    fn description(&self) -> &'static str {
        "Show help information"
    }

    fn usage(&self) -> &'static str {
        "/help"
    }

    fn help(&self) -> String {
        "Shows the help dialogue with available commands and their descriptions.".to_string()
    }

    fn execute<'a>(
        &'a self,
        _args: Vec<&'a str>,
        _ctx: &'a Context,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState>> + Send + 'a>> {
        Box::pin(async move {
            // Return DisplayHelp state with the comprehensive help text
            Ok(ChatState::DisplayHelp {
                help_text: HELP_TEXT.to_string(),
                tool_uses,
                pending_tool_index,
            })
        })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        false // Help command is read-only and doesn't require confirmation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_help_command() {
        let command = HelpCommand::new();
        assert_eq!(command.name(), "help");
        assert_eq!(command.description(), "Show help information");
        assert_eq!(command.usage(), "/help");

        use crate::cli::chat::commands::test_utils::create_test_context;
        let ctx = create_test_context();
        let result = command.execute(vec![], &ctx, None, None).await;
        assert!(result.is_ok());

        if let Ok(state) = result {
            match state {
                ChatState::DisplayHelp { help_text, .. } => {
                    assert!(help_text.contains("/quit"));
                    assert!(help_text.contains("/clear"));
                    assert!(help_text.contains("/help"));
                    assert!(help_text.contains("/context"));
                },
                _ => panic!("Expected DisplayHelp state"),
            }
        }
    }
}
