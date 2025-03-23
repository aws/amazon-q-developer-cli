use std::collections::HashMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use fig_api_client::model::AssistantResponseMessage;
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
    convert_to_conversation_state,
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
async fn test_checkpoint_creation() {
    // Create a temporary directory for testing
    let temp_dir = tempfile::tempdir().unwrap();
    let config = TrajectoryConfig {
        enabled: true,
        output_dir: temp_dir.path().to_path_buf(),
        auto_visualize: false,
        preserve_full_context: false,
        full_context_strategy: FullContextStrategy::default(),
    };

    let mut recorder = TrajectoryRecorder::new(config);
    
    // Create a mock conversation state
    let ctx = create_test_context().await;
    let conversation_state = create_test_conversation_state(ctx).await;
    
    // Record some initial data
    recorder.record_user_instruction("Test instruction").unwrap();
    
    // Create a checkpoint
    let checkpoint_id = recorder.create_checkpoint("test_checkpoint", &conversation_state).unwrap();
    assert!(!checkpoint_id.is_empty());
    
    // Verify the checkpoint exists
    let checkpoints = recorder.list_checkpoints().unwrap();
    assert!(!checkpoints.is_empty());
    
    // Find our checkpoint
    let found = checkpoints.iter().any(|(id, label, _)| 
        *id == checkpoint_id && label == "test_checkpoint"
    );
    assert!(found);
}

#[tokio::test]
async fn test_checkpoint_restoration() {
    // Create a temporary directory for testing
    let temp_dir = tempfile::tempdir().unwrap();
    let config = TrajectoryConfig {
        enabled: true,
        output_dir: temp_dir.path().to_path_buf(),
        auto_visualize: false,
        preserve_full_context: false,
        full_context_strategy: FullContextStrategy::default(),
    };

    let mut recorder = TrajectoryRecorder::new(config);
    
    // Create a mock context and conversation state
    let ctx = create_test_context().await;
    let mut conversation_state = create_test_conversation_state(ctx.clone()).await;
    
    // Add some initial state
    conversation_state.append_new_user_message("Initial message").await;
    
    // Create a checkpoint
    let checkpoint_id = recorder.create_checkpoint("initial", &conversation_state).unwrap();
    
    // Modify the conversation state
    conversation_state.append_new_user_message("Second message").await;
    conversation_state.push_assistant_message(AssistantResponseMessage {
        message_id: None,
        content: "Response to second message".to_string(),
        tool_uses: None,
    });
    
    // Create another checkpoint
    let checkpoint_id2 = recorder.create_checkpoint("modified", &conversation_state).unwrap();
    
    // Restore from the first checkpoint
    let restored_state = recorder.restore_from_checkpoint(&checkpoint_id).unwrap();
    
    // Convert back to conversation state
    let restored_conversation = convert_to_conversation_state(&restored_state, ctx.clone()).await.unwrap();
    
    // Verify the restored state matches the initial state
    assert_eq!(restored_conversation.history().len(), 1);
    assert!(restored_conversation.history()[0].content().contains("Initial message"));
    
    // Restore from the second checkpoint
    let restored_state2 = recorder.restore_from_checkpoint(&checkpoint_id2).unwrap();
    
    // Convert back to conversation state
    let restored_conversation2 = convert_to_conversation_state(&restored_state2, ctx).await.unwrap();
    
    // Verify the restored state matches the modified state
    assert_eq!(restored_conversation2.history().len(), 3);
    assert!(restored_conversation2.history()[2].content().contains("Response to second message"));
}

