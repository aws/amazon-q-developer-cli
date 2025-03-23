use std::collections::HashMap;
use std::fs;
use std::path::{
    Path,
    PathBuf,
};
use std::time::{
    SystemTime,
    UNIX_EPOCH,
};

// Import only what we need
use fig_api_client::model::Tool;
use serde::{
    Deserialize,
    Serialize,
};
use tracing::error;
use uuid::Uuid;

// Define serializable versions of the types we need
// Define serializable versions of the types we need
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableChatMessage {
    pub role: String,
    pub content: String,
    // Store the actual complete content when full context preservation is enabled
    pub full_content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableUserInputMessage {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableTool {
    pub name: String,
    pub description: String,
    // Store the complete tool definition when full context preservation is enabled
    pub full_definition: Option<String>,
}

/// Represents the full model context for exact reproduction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullModelContext {
    /// Complete system prompt
    pub system_prompt: String,
    /// Complete message history with full content
    pub complete_messages: Vec<CompleteMessage>,
    /// Complete context files with content
    pub context_files_content: HashMap<String, String>,
    /// Model parameters
    pub model_parameters: HashMap<String, serde_json::Value>,
    /// Raw token count estimate
    pub token_count_estimate: usize,
}

/// Complete message with all details preserved
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteMessage {
    /// Role (user, assistant, system, tool)
    pub role: String,
    /// Complete content
    pub content: String,
    /// Tool calls if any
    pub tool_calls: Option<Vec<serde_json::Value>>,
    /// Tool results if any
    pub tool_results: Option<Vec<serde_json::Value>>,
}

// Convert from the original types to our serializable versions
impl From<&fig_api_client::model::ChatMessage> for SerializableChatMessage {
    fn from(msg: &fig_api_client::model::ChatMessage) -> Self {
        SerializableChatMessage {
            role: "user".to_string(),               // Simplified
            content: "message content".to_string(), // Simplified
            full_content: None,
        }
    }
}

impl From<&fig_api_client::model::UserInputMessage> for SerializableUserInputMessage {
    fn from(msg: &fig_api_client::model::UserInputMessage) -> Self {
        SerializableUserInputMessage {
            content: msg.content.clone(),
        }
    }
}

impl From<&fig_api_client::model::Tool> for SerializableTool {
    fn from(tool: &fig_api_client::model::Tool) -> Self {
        SerializableTool {
            name: format!("{:?}", tool),
            description: "Tool description".to_string(),
            full_definition: None,
        }
    }
}

/// Represents a step in the agent trajectory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    /// Unique identifier for this step
    pub id: String,
    /// Parent step ID, if any
    pub parent_id: Option<String>,
    /// Timestamp when this step was created
    pub timestamp: String,
    /// User instruction that triggered this step, if any
    pub user_instruction: Option<String>,
    /// Agent's reasoning for this step
    pub agent_reasoning: Option<String>,
    /// Action taken by the agent
    pub agent_action: Option<AgentAction>,
    /// Result of the action
    pub action_result: Option<ActionResult>,
    /// Agent's response to the user
    pub agent_response: Option<String>,
    /// Category of this step (e.g., reasoning, tool_use, response)
    pub category: Option<String>,
    /// Tags for this step
    pub tags: Vec<String>,
    /// Trajectory name this step belongs to
    pub trajectory_name: String,
    /// Serialized conversation state for restoration
    pub conversation_state: Option<SerializableConversationState>,
}

/// Represents an action taken by the agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAction {
    /// Type of action (e.g., reasoning, tool_use, response)
    pub action_type: String,
    /// Name of the action or tool
    pub name: Option<String>,
    /// Parameters for the action
    pub parameters: HashMap<String, serde_json::Value>,
    /// Description of the action
    pub description: Option<String>,
}

/// Represents the result of an agent action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    /// Whether the action was successful
    pub success: bool,
    /// Data returned by the action
    pub data: Option<serde_json::Value>,
    /// Error message if the action failed
    pub error_message: Option<String>,
}

