#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    
    use crate::cli::chat::command::{TrajectorySubcommand, CheckpointSubcommand};
    use crate::cli::chat::conversation_state::ConversationState;
    use crate::cli::chat::trajectory::{TrajectoryRecorder, TrajectoryConfig, TrajectoryCommandHandler};
    
    struct TestSetup {
        recorder: Arc<Mutex<TrajectoryRecorder>>,
        output: Cursor<Vec<u8>>,
        conversation_state: ConversationState,
        ctx: Arc<fig_os_shim::Context>,
    }
    
    impl TestSetup {
        async fn new() -> Self {
            let temp_dir = std::env::temp_dir().join("trajectory_test");
            let _ = std::fs::create_dir_all(&temp_dir);
            
            let config = TrajectoryConfig {
                enabled: true,
                output_dir: temp_dir,
                auto_visualize: false,
                preserve_full_context: false,
                full_context_strategy: crate::cli::chat::trajectory::FullContextStrategy::Never,
            };
            
            let recorder = Arc::new(Mutex::new(TrajectoryRecorder::new(config)));
            let output = Cursor::new(Vec::new());
            let ctx = Arc::new(fig_os_shim::Context::new());
            let conversation_state = ConversationState::new(
                Arc::clone(&ctx),
                std::collections::HashMap::new(),
                None,
            ).await;
            
            Self {
                recorder,
                output,
                conversation_state,
                ctx,
            }
        }
        
        fn get_output(&self) -> String {
            String::from_utf8_lossy(&self.output.get_ref()).to_string()
        }
    }
    
    #[tokio::test]
    async fn test_status_command() {
        let mut setup = TestSetup::new().await;
        
        let mut handler = TrajectoryCommandHandler::new(
            &setup.recorder,
            &mut setup.output,
            &mut setup.conversation_state,
            Arc::clone(&setup.ctx),
        );
        
        let result = handler.handle_command(TrajectorySubcommand::Status).await;
        assert!(result.is_ok());
        
        let output = setup.get_output();
        assert!(output.contains("Trajectory recording is enabled"));
    }
    
    #[tokio::test]
    async fn test_help_command() {
        let mut setup = TestSetup::new().await;
        
        let mut handler = TrajectoryCommandHandler::new(
            &setup.recorder,
            &mut setup.output,
            &mut setup.conversation_state,
            Arc::clone(&setup.ctx),
        );
        
        let result = handler.handle_command(TrajectorySubcommand::Help).await;
        assert!(result.is_ok());
        
        let output = setup.get_output();
        assert!(output.contains("Trajectory Recording"));
        assert!(output.contains("Available commands"));
    }
    
    #[tokio::test]
    async fn test_enable_disable_commands() {
        let mut setup = TestSetup::new().await;
        
        let mut handler = TrajectoryCommandHandler::new(
            &setup.recorder,
            &mut setup.output,
            &mut setup.conversation_state,
            Arc::clone(&setup.ctx),
        );
        
        // Test disable command
        let result = handler.handle_command(TrajectorySubcommand::Disable).await;
        assert!(result.is_ok());
        
        let output = setup.get_output();
        assert!(output.contains("Trajectory recording disabled"));
        assert!(!setup.recorder.lock().unwrap().is_enabled());
        
        // Reset output for next test
        setup.output = Cursor::new(Vec::new());
        
        // Test enable command
        let result = handler.handle_command(TrajectorySubcommand::Enable).await;
        assert!(result.is_ok());
        
        let output = setup.get_output();
        assert!(output.contains("Trajectory recording enabled"));
        assert!(setup.recorder.lock().unwrap().is_enabled());
    }
    
    #[tokio::test]
    async fn test_checkpoint_create_and_list() {
        let mut setup = TestSetup::new().await;
        
        let mut handler = TrajectoryCommandHandler::new(
            &setup.recorder,
            &mut setup.output,
            &mut setup.conversation_state,
            Arc::clone(&setup.ctx),
        );
        
        // Create a checkpoint
        let result = handler.handle_command(
            TrajectorySubcommand::Checkpoint { 
                subcommand: CheckpointSubcommand::Create { 
                    label: "test_checkpoint".to_string() 
                } 
            }
        ).await;
        assert!(result.is_ok());
        
        let output = setup.get_output();
        assert!(output.contains("Checkpoint created with ID:"));
        
        // Reset output for next test
        setup.output = Cursor::new(Vec::new());
        
        // List checkpoints
        let result = handler.handle_command(
            TrajectorySubcommand::Checkpoint { 
                subcommand: CheckpointSubcommand::List 
            }
        ).await;
        assert!(result.is_ok());
        
        let output = setup.get_output();
        assert!(output.contains("Available checkpoints:"));
        assert!(output.contains("test_checkpoint"));
    }
    
    #[tokio::test]
    async fn test_visualize_command() {
        let mut setup = TestSetup::new().await;
        
        let mut handler = TrajectoryCommandHandler::new(
            &setup.recorder,
            &mut setup.output,
            &mut setup.conversation_state,
            Arc::clone(&setup.ctx),
        );
        
        let result = handler.handle_command(TrajectorySubcommand::Visualize).await;
        assert!(result.is_ok());
        
        let output = setup.get_output();
        assert!(output.contains("Visualization generated at:"));
    }
}
