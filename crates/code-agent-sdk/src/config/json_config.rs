use crate::model::types::LanguageServerConfig;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LanguageConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub file_extensions: Vec<String>,
    pub project_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
    pub initialization_options: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LanguagesConfig {
    pub languages: HashMap<String, LanguageConfig>,
}

impl LanguagesConfig {
    /// Get or create configuration in config root folder
    pub fn get_or_create(config_root: &std::path::Path) -> Result<Self> {
        let config_path = config_root.join("languages.json");

        // Create config directory if it doesn't exist
        if !config_root.exists() {
            std::fs::create_dir_all(config_root)?;
        }

        // If config file exists, load it, otherwise create default
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            let default_config = Self::default_config();
            let config_json = serde_json::to_string_pretty(&default_config)?;
            std::fs::write(&config_path, config_json)?;
            Ok(default_config)
        }
    }

    /// Load configuration from JSON file
    pub fn load() -> Result<Self> {
        let config_path = std::path::Path::new("config/languages.json");

        // Try to load from file, fallback to embedded config
        if config_path.exists() {
            let content = std::fs::read_to_string(config_path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            // Embedded fallback configuration
            Ok(Self::default_config())
        }
    }

    /// Get project patterns for a specific language
    pub fn get_project_patterns_for_language(&self, language: &str) -> Vec<String> {
        if let Some(config) = self.languages.get(language) {
            config.project_patterns.clone()
        } else {
            Vec::new()
        }
    }

    /// Get language for file extension
    pub fn get_language_for_extension(&self, extension: &str) -> Option<String> {
        for (language, config) in &self.languages {
            if config.file_extensions.contains(&extension.to_string()) {
                return Some(language.clone());
            }
        }
        None
    }

    /// Get language server config by language name
    pub fn get_config_by_language(&self, language: &str) -> Result<LanguageServerConfig, String> {
        let config = self
            .languages
            .get(language)
            .ok_or_else(|| format!("Language '{}' not supported", language))?;

        Ok(LanguageServerConfig {
            name: config.name.clone(),
            command: config.command.clone(),
            args: config.args.clone(),
            file_extensions: config.file_extensions.clone(),
            exclude_patterns: config.exclude_patterns.clone(),
            initialization_options: config.initialization_options.clone(),
        })
    }

    /// Get all language server configs
    pub fn all_configs(&self) -> Vec<LanguageServerConfig> {
        self.languages
            .values()
            .map(|config| LanguageServerConfig {
                name: config.name.clone(),
                command: config.command.clone(),
                args: config.args.clone(),
                file_extensions: config.file_extensions.clone(),
                exclude_patterns: config.exclude_patterns.clone(),
                initialization_options: config.initialization_options.clone(),
            })
            .collect()
    }

    /// Get server name for language (for backward compatibility)
    pub fn get_server_name_for_language(&self, language: &str) -> Option<String> {
        self.languages
            .get(language)
            .map(|config| config.name.clone())
    }

    /// Default embedded configuration
    pub fn default_config() -> Self {
        let json = r#"{
            "languages": {
                "typescript": {
                    "name": "typescript-language-server",
                    "command": "typescript-language-server",
                    "args": ["--stdio"],
                    "file_extensions": ["ts", "js", "tsx", "jsx"],
                    "project_patterns": ["package.json", "tsconfig.json"],
                    "exclude_patterns": ["**/node_modules/**", "**/dist/**"],
                    "initialization_options": {
                        "preferences": {
                            "disableSuggestions": false
                        }
                    }
                },
                "rust": {
                    "name": "rust-analyzer",
                    "command": "rust-analyzer",
                    "args": [],
                    "file_extensions": ["rs"],
                    "project_patterns": ["Cargo.toml"],
                    "exclude_patterns": ["**/target/**"],
                    "initialization_options": {
                        "cargo": {
                            "buildScripts": {
                                "enable": true
                            }
                        }
                    }
                },
                "python": {
                    "name": "pylsp",
                    "command": "pylsp",
                    "args": [],
                    "file_extensions": ["py"],
                    "project_patterns": ["pyproject.toml", "setup.py"],
                    "exclude_patterns": ["**/__pycache__/**", "**/venv/**"],
                    "initialization_options": {}
                }
            }
        }"#;

        serde_json::from_str(json).expect("Invalid default configuration")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = LanguagesConfig::default_config();
        assert!(!config.languages.is_empty());
        assert!(config.languages.contains_key("typescript"));
        assert!(config.languages.contains_key("rust"));
        assert!(config.languages.contains_key("python"));
    }

    #[test]
    fn test_get_project_patterns_for_language() {
        let config = LanguagesConfig::default_config();
        let patterns = config.get_project_patterns_for_language("typescript");
        assert!(patterns.contains(&"package.json".to_string()));
        
        let empty = config.get_project_patterns_for_language("unknown");
        assert!(empty.is_empty());
    }

    #[test]
    fn test_get_language_for_extension() {
        let config = LanguagesConfig::default_config();
        assert_eq!(config.get_language_for_extension("ts"), Some("typescript".to_string()));
        assert_eq!(config.get_language_for_extension("rs"), Some("rust".to_string()));
        assert_eq!(config.get_language_for_extension("py"), Some("python".to_string()));
        assert_eq!(config.get_language_for_extension("unknown"), None);
    }

    #[test]
    fn test_get_config_by_language() {
        let config = LanguagesConfig::default_config();
        let ts_config = config.get_config_by_language("typescript");
        assert!(ts_config.is_ok());
        
        let invalid = config.get_config_by_language("nonexistent");
        assert!(invalid.is_err());
    }

    #[test]
    fn test_all_configs() {
        let config = LanguagesConfig::default_config();
        let configs = config.all_configs();
        assert_eq!(configs.len(), 3); // typescript, rust, python
    }

    #[test]
    fn test_load_missing_config_file() {
        // Test fallback when config file doesn't exist
        unsafe {
            std::env::set_var("CONFIG_PATH", "/nonexistent/path/config.json");
        }
        let result = LanguagesConfig::load();
        unsafe {
            std::env::remove_var("CONFIG_PATH");
        }
        
        // Should succeed with default config
        assert!(result.is_ok());
        let config = result.unwrap();
        assert!(!config.languages.is_empty());
    }
}