#[tokio::test]
async fn test_full_context_preservation_strategies() {
    // Create a temporary directory for testing
    let temp_dir = tempfile::tempdir().unwrap();
    
    // Test with Never strategy
    {
        let config = TrajectoryConfig {
            enabled: true,
            output_dir: temp_dir.path().join("never"),
            auto_visualize: false,
            preserve_full_context: true,
            full_context_strategy: FullContextStrategy::Never,
        };

        let mut recorder = TrajectoryRecorder::new(config);
        let ctx = create_test_context().await;
        let conversation_state = create_test_conversation_state(ctx).await;
        
        let checkpoint_id = recorder.create_checkpoint("never_test", &conversation_state).unwrap();
        let checkpoint = recorder.restore_from_checkpoint(&checkpoint_id).unwrap();
        
        assert!(checkpoint.full_context.is_none());
    }
    
    // Test with Always strategy
    {
        let config = TrajectoryConfig {
            enabled: true,
            output_dir: temp_dir.path().join("always"),
            auto_visualize: false,
            preserve_full_context: true,
            full_context_strategy: FullContextStrategy::Always,
        };

        let mut recorder = TrajectoryRecorder::new(config);
        let ctx = create_test_context().await;
        let conversation_state = create_test_conversation_state(ctx).await;
        
        let checkpoint_id = recorder.create_checkpoint("always_test", &conversation_state).unwrap();
        let checkpoint = recorder.restore_from_checkpoint(&checkpoint_id).unwrap();
        
        assert!(checkpoint.full_context.is_some());
    }
    
    // Test with UserInputOnly strategy
    {
        let config = TrajectoryConfig {
            enabled: true,
            output_dir: temp_dir.path().join("user_input"),
            auto_visualize: false,
            preserve_full_context: true,
            full_context_strategy: FullContextStrategy::UserInputOnly,
        };

        let mut recorder = TrajectoryRecorder::new(config);
        
        // Record user instruction to trigger the strategy
        recorder.record_user_instruction("Test instruction").unwrap();
        
        let ctx = create_test_context().await;
        let conversation_state = create_test_conversation_state(ctx).await;
        
        let checkpoint_id = recorder.create_checkpoint("user_input_test", &conversation_state).unwrap();
        let checkpoint = recorder.restore_from_checkpoint(&checkpoint_id).unwrap();
        
        assert!(checkpoint.full_context.is_some());
    }
    
    // Test with ExplicitCheckpointsOnly strategy
    {
        let config = TrajectoryConfig {
            enabled: true,
            output_dir: temp_dir.path().join("explicit"),
            auto_visualize: false,
            preserve_full_context: true,
            full_context_strategy: FullContextStrategy::ExplicitCheckpointsOnly,
        };

        let mut recorder = TrajectoryRecorder::new(config);
        let ctx = create_test_context().await;
        let conversation_state = create_test_conversation_state(ctx).await;
        
        let checkpoint_id = recorder.create_checkpoint("explicit_test", &conversation_state).unwrap();
        let checkpoint = recorder.restore_from_checkpoint(&checkpoint_id).unwrap();
        
        assert!(checkpoint.full_context.is_some());
    }
}

#[tokio::test]
async fn test_checkpoint_command_handler() {
    let recorder = Arc::new(Mutex::new(TrajectoryRecorder::new(TrajectoryConfig {
        enabled: true,
        output_dir: tempfile::tempdir().unwrap().path().to_path_buf(),
        auto_visualize: false,
        preserve_full_context: false,
        full_context_strategy: FullContextStrategy::default(),
    })));
    
    let ctx = create_test_context().await;
    let mut conversation_state = create_test_conversation_state(ctx.clone()).await;
    
    // Add some initial state
    conversation_state.append_new_user_message("Test message").await;
    
    let mut output = Cursor::new(Vec::new());
    let mut handler = TrajectoryCommandHandler::new(
        &recorder,
        &mut output,
        &mut conversation_state,
        ctx.clone(),
    );
    
    // Test create checkpoint command
    let result = handler.handle_command(
        TrajectorySubcommand::Checkpoint {
            subcommand: CheckpointSubcommand::Create {
                label: "test_checkpoint".to_string(),
            },
        }
    ).await;
    assert!(result.is_ok());
    
    // Test list checkpoints command
    let mut output = Cursor::new(Vec::new());
    let mut handler = TrajectoryCommandHandler::new(
        &recorder,
        &mut output,
        &mut conversation_state,
        ctx.clone(),
    );
    
    let result = handler.handle_command(
        TrajectorySubcommand::Checkpoint {
            subcommand: CheckpointSubcommand::List,
        }
    ).await;
    assert!(result.is_ok());
    
    let output_str = String::from_utf8(output.into_inner()).unwrap();
    assert!(output_str.contains("test_checkpoint"));
    
    // Modify conversation state
    conversation_state.append_new_user_message("Another message").await;
    
    // Test restore checkpoint command
    let checkpoints = recorder.lock().unwrap().list_checkpoints().unwrap();
    let checkpoint_id = &checkpoints[0].0;
    
    let mut output = Cursor::new(Vec::new());
    let mut handler = TrajectoryCommandHandler::new(
        &recorder,
        &mut output,
        &mut conversation_state,
        ctx,
    );
    
    let result = handler.handle_command(
        TrajectorySubcommand::Checkpoint {
            subcommand: CheckpointSubcommand::Restore {
                id: checkpoint_id.clone(),
            },
        }
    ).await;
    assert!(result.is_ok());
    
    // Verify the conversation state was restored
    assert_eq!(conversation_state.history().len(), 1);
    assert!(conversation_state.history()[0].content().contains("Test message"));
}

