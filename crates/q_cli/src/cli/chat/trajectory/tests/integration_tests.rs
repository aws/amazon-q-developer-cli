#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    
    use crate::cli::chat::trajectory::{TrajectoryRecorder, TrajectoryConfig, FullContextStrategy};
    use crate::cli::chat::conversation_state::ConversationState;
    
    // This test verifies the complete workflow of recording a conversation
    #[tokio::test]
    async fn test_complete_workflow() {
        // Create a temporary directory for the test
        let temp_dir = std::env::temp_dir().join("trajectory_integration_test");
        let _ = std::fs::create_dir_all(&temp_dir);
        
        // Create configuration
        let config = TrajectoryConfig {
            enabled: true,
            output_dir: temp_dir.clone(),
            auto_visualize: true,
            preserve_full_context: false,
            full_context_strategy: FullContextStrategy::Never,
        };
        
        // Create recorder
        let recorder = Arc::new(Mutex::new(TrajectoryRecorder::new(config)));
        
        // Create conversation state
        let ctx = Arc::new(fig_os_shim::Context::new());
        let mut conversation_state = ConversationState::new(
            Arc::clone(&ctx),
            HashMap::new(),
            None,
        ).await;
        
        // Record user instruction
        recorder.lock().unwrap().record_user_instruction("Test user instruction").unwrap();
        
        // Record reasoning
        recorder.lock().unwrap().record_reasoning("Test reasoning").unwrap();
        
        // Record tool use
        let mut params = HashMap::new();
        params.insert("path".to_string(), serde_json::Value::String("/test/path".to_string()));
        params.insert("command".to_string(), serde_json::Value::String("read".to_string()));
        
        let tool_step_id = recorder.lock().unwrap()
            .record_tool_use("fs_read", params, Some("Reading file"))
            .unwrap();
        
        // Record tool result
        recorder.lock().unwrap()
            .record_tool_result(&tool_step_id, true, Some(serde_json::Value::String("File content".to_string())), None)
            .unwrap();
        
        // Record response
        recorder.lock().unwrap().record_response("Test response").unwrap();
        
        // Create checkpoint
        let checkpoint_id = recorder.lock().unwrap()
            .create_checkpoint("test_checkpoint", &conversation_state)
            .unwrap();
        
        // List checkpoints
        let checkpoints = recorder.lock().unwrap().list_checkpoints().unwrap();
        assert_eq!(checkpoints.len(), 1);
        
        // Generate visualization
        let visualization_path = recorder.lock().unwrap().generate_visualization().unwrap();
        assert!(visualization_path.exists());
        
        // Restore from checkpoint
        let state = recorder.lock().unwrap().restore_from_checkpoint(&checkpoint_id).unwrap();
        assert_eq!(state.conversation_id, conversation_state.conversation_id());
        
        // Verify visualization content
        let content = std::fs::read_to_string(visualization_path).unwrap();
        assert!(content.contains("Test user instruction"));
        assert!(content.contains("Test reasoning"));
        assert!(content.contains("fs_read"));
        assert!(content.contains("Test response"));
        assert!(content.contains("test_checkpoint"));
    }
}
