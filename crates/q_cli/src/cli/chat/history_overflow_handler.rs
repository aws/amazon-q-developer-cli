use std::io::Write;
use std::sync::Arc;

use crossterm::style::{Color, Stylize};
use crossterm::{cursor, execute, queue, style, terminal};
use eyre::Result;
use fig_api_client::model::{
    AssistantResponseMessage, ChatMessage, FigConversationState, ToolResult, ToolResultContentBlock,
    ToolResultStatus, UserInputMessage,
};
use fig_os_shim::Context;
use spinners::{Spinner, Spinners};
use tracing::{error, info};

use super::parser::{ResponseEvent, ResponseParser};
use super::{ChatError, ChatState, InputSource};

/// Handles large tool results and history overflow by providing user options to manage the conversation
pub struct HistoryOverflowHandler<'a, W: Write> {
    ctx: &'a Arc<Context>,
    output: &'a mut W,
    input_source: &'a mut InputSource,
    interactive: bool,
    spinner: &'a mut Option<Spinner>,
    client: &'a fig_api_client::StreamingClient,
}

impl<'a, W: Write> HistoryOverflowHandler<'a, W> {
    /// Creates a new HistoryOverflowHandler
    pub fn new(
        ctx: &'a Arc<Context>,
        output: &'a mut W,
        input_source: &'a mut InputSource,
        interactive: bool,
        spinner: &'a mut Option<Spinner>,
        client: &'a fig_api_client::StreamingClient,
    ) -> Self {
        Self {
            ctx,
            output,
            input_source,
            interactive,
            spinner,
            client,
        }
    }

    /// Handles a large tool result by presenting options to the user
    pub async fn handle_large_tool_result(
        &mut self,
        tool_use_id: String,
        name: String,
        conversation_state: &mut super::ConversationState,
        send_tool_use_telemetry: impl Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + '_>>,
    ) -> Result<ChatState, ChatError> {
        // Display message about large tool result
        execute!(
            self.output,
            style::SetForegroundColor(Color::Yellow),
            style::Print("\n⚠️ The tool result is too large to process in one go.\n"),
            style::Print("Choose how you'd like to proceed:\n\n"),
            style::SetForegroundColor(Color::Reset),
            style::Print("1. "),
            style::SetForegroundColor(Color::Green),
            style::Print("Compact"),
            style::SetForegroundColor(Color::Reset),
            style::Print(" - Summarize the history and continue\n"),
            style::Print("2. "),
            style::SetForegroundColor(Color::Red),
            style::Print("Reset"),
            style::SetForegroundColor(Color::Reset),
            style::Print(" - Clear the history and start fresh\n"),
            style::Print("3. "),
            style::SetForegroundColor(Color::Blue),
            style::Print("Retry"),
            style::SetForegroundColor(Color::Reset),
            style::Print(" - Revisit the tool call with more compartmentalization\n\n"),
            style::Print("Enter your choice (1-3): ")
        )?;

        // Get user choice
        let choice = self.input_source.read_line(None)?.unwrap_or_default();
        
        match choice.trim() {
            "1" => self.compact_history_and_continue(tool_use_id, name, conversation_state, send_tool_use_telemetry).await,
            "2" => self.reset_history_and_continue(tool_use_id, name, conversation_state, send_tool_use_telemetry).await,
            "3" => self.retry_with_compartmentalization(tool_use_id, name, conversation_state, send_tool_use_telemetry).await,
            _ => {
                execute!(
                    self.output,
                    style::SetForegroundColor(Color::Red),
                    style::Print("\nInvalid choice. Defaulting to compact.\n\n"),
                    style::SetForegroundColor(Color::Reset)
                )?;
                self.compact_history_and_continue(tool_use_id, name, conversation_state, send_tool_use_telemetry).await
            }
        }
    }

