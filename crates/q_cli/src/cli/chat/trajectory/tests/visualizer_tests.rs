use std::collections::HashMap;
use std::path::PathBuf;

use crate::cli::chat::trajectory::repository::{
    AgentAction,
    ActionResult,
    Repository,
};
use crate::cli::chat::trajectory::visualizer;

#[test]
fn test_generate_visualization() {
    // Create a temporary directory for testing
    let temp_dir = tempfile::tempdir().unwrap();
    let mut repo = Repository::new(temp_dir.path());
    
    // Add a series of steps to simulate a conversation
    
    // 1. User instruction
    let step1 = repo.step_builder()
        .user_instruction("List files in the current directory")
        .category("user_instruction")
        .tag("user-input")
        .build();
    
    let step1_id = repo.record_step(step1).unwrap();
    
    // 2. Agent reasoning
    let step2 = repo.step_builder()
        .parent_id(Some(step1_id))
        .agent_reasoning("I need to use the fs_read tool with Directory mode to list files")
        .category("reasoning")
        .tag("reasoning")
        .build();
    
    let step2_id = repo.record_step(step2).unwrap();
    
    // 3. Tool use
    let mut parameters = HashMap::new();
    parameters.insert("mode".to_string(), serde_json::json!("Directory"));
    parameters.insert("path".to_string(), serde_json::json!("."));
    
    let action = AgentAction {
        action_type: "tool_use".to_string(),
        name: Some("fs_read".to_string()),
        parameters,
        description: Some("Listing files in current directory".to_string()),
    };
    
    let step3 = repo.step_builder()
        .parent_id(Some(step2_id))
        .agent_action(action)
        .category("tool_use")
        .tag("tool-use")
        .build();
    
    let step3_id = repo.record_step(step3).unwrap();
    
    // 4. Tool result
    let mut step4 = repo.steps.get(&step3_id).unwrap().clone();
    step4.action_result = Some(ActionResult {
        success: true,
        data: Some(serde_json::json!({
            "files": ["file1.txt", "file2.txt", "directory1"]
        })),
        error_message: None,
    });
    
    repo.steps.insert(step3_id.clone(), step4);
    
    // 5. Agent response
    let step5 = repo.step_builder()
        .parent_id(Some(step3_id))
        .agent_response("I found the following files in the current directory: file1.txt, file2.txt, and a directory called directory1.")
        .category("response")
        .tag("response")
        .build();
    
    repo.record_step(step5).unwrap();
    
    // Generate visualization
    let output_path = visualizer::generate_visualization(&repo, temp_dir.path()).unwrap();
    
    // Check that the visualization file was created
    assert!(output_path.exists());
    
    // Read the file content to verify it contains expected elements
    let content = std::fs::read_to_string(output_path).unwrap();
    
    // Check for key HTML elements
    assert!(content.contains("<!DOCTYPE html>"));
    assert!(content.contains("<title>Agent Trajectory Visualization</title>"));
    
    // Check for our specific content
    assert!(content.contains("List files in the current directory"));
    assert!(content.contains("fs_read"));
    assert!(content.contains("tool-use"));
    assert!(content.contains("reasoning"));
    assert!(content.contains("response"));
}

#[test]
fn test_visualization_with_complex_tool_use() {
    // Create a temporary directory for testing
    let temp_dir = tempfile::tempdir().unwrap();
    let mut repo = Repository::new(temp_dir.path());
    
    // Add a user instruction
    let step1 = repo.step_builder()
        .user_instruction("Execute a complex command")
        .category("user_instruction")
        .tag("user-input")
        .build();
    
    let step1_id = repo.record_step(step1).unwrap();
    
    // Add a tool use with complex parameters
    let mut parameters = HashMap::new();
    parameters.insert("command".to_string(), serde_json::json!("find . -name \"*.rs\" | grep -v \"target\" | wc -l"));
    
    let action = AgentAction {
        action_type: "tool_use".to_string(),
        name: Some("execute_bash".to_string()),
        parameters,
        description: Some("Counting Rust files excluding target directory".to_string()),
    };
    
    let step2 = repo.step_builder()
        .parent_id(Some(step1_id))
        .agent_action(action)
        .category("tool_use")
        .tag("tool-use")
        .build();
    
    let step2_id = repo.record_step(step2).unwrap();
    
    // Add a tool result
    let mut step3 = repo.steps.get(&step2_id).unwrap().clone();
    step3.action_result = Some(ActionResult {
        success: true,
        data: Some(serde_json::json!("42")),
        error_message: None,
    });
    
    repo.steps.insert(step2_id.clone(), step3);
    
    // Generate visualization
    let output_path = visualizer::generate_visualization(&repo, temp_dir.path()).unwrap();
    
    // Check that the visualization file was created
    assert!(output_path.exists());
    
    // Read the file content to verify it contains expected elements
    let content = std::fs::read_to_string(output_path).unwrap();
    
    // Check for our specific content
    assert!(content.contains("execute_bash"));
    assert!(content.contains("find . -name"));  // Should contain part of the command
}
