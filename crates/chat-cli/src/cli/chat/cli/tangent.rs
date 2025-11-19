use clap::{
    Args,
    Subcommand,
};
use crossterm::execute;
use crossterm::style::{
    self,
};
use dialoguer::Select;

use crate::cli::chat::conversation::ForgetResult;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::cli::experiment::experiment_manager::{
    ExperimentManager,
    ExperimentName,
};
use crate::constants::CLI_NAME;
use crate::os::Os;
use crate::theme::StyledText;

/// Threshold for warning users about large forget operations
const LARGE_FORGET_THRESHOLD: usize = 5;

#[derive(Debug, PartialEq, Args)]
pub struct TangentArgs {
    #[command(subcommand)]
    pub subcommand: Option<TangentSubcommand>,
}

#[derive(Debug, PartialEq, Subcommand)]
pub enum TangentSubcommand {
    /// Exit tangent mode and keep the last conversation entry (user question + assistant response)
    Tail,
    /// Remove the last N conversation entries from history
    Forget {
        /// Number of conversation entries to remove (optional - will prompt if not provided)
        count: Option<usize>,
    },
}

impl TangentArgs {
    async fn send_tangent_telemetry(os: &Os, session: &ChatSession, duration_seconds: i64) {
        if let Err(err) = os
            .telemetry
            .send_tangent_mode_session(
                &os.database,
                session.conversation.conversation_id().to_string(),
                crate::telemetry::TelemetryResult::Succeeded,
                crate::telemetry::core::TangentModeSessionArgs {
                    duration_seconds,
                    is_forget: false,
                    entries_removed: None,
                },
            )
            .await
        {
            tracing::warn!(?err, "Failed to send tangent mode session telemetry");
        }
    }

