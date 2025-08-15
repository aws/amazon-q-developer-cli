// save_config.rs
// Save configuration for Amazon Q CLI automatic naming feature

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use dirs::home_dir;

/// Format for generating filenames
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilenameFormat {
    /// Default format: Q_[MainTopic]_[SubTopic]_[ActionType] - DDMMMYY-HHMM
    Default,
    
    /// Custom format with placeholders:
    /// - {main_topic}: Main topic extracted from conversation
    /// - {sub_topic}: Sub-topic extracted from conversation
    /// - {action_type}: Action type extracted from conversation
    /// - {date}: Date in the configured format
    /// - {id}: Conversation ID
    Custom(String),
}

/// Configuration for the save command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveConfig {
    /// Path to the configuration file
    #[serde(skip)]
    config_path: PathBuf,
    
    /// Default path for saving conversations
    default_path: String,
    
    /// Format for generating filenames
    filename_format: FilenameFormat,
    
    /// Prefix for filenames
    prefix: String,
    
    /// Separator for filename components
    separator: String,
    
    /// Format for dates in filenames
    date_format: String,
    
    /// Name of the topic extractor to use
    topic_extractor_name: String,
    
    /// Templates for generating filenames
    templates: HashMap<String, FilenameFormat>,
    
    /// Custom metadata for saved files
    metadata: HashMap<String, String>,
    
    /// Mock file system error for testing
    #[serde(skip)]
    mock_fs_error: Option<io::Error>,
}

impl SaveConfig {
    /// Create a new save configuration
    pub fn new<P: AsRef<Path>>(config_path: P) -> Self {
        let config_path = config_path.as_ref().to_path_buf();
        
        // Try to load existing configuration
        if config_path.exists() {
            if let Ok(content) = fs::read_to_string(&config_path) {
                if let Ok(mut config) = serde_json::from_str::<SaveConfig>(&content) {
                    config.config_path = config_path;
                    return config;
                }
            }
        }
        
        // Create default configuration
        let default_path = home_dir()
            .map(|p| p.join("qChats"))
            .unwrap_or_else(|| PathBuf::from("./qChats"))
            .to_string_lossy()
            .to_string();
        
        Self {
            config_path,
            default_path,
            filename_format: FilenameFormat::Default,
            prefix: String::from("Q_"),
            separator: String::from("_"),
            date_format: String::from("DDMMMYY-HHMM"),
            topic_extractor_name: String::from("basic"),
            templates: HashMap::new(),
            metadata: HashMap::new(),
            mock_fs_error: None,
        }
    }
    
    /// Get the default save path
    pub fn get_default_path(&self) -> String {
        // Expand ~ to home directory
        if self.default_path.starts_with('~') {
            if let Some(home) = home_dir() {
                return home.join(&self.default_path[2..])
                    .to_string_lossy()
                    .to_string();
            }
        }
        
        self.default_path.clone()
    }
    
    /// Set the default save path
    pub fn set_default_path(&mut self, path: &str) -> io::Result<()> {
        self.default_path = path.to_string();
        self.save()
    }
    
    /// Get the filename format
    pub fn get_filename_format(&self) -> &FilenameFormat {
        &self.filename_format
    }
    
    /// Set the filename format
    pub fn set_filename_format(&mut self, format: FilenameFormat) -> io::Result<()> {
        self.filename_format = format;
        self.save()
    }
    
    /// Get the prefix for filenames
    pub fn get_prefix(&self) -> &str {
        &self.prefix
    }
    
    /// Set the prefix for filenames
    pub fn set_prefix(&mut self, prefix: &str) -> io::Result<()> {
        self.prefix = prefix.to_string();
        self.save()
    }
    
    /// Get the separator for filename components
    pub fn get_separator(&self) -> &str {
        &self.separator
    }
    
    /// Set the separator for filename components
    pub fn set_separator(&mut self, separator: &str) -> io::Result<()> {
        self.separator = separator.to_string();
        self.save()
    }
    
    /// Get the format for dates in filenames
    pub fn get_date_format(&self) -> &str {
        &self.date_format
    }
    
    /// Set the format for dates in filenames
    pub fn set_date_format(&mut self, format: &str) -> io::Result<()> {
        self.date_format = format.to_string();
        self.save()
    }
    
    /// Get the name of the topic extractor to use
    pub fn get_topic_extractor_name(&self) -> &str {
        &self.topic_extractor_name
    }
    
    /// Set the name of the topic extractor to use
    pub fn set_topic_extractor_name(&mut self, name: &str) -> io::Result<()> {
        self.topic_extractor_name = name.to_string();
        self.save()
    }
    
    /// Get a template for generating filenames
    pub fn get_template(&self, name: &str) -> Option<&FilenameFormat> {
        self.templates.get(name)
    }
    
    /// Add a template for generating filenames
    pub fn add_template(&mut self, name: &str, format: FilenameFormat) -> io::Result<()> {
        self.templates.insert(name.to_string(), format);
        self.save()
    }
    