#[tokio::test]
async fn test_restore_nonexistent_checkpoint() {
    let recorder = Arc::new(Mutex::new(TrajectoryRecorder::new(TrajectoryConfig {
        enabled: true,
        output_dir: tempfile::tempdir().unwrap().path().to_path_buf(),
        auto_visualize: false,
        preserve_full_context: false,
        full_context_strategy: FullContextStrategy::default(),
    })));
    
    let ctx = create_test_context().await;
    let mut conversation_state = create_test_conversation_state(ctx.clone()).await;
    let mut output = Cursor::new(Vec::new());
    
    let mut handler = TrajectoryCommandHandler::new(
        &recorder,
        &mut output,
        &mut conversation_state,
        ctx,
    );
    
    // Try to restore from a non-existent checkpoint
    let result = handler.handle_command(
        TrajectorySubcommand::Checkpoint {
            subcommand: CheckpointSubcommand::Restore {
                id: "non_existent_id".to_string(),
            },
        }
    ).await;
    
    assert!(result.is_ok()); // The command handler should handle the error gracefully
    
    let output_str = String::from_utf8(output.into_inner()).unwrap();
    assert!(output_str.contains("Failed to restore checkpoint"));
}

#[tokio::test]
async fn test_create_checkpoint_duplicate_label() {
    let recorder = Arc::new(Mutex::new(TrajectoryRecorder::new(TrajectoryConfig {
        enabled: true,
        output_dir: tempfile::tempdir().unwrap().path().to_path_buf(),
        auto_visualize: false,
        preserve_full_context: false,
        full_context_strategy: FullContextStrategy::default(),
    })));
    
    let ctx = create_test_context().await;
    let mut conversation_state = create_test_conversation_state(ctx.clone()).await;
    
    // Create first checkpoint
    {
        let mut output = Cursor::new(Vec::new());
        let mut handler = TrajectoryCommandHandler::new(
            &recorder,
            &mut output,
            &mut conversation_state,
            ctx.clone(),
        );
        
        let result = handler.handle_command(
            TrajectorySubcommand::Checkpoint {
                subcommand: CheckpointSubcommand::Create {
                    label: "duplicate_label".to_string(),
                },
            }
        ).await;
        assert!(result.is_ok());
    }
    
    // Create second checkpoint with same label
    {
        let mut output = Cursor::new(Vec::new());
        let mut handler = TrajectoryCommandHandler::new(
            &recorder,
            &mut output,
            &mut conversation_state,
            ctx,
        );
        
        let result = handler.handle_command(
            TrajectorySubcommand::Checkpoint {
                subcommand: CheckpointSubcommand::Create {
                    label: "duplicate_label".to_string(),
                },
            }
        ).await;
        assert!(result.is_ok());
        
        // Should succeed but create a different checkpoint
        let checkpoints = recorder.lock().unwrap().list_checkpoints().unwrap();
        assert_eq!(checkpoints.len(), 2);
        assert_eq!(checkpoints[0].1, "duplicate_label");
        assert_eq!(checkpoints[1].1, "duplicate_label");
        assert_ne!(checkpoints[0].0, checkpoints[1].0);
    }
}

#[tokio::test]
async fn test_restore_with_disabled_recording() {
    let recorder = Arc::new(Mutex::new(TrajectoryRecorder::new(TrajectoryConfig {
        enabled: false, // Disabled
        output_dir: tempfile::tempdir().unwrap().path().to_path_buf(),
        auto_visualize: false,
        preserve_full_context: false,
        full_context_strategy: FullContextStrategy::default(),
    })));
    
    let ctx = create_test_context().await;
    let mut conversation_state = create_test_conversation_state(ctx.clone()).await;
    let mut output = Cursor::new(Vec::new());
    
    let mut handler = TrajectoryCommandHandler::new(
        &recorder,
        &mut output,
        &mut conversation_state,
        ctx,
    );
    
    // Try to restore when disabled
    let result = handler.handle_command(
        TrajectorySubcommand::Checkpoint {
            subcommand: CheckpointSubcommand::Restore {
                id: "some_id".to_string(),
            },
        }
    ).await;
    
    assert!(result.is_ok()); // The command handler should handle the error gracefully
    
    let output_str = String::from_utf8(output.into_inner()).unwrap();
    assert!(output_str.contains("Trajectory recording was not properly enabled"));
}
