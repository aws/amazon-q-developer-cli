//! Configuration management for the semantic search client.
//!
//! This module provides a centralized configuration system for semantic search settings.
//! It supports loading configuration from a JSON file and provides default values.
//! It also manages model paths and directory structure.

use std::fs;
use std::path::{
    Path,
    PathBuf,
};

use once_cell::sync::OnceCell;
use serde::{
    Deserialize,
    Serialize,
};

/// Main configuration structure for the semantic search client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchConfig {
    /// Chunk size for text splitting
    pub chunk_size: usize,

    /// Chunk overlap for text splitting
    pub chunk_overlap: usize,

    /// Default number of results to return from searches
    pub default_results: usize,

    /// Model name for embeddings
    pub model_name: String,

    /// Timeout in milliseconds for embedding operations
    pub timeout: u64,

    /// Base directory for storing persistent contexts
    pub base_dir: PathBuf,
}

impl Default for SemanticSearchConfig {
    fn default() -> Self {
        Self {
            chunk_size: 512,
            chunk_overlap: 128,
            default_results: 5,
            model_name: "all-MiniLM-L6-v2".to_string(),
            timeout: 30000, // 30 seconds
            base_dir: get_default_base_dir(),
        }
    }
}

// Global configuration instance using OnceCell for thread-safe initialization
static CONFIG: OnceCell<SemanticSearchConfig> = OnceCell::new();

/// Get the default base directory for semantic search
///
/// # Returns
///
/// The default base directory path
pub fn get_default_base_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".semantic_search")
}

/// Get the models directory path
///
/// # Arguments
///
/// * `base_dir` - Base directory for semantic search
///
/// # Returns
///
/// The models directory path
pub fn get_models_dir(base_dir: &Path) -> PathBuf {
    base_dir.join("models")
}

/// Get the model directory for a specific model
///
/// # Arguments
///
/// * `base_dir` - Base directory for semantic search
/// * `model_name` - Name of the model
///
/// # Returns
///
/// The model directory path
pub fn get_model_dir(base_dir: &Path, model_name: &str) -> PathBuf {
    get_models_dir(base_dir).join(model_name)
}

/// Get the model file path for a specific model
///
/// # Arguments
///
/// * `base_dir` - Base directory for semantic search
/// * `model_name` - Name of the model
/// * `file_name` - Name of the file
///
/// # Returns
///
/// The model file path
pub fn get_model_file_path(base_dir: &Path, model_name: &str, file_name: &str) -> PathBuf {
    get_model_dir(base_dir, model_name).join(file_name)
}

/// Ensure the models directory exists
///
/// # Arguments
///
/// * `base_dir` - Base directory for semantic search
///
/// # Returns
///
/// Result indicating success or failure
pub fn ensure_models_dir(base_dir: &Path) -> std::io::Result<()> {
    let models_dir = get_models_dir(base_dir);
    std::fs::create_dir_all(models_dir)
}

/// Initializes the global configuration.
///
/// # Arguments
///
/// * `base_dir` - Base directory where the configuration file should be stored
///
/// # Returns
///
/// A Result indicating success or failure
pub fn init_config(base_dir: &Path) -> std::io::Result<()> {
    let config_path = base_dir.join("semantic_search_config.json");
    let config = load_or_create_config(&config_path)?;

    // Set the configuration if it hasn't been set already
    // This is thread-safe and will only succeed once
    if CONFIG.set(config).is_err() {
        // Configuration was already initialized, which is fine
    }

    Ok(())
}

/// Gets a reference to the global configuration.
///
/// # Returns
///
/// A reference to the global configuration
///
/// # Panics
///
/// Panics if the configuration has not been initialized
pub fn get_config() -> &'static SemanticSearchConfig {
    CONFIG.get().expect("Semantic search configuration not initialized")
}

/// Loads the configuration from a file or creates a new one with default values.
///
/// # Arguments
///
/// * `config_path` - Path to the configuration file
///
/// # Returns
///
/// A Result containing the loaded or created configuration
fn load_or_create_config(config_path: &Path) -> std::io::Result<SemanticSearchConfig> {
    if config_path.exists() {
        // Load existing config
        let content = fs::read_to_string(config_path)?;
        match serde_json::from_str(&content) {
            Ok(config) => Ok(config),
            Err(_) => {
                // If parsing fails, create a new default config
                let config = SemanticSearchConfig::default();
                save_config(&config, config_path)?;
                Ok(config)
            },
        }
    } else {
        // Create new config with default values
        let config = SemanticSearchConfig::default();

        // Ensure parent directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        save_config(&config, config_path)?;
        Ok(config)
    }
}

