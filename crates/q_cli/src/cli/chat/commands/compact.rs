use std::future::Future;
use std::pin::Pin;

use color_print::cformat;
use eyre::Result;
use fig_os_shim::Context;

use crate::cli::chat::commands::CommandHandler;
use crate::cli::chat::{
    ChatState,
    QueuedTool,
};

/// Handler for the compact command
pub struct CompactCommand;

impl CompactCommand {
    pub fn new() -> Self {
        Self
    }
}

impl CommandHandler for CompactCommand {
    fn name(&self) -> &'static str {
        "compact"
    }

    fn description(&self) -> &'static str {
        "Summarize the conversation history to free up context space"
    }

    fn usage(&self) -> &'static str {
        "/compact [prompt] [--summary]"
    }

    fn help(&self) -> String {
        compact_help_text()
    }

    fn llm_description(&self) -> String {
        r#"The compact command summarizes the conversation history to free up context space while preserving essential information. This is useful for long-running conversations that may eventually reach memory constraints.

Usage:
- /compact - Summarize the conversation and clear history
- /compact [prompt] - Provide custom guidance for summarization
- /compact --summary - Show the summary after compacting

Examples:
- /compact - Create a standard summary of the conversation
- /compact focus on code examples - Create a summary with emphasis on code examples
- /compact --summary - Create a summary and display it after compacting
- /compact focus on AWS services --summary - Create a focused summary and display it"#.to_string()
    }

    fn execute<'a>(
        &'a self,
        args: Vec<&'a str>,
        _ctx: &'a Context,
        _tool_uses: Option<Vec<QueuedTool>>,
        _pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState>> + Send + 'a>> {
        Box::pin(async move {
            // Parse arguments
            let mut prompt = None;
            let mut show_summary = false;
            let mut _help = false;

            // Check if "help" is the first argument
            if !args.is_empty() && args[0].to_lowercase() == "help" {
                _help = true;
            } else {
                let mut remaining_parts = Vec::new();

                // Parse the parts to handle both prompt and flags
                for part in &args {
                    if *part == "--summary" {
                        show_summary = true;
                    } else {
                        remaining_parts.push(*part);
                    }
                }

                // If we have remaining parts after parsing flags, join them as the prompt
                if !remaining_parts.is_empty() {
                    prompt = Some(remaining_parts.join(" "));
                }
            }

            // Return the Compact command state
            Ok(ChatState::Compact {
                prompt: prompt.map(|s| s.to_string()),
                show_summary,
            })
        })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        false
    }

    fn parse_args<'a>(&self, args: Vec<&'a str>) -> Result<Vec<&'a str>> {
        Ok(args)
    }
}

/// Help text for the compact command
pub fn compact_help_text() -> String {
    cformat!(
        r#"
<magenta,em>Conversation Compaction</magenta,em>

The <em>/compact</em> command summarizes the conversation history to free up context space
while preserving essential information. This is useful for long-running conversations
that may eventually reach memory constraints.

<cyan!>Usage</cyan!>
  <em>/compact</em>                   <black!>Summarize the conversation and clear history</black!>
  <em>/compact [prompt]</em>          <black!>Provide custom guidance for summarization</black!>
  <em>/compact --summary</em>         <black!>Show the summary after compacting</black!>

<cyan!>When to use</cyan!>
- When you see the memory constraint warning message
- When a conversation has been running for a long time"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::chat::commands::test_utils::create_test_context;

    #[tokio::test]
    async fn test_compact_command_help() {
        let command = CompactCommand::new();
        assert_eq!(command.name(), "compact");
        assert_eq!(
            command.description(),
            "Summarize the conversation history to free up context space"
        );
        assert_eq!(command.usage(), "/compact [prompt] [--summary]");

        let ctx = create_test_context();
        let result = command.execute(vec!["help"], &ctx, None, None).await;
        assert!(result.is_ok());

        if let Ok(state) = result {
            match state {
                ChatState::Compact { help, .. } => {
                    assert!(help);
                },
                _ => panic!("Expected Compact state with help=true"),
            }
        }
    }

    #[tokio::test]
    async fn test_compact_command_no_args() {
        let command = CompactCommand::new();
        let ctx = create_test_context();
        let result = command.execute(vec![], &ctx, None, None).await;
        assert!(result.is_ok());

        if let Ok(state) = result {
            match state {
                ChatState::Compact {
                    prompt,
                    show_summary,
                    help,
                } => {
                    assert_eq!(prompt, None);
                    assert_eq!(show_summary, false);
                    assert_eq!(help, false);
                },
                _ => panic!("Expected Compact state"),
            }
        }
    }

    #[tokio::test]
    async fn test_compact_command_with_prompt() {
        let command = CompactCommand::new();
        let ctx = create_test_context();
        let result = command
            .execute(vec!["focus", "on", "code", "examples"], &ctx, None, None)
            .await;
        assert!(result.is_ok());

        if let Ok(state) = result {
            match state {
                ChatState::Compact {
                    prompt,
                    show_summary,
                    help,
                } => {
                    assert_eq!(prompt, Some("focus on code examples".to_string()));
                    assert_eq!(show_summary, false);
                    assert_eq!(help, false);
                },
                _ => panic!("Expected Compact state"),
            }
        }
    }

    #[tokio::test]
    async fn test_compact_command_with_summary_flag() {
        let command = CompactCommand::new();
        let ctx = create_test_context();
        let result = command.execute(vec!["--summary"], &ctx, None, None).await;
        assert!(result.is_ok());

        if let Ok(state) = result {
            match state {
                ChatState::Compact {
                    prompt,
                    show_summary,
                    help,
                } => {
                    assert_eq!(prompt, None);
                    assert_eq!(show_summary, true);
                    assert_eq!(help, false);
                },
                _ => panic!("Expected Compact state"),
            }
        }
    }

    #[tokio::test]
    async fn test_compact_command_with_prompt_and_summary() {
        let command = CompactCommand::new();
        let ctx = create_test_context();
        let result = command
            .execute(vec!["focus", "on", "code", "examples", "--summary"], &ctx, None, None)
            .await;
        assert!(result.is_ok());

        if let Ok(state) = result {
            match state {
                ChatState::Compact {
                    prompt,
                    show_summary,
                    help,
                } => {
                    assert_eq!(prompt, Some("focus on code examples".to_string()));
                    assert_eq!(show_summary, true);
                    assert_eq!(help, false);
                },
                _ => panic!("Expected Compact state"),
            }
        }
    }
}
