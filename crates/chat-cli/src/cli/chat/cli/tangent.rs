use clap::{
    Args,
    Subcommand,
};
use crossterm::execute;
use crossterm::style::{
    self,
    Color,
};

use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::cli::experiment::experiment_manager::{
    ExperimentManager,
    ExperimentName,
};
use crate::os::Os;

use super::compact::CompactArgs;

#[derive(Debug, PartialEq, Args)]
pub struct TangentArgs {
    #[command(subcommand)]
    pub subcommand: Option<TangentSubcommand>,
}

#[derive(Debug, PartialEq, Subcommand)]
pub enum TangentSubcommand {
    /// Exit tangent mode and keep the last conversation entry (user question + assistant response)
    Tail,
    /// Compact tangent conversation and return to main session with summary
    Compact(CompactArgs),
}

impl TangentArgs {
    async fn send_tangent_telemetry(os: &Os, session: &ChatSession, duration_seconds: i64) {
        if let Err(err) = os
            .telemetry
            .send_tangent_mode_session(
                &os.database,
                session.conversation.conversation_id().to_string(),
                crate::telemetry::TelemetryResult::Succeeded,
                crate::telemetry::core::TangentModeSessionArgs { duration_seconds },
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
                style::SetForegroundColor(Color::Red),
                style::Print("\nTangent mode is disabled. Enable it with: q settings chat.enableTangentMode true\n"),
                style::SetForegroundColor(Color::Reset)
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        }

        match self.subcommand {
            Some(TangentSubcommand::Compact(compact_args)) => {
                if !session.conversation.is_in_tangent_mode() {
                    execute!(
                        session.stderr,
                        style::SetForegroundColor(Color::Red),
                        style::Print("You need to be in tangent mode to use compact.\n"),
                        style::SetForegroundColor(Color::Reset)
                    )?;
                    return Ok(ChatState::PromptUser {
                        skip_printing_tools: true,
                    });
                }

                // If tangent conversation is empty, just exit
                if session.conversation.is_tangent_empty() {
                    let duration_seconds = session.conversation.get_tangent_duration_seconds().unwrap_or(0);
                    session.conversation.exit_tangent_mode();
                    Self::send_tangent_telemetry(os, session, duration_seconds).await;

                    execute!(
                        session.stderr,
                        style::SetForegroundColor(Color::DarkGrey),
                        style::Print("Tangent conversation was empty. Restored conversation from checkpoint ("),
                        style::SetForegroundColor(Color::Yellow),
                        style::Print("↯"),
                        style::SetForegroundColor(Color::DarkGrey),
                        style::Print(").\n"),
                        style::SetForegroundColor(Color::Reset)
                    )?;
                    return Ok(ChatState::PromptUser {
                        skip_printing_tools: true,
                    });
                }

                // Execute compact on the tangent conversation with error handling
                let duration_seconds = session.conversation.get_tangent_duration_seconds().unwrap_or(0);
                let result = compact_args.execute(&mut os.clone(), session).await;

                // Handle result - exit tangent mode regardless of success/failure
                match result {
                    Ok(state) => {
                        // Success: exit tangent mode with the summary
                        session.conversation.exit_tangent_mode_with_compact();
                        Self::send_tangent_telemetry(os, session, duration_seconds).await;

                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::DarkGrey),
                            style::Print("Restored conversation from checkpoint ("),
                            style::SetForegroundColor(Color::Yellow),
                            style::Print("↯"),
                            style::SetForegroundColor(Color::DarkGrey),
                            style::Print(") with tangent summary preserved.\n"),
                            style::SetForegroundColor(Color::Reset)
                        )?;

                        return Ok(state);
                    },
                    Err(err) => {
                        // Error: exit tangent mode without preserving anything
                        session.conversation.exit_tangent_mode();
                        Self::send_tangent_telemetry(os, session, duration_seconds).await;

                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Yellow),
                            style::Print("Compact failed. Restored conversation from checkpoint ("),
                            style::SetForegroundColor(Color::Yellow),
                            style::Print("↯"),
                            style::SetForegroundColor(Color::Yellow),
                            style::Print(") without changes.\n"),
                            style::SetForegroundColor(Color::Reset)
                        )?;

                        return Err(err);
                    }
                }
            },
            Some(TangentSubcommand::Tail) => {
                // Check if checkpoint is enabled
                if ExperimentManager::is_enabled(os, ExperimentName::Checkpoint) {
                    execute!(
                        session.stderr,
                        style::SetForegroundColor(Color::Yellow),
                        style::Print(
                            "⚠️ Checkpoint is disabled while in tangent mode. Please exit tangent mode if you want to use checkpoint.\n"
                        ),
                        style::SetForegroundColor(Color::Reset),
                    )?;
                }
                if session.conversation.is_in_tangent_mode() {
                    let duration_seconds = session.conversation.get_tangent_duration_seconds().unwrap_or(0);
                    session.conversation.exit_tangent_mode_with_tail();
                    Self::send_tangent_telemetry(os, session, duration_seconds).await;

                    execute!(
                        session.stderr,
                        style::SetForegroundColor(Color::DarkGrey),
                        style::Print("Restored conversation from checkpoint ("),
                        style::SetForegroundColor(Color::Yellow),
                        style::Print("↯"),
                        style::SetForegroundColor(Color::DarkGrey),
                        style::Print(") with last conversation entry preserved.\n"),
                        style::SetForegroundColor(Color::Reset)
                    )?;
                } else {
                    execute!(
                        session.stderr,
                        style::SetForegroundColor(Color::Red),
                        style::Print("You need to be in tangent mode to use tail.\n"),
                        style::SetForegroundColor(Color::Reset)
                    )?;
                }
            },
            None => {
                if session.conversation.is_in_tangent_mode() {
                    let duration_seconds = session.conversation.get_tangent_duration_seconds().unwrap_or(0);
                    session.conversation.exit_tangent_mode();
                    Self::send_tangent_telemetry(os, session, duration_seconds).await;

                    execute!(
                        session.stderr,
                        style::SetForegroundColor(Color::DarkGrey),
                        style::Print("Restored conversation from checkpoint ("),
                        style::SetForegroundColor(Color::Yellow),
                        style::Print("↯"),
                        style::SetForegroundColor(Color::DarkGrey),
                        style::Print("). - Returned to main conversation.\n"),
                        style::SetForegroundColor(Color::Reset)
                    )?;
                } else {
                    // Check if checkpoint is enabled
                    if ExperimentManager::is_enabled(os, ExperimentName::Checkpoint) {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Yellow),
                            style::Print(
                                "⚠️ Checkpoint is disabled while in tangent mode. Please exit tangent mode if you want to use checkpoint.\n"
                            ),
                            style::SetForegroundColor(Color::Reset),
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
                        style::SetForegroundColor(Color::DarkGrey),
                        style::Print("Created a conversation checkpoint ("),
                        style::SetForegroundColor(Color::Yellow),
                        style::Print("↯"),
                        style::SetForegroundColor(Color::DarkGrey),
                        style::Print("). Use "),
                        style::SetForegroundColor(Color::Green),
                        style::Print(&tangent_key_display),
                        style::SetForegroundColor(Color::DarkGrey),
                        style::Print(" or "),
                        style::SetForegroundColor(Color::Green),
                        style::Print("/tangent"),
                        style::SetForegroundColor(Color::DarkGrey),
                        style::Print(" to restore the conversation later.\n"),
                        style::Print(
                            "Note: this functionality is experimental and may change or be removed in the future.\n"
                        ),
                        style::SetForegroundColor(Color::Reset)
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

    #[tokio::test]
    async fn test_exit_tangent_mode_with_compact() {
        use crate::cli::chat::message::{AssistantMessage, UserMessage};
        use crate::cli::chat::RequestMetadata;
        
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
            false,
        )
        .await;

        // Add some history to main conversation
        let main_user = UserMessage::new_prompt("Main question".to_string(), None);
        let main_assistant = AssistantMessage::new_response(None, "Main answer".to_string());
        conversation.append_to_history_for_test(main_user, main_assistant, None);
        
        let main_history_len = conversation.history().len();
        assert_eq!(main_history_len, 1, "Main conversation should have 1 entry");

        // Enter tangent mode
        conversation.enter_tangent_mode();
        assert!(conversation.is_in_tangent_mode());
        
        // Add tangent conversation
        let tangent_user = UserMessage::new_prompt("Tangent question".to_string(), None);
        let tangent_assistant = AssistantMessage::new_response(None, "Tangent answer".to_string());
        conversation.append_to_history_for_test(tangent_user, tangent_assistant, None);
        
        // Simulate compact creating a summary
        let summary_text = "Summary of tangent conversation".to_string();
        let summary_metadata = RequestMetadata {
            request_id: Some("test_request".to_string()),
            message_id: "test_message".to_string(),
            ..Default::default()
        };
        conversation.set_latest_summary_for_test(summary_text.clone(), summary_metadata.clone());
        
        let tangent_history_len = conversation.history().len();
        assert_eq!(tangent_history_len, 2, "Tangent should have added 1 entry (total 2)");

        // Exit with compact
        conversation.exit_tangent_mode_with_compact();
        assert!(!conversation.is_in_tangent_mode());
        
        // Verify main conversation was restored WITH summary entry added
        assert_eq!(
            conversation.history().len(), 
            main_history_len + 1,
            "Main conversation should have original entries plus summary entry"
        );
        
        // Verify the last entry is the summary entry
        let last_entry = conversation.history().back().unwrap();
        assert_eq!(
            last_entry.user.prompt(),
            Some("[Tangent conversation]"),
            "Summary entry user message should be tangent marker"
        );
        assert!(
            last_entry.assistant.content().contains(&summary_text),
            "Summary entry assistant message should contain the summary"
        );
        
        // Verify latest_summary was NOT set (summary is in history instead)
        let summary_info = conversation.get_latest_summary_for_test();
        assert!(
            summary_info.is_none(),
            "latest_summary should be None (summary is in history)"
        );
    }

    #[tokio::test]
    async fn test_multiple_tangent_compacts() {
        use crate::cli::chat::message::{AssistantMessage, UserMessage};
        use crate::cli::chat::RequestMetadata;
        
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
            false,
        )
        .await;

        // Add main conversation
        conversation.append_to_history_for_test(
            UserMessage::new_prompt("Main Q1".to_string(), None),
            AssistantMessage::new_response(None, "Main A1".to_string()),
            None,
        );

        // First tangent
        conversation.enter_tangent_mode();
        conversation.append_to_history_for_test(
            UserMessage::new_prompt("Tangent1 Q".to_string(), None),
            AssistantMessage::new_response(None, "Tangent1 A".to_string()),
            None,
        );
        conversation.set_latest_summary_for_test(
            "Summary of tangent 1".to_string(),
            RequestMetadata {
                request_id: Some("req1".to_string()),
                message_id: "msg1".to_string(),
                ..Default::default()
            },
        );
        conversation.exit_tangent_mode_with_compact();

        assert_eq!(conversation.history().len(), 2, "Should have main + tangent1 summary");
        assert_eq!(
            conversation.history().back().unwrap().user.prompt(),
            Some("[Tangent conversation]"),
            "Last entry should be tangent1 summary"
        );

        // Add more main conversation
        conversation.append_to_history_for_test(
            UserMessage::new_prompt("Main Q2".to_string(), None),
            AssistantMessage::new_response(None, "Main A2".to_string()),
            None,
        );

        // Second tangent
        conversation.enter_tangent_mode();
        conversation.append_to_history_for_test(
            UserMessage::new_prompt("Tangent2 Q".to_string(), None),
            AssistantMessage::new_response(None, "Tangent2 A".to_string()),
            None,
        );
        conversation.set_latest_summary_for_test(
            "Summary of tangent 2".to_string(),
            RequestMetadata {
                request_id: Some("req2".to_string()),
                message_id: "msg2".to_string(),
                ..Default::default()
            },
        );
        conversation.exit_tangent_mode_with_compact();

        // Verify final state
        assert_eq!(
            conversation.history().len(),
            4,
            "Should have: main1, tangent1_summary, main2, tangent2_summary"
        );

        // Verify chronological order
        let history: Vec<_> = conversation.history().iter().collect();
        assert_eq!(history[0].user.prompt(), Some("Main Q1"));
        assert_eq!(history[1].user.prompt(), Some("[Tangent conversation]"));
        assert!(history[1].assistant.content().contains("Summary of tangent 1"));
        assert_eq!(history[2].user.prompt(), Some("Main Q2"));
        assert_eq!(history[3].user.prompt(), Some("[Tangent conversation]"));
        assert!(history[3].assistant.content().contains("Summary of tangent 2"));
    }
}