    /// Handles history overflow by presenting options to the user
    pub async fn handle_history_overflow(
        &mut self,
        conversation_state: &mut super::ConversationState,
    ) -> Result<ChatState, ChatError> {
        // Display message about history overflow
        execute!(
            self.output,
            style::SetForegroundColor(Color::Yellow),
            style::Print("\n⚠️ Your conversation history is getting too large.\n"),
            style::Print("Choose how you'd like to proceed:\n\n"),
            style::SetForegroundColor(Color::Reset),
            style::Print("1. "),
            style::SetForegroundColor(Color::Green),
            style::Print("Compact"),
            style::SetForegroundColor(Color::Reset),
            style::Print(" - Summarize the history and continue\n"),
            style::Print("2. "),
            style::SetForegroundColor(Color::Red),
            style::Print("Reset"),
            style::SetForegroundColor(Color::Reset),
            style::Print(" - Clear the history and start fresh\n"),
            style::Print("3. "),
            style::SetForegroundColor(Color::Blue),
            style::Print("Continue"),
            style::SetForegroundColor(Color::Reset),
            style::Print(" - Keep the history as is (may cause issues)\n\n"),
            style::Print("Enter your choice (1-3): ")
        )?;

        // Get user choice
        let choice = self.input_source.read_line(None)?.unwrap_or_default();
        
        match choice.trim() {
            "1" => self.compact_history_for_overflow(conversation_state).await,
            "2" => self.reset_history_for_overflow(conversation_state).await,
            "3" => self.continue_with_warning(conversation_state).await,
            _ => {
                execute!(
                    self.output,
                    style::SetForegroundColor(Color::Red),
                    style::Print("\nInvalid choice. Defaulting to compact.\n\n"),
                    style::SetForegroundColor(Color::Reset)
                )?;
                self.compact_history_for_overflow(conversation_state).await
            }
        }
    }

    /// Compacts the conversation history by summarizing it (for tool results)
    async fn compact_history_and_continue(
        &mut self,
        tool_use_id: String,
        name: String,
        conversation_state: &mut super::ConversationState,
        send_tool_use_telemetry: impl Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + '_>>,
    ) -> Result<ChatState, ChatError> {
        execute!(
            self.output,
            style::SetForegroundColor(Color::Green),
            style::Print("\nCompacting conversation history...\n"),
            style::Print("This may take a moment...\n\n"),
            style::SetForegroundColor(Color::Reset)
        )?;
        
        if self.interactive {
            *self.spinner = Some(Spinner::new(Spinners::Dots, "Summarizing conversation...".to_string()));
        }
        
        // Extract the current conversation history
        let current_history = conversation_state.extract_history();
        
        // Create a request to summarize the conversation
        let summarize_request = UserInputMessage {
            content: "I need you to summarize our conversation so far into a concise summary. Focus on the key points, decisions, and context that would be important for continuing our discussion. Format your response as a summary only, without any introduction or meta-commentary.".to_string(),
            user_input_message_context: None,
            user_intent: None,
        };
        
        // Create a temporary conversation state for the summarization request
        let temp_conversation = FigConversationState {
            conversation_id: Some("summary_request".to_string()),
            user_input_message: summarize_request,
            history: Some(current_history),
        };
        
        // Send the summarization request
        let summary_response = match self.client.send_message(temp_conversation).await {
            Ok(response) => {
                // Process the response to extract the summary
                let mut summary_text = String::new();
                let mut parser = ResponseParser::new(response);
                
                loop {
                    match parser.recv().await {
                        Ok(ResponseEvent::AssistantText(text)) => {
                            summary_text.push_str(&text);
                        },
                        Ok(ResponseEvent::EndStream { .. }) => break,
                        Ok(_) => continue, // Ignore other events
                        Err(e) => {
                            error!(?e, "Error receiving summary response");
                            return Err(ChatError::Custom("Failed to summarize conversation".into()));
                        }
                    }
                }
                
                summary_text
            },
            Err(e) => {
                error!(?e, "Failed to get conversation summary");
                "Previous conversation summary unavailable due to an error.".to_string()
            }
        };
        
        // Stop the spinner if it exists
        if let Some(spinner) = self.spinner.take() {
            spinner.stop();
        }
        
        // Clear the conversation history
        conversation_state.clear();
        
        // Add the summary as a system message
        conversation_state.append_new_user_message(
            "Here's a summary of our previous conversation:".to_string()
        ).await;
        
        conversation_state.push_assistant_message(AssistantResponseMessage {
            message_id: None,
            content: summary_text,
            tool_uses: None,
        });
        
        // Add the error tool result
        let tool_results = vec![ToolResult {
            tool_use_id,
            content: vec![ToolResultContentBlock::Text(
                "The tool result was too large. I'll break down this task into smaller steps.".to_string(),
            )],
            status: ToolResultStatus::Error,
        }];
        
        // Add a message explaining what happened
        conversation_state.append_new_user_message(
            format!("The result from the {} tool was too large to process. Please continue with the task but break it down into smaller steps.", name)
        ).await;
        
        send_tool_use_telemetry().await;
        
        Ok(ChatState::HandleResponseStream(
            self.client
                .send_message(conversation_state.as_sendable_conversation_state().await)
                .await?,
        ))
    }

