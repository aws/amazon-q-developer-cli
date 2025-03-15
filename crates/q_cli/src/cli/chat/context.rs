use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use eyre::{Result, eyre};
use serde::{Deserialize, Serialize};

/// Configuration for context files, containing paths to include in the context.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextConfig {
    /// List of file paths or glob patterns to include in the context.
    pub paths: Vec<String>,
}

/// Manager for context files and profiles.
#[derive(Debug, Clone)]
pub struct ContextManager {
    /// Directory where context configurations are stored.
    config_dir: PathBuf,
    
    /// Directory where profile configurations are stored.
    profiles_dir: PathBuf,
    
    /// Global context configuration that applies to all profiles.
    pub global_config: ContextConfig,
    
    /// Name of the current active profile.
    pub current_profile: String,
    
    /// Context configuration for the current profile.
    pub profile_config: ContextConfig,
}

impl ContextManager {
    /// Create a new ContextManager with default settings.
    ///
    /// This will:
    /// 1. Create the necessary directories if they don't exist
    /// 2. Load the global configuration
    /// 3. Load the default profile configuration
    ///
    /// # Returns
    /// A Result containing the new ContextManager or an error
    pub fn new() -> Result<Self> {
        // Get the home directory
        let home_dir = dirs::home_dir()
            .ok_or_else(|| eyre!("Could not determine home directory"))?;
        
        // Set up the configuration directories
        let config_dir = home_dir
            .join(".aws")
            .join("amazonq")
            .join("context");
        
        let profiles_dir = config_dir.join("profiles");
        
        // Create directories if they don't exist
        fs::create_dir_all(&profiles_dir)?;
        
        // Load global configuration
        let global_config = Self::load_global_config(&config_dir)?;
        
        // Load default profile
        let current_profile = "default".to_string();
        let profile_config = Self::load_profile_config(&profiles_dir, &current_profile)?;
        
        Ok(Self {
            config_dir,
            profiles_dir,
            global_config,
            current_profile,
            profile_config,
        })
    }
    
    /// Load the global context configuration.
    ///
    /// If the global configuration file doesn't exist, returns a default configuration.
    ///
    /// # Arguments
    /// * `config_dir` - The directory where the global configuration is stored
    ///
    /// # Returns
    /// A Result containing the global ContextConfig or an error
    fn load_global_config(config_dir: &Path) -> Result<ContextConfig> {
        let global_path = config_dir.join("global.json");
        
        if global_path.exists() {
            // Read and parse the existing configuration file
            let mut file = File::open(&global_path)?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;
            
            let config: ContextConfig = serde_json::from_str(&contents)
                .map_err(|e| eyre!("Failed to parse global configuration: {}", e))?;
            
            Ok(config)
        } else {
            // Return default global configuration with predefined paths
            Ok(ContextConfig {
                paths: vec![
                    "~/.aws/amazonq/rules/**/*.md".to_string(),
                    "AmazonQ.md".to_string(),
                ],
            })
        }
    }
    
    /// Load a profile's context configuration.
    ///
    /// If the profile configuration file doesn't exist, creates a default configuration.
    ///
    /// # Arguments
    /// * `profiles_dir` - The directory where profile configurations are stored
    /// * `profile` - The name of the profile to load
    ///
    /// # Returns
    /// A Result containing the profile's ContextConfig or an error
    fn load_profile_config(profiles_dir: &Path, profile: &str) -> Result<ContextConfig> {
        let profile_path = profiles_dir.join(format!("{}.json", profile));
        
        if profile_path.exists() {
            // Read and parse the existing configuration file
            let mut file = File::open(&profile_path)?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;
            
            let config: ContextConfig = serde_json::from_str(&contents)
                .map_err(|e| eyre!("Failed to parse profile configuration: {}", e))?;
            
            Ok(config)
        } else {
            // Return empty configuration for new profiles
            Ok(ContextConfig::default())
        }
    }
    
