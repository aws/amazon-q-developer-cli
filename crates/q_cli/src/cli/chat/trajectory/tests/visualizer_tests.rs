#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    
    use crate::cli::chat::trajectory::repository::{Repository, Step, AgentAction, ActionResult};
    use crate::cli::chat::trajectory::visualizer;
    
    fn create_test_repository() -> Repository {
        let temp_dir = std::env::temp_dir().join("trajectory_visualizer_test");
        let _ = std::fs::create_dir_all(&temp_dir);
        let mut repo = Repository::new(temp_dir);
        
        // Add some test steps
        let user_step = repo.step_builder()
            .user_instruction("Test user instruction")
            .category("user_instruction")
            .tag("user-input")
            .build();
        let user_step_id = repo.record_step(user_step).unwrap();
        
        let reasoning_step = repo.step_builder()
            .parent_id(Some(user_step_id.clone()))
            .agent_reasoning("Test reasoning")
            .category("reasoning")
            .tag("reasoning")
            .build();
        let reasoning_step_id = repo.record_step(reasoning_step).unwrap();
        
        // Create a tool use step
        let mut params = HashMap::new();
        params.insert("path".to_string(), serde_json::Value::String("/test/path".to_string()));
        params.insert("command".to_string(), serde_json::Value::String("read".to_string()));
        
        let action = AgentAction {
            action_type: "tool_use".to_string(),
            name: Some("fs_read".to_string()),
            parameters: params,
            description: Some("Reading file".to_string()),
        };
        
        let tool_step = repo.step_builder()
            .parent_id(Some(reasoning_step_id))
            .agent_action(action)
            .category("tool_use")
            .tag("tool-use")
            .build();
        let tool_step_id = repo.record_step(tool_step).unwrap();
        
        // Add a result to the tool step
        if let Some(step) = repo.steps.get_mut(&tool_step_id) {
            step.action_result = Some(ActionResult {
                success: true,
                data: Some(serde_json::Value::String("File content".to_string())),
                error_message: None,
            });
        }
        
        // Add a response step
        let response_step = repo.step_builder()
            .parent_id(Some(tool_step_id))
            .agent_response("Test response")
            .category("response")
            .tag("response")
            .build();
        repo.record_step(response_step).unwrap();
        
        repo
    }
    
    #[test]
    fn test_generate_visualization() {
        let repo = create_test_repository();
        let output_dir = std::env::temp_dir().join("trajectory_visualizer_output");
        let _ = std::fs::create_dir_all(&output_dir);
        
        let result = visualizer::generate_visualization(&repo, &output_dir);
        assert!(result.is_ok());
        
        let output_path = result.unwrap();
        assert_eq!(output_path, output_dir.join("trajectory.html"));
        assert!(output_path.exists());
        
        // Check content of the visualization
        let content = std::fs::read_to_string(output_path).unwrap();
        assert!(content.contains("Agent Trajectory Visualization"));
        assert!(content.contains("Test user instruction"));
        assert!(content.contains("Test reasoning"));
        assert!(content.contains("fs_read"));
        assert!(content.contains("Test response"));
    }
    
    #[test]
    fn test_html_generation() {
        let repo = create_test_repository();
        
        // Use the private function through a public wrapper for testing
        let html_content = visualizer::generate_html_visualization(&repo).unwrap();
        
        // Check basic structure
        assert!(html_content.contains("<!DOCTYPE html>"));
        assert!(html_content.contains("<title>Agent Trajectory Visualization</title>"));
        
        // Check step content
        assert!(html_content.contains("Test user instruction"));
        assert!(html_content.contains("Test reasoning"));
        assert!(html_content.contains("fs_read"));
        assert!(html_content.contains("Test response"));
        
        // Check styling elements
        assert!(html_content.contains("history-table"));
        assert!(html_content.contains("graph-node"));
        assert!(html_content.contains("step-message"));
    }
}
