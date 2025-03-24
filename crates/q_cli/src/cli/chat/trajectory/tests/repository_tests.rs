use std::collections::HashMap;

use crate::cli::chat::trajectory::repository::{
    AgentAction,
    Repository,
    SerializableConversationState,
    SerializableChatMessage,
};

#[test]
fn test_repository_creation() {
    // Create a temporary directory for testing
    let temp_dir = tempfile::tempdir().unwrap();
    let repo = Repository::new(temp_dir.path());
    
    // Check that the repository was created with a main trajectory
    assert_eq!(repo.current_trajectory, "main");
    assert!(repo.trajectories.contains_key("main"));
    assert!(repo.steps.is_empty());
}

#[test]
fn test_repository_save_load() {
    // Create a temporary directory for testing
    let temp_dir = tempfile::tempdir().unwrap();
    let mut repo = Repository::new(temp_dir.path());
    
    // Add a step
    let step = repo.step_builder()
        .user_instruction("Test instruction")
        .category("user_instruction")
        .tag("user-input")
        .build();
    
    let step_id = repo.record_step(step).unwrap();
    assert!(!step_id.is_empty());
    
    // Save the repository
    let save_result = repo.save();
    assert!(save_result.is_ok());
    
    // Load the repository
    let loaded_repo = Repository::load(temp_dir.path()).unwrap();
    
    // Check that the loaded repository has the same data
    assert_eq!(loaded_repo.current_trajectory, repo.current_trajectory);
    assert_eq!(loaded_repo.trajectories.len(), repo.trajectories.len());
    assert_eq!(loaded_repo.steps.len(), repo.steps.len());
    assert!(loaded_repo.steps.contains_key(&step_id));
}

#[test]
fn test_create_trajectory() {
    // Create a temporary directory for testing
    let temp_dir = tempfile::tempdir().unwrap();
    let mut repo = Repository::new(temp_dir.path());
    
    // Create a new trajectory
    let result = repo.create_trajectory("test_trajectory");
    assert!(result.is_ok());
    
    // Check that the trajectory was created
    assert_eq!(repo.current_trajectory, "test_trajectory");
    assert!(repo.trajectories.contains_key("test_trajectory"));
}

#[test]
fn test_switch_trajectory() {
    // Create a temporary directory for testing
    let temp_dir = tempfile::tempdir().unwrap();
    let mut repo = Repository::new(temp_dir.path());
    
    // Create a new trajectory
    repo.create_trajectory("test_trajectory").unwrap();
    assert_eq!(repo.current_trajectory, "test_trajectory");
    
    // Switch back to main
    let result = repo.switch_trajectory("main");
    assert!(result.is_ok());
    assert_eq!(repo.current_trajectory, "main");
}

#[test]
fn test_record_step() {
    // Create a temporary directory for testing
    let temp_dir = tempfile::tempdir().unwrap();
    let mut repo = Repository::new(temp_dir.path());
    
    // Create a step with user instruction
    let step1 = repo.step_builder()
        .user_instruction("Test instruction")
        .category("user_instruction")
        .tag("user-input")
        .build();
    
    let step1_id = repo.record_step(step1).unwrap();
    
    // Create a step with agent action
    let mut parameters = HashMap::new();
    parameters.insert("path".to_string(), serde_json::json!("/test/path"));
    
    let action = AgentAction {
        action_type: "tool_use".to_string(),
        name: Some("fs_read".to_string()),
        parameters,
        description: Some("Reading a file".to_string()),
    };
    
    let step2 = repo.step_builder()
        .parent_id(Some(step1_id.clone()))
        .agent_action(action)
        .category("tool_use")
        .tag("tool-use")
        .build();
    
    let step2_id = repo.record_step(step2).unwrap();
    
    // Check that both steps were recorded
    assert!(repo.steps.contains_key(&step1_id));
    assert!(repo.steps.contains_key(&step2_id));
    
    // Check that the trajectory was updated
    let trajectory = repo.trajectories.get("main").unwrap();
    assert_eq!(trajectory.latest_step_id, step2_id);
    assert_eq!(trajectory.step_ids.len(), 2);
}

#[test]
fn test_checkpoints() {
    // Create a temporary directory for testing
    let temp_dir = tempfile::tempdir().unwrap();
    let mut repo = Repository::new(temp_dir.path());
    
    // Create a conversation state
    let state = SerializableConversationState {
        conversation_id: "test_conversation".to_string(),
        history: vec![
            SerializableChatMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
                full_content: None,
            },
            SerializableChatMessage {
                role: "assistant".to_string(),
                content: "Hi there".to_string(),
                full_content: None,
            },
        ],
        next_message: None,
        tools: vec![],
        context_files: HashMap::new(),
        env_state: None,
        shell_state: None,
        metadata: HashMap::new(),
        full_context: None,
    };
    
    // Create a checkpoint
    let step = repo.step_builder()
        .category("checkpoint")
        .tag("checkpoint")
        .tag("test_label")
        .conversation_state(state)
        .build();
    
    let checkpoint_id = repo.record_step(step).unwrap();
    
    // List checkpoints
    let checkpoints = repo.list_checkpoints();
    assert_eq!(checkpoints.len(), 1);
    
    // Get checkpoint by ID
    let checkpoint = repo.get_checkpoint(&checkpoint_id);
    assert!(checkpoint.is_some());
    
    // Get checkpoint by label
    let checkpoint = repo.get_checkpoint("test_label");
    assert!(checkpoint.is_some());
}
