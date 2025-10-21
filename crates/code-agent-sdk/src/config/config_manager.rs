use super::json_config::LanguagesConfig;
use crate::model::types::LanguageServerConfig;
use std::sync::OnceLock;

static LANGUAGES_CONFIG: OnceLock<LanguagesConfig> = OnceLock::new();

pub struct ConfigManager;

impl ConfigManager {
    /// Get the global languages configuration
    fn get_languages_config() -> &'static LanguagesConfig {
        LANGUAGES_CONFIG.get_or_init(|| {
            LanguagesConfig::load().unwrap_or_else(|e| {
                eprintln!("Failed to load languages config: {}, using defaults", e);
                LanguagesConfig::default_config()
            })
        })
    }

    /// Get project patterns for a specific language
    pub fn get_project_patterns_for_language(language: &str) -> Vec<String> {
        Self::get_languages_config().get_project_patterns_for_language(language)
    }

    /// Get language for file extension
    pub fn get_language_for_extension(extension: &str) -> Option<String> {
        Self::get_languages_config().get_language_for_extension(extension)
    }

    /// Get all language server configurations
    pub fn all_configs() -> Vec<LanguageServerConfig> {
        Self::get_languages_config().all_configs()
    }

    /// Get language server config by language name
    pub fn get_config_by_language(language: &str) -> Result<LanguageServerConfig, String> {
        Self::get_languages_config().get_config_by_language(language)
    }

    /// Get server name for language (for workspace manager mapping)
    pub fn get_server_name_for_language(language: &str) -> Option<String> {
        Self::get_languages_config().get_server_name_for_language(language)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_project_patterns_for_language() {
        let patterns = ConfigManager::get_project_patterns_for_language("typescript");
        assert!(!patterns.is_empty());
    }

    #[test]
    fn test_get_language_for_extension() {
        assert_eq!(ConfigManager::get_language_for_extension("ts"), Some("typescript".to_string()));
        assert_eq!(ConfigManager::get_language_for_extension("rs"), Some("rust".to_string()));
        assert_eq!(ConfigManager::get_language_for_extension("unknown"), None);
    }

    #[test]
    fn test_all_configs() {
        let configs = ConfigManager::all_configs();
        assert!(!configs.is_empty());
    }

    #[test]
    fn test_get_config_by_language() {
        let config = ConfigManager::get_config_by_language("typescript");
        assert!(config.is_ok());
        
        let invalid = ConfigManager::get_config_by_language("nonexistent");
        assert!(invalid.is_err());
    }

    #[test]
    fn test_get_server_name_for_language() {
        assert!(ConfigManager::get_server_name_for_language("typescript").is_some());
        assert_eq!(ConfigManager::get_server_name_for_language("unknown"), None);
    }

    #[test]
    fn test_config_fallback_on_error() {
        // Verify default config works (covers error fallback path)
        let default_config = crate::config::json_config::LanguagesConfig::default_config();
        assert!(!default_config.languages.is_empty());
    }
}