/// Saves the configuration to a file.
///
/// # Arguments
///
/// * `config` - The configuration to save
/// * `config_path` - Path to the configuration file
///
/// # Returns
///
/// A Result indicating success or failure
fn save_config(config: &SemanticSearchConfig, config_path: &Path) -> std::io::Result<()> {
    let content = serde_json::to_string_pretty(config)?;
    fs::write(config_path, content)
}

/// Updates the configuration with new values and saves it to disk.
///
/// # Arguments
///
/// * `base_dir` - Base directory where the configuration file is stored
/// * `new_config` - The new configuration values
///
/// # Returns
///
/// A Result indicating success or failure
pub fn update_config(base_dir: &Path, new_config: SemanticSearchConfig) -> std::io::Result<()> {
    let config_path = base_dir.join("semantic_search_config.json");

    // Save the new config to disk
    save_config(&new_config, &config_path)?;

    // Update the global config
    // This will only work if the config hasn't been initialized yet
    // Otherwise, we need to restart the application to apply changes
    let _ = CONFIG.set(new_config);

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn test_default_config() {
        let config = SemanticSearchConfig::default();
        assert_eq!(config.chunk_size, 512);
        assert_eq!(config.chunk_overlap, 128);
        assert_eq!(config.default_results, 5);
        assert_eq!(config.model_name, "all-MiniLM-L6-v2");
    }

    #[test]
    fn test_load_or_create_config() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("semantic_search_config.json");

        // Test creating a new config
        let config = load_or_create_config(&config_path).unwrap();
        assert_eq!(config.chunk_size, 512);
        assert!(config_path.exists());

        // Test loading an existing config
        let mut modified_config = config.clone();
        modified_config.chunk_size = 1024;
        save_config(&modified_config, &config_path).unwrap();

        let loaded_config = load_or_create_config(&config_path).unwrap();
        assert_eq!(loaded_config.chunk_size, 1024);
    }

    #[test]
    fn test_update_config() {
        let temp_dir = tempdir().unwrap();

        // Initialize with default config
        init_config(temp_dir.path()).unwrap();

        // Create a new config with different values
        let new_config = SemanticSearchConfig {
            chunk_size: 1024,
            chunk_overlap: 256,
            default_results: 10,
            model_name: "different-model".to_string(),
            timeout: 30000,
            base_dir: temp_dir.path().to_path_buf(),
        };

        // Update the config
        update_config(temp_dir.path(), new_config).unwrap();

        // Check that the file was updated
        let config_path = temp_dir.path().join("semantic_search_config.json");
        let content = fs::read_to_string(config_path).unwrap();
        let loaded_config: SemanticSearchConfig = serde_json::from_str(&content).unwrap();

        assert_eq!(loaded_config.chunk_size, 1024);
        assert_eq!(loaded_config.chunk_overlap, 256);
        assert_eq!(loaded_config.default_results, 10);
        assert_eq!(loaded_config.model_name, "different-model");
    }

    #[test]
    fn test_directory_structure() {
        let temp_dir = tempdir().unwrap();
        let base_dir = temp_dir.path();

        // Test models directory path
        let models_dir = get_models_dir(base_dir);
        assert_eq!(models_dir, base_dir.join("models"));

        // Test model directory path
        let model_dir = get_model_dir(base_dir, "test-model");
        assert_eq!(model_dir, base_dir.join("models").join("test-model"));

        // Test model file path
        let model_file = get_model_file_path(base_dir, "test-model", "model.bin");
        assert_eq!(model_file, base_dir.join("models").join("test-model").join("model.bin"));
    }

    #[test]
    fn test_ensure_models_dir() {
        let temp_dir = tempdir().unwrap();
        let base_dir = temp_dir.path();

        // Ensure models directory exists
        ensure_models_dir(base_dir).unwrap();

        // Check that directory was created
        let models_dir = get_models_dir(base_dir);
        assert!(models_dir.exists());
        assert!(models_dir.is_dir());
    }
}