    /// Remove a template for generating filenames
    pub fn remove_template(&mut self, name: &str) -> io::Result<()> {
        self.templates.remove(name);
        self.save()
    }
    
    /// Get all templates for generating filenames
    pub fn get_templates(&self) -> &HashMap<String, FilenameFormat> {
        &self.templates
    }
    
    /// Get custom metadata for saved files
    pub fn get_metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }
    
    /// Add custom metadata for saved files
    pub fn add_metadata(&mut self, key: &str, value: &str) -> io::Result<()> {
        self.metadata.insert(key.to_string(), value.to_string());
        self.save()
    }
    
    /// Remove custom metadata for saved files
    pub fn remove_metadata(&mut self, key: &str) -> io::Result<()> {
        self.metadata.remove(key);
        self.save()
    }
    
    /// Save the configuration to file
    pub fn save(&self) -> io::Result<()> {
        // Check for mock error
        if let Some(ref err) = self.mock_fs_error {
            return Err(io::Error::new(err.kind(), err.to_string()));
        }
        
        // Create parent directory if it doesn't exist
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // Serialize and write to file
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&self.config_path, content)
    }
    
    /// Check if a path exists and is writable
    pub fn is_path_writable<P: AsRef<Path>>(&self, path: P) -> bool {
        // Check for mock error
        if self.mock_fs_error.is_some() {
            return false;
        }
        
        let path = path.as_ref();
        
        // If the path exists, check if it's writable
        if path.exists() {
            if path.is_dir() {
                // For directories, check if we can create a temporary file
                let temp_file = path.join(".q_write_test");
                let result = fs::write(&temp_file, "test");
                if result.is_ok() {
                    let _ = fs::remove_file(temp_file);
                    return true;
                }
                return false;
            } else {
                // For files, check if we can open them for writing
                fs::OpenOptions::new()
                    .write(true)
                    .open(path)
                    .is_ok()
            }
        } else {
            // If the path doesn't exist, check if we can create it
            if let Some(parent) = path.parent() {
                if !parent.exists() {
                    return fs::create_dir_all(parent).is_ok();
                }
                return self.is_path_writable(parent);
            }
        }
        
        false
    }
    
    /// Create directories for a path if they don't exist
    pub fn create_dirs_for_path<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        // Check for mock error
        if let Some(ref err) = self.mock_fs_error {
            return Err(io::Error::new(err.kind(), err.to_string()));
        }
        
        let path = path.as_ref();
        
        // If the path is a file, get its parent directory
        let dir = if path.extension().is_some() {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        
        // Create the directory if it doesn't exist
        if !dir.exists() {
            fs::create_dir_all(dir)?;
        }
        
        Ok(())
    }
    
    /// Convert configuration to JSON
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }
    
    /// Create configuration from JSON
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }
    
    /// Set a mock file system error for testing
    #[cfg(test)]
    pub fn set_mock_fs_error(&mut self, error: Option<io::Error>) {
        self.mock_fs_error = error;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[test]
    fn test_new_config() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        
        let config = SaveConfig::new(&config_path);
        
        assert_eq!(config.config_path, config_path);
        assert!(config.get_default_path().contains("qChats"));
        assert_eq!(config.get_prefix(), "Q_");
        assert_eq!(config.get_separator(), "_");
        assert_eq!(config.get_date_format(), "DDMMMYY-HHMM");
        assert_eq!(config.get_topic_extractor_name(), "basic");
        assert!(config.get_templates().is_empty());
        assert!(config.get_metadata().is_empty());
    }
    
    #[test]
    fn test_load_existing_config() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        
        // Create a config file
        let mut config = SaveConfig::new(&config_path);
        config.default_path = "/custom/path".to_string();
        config.prefix = "Custom_".to_string();
        config.save().unwrap();
        
        // Load the config
        let loaded_config = SaveConfig::new(&config_path);
        
        assert_eq!(loaded_config.default_path, "/custom/path");
        assert_eq!(loaded_config.prefix, "Custom_");
    }
    
    #[test]
    fn test_set_default_path() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        
        let mut config = SaveConfig::new(&config_path);
        config.set_default_path("/new/path").unwrap();
        
        assert_eq!(config.default_path, "/new/path");
        
        // Check that the config was saved
        let loaded_config = SaveConfig::new(&config_path);
        assert_eq!(loaded_config.default_path, "/new/path");
    }
    
    #[test]
    fn test_set_filename_format() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        
        let mut config = SaveConfig::new(&config_path);
        config.set_filename_format(FilenameFormat::Custom(String::from("{main_topic}-{date}"))).unwrap();
        
        match config.get_filename_format() {
            FilenameFormat::Custom(format) => assert_eq!(format, "{main_topic}-{date}"),
            _ => panic!("Expected Custom format"),
        }
        
        // Check that the config was saved
        let loaded_config = SaveConfig::new(&config_path);
        match loaded_config.get_filename_format() {
            FilenameFormat::Custom(format) => assert_eq!(format, "{main_topic}-{date}"),
            _ => panic!("Expected Custom format"),
        }
    }
    
    #[test]
    fn test_templates() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        
        let mut config = SaveConfig::new(&config_path);
        config.add_template(
            "technical",
            FilenameFormat::Custom(String::from("Tech_{main_topic}"))
        ).unwrap();
        
        let template = config.get_template("technical").expect("Template not found");
        match template {
            FilenameFormat::Custom(format) => assert_eq!(format, "Tech_{main_topic}"),
            _ => panic!("Expected Custom format"),
        }
        
        // Check that the config was saved
        let loaded_config = SaveConfig::new(&config_path);
        let loaded_template = loaded_config.get_template("technical").expect("Template not found");
        match loaded_template {
            FilenameFormat::Custom(format) => assert_eq!(format, "Tech_{main_topic}"),
            _ => panic!("Expected Custom format"),
        }
        
        // Remove template
        config.remove_template("technical").unwrap();
        assert!(config.get_template("technical").is_none());
    }
    
    #[test]
    fn test_metadata() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        
        let mut config = SaveConfig::new(&config_path);
        config.add_metadata("category", "test").unwrap();
        
        assert_eq!(config.get_metadata().get("category"), Some(&String::from("test")));
        
        // Check that the config was saved
        let loaded_config = SaveConfig::new(&config_path);
        assert_eq!(loaded_config.get_metadata().get("category"), Some(&String::from("test")));
        
        // Remove metadata
        config.remove_metadata("category").unwrap();
        assert!(config.get_metadata().get("category").is_none());
    }
    
    #[test]
    fn test_is_path_writable() {
        let temp_dir = tempdir().unwrap();
        let config = SaveConfig::new("/tmp/config.json");
        
        // Existing directory
        assert!(config.is_path_writable(temp_dir.path()));
        
        // Non-existent path with writable parent
        let non_existent = temp_dir.path().join("non_existent");
        assert!(config.is_path_writable(&non_existent));
        
        // Non-existent path with non-existent parent
        let deep_non_existent = temp_dir.path().join("a/b/c/non_existent");
        assert!(config.is_path_writable(&deep_non_existent));
    }
    
    #[test]
    fn test_create_dirs_for_path() {
        let temp_dir = tempdir().unwrap();
        let config = SaveConfig::new("/tmp/config.json");
        
        // Create directories for a file path
        let file_path = temp_dir.path().join("a/b/c/file.txt");
        config.create_dirs_for_path(&file_path).unwrap();
        
        assert!(file_path.parent().unwrap().exists());
        
        // Create directories for a directory path
        let dir_path = temp_dir.path().join("d/e/f");
        config.create_dirs_for_path(&dir_path).unwrap();
        
        assert!(dir_path.exists());
    }
    
    #[test]
    fn test_mock_fs_error() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        
        let mut config = SaveConfig::new(&config_path);
        config.set_mock_fs_error(Some(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Mock permission denied"
        )));
        
        // Test save
        let result = config.save();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        
        // Test create_dirs_for_path
        let result = config.create_dirs_for_path(temp_dir.path().join("test"));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        
        // Test is_path_writable
        assert!(!config.is_path_writable(temp_dir.path()));
    }
    
    #[test]
    fn test_serialization() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        
        let mut config = SaveConfig::new(&config_path);
        config.prefix = "Test_".to_string();
        config.add_template(
            "technical",
            FilenameFormat::Custom(String::from("Tech_{main_topic}"))
        ).unwrap();
        config.add_metadata("category", "test").unwrap();
        
        let json = config.to_json().expect("Failed to serialize");
        let deserialized = SaveConfig::from_json(&json).expect("Failed to deserialize");
        
        assert_eq!(deserialized.prefix, "Test_");
        assert_eq!(deserialized.get_metadata().get("category"), Some(&String::from("test")));
        
        let template = deserialized.get_template("technical").expect("Template not found");
        match template {
            FilenameFormat::Custom(format) => assert_eq!(format, "Tech_{main_topic}"),
            _ => panic!("Expected Custom format"),
        }
    }
    
    #[test]
    fn test_integration_with_filename_generator() {
        use crate::conversation::Conversation;
        use crate::filename_generator::generate_filename;
        
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        
        let mut config = SaveConfig::new(&config_path);
        let default_path = temp_dir.path().join("qChats").to_string_lossy().to_string();
        config.set_default_path(&default_path).unwrap();
        
        // Create a conversation
        let mut conv = Conversation::new("test-id".to_string());
        conv.add_user_message("I need help with Amazon Q CLI".to_string());
        
        // Generate a filename
        let filename = generate_filename(&conv);
        
        // Combine with default path
        let full_path = Path::new(&config.get_default_path()).join(filename);
        
        // Create directories
        config.create_dirs_for_path(&full_path).unwrap();
        
        assert!(Path::new(&config.get_default_path()).exists());
    }
}
