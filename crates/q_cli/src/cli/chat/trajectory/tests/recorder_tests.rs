#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    
    use crate::cli::chat::trajectory::{TrajectoryRecorder, TrajectoryConfig, FullContextStrategy};
    
    fn create_test_config() -> TrajectoryConfig {
        let temp_dir = std::env::temp_dir().join("trajectory_recorder_test");
        let _ = std::fs::create_dir_all(&temp_dir);
        
        TrajectoryConfig {
            enabled: true,
            output_dir: temp_dir,
            auto_visualize: false,
            preserve_full_context: false,
            full_context_strategy: FullContextStrategy::Never,
        }
    }
    
    #[test]
    fn test_recorder_initialization() {
        let config = create_test_config();
        let recorder = TrajectoryRecorder::new(config.clone());
        
        assert!(recorder.is_enabled());
        assert_eq!(recorder.get_config().get("output_dir").unwrap(), &config.output_dir.to_string_lossy().to_string());
    }
    
    #[test]
    fn test_enable_disable() {
        let config = create_test_config();
        let mut recorder = TrajectoryRecorder::new(config);
        
        // Test initial state
        assert!(recorder.is_enabled());
        
        // Test disable
        recorder.set_enabled(false);
        assert!(!recorder.is_enabled());
        
        // Test enable
        recorder.set_enabled(true);
        assert!(recorder.is_enabled());
        
        // Test no-op when already in desired state
        recorder.set_enabled(true);
        assert!(recorder.is_enabled());
    }
    
    #[test]
    fn test_record_user_instruction() {
        let config = create_test_config();
        let mut recorder = TrajectoryRecorder::new(config);
        
        let result = recorder.record_user_instruction("Test instruction");
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_get_config() {
        let config = create_test_config();
        let recorder = TrajectoryRecorder::new(config.clone());
        
        let config_map = recorder.get_config();
        
        assert_eq!(config_map.get("enabled").unwrap(), "true");
        assert_eq!(config_map.get("output_dir").unwrap(), &config.output_dir.to_string_lossy().to_string());
        assert_eq!(config_map.get("auto_visualize").unwrap(), "false");
        assert_eq!(config_map.get("preserve_full_context").unwrap(), "false");
        assert_eq!(config_map.get("full_context_strategy").unwrap(), "never");
    }
    
    #[test]
    fn test_disabled_recorder_operations() {
        let mut config = create_test_config();
        config.enabled = false;
        let mut recorder = TrajectoryRecorder::new(config);
        
        // Operations should succeed but do nothing when disabled
        assert!(recorder.record_user_instruction("Test").is_ok());
        assert!(recorder.record_reasoning("Test reasoning").is_ok());
        assert!(recorder.record_response("Test response").is_ok());
        
        // Tool use should return empty string when disabled
        let tool_id = recorder.record_tool_use("test_tool", HashMap::new(), None).unwrap();
        assert!(tool_id.is_empty());
        
        // Tool result should succeed but do nothing when disabled
        assert!(recorder.record_tool_result("test_id", true, None, None).is_ok());
    }
    
    #[test]
    fn test_should_preserve_full_context() {
        let mut config = create_test_config();
        config.preserve_full_context = true;
        
        // Test Never strategy
        config.full_context_strategy = FullContextStrategy::Never;
        let recorder = TrajectoryRecorder::new(config.clone());
        assert!(!recorder.should_preserve_full_context(false));
        assert!(!recorder.should_preserve_full_context(true));
        
        // Test Always strategy
        config.full_context_strategy = FullContextStrategy::Always;
        let recorder = TrajectoryRecorder::new(config.clone());
        assert!(recorder.should_preserve_full_context(false));
        assert!(recorder.should_preserve_full_context(true));
        
        // Test UserInputOnly strategy
        config.full_context_strategy = FullContextStrategy::UserInputOnly;
        let mut recorder = TrajectoryRecorder::new(config.clone());
        
        // Without current instruction
        assert!(!recorder.should_preserve_full_context(false));
        assert!(!recorder.should_preserve_full_context(true));
        
        // With current instruction
        let _ = recorder.record_user_instruction("Test");
        assert!(recorder.should_preserve_full_context(false));
        assert!(recorder.should_preserve_full_context(true));
        
        // Test ExplicitCheckpointsOnly strategy
        config.full_context_strategy = FullContextStrategy::ExplicitCheckpointsOnly;
        let recorder = TrajectoryRecorder::new(config);
        assert!(!recorder.should_preserve_full_context(false));
        assert!(recorder.should_preserve_full_context(true));
    }
}
