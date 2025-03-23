use std::collections::HashMap;
use std::path::PathBuf;

use tracing::{
    debug,
    error,
    info,
    warn,
};

use super::repository::{
    ActionResult,
    AgentAction,
    CompleteMessage,
    FullModelContext,
    Repository,
    SerializableChatMessage,
    SerializableConversationState,
    SerializableTool,
    SerializableUserInputMessage,
};
use super::{
    FullContextStrategy,
    TrajectoryConfig,
    visualizer,
};
use crate::cli::chat::conversation_state::ConversationState;

/// Records the trajectory of an agent during a conversation
pub struct TrajectoryRecorder {
    /// Configuration for the recorder
    config: TrajectoryConfig,
    /// Repository for storing trajectory data
    repository: Option<Repository>,
    /// ID of the last step
    last_step_id: Option<String>,
    /// Current user instruction being processed
    current_instruction: Option<String>,
    /// Whether the recorder is enabled
    enabled: bool,
    /// Whether the browser has been opened for visualization
    browser_opened: bool,
}

impl TrajectoryRecorder {
    /// Creates a new trajectory recorder
    pub fn new(config: TrajectoryConfig) -> Self {
        let repository = if config.enabled {
            match Repository::load(&config.output_dir) {
                Ok(repo) => Some(repo),
                Err(e) => {
                    error!("Failed to load repository: {}", e);
                    None
                },
            }
        } else {
            None
        };

        Self {
            enabled: config.enabled,
            config,
            repository,
            last_step_id: None,
            current_instruction: None,
            browser_opened: false,
        }
    }

