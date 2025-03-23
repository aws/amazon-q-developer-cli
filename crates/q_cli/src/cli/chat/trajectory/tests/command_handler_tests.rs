use std::io::Cursor;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crossterm::style::Stylize;
use fig_os_shim::Context;

use crate::cli::chat::command::{
    CheckpointSubcommand,
    TrajectorySubcommand,
};
use crate::cli::chat::conversation_state::ConversationState;
use crate::cli::chat::trajectory::{
    FullContextStrategy,
    TrajectoryConfig,
    TrajectoryRecorder,
    TrajectoryCommandHandler,
};

// Helper function to create a test recorder
fn create_test_recorder() -> Arc<Mutex<TrajectoryRecorder>> {
    let temp_dir = tempfile::tempdir().unwrap();
    let config = TrajectoryConfig {
        enabled: true,
        output_dir: temp_dir.path().to_path_buf(),
        auto_visualize: false,
        preserve_full_context: false,
        full_context_strategy: FullContextStrategy::default(),
    };
    
    Arc::new(Mutex::new(TrajectoryRecorder::new(config)))
}

// Helper function to create a test context
async fn create_test_context() -> Arc<Context> {
    Arc::new(Context::builder().with_test_home().await.unwrap().build_fake())
}

// Helper function to create a test conversation state
async fn create_test_conversation_state(ctx: Arc<Context>) -> ConversationState {
    ConversationState::new(ctx, std::collections::HashMap::new(), None).await
}

#[tokio::test]
async fn test_handle_status_command() {
    let recorder = create_test_recorder();
    let ctx = create_test_context().await;
    let mut conversation_state = create_test_conversation_state(ctx.clone()).await;
    let mut output = Cursor::new(Vec::new());
    
    let mut handler = TrajectoryCommandHandler::new(
        &recorder,
        &mut output,
        &mut conversation_state,
        ctx,
    );
    
    // Handle status command
    let result = handler.handle_command(TrajectorySubcommand::Status).await;
    assert!(result.is_ok());
    
    // Check output
    let output_str = String::from_utf8(output.into_inner()).unwrap();
    assert!(output_str.contains("enabled"));
    assert!(output_str.contains("Configuration"));
}

#[tokio::test]
async fn test_handle_enable_disable_commands() {
    let recorder = create_test_recorder();
    let ctx = create_test_context().await;
    let mut conversation_state = create_test_conversation_state(ctx.clone()).await;
    
    // Test enable command
    {
        let mut output = Cursor::new(Vec::new());
        let mut handler = TrajectoryCommandHandler::new(
            &recorder,
            &mut output,
            &mut conversation_state,
            ctx.clone(),
        );
        
        // Disable first
        recorder.lock().unwrap().set_enabled(false);
        
        // Handle enable command
        let result = handler.handle_command(TrajectorySubcommand::Enable).await;
        assert!(result.is_ok());
        
        // Check output
        let output_str = String::from_utf8(output.into_inner()).unwrap();
        assert!(output_str.contains("enabled"));
        
        // Check that recorder is enabled
        assert!(recorder.lock().unwrap().is_enabled());
    }
    
    // Test disable command
    {
        let mut output = Cursor::new(Vec::new());
        let mut handler = TrajectoryCommandHandler::new(
            &recorder,
            &mut output,
            &mut conversation_state,
            ctx.clone(),
        );
        
        // Handle disable command
        let result = handler.handle_command(TrajectorySubcommand::Disable).await;
        assert!(result.is_ok());
        
        // Check output
        let output_str = String::from_utf8(output.into_inner()).unwrap();
        assert!(output_str.contains("disabled"));
        
        // Check that recorder is disabled
        assert!(!recorder.lock().unwrap().is_enabled());
    }
}

#[tokio::test]
async fn test_handle_help_command() {
    let recorder = create_test_recorder();
    let ctx = create_test_context().await;
    let mut conversation_state = create_test_conversation_state(ctx.clone()).await;
    let mut output = Cursor::new(Vec::new());
    
    let mut handler = TrajectoryCommandHandler::new(
        &recorder,
        &mut output,
        &mut conversation_state,
        ctx,
    );
    
    // Handle help command
    let result = handler.handle_command(TrajectorySubcommand::Help).await;
    assert!(result.is_ok());
    
    // Check output
    let output_str = String::from_utf8(output.into_inner()).unwrap();
    assert!(output_str.contains("help"));
}

#[tokio::test]
async fn test_handle_checkpoint_commands() {
    let recorder = create_test_recorder();
    let ctx = create_test_context().await;
    let mut conversation_state = create_test_conversation_state(ctx.clone()).await;
    
    // Test create checkpoint command
    let checkpoint_id = {
        let mut output = Cursor::new(Vec::new());
        let mut handler = TrajectoryCommandHandler::new(
            &recorder,
            &mut output,
            &mut conversation_state,
            ctx.clone(),
        );
        
        // Handle create checkpoint command
        let result = handler.handle_command(
            TrajectorySubcommand::Checkpoint {
                subcommand: CheckpointSubcommand::Create {
                    label: "test_checkpoint".to_string(),
                },
            }
        ).await;
        assert!(result.is_ok());
        
        // Check output
        let output_str = String::from_utf8(output.into_inner()).unwrap();
        assert!(output_str.contains("Checkpoint created"));
        
        // Extract the checkpoint ID from the output
        let id_start = output_str.find("ID: ").unwrap() + 4;
        let id_end = output_str[id_start..].find('\n').unwrap() + id_start;
        output_str[id_start..id_end].to_string()
    };
    
    // Test list checkpoints command
    {
        let mut output = Cursor::new(Vec::new());
        let mut handler = TrajectoryCommandHandler::new(
            &recorder,
            &mut output,
            &mut conversation_state,
            ctx.clone(),
        );
        
        // Handle list checkpoints command
        let result = handler.handle_command(
            TrajectorySubcommand::Checkpoint {
                subcommand: CheckpointSubcommand::List,
            }
        ).await;
        assert!(result.is_ok());
        
        // Check output
        let output_str = String::from_utf8(output.into_inner()).unwrap();
        assert!(output_str.contains("Available checkpoints"));
        assert!(output_str.contains(&checkpoint_id));
        assert!(output_str.contains("test_checkpoint"));
    }
}

#[tokio::test]
async fn test_handle_visualize_command() {
    let recorder = create_test_recorder();
    let ctx = create_test_context().await;
    let mut conversation_state = create_test_conversation_state(ctx.clone()).await;
    let mut output = Cursor::new(Vec::new());
    
    // Record some data first
    recorder.lock().unwrap().record_user_instruction("Test instruction").unwrap();
    
    let mut handler = TrajectoryCommandHandler::new(
        &recorder,
        &mut output,
        &mut conversation_state,
        ctx,
    );
    
    // Handle visualize command
    let result = handler.handle_command(TrajectorySubcommand::Visualize).await;
    assert!(result.is_ok());
    
    // Check output
    let output_str = String::from_utf8(output.into_inner()).unwrap();
    assert!(output_str.contains("Visualization generated"));
}
