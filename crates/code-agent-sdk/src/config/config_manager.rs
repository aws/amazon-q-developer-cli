use super::json_config::LanguagesConfig;
use crate::model::types::LanguageServerConfig;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const CONFIG_TTL: Duration = Duration::from_secs(60); // 1 minute TTL

#[derive(Debug)]
struct CachedConfig {
    config: LanguagesConfig,
    loaded_at: Instant,
}

#[derive(Debug)]
pub struct ConfigManager {
    config_root: PathBuf,
    cached_config: Arc<Mutex<Option<CachedConfig>>>,
}

impl ConfigManager {
    /// Create a new ConfigManager with the specified config root
    pub fn new(config_root: PathBuf) -> Self {
        Self {
            config_root,
            cached_config: Arc::new(Mutex::new(None)),
        }
    }

    /// Get the languages configuration (with TTL caching)
    pub fn get_config(&self) -> anyhow::Result<LanguagesConfig> {
        let mut cache = self.cached_config.lock().unwrap();
        
        // Check if we need to reload
        let needs_reload = cache.as_ref()
            .map(|c| c.loaded_at.elapsed() > CONFIG_TTL)
            .unwrap_or(true);

        if needs_reload {
            let config = LanguagesConfig::get_or_create(&self.config_root)?;
            *cache = Some(CachedConfig {
                config: config.clone(),
                loaded_at: Instant::now(),
            });
            Ok(config)
        } else {
            Ok(cache.as_ref().unwrap().config.clone())
        }
    }

    /// Get project patterns for a specific language
    pub fn get_project_patterns_for_language(&self, language: &str) -> Vec<String> {
        self.get_config()
            .map(|c| c.get_project_patterns_for_language(language))
            .unwrap_or_default()
    }

    /// Get language for file extension
    pub fn get_language_for_extension(&self, extension: &str) -> Option<String> {
        self.get_config()
            .ok()
            .and_then(|c| c.get_language_for_extension(extension))
    }

    /// Get all language server configurations
    pub fn all_configs(&self) -> Vec<LanguageServerConfig> {
        self.get_config()
            .map(|c| c.all_configs())
            .unwrap_or_default()
    }

    /// Get configuration for a specific language
    pub fn get_config_by_language(&self, language: &str) -> Result<LanguageServerConfig, String> {
        self.get_config()
            .map_err(|e| e.to_string())
            .and_then(|c| c.get_config_by_language(language))
    }

    /// Get server name for a language
    pub fn get_server_name_for_language(&self, language: &str) -> Option<String> {
        self.get_config()
            .ok()
            .and_then(|c| c.get_server_name_for_language(language))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_config_manager_new() {
        let temp_dir = TempDir::new().unwrap();
        let config_manager = ConfigManager::new(temp_dir.path().to_path_buf());
        assert_eq!(config_manager.config_root, temp_dir.path());
    }

    #[test]
    fn test_get_project_patterns_for_language() {
        let temp_dir = TempDir::new().unwrap();
        let config_manager = ConfigManager::new(temp_dir.path().to_path_buf());
        let patterns = config_manager.get_project_patterns_for_language("typescript");
        assert!(patterns.contains(&"package.json".to_string()));
    }

    #[test]
    fn test_get_language_for_extension() {
        let temp_dir = TempDir::new().unwrap();
        let config_manager = ConfigManager::new(temp_dir.path().to_path_buf());
        assert_eq!(config_manager.get_language_for_extension("ts"), Some("typescript".to_string()));
        assert_eq!(config_manager.get_language_for_extension("rs"), Some("rust".to_string()));
        assert_eq!(config_manager.get_language_for_extension("unknown"), None);
    }

    #[test]
    fn test_all_configs() {
        let temp_dir = TempDir::new().unwrap();
        let config_manager = ConfigManager::new(temp_dir.path().to_path_buf());
        let configs = config_manager.all_configs();
        assert_eq!(configs.len(), 3); // typescript, rust, python
    }

    #[test]
    fn test_get_config_by_language() {
        let temp_dir = TempDir::new().unwrap();
        let config_manager = ConfigManager::new(temp_dir.path().to_path_buf());
        let config = config_manager.get_config_by_language("typescript");
        assert!(config.is_ok());
        
        let invalid = config_manager.get_config_by_language("nonexistent");
        assert!(invalid.is_err());
    }

    #[test]
    fn test_get_server_name_for_language() {
        let temp_dir = TempDir::new().unwrap();
        let config_manager = ConfigManager::new(temp_dir.path().to_path_buf());
        assert!(config_manager.get_server_name_for_language("typescript").is_some());
        assert_eq!(config_manager.get_server_name_for_language("unknown"), None);
    }
}