/// Represents a trajectory in the agent's history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trajectory {
    /// Name of the trajectory
    pub name: String,
    /// ID of the latest step in this trajectory
    pub latest_step_id: String,
    /// IDs of all steps in this trajectory
    pub step_ids: Vec<String>,
}

/// Serializable representation of conversation state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableConversationState {
    /// Conversation ID
    pub conversation_id: String,
    /// Message history
    pub history: Vec<SerializableChatMessage>,
    /// Next message to be sent
    pub next_message: Option<SerializableUserInputMessage>,
    /// Available tools
    pub tools: Vec<SerializableTool>,
    /// Context files
    pub context_files: HashMap<String, Vec<String>>,
    /// Environment state
    pub env_state: Option<HashMap<String, String>>,
    /// Shell state
    pub shell_state: Option<HashMap<String, String>>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
    /// Full model context (if preserved)
    pub full_context: Option<FullModelContext>,
}

/// Repository for storing and managing agent trajectory data
#[derive(Debug, Serialize, Deserialize)]
pub struct Repository {
    /// Map of step ID to step
    pub steps: HashMap<String, Step>,
    /// Map of trajectory name to trajectory
    pub trajectories: HashMap<String, Trajectory>,
    /// Name of the current active trajectory
    pub current_trajectory: String,
    /// Path to the repository directory
    #[serde(skip)]
    pub path: PathBuf,
}

impl Repository {
    /// Creates a new repository at the specified path
    pub fn new(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();

        // Create directory if it doesn't exist
        if !path.exists() {
            if let Err(e) = fs::create_dir_all(&path) {
                error!("Failed to create repository directory: {}", e);
            }
        }

        // Initialize with main trajectory
        let mut trajectories = HashMap::new();
        trajectories.insert("main".to_string(), Trajectory {
            name: "main".to_string(),
            latest_step_id: String::new(),
            step_ids: Vec::new(),
        });

        Self {
            steps: HashMap::new(),
            trajectories,
            current_trajectory: "main".to_string(),
            path,
        }
    }

    /// Loads a repository from the specified path
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        let repo_file = path.join("repository.json");

        if !repo_file.exists() {
            return Ok(Self::new(path));
        }

        match fs::read_to_string(&repo_file) {
            Ok(content) => match serde_json::from_str::<Self>(&content) {
                Ok(mut repo) => {
                    repo.path = path;
                    Ok(repo)
                },
                Err(e) => Err(format!("Failed to parse repository file: {}", e)),
            },
            Err(e) => Err(format!("Failed to read repository file: {}", e)),
        }
    }

    /// Saves the repository to disk
    pub fn save(&self) -> Result<(), String> {
        let repo_file = self.path.join("repository.json");

        match serde_json::to_string_pretty(self) {
            Ok(content) => {
                if let Err(e) = fs::write(&repo_file, content) {
                    return Err(format!("Failed to write repository file: {}", e));
                }
                Ok(())
            },
            Err(e) => Err(format!("Failed to serialize repository: {}", e)),
        }
    }

    /// Records a new step in the current trajectory
    pub fn record_step(&mut self, step: Step) -> Result<String, String> {
        let step_id = step.id.clone();

        // Add step to the repository
        self.steps.insert(step_id.clone(), step);

        // Update the trajectory
        if let Some(trajectory) = self.trajectories.get_mut(&self.current_trajectory) {
            trajectory.step_ids.push(step_id.clone());
            trajectory.latest_step_id = step_id.clone();
        } else {
            return Err(format!("Trajectory {} not found", self.current_trajectory));
        }

        // Save the repository
        self.save()?;

        Ok(step_id)
    }

    /// Creates a new trajectory from the current trajectory
    pub fn create_trajectory(&mut self, name: &str) -> Result<(), String> {
        if self.trajectories.contains_key(name) {
            return Err(format!("Trajectory {} already exists", name));
        }

        // Get the current trajectory's latest step
        let latest_step_id = match self.trajectories.get(&self.current_trajectory) {
            Some(trajectory) => trajectory.latest_step_id.clone(),
            None => return Err(format!("Current trajectory {} not found", self.current_trajectory)),
        };

        // Create the new trajectory
        self.trajectories.insert(name.to_string(), Trajectory {
            name: name.to_string(),
            latest_step_id,
            step_ids: Vec::new(),
        });

        // Switch to the new trajectory
        self.current_trajectory = name.to_string();

        // Save the repository
        self.save()?;

        Ok(())
    }

    /// Switches to an existing trajectory
    pub fn switch_trajectory(&mut self, name: &str) -> Result<(), String> {
        if !self.trajectories.contains_key(name) {
            return Err(format!("Trajectory {} does not exist", name));
        }

        self.current_trajectory = name.to_string();
        Ok(())
    }

    /// Gets the current trajectory
    pub fn current_trajectory(&self) -> Result<&Trajectory, String> {
        match self.trajectories.get(&self.current_trajectory) {
            Some(trajectory) => Ok(trajectory),
            None => Err(format!("Current trajectory {} not found", self.current_trajectory)),
        }
    }

    /// Gets a step by ID
    pub fn get_step(&self, id: &str) -> Option<&Step> {
        self.steps.get(id)
    }

    /// Creates a new step builder
    pub fn step_builder(&self) -> StepBuilder {
        StepBuilder::new(self.current_trajectory.clone())
    }

    /// Lists all checkpoints in the repository
    pub fn list_checkpoints(&self) -> Vec<(&str, &Step)> {
        self.steps
            .iter()
            .filter(|(_, step)| step.tags.contains(&"checkpoint".to_string()))
            .map(|(id, step)| (id.as_str(), step))
            .collect()
    }

    /// Gets a checkpoint by ID or label
    pub fn get_checkpoint(&self, id_or_label: &str) -> Option<&Step> {
        // Try to find by ID first
        if let Some(step) = self.steps.get(id_or_label) {
            if step.tags.contains(&"checkpoint".to_string()) {
                return Some(step);
            }
        }

        // Try to find by label
        self.steps
            .values()
            .find(|step| step.tags.contains(&"checkpoint".to_string()) && step.tags.contains(&id_or_label.to_string()))
    }
}

