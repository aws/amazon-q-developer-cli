use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::cli::chat::trajectory::{
    FullContextStrategy,
    TrajectoryConfig,
    TrajectoryRecorder,
};

#[test]
fn test_recorder_initialization() {
    // Create a temporary directory for testing
    let temp_dir = tempfile::tempdir().unwrap();
    let config = TrajectoryConfig {
        enabled: true,
        output_dir: temp_dir.path().to_path_buf(),
        auto_visualize: false,
        preserve_full_context: false,
        full_context_strategy: FullContextStrategy::default(),
    };

    let recorder = TrajectoryRecorder::new(config);
    assert!(recorder.is_enabled());
}

#[test]
fn test_recorder_disable_enable() {
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
    assert!(recorder.is_enabled());

    // Disable the recorder
    recorder.set_enabled(false);
    assert!(!recorder.is_enabled());

    // Enable the recorder again
    recorder.set_enabled(true);
    assert!(recorder.is_enabled());
}

#[test]
fn test_recorder_config_options() {
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
    
    // Test setting auto_visualize
    recorder.set_config_option("auto_visualize", "true").unwrap();
    let config = recorder.get_config();
    assert_eq!(config.get("auto_visualize"), Some(&"true".to_string()));

    // Test setting preserve_full_context
    recorder.set_config_option("preserve_full_context", "true").unwrap();
    let config = recorder.get_config();
    assert_eq!(config.get("preserve_full_context"), Some(&"true".to_string()));

    // Test setting full_context_strategy
    recorder.set_config_option("full_context_strategy", "always").unwrap();
    let config = recorder.get_config();
    assert_eq!(config.get("full_context_strategy"), Some(&"always".to_string()));
}

#[test]
fn test_record_user_instruction() {
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
    
    // Record a user instruction
    let result = recorder.record_user_instruction("Test instruction");
    assert!(result.is_ok());
}

#[test]
fn test_record_tool_use_and_result() {
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
    
    // Record a tool use
    let mut parameters = HashMap::new();
    parameters.insert("path".to_string(), serde_json::json!("/test/path"));
    parameters.insert("command".to_string(), serde_json::json!("read"));
    
    let step_id = recorder.record_tool_use("fs_read", parameters, Some("Reading a file")).unwrap();
    assert!(!step_id.is_empty());
    
    // Record a successful result
    let result = recorder.record_tool_result(
        &step_id, 
        true, 
        Some(serde_json::json!({"content": "File content"})), 
        None
    );
    assert!(result.is_ok());
}

#[test]
fn test_record_tool_use_error() {
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
    
    // Record a tool use
    let mut parameters = HashMap::new();
    parameters.insert("path".to_string(), serde_json::json!("/test/path"));
    parameters.insert("command".to_string(), serde_json::json!("read"));
    
    let step_id = recorder.record_tool_use("fs_read", parameters, Some("Reading a file")).unwrap();
    assert!(!step_id.is_empty());
    
    // Record a failed result
    let result = recorder.record_tool_result(
        &step_id, 
        false, 
        None, 
        Some("File not found")
    );
    assert!(result.is_ok());
}

#[test]
fn test_disabled_recorder() {
    // Create a temporary directory for testing
    let temp_dir = tempfile::tempdir().unwrap();
    let config = TrajectoryConfig {
        enabled: false,  // Disabled
        output_dir: temp_dir.path().to_path_buf(),
        auto_visualize: false,
        preserve_full_context: false,
        full_context_strategy: FullContextStrategy::default(),
    };

    let mut recorder = TrajectoryRecorder::new(config);
    assert!(!recorder.is_enabled());
    
    // Operations should succeed but do nothing when disabled
    let result = recorder.record_user_instruction("Test instruction");
    assert!(result.is_ok());
    
    let mut parameters = HashMap::new();
    parameters.insert("path".to_string(), serde_json::json!("/test/path"));
    
    let step_id = recorder.record_tool_use("fs_read", parameters, Some("Reading a file")).unwrap();
    assert!(step_id.is_empty());  // Should return empty string when disabled
}