    /// Save the current configuration to disk.
    ///
    /// # Arguments
    /// * `global` - If true, save the global configuration; otherwise, save the current profile configuration
    ///
    /// # Returns
    /// A Result indicating success or an error
    fn save_config(&self, global: bool) -> Result<()> {
        if global {
            // Save global configuration
            let global_path = self.config_dir.join("global.json");
            let contents = serde_json::to_string_pretty(&self.global_config)
                .map_err(|e| eyre!("Failed to serialize global configuration: {}", e))?;
            
            let mut file = File::create(&global_path)?;
            file.write_all(contents.as_bytes())?;
        } else {
            // Save profile configuration
            let profile_path = self.profiles_dir.join(format!("{}.json", self.current_profile));
            let contents = serde_json::to_string_pretty(&self.profile_config)
                .map_err(|e| eyre!("Failed to serialize profile configuration: {}", e))?;
            
            let mut file = File::create(&profile_path)?;
            file.write_all(contents.as_bytes())?;
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::tempdir;
    
    // Helper function to create a test ContextManager with temporary directories
    fn create_test_context_manager() -> Result<(ContextManager, tempfile::TempDir)> {
        // Create a temporary directory for testing
        let temp_dir = tempdir()?;
        let config_dir = temp_dir.path().join("context");
        let profiles_dir = config_dir.join("profiles");
        
        // Create the directory structure
        fs::create_dir_all(&profiles_dir)?;
        
        // Create a ContextManager with test directories
        let manager = ContextManager {
            config_dir,
            profiles_dir,
            global_config: ContextConfig {
                paths: vec![
                    "~/.aws/amazonq/rules/**/*.md".to_string(),
                    "AmazonQ.md".to_string(),
                ],
            },
            current_profile: "default".to_string(),
            profile_config: ContextConfig::default(),
        };
        
        Ok((manager, temp_dir))
    }
    
    #[test]
    fn test_load_global_config_new() -> Result<()> {
        // Create a temporary directory
        let temp_dir = tempdir()?;
        let config_dir = temp_dir.path();
        
        // Load global config from a directory that doesn't have a config file
        let config = ContextManager::load_global_config(config_dir)?;
        
        // Verify default paths are set
        assert_eq!(config.paths.len(), 2);
        assert_eq!(config.paths[0], "~/.aws/amazonq/rules/**/*.md");
        assert_eq!(config.paths[1], "AmazonQ.md");
        
        Ok(())
    }
    
    #[test]
    fn test_load_global_config_existing() -> Result<()> {
        // Create a temporary directory
        let temp_dir = tempdir()?;
        let config_dir = temp_dir.path();
        
        // Create a global.json file with custom paths
        let global_path = config_dir.join("global.json");
        let test_config = ContextConfig {
            paths: vec!["test/path1.md".to_string(), "test/path2.md".to_string()],
        };
        let contents = serde_json::to_string_pretty(&test_config)?;
        
        let mut file = File::create(&global_path)?;
        file.write_all(contents.as_bytes())?;
        
        // Load the global config
        let config = ContextManager::load_global_config(config_dir)?;
        
        // Verify custom paths are loaded
        assert_eq!(config.paths.len(), 2);
        assert_eq!(config.paths[0], "test/path1.md");
        assert_eq!(config.paths[1], "test/path2.md");
        
        Ok(())
    }
    
    #[test]
    fn test_load_profile_config_new() -> Result<()> {
        // Create a temporary directory
        let temp_dir = tempdir()?;
        let profiles_dir = temp_dir.path();
        
        // Load profile config for a profile that doesn't exist
        let config = ContextManager::load_profile_config(profiles_dir, "test_profile")?;
        
        // Verify it's an empty config
        assert_eq!(config.paths.len(), 0);
        
        Ok(())
    }
    
    #[test]
    fn test_load_profile_config_existing() -> Result<()> {
        // Create a temporary directory
        let temp_dir = tempdir()?;
        let profiles_dir = temp_dir.path();
        
        // Create a profile config file
        let profile_path = profiles_dir.join("test_profile.json");
        let test_config = ContextConfig {
            paths: vec!["profile/path1.md".to_string(), "profile/path2.md".to_string()],
        };
        let contents = serde_json::to_string_pretty(&test_config)?;
        
        let mut file = File::create(&profile_path)?;
        file.write_all(contents.as_bytes())?;
        
        // Load the profile config
        let config = ContextManager::load_profile_config(profiles_dir, "test_profile")?;
        
        // Verify custom paths are loaded
        assert_eq!(config.paths.len(), 2);
        assert_eq!(config.paths[0], "profile/path1.md");
        assert_eq!(config.paths[1], "profile/path2.md");
        
        Ok(())
    }
    
    #[test]
    fn test_save_config_global() -> Result<()> {
        // Create a test context manager
        let (mut manager, _temp_dir) = create_test_context_manager()?;
        
        // Modify the global config
        manager.global_config.paths = vec!["new/global/path.md".to_string()];
        
        // Save the global config
        manager.save_config(true)?;
        
        // Verify the file was created
        let global_path = manager.config_dir.join("global.json");
        assert!(global_path.exists());
        
        // Read the file and verify its contents
        let mut file = File::open(&global_path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        
        let saved_config: ContextConfig = serde_json::from_str(&contents)?;
        assert_eq!(saved_config.paths.len(), 1);
        assert_eq!(saved_config.paths[0], "new/global/path.md");
        
        Ok(())
    }
    
    #[test]
    fn test_save_config_profile() -> Result<()> {
        // Create a test context manager
        let (mut manager, _temp_dir) = create_test_context_manager()?;
        
        // Modify the profile config
        manager.profile_config.paths = vec!["new/profile/path.md".to_string()];
        
        // Save the profile config
        manager.save_config(false)?;
        
        // Verify the file was created
        let profile_path = manager.profiles_dir.join("default.json");
        assert!(profile_path.exists());
        
        // Read the file and verify its contents
        let mut file = File::open(&profile_path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        
        let saved_config: ContextConfig = serde_json::from_str(&contents)?;
        assert_eq!(saved_config.paths.len(), 1);
        assert_eq!(saved_config.paths[0], "new/profile/path.md");
        
        Ok(())
    }
    
    #[test]
    fn test_new_creates_directories() -> Result<()> {
        // Override home directory for testing
        let temp_dir = tempdir()?;
        let home_dir = temp_dir.path();
        
        // Set the HOME environment variable to our temp directory
        env::set_var("HOME", home_dir.to_str().unwrap());
        
        // Create a new ContextManager
        let _manager = ContextManager::new()?;
        
        // Verify directories were created
        let config_dir = home_dir.join(".aws").join("amazonq").join("context");
        let profiles_dir = config_dir.join("profiles");
        
        assert!(config_dir.exists());
        assert!(profiles_dir.exists());
        
        Ok(())
    }
}