/// Builder for creating steps
pub struct StepBuilder {
    step: Step,
}

impl StepBuilder {
    /// Creates a new step builder
    pub fn new(trajectory_name: String) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        let timestamp = chrono::Utc::now().to_rfc3339();

        Self {
            step: Step {
                id: Uuid::new_v4().to_string(),
                parent_id: None,
                timestamp,
                user_instruction: None,
                agent_reasoning: None,
                agent_action: None,
                action_result: None,
                agent_response: None,
                category: None,
                tags: Vec::new(),
                trajectory_name,
                conversation_state: None,
            },
        }
    }

    /// Sets the parent step ID
    pub fn parent_id(mut self, parent_id: Option<String>) -> Self {
        self.step.parent_id = parent_id;
        self
    }

    /// Sets the user instruction
    pub fn user_instruction(mut self, instruction: impl Into<String>) -> Self {
        self.step.user_instruction = Some(instruction.into());
        self
    }

    /// Sets the agent reasoning
    pub fn agent_reasoning(mut self, reasoning: impl Into<String>) -> Self {
        self.step.agent_reasoning = Some(reasoning.into());
        self
    }

    /// Sets the agent action
    pub fn agent_action(mut self, action: AgentAction) -> Self {
        self.step.agent_action = Some(action);
        self
    }

    /// Sets the action result
    pub fn action_result(mut self, result: ActionResult) -> Self {
        self.step.action_result = Some(result);
        self
    }

    /// Sets the agent response
    pub fn agent_response(mut self, response: impl Into<String>) -> Self {
        self.step.agent_response = Some(response.into());
        self
    }

    /// Sets the category
    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.step.category = Some(category.into());
        self
    }

    /// Adds a tag
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.step.tags.push(tag.into());
        self
    }

    /// Adds multiple tags
    pub fn tags(mut self, tags: Vec<String>) -> Self {
        self.step.tags.extend(tags);
        self
    }

    /// Sets the conversation state
    pub fn conversation_state(mut self, state: SerializableConversationState) -> Self {
        self.step.conversation_state = Some(state);
        self
    }

    /// Builds the step
    pub fn build(self) -> Step {
        self.step
    }
}