    /// Compacts the conversation history by summarizing it (for history overflow)
    async fn compact_history_for_overflow(
        &mut self,
        conversation_state: &mut super::ConversationState,
    ) -> Result<ChatState, ChatError> {
        execute!(
            self.output,
            style::SetForegroundColor(Color::Green),
            style::Print("\nCompacting conversation history...\n"),
            style::Print("This may take a moment...\n\n"),
            style::SetForegroundColor(Color::Reset)
        )?;
        
        if self.interactive {
            *self.spinner = Some(Spinner::new(Spinners::Dots, "Summarizing conversation...".to_string()));
        }
        
        // Extract the current conversation history
        let current_history = conversation_state.extract_history();
        
        // Create a request to summarize the conversation
        let summarize_request = UserInputMessage {
            content: "I need you to summarize our conversation so far into a concise summary. Focus on the key points, decisions, and context that would be important for continuing our discussion. Format your response as a summary only, without any introduction or meta-commentary.".to_string(),
            user_input_message_context: None,
            user_intent: None,
        };
        
        // Create a temporary conversation state for the summarization request
        let temp_conversation = FigConversationState {
            conversation_id: Some("summary_request".to_string()),
            user_input_message: summarize_request,
            history: Some(current_history),
        };
        
        // Send the summarization request
        let summary_response = match self.client.send_message(temp_conversation).await {
            Ok(response) => {
                // Process the response to extract the summary
                let mut summary_text = String::new();
                let mut parser = ResponseParser::new(response);
                
                loop {
                    match parser.recv().await {
                        Ok(ResponseEvent::AssistantText(text)) => {
                            summary_text.push_str(&text);
                        },
                        Ok(ResponseEvent::EndStream { .. }) => break,
                        Ok(_) => continue, // Ignore other events
                        Err(e) => {
                            error!(?e, "Error receiving summary response");
                            return Err(ChatError::Custom("Failed to summarize conversation".into()));
                        }
                    }
                }
                
                summary_text
            },
            Err(e) => {
                error!(?e, "Failed to get conversation summary");
                "Previous conversation summary unavailable due to an error.".to_string()
            }
        };
        
        // Stop the spinner if it exists
        if let Some(spinner) = self.spinner.take() {
            spinner.stop();
        }
        
        // Clear the conversation history
        conversation_state.clear();
        
        // Add the summary as a system message
        conversation_state.append_new_user_message(
            "Here's a summary of our previous conversation:".to_string()
        ).await;
        
        conversation_state.push_assistant_message(AssistantResponseMessage {
            message_id: None,
            content: summary_text,
            tool_uses: None,
        });
        
        // Add a message explaining what happened
        conversation_state.append_new_user_message(
            "The conversation history was getting too large, so I've summarized it. Let's continue from here.".to_string()
        ).await;
        
        Ok(ChatState::HandleResponseStream(
            self.client
                .send_message(conversation_state.as_sendable_conversation_state().await)
                .await?,
        ))
    }