    pub async fn execute(self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // Check if tangent mode is enabled
        if !ExperimentManager::is_enabled(os, ExperimentName::TangentMode) {
            execute!(
                session.stderr,
                StyledText::error_fg(),
                style::Print(&format!(
                    "\nTangent mode is disabled. Enable it with: {CLI_NAME} settings chat.enableTangentMode true\n"
                )),
                StyledText::reset(),
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        }

        match self.subcommand {
            Some(TangentSubcommand::Tail) => {
                // Check if checkpoint is enabled
                if ExperimentManager::is_enabled(os, ExperimentName::Checkpoint) {
                    execute!(
                        session.stderr,
                        StyledText::warning_fg(),
                        style::Print(
                            "⚠️ Checkpoint is disabled while in tangent mode. Please exit tangent mode if you want to use checkpoint.\n"
                        ),
                        StyledText::reset(),
                    )?;
                }
                if session.conversation.is_in_tangent_mode() {
                    let duration_seconds = session.conversation.get_tangent_duration_seconds().unwrap_or(0);
                    session.conversation.exit_tangent_mode_with_tail();
                    Self::send_tangent_telemetry(os, session, duration_seconds).await;

                    execute!(
                        session.stderr,
                        StyledText::secondary_fg(),
                        style::Print("Restored conversation from checkpoint ("),
                        StyledText::warning_fg(),
                        style::Print("↯"),
                        StyledText::secondary_fg(),
                        style::Print(") with last conversation entry preserved.\n"),
                        StyledText::reset(),
                    )?;
                } else {
                    execute!(
                        session.stderr,
                        StyledText::error_fg(),
                        style::Print("You need to be in tangent mode to use tail.\n"),
                        StyledText::reset(),
                    )?;
                }
            },
            Some(TangentSubcommand::Forget { count }) => {
                // If no count provided, show interactive selection
                let count = if let Some(c) = count {
                    c
                } else {
                    // Get user prompts (most recent first)
                    let prompts = session.conversation.get_user_prompts();

                    if prompts.is_empty() {
                        execute!(
                            session.stderr,
                            StyledText::error_fg(),
                            style::Print("No messages to remove.\n"),
                            StyledText::reset(),
                        )?;
                        return Ok(ChatState::PromptUser {
                            skip_printing_tools: true,
                        });
                    }

                    // Skip the first (most recent) message, show up to 8 older messages
                    let messages_to_show: Vec<String> = prompts
                        .iter()
                        .skip(1)  // Skip the most recent
                        .take(8)  // Limit to 8
                        .enumerate()
                        .map(|(idx, prompt)| {
                            let words: Vec<&str> = prompt.split_whitespace().take(10).collect();
                            let preview = words.join(" ");
                            let suffix = if prompt.split_whitespace().count() > 10 { "..." } else { "" };
                            let forget_count = idx + 1;
                            format!(
                                "{}{} (forget {} {} after this)",
                                preview,
                                suffix,
                                forget_count,
                                if forget_count == 1 { "message" } else { "messages" }
                            )
                        })
                        .collect();

                    // Add "Clear all messages" option
                    let mut options = messages_to_show;
                    options.push(format!(
                        "Clear all messages (forget {} {})",
                        prompts.len(),
                        if prompts.len() == 1 { "message" } else { "messages" }
                    ));

                    match Select::with_theme(&crate::util::dialoguer_theme())
                        .with_prompt("Select the message to revert back to (newer messages will be forgotten)")
                        .items(&options)
                        .default(0)
                        .interact_on_opt(&dialoguer::console::Term::stdout())
                    {
                        Ok(Some(idx)) => {
                            // If last option selected (Clear all), forget all messages
                            if idx == options.len() - 1 {
                                prompts.len()
                            } else {
                                idx + 1 // Otherwise forget idx+1 messages
                            }
                        },
                        Ok(None) | Err(_) => {
                            return Ok(ChatState::PromptUser {
                                skip_printing_tools: true,
                            });
                        },
                    }
                };

                // Early return for zero count
                if count == 0 {
                    execute!(
                        session.stderr,
                        StyledText::error_fg(),
                        style::Print("Cannot forget 0 messages.\n"),
                        StyledText::reset(),
                    )?;
                    return Ok(ChatState::PromptUser {
                        skip_printing_tools: true,
                    });
                }

                // Warn for large counts
                if count > LARGE_FORGET_THRESHOLD {
                    execute!(
                        session.stderr,
                        StyledText::warning_fg(),
                        style::Print(&format!("Warning: Removing {count} messages. This cannot be undone.\n")),
                        StyledText::reset(),
                    )?;
                }

                let result = session.conversation.forget_last_entries(count);
                match result {
                    ForgetResult::Success(messages_removed) => {
                        // Send telemetry for forget command
                        if let Err(err) = os
                            .telemetry
                            .send_tangent_mode_session(
                                &os.database,
                                session.conversation.conversation_id().to_string(),
                                crate::telemetry::TelemetryResult::Succeeded,
                                crate::telemetry::core::TangentModeSessionArgs {
                                    duration_seconds: 0,
                                    is_forget: true,
                                    entries_removed: Some(messages_removed as i64),
                                },
                            )
                            .await
                        {
                            tracing::warn!(?err, "Failed to send tangent forget telemetry");
                        }

                        execute!(
                            session.stderr,
                            StyledText::secondary_fg(),
                            style::Print(&format!(
                                "Seems like you went on a tangent! Forgetting the last {} {}.\n",
                                messages_removed,
                                if messages_removed == 1 { "message" } else { "messages" }
                            )),
                            StyledText::reset(),
                        )?;
                    },
                    ForgetResult::NoEntries => {
                        execute!(
                            session.stderr,
                            StyledText::error_fg(),
                            style::Print("No messages to remove.\n"),
                            StyledText::reset(),
                        )?;
                    },
                }
            },
            None => {
                if session.conversation.is_in_tangent_mode() {
                    let duration_seconds = session.conversation.get_tangent_duration_seconds().unwrap_or(0);
                    session.conversation.exit_tangent_mode();
                    Self::send_tangent_telemetry(os, session, duration_seconds).await;

                    execute!(
                        session.stderr,
                        StyledText::secondary_fg(),
                        style::Print("Restored conversation from checkpoint ("),
                        StyledText::warning_fg(),
                        style::Print("↯"),
                        StyledText::secondary_fg(),
                        style::Print("). - Returned to main conversation.\n"),
                        StyledText::reset(),
                    )?;
                } else {
                    // Check if checkpoint is enabled
                    if ExperimentManager::is_enabled(os, ExperimentName::Checkpoint) {
                        execute!(
                            session.stderr,
                            StyledText::warning_fg(),
                            style::Print(
                                "⚠️ Checkpoint is disabled while in tangent mode. Please exit tangent mode if you want to use checkpoint.\n"
                            ),
                            StyledText::reset(),
                        )?;
                    }

                    session.conversation.enter_tangent_mode();

                    // Get the configured tangent mode key for display
                    let tangent_key_char = match os
                        .database
                        .settings
                        .get_string(crate::database::settings::Setting::TangentModeKey)
                    {
                        Some(key) if key.len() == 1 => key.chars().next().unwrap_or('t'),
                        _ => 't', // Default to 't' if setting is missing or invalid
                    };
                    let tangent_key_display = format!("ctrl + {}", tangent_key_char.to_lowercase());

                    execute!(
                        session.stderr,
                        StyledText::secondary_fg(),
                        style::Print("Created a conversation checkpoint ("),
                        StyledText::warning_fg(),
                        style::Print("↯"),
                        StyledText::secondary_fg(),
                        style::Print("). Use "),
                        StyledText::success_fg(),
                        style::Print(&tangent_key_display),
                        StyledText::secondary_fg(),
                        style::Print(" or "),
                        StyledText::success_fg(),
                        style::Print("/tangent"),
                        StyledText::secondary_fg(),
                        style::Print(" to restore the conversation later.\n"),
                        style::Print(
                            "Note: this functionality is experimental and may change or be removed in the future.\n"
                        ),
                        StyledText::reset(),
                    )?;
                }
            },
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::cli::agent::Agents;
    use crate::cli::chat::conversation::ConversationState;
    use crate::cli::chat::tool_manager::ToolManager;
    use crate::os::Os;

    #[tokio::test]
    async fn test_tangent_mode_duration_tracking() {
        let mut os = Os::new().await.unwrap();
        let agents = Agents::default();
        let mut tool_manager = ToolManager::default();
        let mut conversation = ConversationState::new(
            "test_conv_id",
            agents,
            tool_manager.load_tools(&mut os, &mut vec![]).await.unwrap(),
            tool_manager,
            None,
            &os,
            false, // mcp_enabled
        )
        .await;

        // Test entering tangent mode
        assert!(!conversation.is_in_tangent_mode());
        conversation.enter_tangent_mode();
        assert!(conversation.is_in_tangent_mode());

        // Should have a duration
        let duration = conversation.get_tangent_duration_seconds();
        assert!(duration.is_some());
        assert!(duration.unwrap() >= 0);

        // Test exiting tangent mode
        conversation.exit_tangent_mode();
        assert!(!conversation.is_in_tangent_mode());
        assert!(conversation.get_tangent_duration_seconds().is_none());
    }
}