    /// Records a user instruction
    pub fn record_user_instruction(&mut self, instruction: &str) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }

        debug!("Recording user instruction: {}", instruction);
        self.current_instruction = Some(instruction.to_string());

        // Determine if we should preserve full context based on config
        let _preserve_full_context = self.should_preserve_full_context(false);

        // Create a step for the user instruction
        if let Some(repo) = &mut self.repository {
            // Check if this is a command (starts with /)
            let is_command = instruction.trim().starts_with('/');

            // Create a step builder with appropriate tags
            let mut step_builder = repo
                .step_builder()
                .parent_id(self.last_step_id.clone())
                .user_instruction(instruction)
                .category("user_instruction");

            // Add appropriate tag based on whether this is a command
            if is_command {
                step_builder = step_builder.tag("user-command");
            } else {
                step_builder = step_builder.tag("user-input");
            }

            let step = step_builder.build();

            let step_id = repo.record_step(step)?;
            self.last_step_id = Some(step_id.clone());
            debug!("Created step for user instruction: {}", step_id);
        }

        Ok(())
    }

    /// Records agent reasoning
    #[allow(dead_code)]
    pub fn record_reasoning(&mut self, reasoning: &str) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }

        debug!("Recording agent reasoning");

        if let Some(repo) = &mut self.repository {
            let step = repo
                .step_builder()
                .parent_id(self.last_step_id.clone())
                .agent_reasoning(reasoning)
                .category("reasoning")
                .tag("reasoning")
                .build();

            let step_id = repo.record_step(step)?;
            self.last_step_id = Some(step_id.clone());
            debug!("Created step for agent reasoning: {}", step_id);
        }

        Ok(())
    }

    /// Records a tool use by the agent
    pub fn record_tool_use(
        &mut self,
        tool_name: &str,
        parameters: HashMap<String, serde_json::Value>,
        description: Option<&str>,
    ) -> Result<String, String> {
        if !self.enabled {
            return Ok(String::new());
        }

        debug!("Recording tool use: {}", tool_name);

        let step_id = if let Some(repo) = &mut self.repository {
            let action = AgentAction {
                action_type: "tool_use".to_string(),
                name: Some(tool_name.to_string()),
                parameters,
                description: description.map(|s| s.to_string()),
            };

            let step = repo
                .step_builder()
                .parent_id(self.last_step_id.clone())
                .agent_action(action)
                .category("tool_use")
                .tag("tool-use")
                .build();

            let step_id = repo.record_step(step)?;
            self.last_step_id = Some(step_id.clone());
            debug!("Created step for tool use: {}", step_id);
            step_id
        } else {
            String::new()
        };

        Ok(step_id)
    }

    /// Records the result of a tool use
    pub fn record_tool_result(
        &mut self,
        step_id: &str,
        success: bool,
        data: Option<serde_json::Value>,
        error_message: Option<&str>,
    ) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }

        debug!("Recording tool result for step: {}", step_id);

        if let Some(repo) = &mut self.repository {
            // Find the step
            if let Some(step) = repo.steps.get_mut(step_id) {
                let result = ActionResult {
                    success,
                    data,
                    error_message: error_message.map(|s| s.to_string()),
                };

                step.action_result = Some(result);
                repo.save()?;
                debug!("Updated step with tool result: {}", step_id);
            }
        }

        Ok(())
    }

    /// Records the agent's response to the user
    #[allow(dead_code)]
    pub fn record_response(&mut self, response: &str) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }

        debug!("Recording agent response");

        if let Some(repo) = &mut self.repository {
            let step = repo
                .step_builder()
                .parent_id(self.last_step_id.clone())
                .agent_response(response)
                .category("response")
                .tag("response")
                .build();

            let step_id = repo.record_step(step)?;
            self.last_step_id = Some(step_id.clone());
            debug!("Created step for agent response: {}", step_id);
        }

        // Generate visualization if configured
        if self.config.auto_visualize {
            self.generate_visualization()?;
        }

        Ok(())
    }

    /// Creates a checkpoint of the current conversation state
    pub fn create_checkpoint(&mut self, label: &str, conversation_state: &ConversationState) -> Result<String, String> {
        if !self.enabled {
            return Err("Trajectory recording is not enabled".to_string());
        }

        debug!("Creating checkpoint: {}", label);

        // Determine if we should preserve full context based on config
        let preserve_full_context = self.should_preserve_full_context(true);

        // Create serializable conversation state
        let serializable_state = self.serialize_conversation_state(conversation_state, preserve_full_context)?;

        if let Some(repo) = &mut self.repository {
            let step = repo
                .step_builder()
                .parent_id(self.last_step_id.clone())
                .category("checkpoint")
                .tag("checkpoint")
                .tag(label)
                .conversation_state(serializable_state)
                .build();

            let step_id = repo.record_step(step)?;
            self.last_step_id = Some(step_id.clone());
            debug!("Created checkpoint step: {}", step_id);

            Ok(step_id)
        } else {
            Err("Repository not initialized".to_string())
        }
    }

    /// Restores a conversation state from a checkpoint
    pub fn restore_from_checkpoint(
        &self,
        checkpoint_id_or_label: &str,
    ) -> Result<SerializableConversationState, String> {
        if !self.enabled {
            return Err("Trajectory recording is not enabled".to_string());
        }

        if let Some(repo) = &self.repository {
            // Find the checkpoint
            let checkpoint = repo
                .get_checkpoint(checkpoint_id_or_label)
                .ok_or_else(|| format!("Checkpoint not found: {}", checkpoint_id_or_label))?;

            // Get the conversation state
            let state = checkpoint
                .conversation_state
                .as_ref()
                .ok_or_else(|| "Checkpoint does not contain conversation state".to_string())?;

            Ok(state.clone())
        } else {
            Err("Repository not initialized".to_string())
        }
    }

    /// Lists all available checkpoints
    pub fn list_checkpoints(&self) -> Result<Vec<(String, String, String)>, String> {
        if !self.enabled {
            return Err("Trajectory recording is not enabled".to_string());
        }

        if let Some(repo) = &self.repository {
            let checkpoints = repo
                .list_checkpoints()
                .iter()
                .map(|(id, step)| {
                    let label = step
                        .tags
                        .iter()
                        .find(|&tag| tag != "checkpoint")
                        .unwrap_or(&"".to_string())
                        .clone();

                    ((*id).to_string(), label, step.timestamp.clone())
                })
                .collect();

            Ok(checkpoints)
        } else {
            Err("Repository not initialized".to_string())
        }
    }

    /// Serializes a conversation state to a serializable format
    fn serialize_conversation_state(
        &self,
        conversation_state: &ConversationState,
        preserve_full_context: bool,
    ) -> Result<SerializableConversationState, String> {
        // Extract context files if available
        let context_files = if let Some(context_manager) = &conversation_state.context_manager {
            // Use blocking to avoid async issues
            match context_manager.get_context_files_sync(false) {
                Ok(files) => files,
                Err(e) => {
                    warn!("Failed to get context files: {}", e);
                    HashMap::new()
                },
            }
        } else {
            HashMap::new()
        };

        // Create serializable history with optional full content
        let history = conversation_state
            .history()
            .iter()
            .map(|msg| {
                let mut serializable = SerializableChatMessage {
                    role: "user".to_string(),               // Simplified
                    content: "message content".to_string(), // Simplified
                    full_content: None,
                };

                if preserve_full_context {
                    // Store the actual complete content when full context preservation is enabled
                    serializable.full_content = Some(format!("{:?}", msg));
                }

                serializable
            })
            .collect();

        // Create serializable tools with optional full definitions
        let tools = conversation_state
            .tools()
            .iter()
            .map(|tool| {
                let mut serializable = SerializableTool {
                    name: format!("{:?}", tool),
                    description: "Tool description".to_string(),
                    full_definition: None,
                };

                if preserve_full_context {
                    // Store the complete tool definition when full context preservation is enabled
                    serializable.full_definition = Some(format!("{:?}", tool));
                }

                serializable
            })
            .collect();

        // Create full model context if needed
        let full_context = if preserve_full_context {
            // Collect complete messages with full content
            let complete_messages = conversation_state
                .history()
                .iter()
                .map(|msg| {
                    CompleteMessage {
                        role: "user".to_string(),      // Would be actual role in real implementation
                        content: format!("{:?}", msg), // Full content
                        tool_calls: None,              // Would be actual tool calls in real implementation
                        tool_results: None,            // Would be actual tool results in real implementation
                    }
                })
                .collect();

            // Collect context file contents
            let mut context_files_content = HashMap::new();
            if let Some(_context_manager) = &conversation_state.context_manager {
                for (path, _) in &context_files {
                    if let Ok(content) = std::fs::read_to_string(path) {
                        context_files_content.insert(path.clone(), content);
                    }
                }
            }

            Some(FullModelContext {
                system_prompt: "System prompt would go here".to_string(), // Would be actual system prompt
                complete_messages,
                context_files_content,
                model_parameters: HashMap::new(), // Would be actual model parameters
                token_count_estimate: 0,          // Would be actual token count estimate
            })
        } else {
            None
        };

        // Create serializable state
        let serializable_state = SerializableConversationState {
            conversation_id: conversation_state.conversation_id().to_string(),
            history,
            next_message: conversation_state
                .next_message()
                .map(|msg| SerializableUserInputMessage::from(msg)),
            tools,
            context_files,
            env_state: Some(HashMap::new()),   // Simplified
            shell_state: Some(HashMap::new()), // Simplified
            metadata: HashMap::new(),
            full_context,
        };

        Ok(serializable_state)
    }

    /// Determines whether to preserve full context based on configuration and step type
    fn should_preserve_full_context(&self, is_explicit_checkpoint: bool) -> bool {
        if !self.config.preserve_full_context {
            return false;
        }

        match self.config.full_context_strategy {
            FullContextStrategy::Always => true,
            FullContextStrategy::Never => false,
            FullContextStrategy::UserInputOnly => self.current_instruction.is_some(),
            FullContextStrategy::ExplicitCheckpointsOnly => is_explicit_checkpoint,
        }
    }

    /// Generates a visualization of the agent trajectory
    pub fn generate_visualization(&mut self) -> Result<PathBuf, String> {
        if !self.enabled {
            return Err("Trajectory recording is not enabled".to_string());
        }

        if let Some(repo) = &self.repository {
            let output_path = visualizer::generate_visualization(repo, &self.config.output_dir)?;
            info!("Generated visualization at: {:?}", output_path);

            // Only open the browser if it hasn't been opened before
            if !self.browser_opened {
                if let Err(e) = open::that(&output_path) {
                    warn!("Failed to open visualization in browser: {}", e);
                    // Return success even if browser opening fails, since the file was generated
                } else {
                    // Mark browser as opened
                    self.browser_opened = true;
                    info!("Opened visualization in browser");
                }
            } else {
                debug!("Browser already opened, skipping automatic opening");
            }

            Ok(output_path)
        } else {
            Err("Repository not initialized".to_string())
        }
    }

    /// Enables or disables the recorder
    pub fn set_enabled(&mut self, enabled: bool) {
        // Only update if the value is changing
        if self.enabled != enabled {
            self.enabled = enabled;

            // Initialize repository if enabling and not already initialized
            if enabled && self.repository.is_none() {
                match Repository::load(&self.config.output_dir) {
                    Ok(repo) => {
                        self.repository = Some(repo);
                    },
                    Err(e) => {
                        error!("Failed to load repository: {}", e);
                    },
                }
            }
        }
    }

    /// Returns whether the recorder is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Creates a new trajectory in the repository
    #[allow(dead_code)]
    pub fn create_trajectory(&mut self, name: &str) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }

        if let Some(repo) = &mut self.repository {
            repo.create_trajectory(name)?;
            info!("Created new trajectory: {}", name);
        }

        Ok(())
    }

    /// Switches to an existing trajectory
    #[allow(dead_code)]
    pub fn switch_trajectory(&mut self, name: &str) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }

        if let Some(repo) = &mut self.repository {
            repo.switch_trajectory(name)?;

            // Update last step ID to the latest of the trajectory
            if let Some(trajectory) = repo.trajectories.get(name) {
                if !trajectory.latest_step_id.is_empty() {
                    self.last_step_id = Some(trajectory.latest_step_id.clone());
                } else {
                    self.last_step_id = None;
                }
            }

            info!("Switched to trajectory: {}", name);
        }

        Ok(())
    }

    /// Sets configuration options for the recorder
    #[allow(dead_code)]
    pub fn set_config_option(&mut self, option: &str, value: &str) -> Result<(), String> {
        match option {
            "preserve_full_context" => match value.to_lowercase().as_str() {
                "true" | "yes" | "1" | "on" => self.config.preserve_full_context = true,
                "false" | "no" | "0" | "off" => self.config.preserve_full_context = false,
                _ => return Err(format!("Invalid value for preserve_full_context: {}", value)),
            },
            "full_context_strategy" => {
                self.config.full_context_strategy = match value.to_lowercase().as_str() {
                    "never" => FullContextStrategy::Never,
                    "always" => FullContextStrategy::Always,
                    "user_input_only" | "user" => FullContextStrategy::UserInputOnly,
                    "explicit_checkpoints_only" | "checkpoints" => FullContextStrategy::ExplicitCheckpointsOnly,
                    _ => return Err(format!("Invalid value for full_context_strategy: {}", value)),
                };
            },
            "auto_visualize" => match value.to_lowercase().as_str() {
                "true" | "yes" | "1" | "on" => self.config.auto_visualize = true,
                "false" | "no" | "0" | "off" => self.config.auto_visualize = false,
                _ => return Err(format!("Invalid value for auto_visualize: {}", value)),
            },
            _ => return Err(format!("Unknown configuration option: {}", option)),
        }

        Ok(())
    }

    /// Gets the current configuration as a string map
    pub fn get_config(&self) -> HashMap<String, String> {
        let mut config = HashMap::new();

        config.insert("enabled".to_string(), self.enabled.to_string());
        config.insert(
            "output_dir".to_string(),
            self.config.output_dir.to_string_lossy().to_string(),
        );
        config.insert("auto_visualize".to_string(), self.config.auto_visualize.to_string());
        config.insert(
            "preserve_full_context".to_string(),
            self.config.preserve_full_context.to_string(),
        );

        let strategy = match self.config.full_context_strategy {
            FullContextStrategy::Never => "never",
            FullContextStrategy::Always => "always",
            FullContextStrategy::UserInputOnly => "user_input_only",
            FullContextStrategy::ExplicitCheckpointsOnly => "explicit_checkpoints_only",
        };
        config.insert("full_context_strategy".to_string(), strategy.to_string());

        config
    }
}