    /// Resets the conversation history and starts fresh (for tool results)
    async fn reset_history_and_continue(
        &mut self,
        tool_use_id: String,
        name: String,
        conversation_state: &mut super::ConversationState,
        send_tool_use_telemetry: impl Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + '_>>,
    ) -> Result<ChatState, ChatError> {
        execute!(
            self.output,
            style::SetForegroundColor(Color::Red),
            style::Print("\nClearing conversation history...\n\n"),
            style::SetForegroundColor(Color::Reset)
        )?;
        
        // Clear the conversation history
        conversation_state.clear();
        
        // Add a new message explaining what happened
        conversation_state.append_new_user_message(
            format!("I was trying to use the {} tool but the result was too large. Let's start fresh. Can you help me with this task by breaking it down into smaller steps?", name)
        ).await;
        
        send_tool_use_telemetry().await;
        
        Ok(ChatState::HandleResponseStream(
            self.client
                .send_message(conversation_state.as_sendable_conversation_state().await)
                .await?,
        ))
    }

    /// Resets the conversation history and starts fresh (for history overflow)
    async fn reset_history_for_overflow(
        &mut self,
        conversation_state: &mut super::ConversationState,
    ) -> Result<ChatState, ChatError> {
        execute!(
            self.output,
            style::SetForegroundColor(Color::Red),
            style::Print("\nClearing conversation history...\n\n"),
            style::SetForegroundColor(Color::Reset)
        )?;
        
        // Clear the conversation history
        conversation_state.clear();
        
        // Add a new message explaining what happened
        conversation_state.append_new_user_message(
            "The conversation history was getting too large, so I've cleared it. Let's start fresh.".to_string()
        ).await;
        
        Ok(ChatState::HandleResponseStream(
            self.client
                .send_message(conversation_state.as_sendable_conversation_state().await)
                .await?,
        ))
    }

    /// Retries the tool call with better compartmentalization
    async fn retry_with_compartmentalization(
        &mut self,
        tool_use_id: String,
        name: String,
        conversation_state: &mut super::ConversationState,
        send_tool_use_telemetry: impl Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + '_>>,
    ) -> Result<ChatState, ChatError> {
        execute!(
            self.output,
            style::SetForegroundColor(Color::Blue),
            style::Print("\nRetrying with better compartmentalization...\n\n"),
            style::SetForegroundColor(Color::Reset)
        )?;
        
        // Add error tool result
        let tool_results = vec![ToolResult {
            tool_use_id,
            content: vec![ToolResultContentBlock::Text(
                "The tool result was too large. Please break this task into multiple smaller tool calls.".to_string(),
            )],
            status: ToolResultStatus::Error,
        }];
        
        conversation_state.add_tool_results(tool_results);
        send_tool_use_telemetry().await;
        
        Ok(ChatState::HandleResponseStream(
            self.client
                .send_message(conversation_state.as_sendable_conversation_state().await)
                .await?,
        ))
    }

    /// Continues the conversation with a warning about large history
    async fn continue_with_warning(
        &mut self,
        conversation_state: &mut super::ConversationState,
    ) -> Result<ChatState, ChatError> {
        execute!(
            self.output,
            style::SetForegroundColor(Color::Blue),
            style::Print("\nContinuing with current history...\n"),
            style::Print("Note: Large conversation history may cause issues with context or response quality.\n\n"),
            style::SetForegroundColor(Color::Reset)
        )?;
        
        // Force the history to be valid without clearing
        conversation_state.force_valid_history();
        
        // Add a message explaining the situation
        conversation_state.append_new_user_message(
            "I've chosen to continue with the full conversation history. Please let me know if you notice any issues with context or response quality.".to_string()
        ).await;
        
        Ok(ChatState::HandleResponseStream(
            self.client
                .send_message(conversation_state.as_sendable_conversation_state().await)
                .await?,
        ))
    }
}
