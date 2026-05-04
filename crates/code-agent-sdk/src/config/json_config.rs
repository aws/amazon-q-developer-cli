use anyhow::{
    Context as _,
    Result,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;

use crate::model::types::LanguageServerConfig;
use crate::utils::glob_matching::{
    combine_patterns,
    resolve_language_matches,
};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LanguageConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub file_extensions: Vec<String>,
    #[serde(default)]
    pub file_patterns: Vec<String>,
    pub project_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
    #[serde(default)]
    pub multi_workspace: bool,
    pub initialization_options: Option<Value>,
    #[serde(default = "default_timeout")]
    pub request_timeout_secs: Option<u64>,
}

impl LanguageConfig {
    /// All file matching patterns: file_patterns + file_extensions as *.ext globs
    pub fn all_patterns(&self) -> Vec<String> {
        combine_patterns(&self.file_patterns, &self.file_extensions)
    }
}

fn default_timeout() -> Option<u64> {
    Some(60)
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LanguagesConfig {
    pub languages: indexmap::IndexMap<String, LanguageConfig>,
}

impl LanguagesConfig {
    /// Get or create configuration in config root folder
    pub fn get_or_create(config_root: &std::path::Path) -> Result<Self> {
        let config_path = config_root.join("lsp.json");

        // Create config directory if it doesn't exist
        if !config_root.exists() {
            std::fs::create_dir_all(config_root)?;
        }

        // If config file exists, load it, otherwise create default
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            Ok(serde_json::from_str(&content).with_context(|| format!("failed to parse {}", config_path.display()))?)
        } else {
            let default_config = Self::default_config();
            let config_json = serde_json::to_string_pretty(&default_config)?;
            std::fs::write(&config_path, config_json)?;
            Ok(default_config)
        }
    }

    /// Load configuration if it exists, otherwise return default without creating file
    pub fn load_if_exists(config_root: &std::path::Path) -> Result<Self> {
        let config_path = config_root.join("lsp.json");

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            Ok(serde_json::from_str(&content).with_context(|| format!("failed to parse {}", config_path.display()))?)
        } else {
            // Return default config without creating file
            Ok(Self::default_config())
        }
    }

    /// Load configuration from JSON file
    pub fn load() -> Result<Self> {
        let config_path = std::path::Path::new("config/lsp.json");

        // Try to load from file, fallback to embedded config
        if config_path.exists() {
            let content = std::fs::read_to_string(config_path)?;
            Ok(serde_json::from_str(&content).with_context(|| format!("failed to parse {}", config_path.display()))?)
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

    /// Get all configured languages
    pub fn all_languages(&self) -> Vec<String> {
        self.languages.keys().cloned().collect()
    }

    /// Get language for file extension (e.g., "rs", "ts")
    pub fn get_language_for_extension(&self, extension: &str) -> Option<String> {
        self.get_language_for_file(&format!("_.{extension}"))
    }

    /// Get language for a file by matching its filename against file_patterns (glob)
    /// and file_extensions (as *.ext globs). Exact matches take priority, then most
    /// specific glob (most literal characters), then declaration order.
    pub fn get_language_for_file(&self, filename: &str) -> Option<String> {
        self.get_all_languages_for_file(filename).into_iter().next()
    }

    /// Get all languages that match a file, ranked by specificity.
    /// Used for workspace detection (all matching LSPs should be registered)
    /// and routing (first result is the best match).
    pub fn get_all_languages_for_file(&self, filename: &str) -> Vec<String> {
        let configs = self
            .languages
            .iter()
            .map(|(lang, config)| (lang.as_str(), config.all_patterns()));
        resolve_language_matches(configs, filename)
    }

    /// Get language server config by language name
    pub fn get_config_by_language(&self, language: &str) -> Result<LanguageServerConfig, String> {
        let config = self
            .languages
            .get(language)
            .ok_or_else(|| format!("Language '{language}' not supported"))?;

        Ok(LanguageServerConfig {
            language: language.to_string(),
            name: config.name.clone(),
            command: config.command.clone(),
            args: config.args.clone(),
            file_extensions: config.file_extensions.clone(),
            file_patterns: config.file_patterns.clone(),
            project_patterns: config.project_patterns.clone(),
            exclude_patterns: config.exclude_patterns.clone(),
            multi_workspace: config.multi_workspace,
            initialization_options: config.initialization_options.clone(),
            request_timeout_secs: config.request_timeout_secs.unwrap_or(60),
        })
    }

    /// Get all language server configs
    pub fn all_configs(&self) -> Vec<LanguageServerConfig> {
        self.languages
            .iter()
            .map(|(language, config)| LanguageServerConfig {
                language: language.clone(),
                name: config.name.clone(),
                command: config.command.clone(),
                args: config.args.clone(),
                file_extensions: config.file_extensions.clone(),
                file_patterns: config.file_patterns.clone(),
                project_patterns: config.project_patterns.clone(),
                exclude_patterns: config.exclude_patterns.clone(),
                multi_workspace: config.multi_workspace,
                initialization_options: config.initialization_options.clone(),
                request_timeout_secs: config.request_timeout_secs.unwrap_or(60),
            })
            .collect()
    }

    /// Get server name for language (for backward compatibility)
    pub fn get_server_name_for_language(&self, language: &str) -> Option<String> {
        self.languages.get(language).map(|config| config.name.clone())
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
                        },
                        "diagnostics": {
                            "enable": true,
                            "enableExperimental": true
                        },
                        "workspace": {
                            "symbol": {
                                "search": {
                                    "scope": "workspace"
                                }
                            }
                        }
                    }
                },
                "python": {
                    "name": "pyright",
                    "command": "pyright-langserver",
                    "args": ["--stdio"],
                    "file_extensions": ["py"],
                    "project_patterns": ["pyproject.toml", "setup.py", "requirements.txt", "pyrightconfig.json"],
                    "exclude_patterns": ["**/__pycache__/**", "**/venv/**", "**/.venv/**", "**/.pytest_cache/**"],
                    "initialization_options": {}
                },
                "java": {
                    "name": "jdtls",
                    "command": "jdtls",
                    "args": [],
                    "file_extensions": ["java"],
                    "project_patterns": ["pom.xml", "build.gradle", "build.gradle.kts", ".project"],
                    "exclude_patterns": ["**/target/**", "**/build/**", "**/.gradle/**"],
                    "initialization_options": {
                        "settings": {
                            "java": {
                                "compile": {
                                    "nullAnalysis": {
                                        "mode": "automatic"
                                    }
                                },
                                "configuration": {
                                    "annotationProcessing": {
                                        "enabled": true
                                    }
                                }
                            }
                        }
                    }
                },
                "go": {
                    "name": "gopls",
                    "command": "gopls",
                    "args": [],
                    "file_extensions": ["go"],
                    "project_patterns": ["go.mod", "go.sum"],
                    "exclude_patterns": ["**/vendor/**"],
                    "initialization_options": {
                        "usePlaceholders": true,
                        "completeUnimported": true
                    }
                },
                "ruby": {
                    "name": "solargraph",
                    "command": "solargraph",
                    "args": ["stdio"],
                    "file_extensions": ["rb"],
                    "project_patterns": ["Gemfile", "Rakefile"],
                    "exclude_patterns": ["**/vendor/**", "**/tmp/**"],
                    "initialization_options": {}
                },
                "cpp": {
                    "name": "clangd",
                    "command": "clangd",
                    "args": ["--background-index"],
                    "file_extensions": ["cpp", "cc", "cxx", "c", "h", "hpp", "hxx"],
                    "project_patterns": ["CMakeLists.txt", "compile_commands.json", "Makefile"],
                    "exclude_patterns": ["**/build/**", "**/cmake-build-**/**"],
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
        assert!(config.languages.contains_key("java"));
        assert!(config.languages.contains_key("go"));
        assert!(config.languages.contains_key("ruby"));
        assert!(config.languages.contains_key("cpp"));
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
        assert_eq!(configs.len(), 7); // typescript, rust, python, java, go, ruby, cpp
    }

    #[test]
    fn test_get_language_for_file() {
        let mut config = LanguagesConfig {
            languages: indexmap::IndexMap::new(),
        };
        config.languages.insert("docker".to_string(), LanguageConfig {
            name: "docker-lsp".to_string(),
            command: "docker-lsp".to_string(),
            args: vec![],
            file_extensions: vec!["dockerfile".to_string()],
            file_patterns: vec![
                "Dockerfile".to_string(),
                "Dockerfile.*".to_string(),
                "docker-compose*.yml".to_string(),
            ],
            project_patterns: vec![],
            exclude_patterns: vec![],
            multi_workspace: false,
            initialization_options: None,
            request_timeout_secs: Some(60),
        });
        // Exact filename match
        assert_eq!(config.get_language_for_file("Dockerfile"), Some("docker".to_string()));
        // Glob wildcard
        assert_eq!(
            config.get_language_for_file("Dockerfile.dev"),
            Some("docker".to_string())
        );
        assert_eq!(
            config.get_language_for_file("docker-compose.override.yml"),
            Some("docker".to_string())
        );
        // Extension match via file_extensions
        assert_eq!(
            config.get_language_for_file("app.dockerfile"),
            Some("docker".to_string())
        );
        // No match
        assert_eq!(config.get_language_for_file("Makefile"), None);
        assert_eq!(config.get_language_for_file("config.yml"), None);
    }

    #[test]
    fn test_get_language_for_file_exact_match_wins_over_glob() {
        let mut config = LanguagesConfig {
            languages: indexmap::IndexMap::new(),
        };
        config.languages.insert("yaml".to_string(), LanguageConfig {
            name: "yaml-lsp".to_string(),
            command: "yaml-lsp".to_string(),
            args: vec![],
            file_extensions: vec!["yml".to_string()],
            file_patterns: vec![],
            project_patterns: vec![],
            exclude_patterns: vec![],
            multi_workspace: false,
            initialization_options: None,
            request_timeout_secs: Some(60),
        });
        config.languages.insert("docker-compose".to_string(), LanguageConfig {
            name: "docker-compose-lsp".to_string(),
            command: "docker-compose-lsp".to_string(),
            args: vec![],
            file_extensions: vec![],
            file_patterns: vec!["docker-compose.yml".to_string()],
            project_patterns: vec![],
            exclude_patterns: vec![],
            multi_workspace: false,
            initialization_options: None,
            request_timeout_secs: Some(60),
        });
        // Exact pattern "docker-compose.yml" wins over glob "*.yml"
        assert_eq!(
            config.get_language_for_file("docker-compose.yml"),
            Some("docker-compose".to_string())
        );
        // Regular yml files still match yaml
        assert_eq!(config.get_language_for_file("config.yml"), Some("yaml".to_string()));
    }

    #[test]
    fn test_get_language_for_file_more_specific_glob_wins() {
        let mut config = LanguagesConfig {
            languages: indexmap::IndexMap::new(),
        };
        // Broader glob declared FIRST
        config.languages.insert("yaml".to_string(), LanguageConfig {
            name: "yaml-lsp".to_string(),
            command: "yaml-lsp".to_string(),
            args: vec![],
            file_extensions: vec![],
            file_patterns: vec!["*.yml".to_string()],
            project_patterns: vec![],
            exclude_patterns: vec![],
            multi_workspace: false,
            initialization_options: None,
            request_timeout_secs: Some(60),
        });
        // More specific glob declared SECOND — should still win
        config.languages.insert("docker-compose".to_string(), LanguageConfig {
            name: "dc-lsp".to_string(),
            command: "dc-lsp".to_string(),
            args: vec![],
            file_extensions: vec![],
            file_patterns: vec!["docker-compose*.yml".to_string()],
            project_patterns: vec![],
            exclude_patterns: vec![],
            multi_workspace: false,
            initialization_options: None,
            request_timeout_secs: Some(60),
        });
        // More specific glob wins despite being declared second
        assert_eq!(
            config.get_language_for_file("docker-compose.yml"),
            Some("docker-compose".to_string())
        );
        // Broader glob still works for non-matching files
        assert_eq!(config.get_language_for_file("config.yml"), Some("yaml".to_string()));
    }

    #[test]
    fn test_get_all_languages_for_file_returns_all_matches() {
        let mut config = LanguagesConfig {
            languages: indexmap::IndexMap::new(),
        };
        config.languages.insert("broad".to_string(), LanguageConfig {
            name: "broad-lsp".to_string(),
            command: "broad-lsp".to_string(),
            args: vec![],
            file_extensions: vec![],
            file_patterns: vec!["C*g".to_string()],
            project_patterns: vec![],
            exclude_patterns: vec![],
            multi_workspace: false,
            initialization_options: None,
            request_timeout_secs: Some(60),
        });
        config.languages.insert("specific".to_string(), LanguageConfig {
            name: "specific-lsp".to_string(),
            command: "specific-lsp".to_string(),
            args: vec![],
            file_extensions: vec![],
            file_patterns: vec!["Con*g".to_string()],
            project_patterns: vec![],
            exclude_patterns: vec![],
            multi_workspace: false,
            initialization_options: None,
            request_timeout_secs: Some(60),
        });
        // Both languages match "Config"
        let languages = config.get_all_languages_for_file("Config");
        assert_eq!(languages.len(), 2);
        assert!(languages.contains(&"broad".to_string()));
        assert!(languages.contains(&"specific".to_string()));

        // Only broad matches "Cog"
        let languages = config.get_all_languages_for_file("Cog");
        assert_eq!(languages, vec!["broad".to_string()]);

        // Neither matches "Other"
        let languages = config.get_all_languages_for_file("Other");
        assert!(languages.is_empty());
    }

    #[test]
    fn test_get_language_for_file_picks_most_specific() {
        let mut config = LanguagesConfig {
            languages: indexmap::IndexMap::new(),
        };
        config.languages.insert("broad".to_string(), LanguageConfig {
            name: "broad-lsp".to_string(),
            command: "broad-lsp".to_string(),
            args: vec![],
            file_extensions: vec![],
            file_patterns: vec!["C*g".to_string()],
            project_patterns: vec![],
            exclude_patterns: vec![],
            multi_workspace: false,
            initialization_options: None,
            request_timeout_secs: Some(60),
        });
        config.languages.insert("specific".to_string(), LanguageConfig {
            name: "specific-lsp".to_string(),
            command: "specific-lsp".to_string(),
            args: vec![],
            file_extensions: vec![],
            file_patterns: vec!["Con*g".to_string()],
            project_patterns: vec![],
            exclude_patterns: vec![],
            multi_workspace: false,
            initialization_options: None,
            request_timeout_secs: Some(60),
        });
        // get_language_for_file picks the most specific
        assert_eq!(config.get_language_for_file("Config"), Some("specific".to_string()));
        // get_all_languages_for_file returns both for detection
        assert_eq!(config.get_all_languages_for_file("Config").len(), 2);
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
