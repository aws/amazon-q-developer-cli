// Agent trajectory recorder module
//
// This module provides functionality to record and visualize the trajectory of an agent
// during a conversation, including user instructions, agent reasoning, actions, and responses.

mod command_handler;
mod recorder;
mod repository;
mod visualizer;

use std::path::PathBuf;
use std::sync::{
    Arc,
    Mutex,
};

pub use command_handler::TrajectoryCommandHandler;
pub use recorder::TrajectoryRecorder;
pub use repository::{
    Repository,
    SerializableConversationState,
    Step,
    Trajectory,
};
pub use visualizer::generate_visualization;

/// Configuration for the trajectory recorder
#[derive(Debug, Clone)]
pub struct TrajectoryConfig {
    /// Whether to enable trajectory recording
    pub enabled: bool,
    /// Directory where trajectory data will be stored
    pub output_dir: PathBuf,
    /// Whether to generate visualization automatically
    pub auto_visualize: bool,
    /// Whether to preserve full context in checkpoints
    pub preserve_full_context: bool,
    /// Checkpoint strategy for full context preservation
    pub full_context_strategy: FullContextStrategy,
}

/// Strategy for preserving full context in checkpoints
#[derive(Debug, Clone, PartialEq)]
pub enum FullContextStrategy {
    /// Never preserve full context
    Never,
    /// Always preserve full context
    Always,
    /// Only preserve full context for user input steps
    UserInputOnly,
    /// Only preserve full context for explicit checkpoints
    ExplicitCheckpointsOnly,
}

impl Default for FullContextStrategy {
    fn default() -> Self {
        Self::Never
    }
}

impl Default for TrajectoryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            output_dir: PathBuf::from("q-agent-trajectory"),
            auto_visualize: false,
            preserve_full_context: false,
            full_context_strategy: FullContextStrategy::default(),
        }
    }
}

/// Creates a new trajectory recorder with the given configuration
pub fn create_recorder(config: TrajectoryConfig) -> Arc<Mutex<TrajectoryRecorder>> {
    Arc::new(Mutex::new(TrajectoryRecorder::new(config)))
}

/// Converts a SerializableConversationState to a ConversationState
pub async fn convert_to_conversation_state(
    serializable: &SerializableConversationState,
    context: Arc<fig_os_shim::Context>,
) -> Result<crate::cli::chat::conversation_state::ConversationState, String> {
    use std::collections::HashMap;

    use tracing::warn;

    use crate::cli::chat::conversation_state;

    // Create a new conversation state with empty tool config and default profile
    let mut conversation_state = conversation_state::ConversationState::new(context, HashMap::new(), None).await;

    // We can't directly set the conversation ID as it's private
    // Instead, we'll use the conversation ID when adding messages

    // Add messages from the serializable state
    for message in &serializable.history {
        match message.role.as_str() {
            "user" => {
                // Use append_new_user_message instead of add_user_input
                conversation_state
                    .append_new_user_message(message.content.clone())
                    .await;
            },
            "assistant" => {
                conversation_state.push_assistant_message(fig_api_client::model::AssistantResponseMessage {
                    message_id: None,
                    content: message.content.clone(),
                    tool_uses: None,
                });
            },
            _ => {
                // Skip system messages or other types
            },
        }
    }

    // Restore context manager if available
    if let Some(profile) = serializable.metadata.get("profile") {
        if let Some(context_manager) = &mut conversation_state.context_manager {
            if let Err(e) = context_manager.switch_profile(profile).await {
                warn!("Failed to switch to profile {}: {}", profile, e);
            }
        }
    }

    Ok(conversation_state)
}
