#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    
    use crate::cli::chat::trajectory::repository::{Repository, Step, AgentAction, ActionResult};
    
    fn create_test_repository() -> Repository {
        let temp_dir = std::env::temp_dir().join("trajectory_repository_test");
        let _ = std::fs::create_dir_all(&temp_dir);
        Repository::new(temp_dir)
    }
    
    #[test]
    fn test_repository_initialization() {
        let repo = create_test_repository();
        
        // Check default trajectory exists
        assert!(repo.trajectories.contains_key("main"));
        assert_eq!(repo.current_trajectory, "main");
        assert!(repo.steps.is_empty());
    }
    
    #[test]
    fn test_step_builder() {
        let repo = create_test_repository();
        let step = repo.step_builder()
            .user_instruction("Test instruction")
            .category("user_instruction")
            .tag("user-input")
            .build();
        
        assert_eq!(step.trajectory_name, "main");
        assert!(step.user_instruction.is_some());
        assert_eq!(step.user_instruction.unwrap(), "Test instruction");
        assert_eq!(step.category.unwrap(), "user_instruction");
        assert!(step.tags.contains(&"user-input".to_string()));
    }
    
    #[test]
    fn test_record_step() {
        let mut repo = create_test_repository();
        let step = repo.step_builder()
            .user_instruction("Test instruction")
            .category("user_instruction")
            .tag("user-input")
            .build();
        
        let step_id = repo.record_step(step).unwrap();
        
        // Check step was added
        assert!(repo.steps.contains_key(&step_id));
        assert_eq!(repo.steps.len(), 1);
        
        // Check trajectory was updated
        let trajectory = repo.trajectories.get("main").unwrap();
        assert_eq!(trajectory.latest_step_id, step_id);
        assert_eq!(trajectory.step_ids.len(), 1);
        assert_eq!(trajectory.step_ids[0], step_id);
    }
    
    #[test]
    fn test_create_trajectory() {
        let mut repo = create_test_repository();
        
        // Create a step in the main trajectory
        let step = repo.step_builder()
            .user_instruction("Test instruction")
            .build();
        let step_id = repo.record_step(step).unwrap();
        
        // Create a new trajectory
        repo.create_trajectory("test_trajectory").unwrap();
        
        // Check new trajectory was created
        assert!(repo.trajectories.contains_key("test_trajectory"));
        assert_eq!(repo.current_trajectory, "test_trajectory");
        
        // Check new trajectory has the latest step ID from main
        let new_trajectory = repo.trajectories.get("test_trajectory").unwrap();
        assert_eq!(new_trajectory.latest_step_id, step_id);
        assert!(new_trajectory.step_ids.is_empty());
    }
    
    #[test]
    fn test_switch_trajectory() {
        let mut repo = create_test_repository();
        
        // Create a new trajectory
        repo.create_trajectory("test_trajectory").unwrap();
        assert_eq!(repo.current_trajectory, "test_trajectory");
        
        // Switch back to main
        repo.switch_trajectory("main").unwrap();
        assert_eq!(repo.current_trajectory, "main");
    }
    
    #[test]
    fn test_checkpoints() {
        let mut repo = create_test_repository();
        
        // Create a step with checkpoint tag
        let step = repo.step_builder()
            .category("checkpoint")
            .tag("checkpoint")
            .tag("test_label")
            .build();
        let step_id = repo.record_step(step).unwrap();
        
        // List checkpoints
        let checkpoints = repo.list_checkpoints();
        assert_eq!(checkpoints.len(), 1);
        assert_eq!(checkpoints[0].0, step_id);
        
        // Get checkpoint by ID
        let checkpoint = repo.get_checkpoint(&step_id).unwrap();
        assert!(checkpoint.tags.contains(&"checkpoint".to_string()));
        assert!(checkpoint.tags.contains(&"test_label".to_string()));
        
        // Get checkpoint by label
        let checkpoint = repo.get_checkpoint("test_label").unwrap();
        assert!(checkpoint.tags.contains(&"checkpoint".to_string()));
        assert!(checkpoint.tags.contains(&"test_label".to_string()));
    }
}
