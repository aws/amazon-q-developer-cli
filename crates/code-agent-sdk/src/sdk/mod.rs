//! # Code Intelligence SDK
//!
//! This module contains the main SDK client for performing code intelligence operations.
//! The SDK provides a high-level interface for interacting with language servers and
//! performing code analysis tasks.

pub mod client;
pub mod services;
pub mod workspace_manager;

use crate::config::ConfigManager;
use crate::sdk::client::CodeIntelligence;
use std::path::PathBuf;
pub use workspace_manager::WorkspaceManager;

/// **Builder for configuring CodeIntelligence instances**
///
/// Provides a fluent interface for setting up code intelligence clients with
/// specific language support, workspace configuration, and initialization options.
///
/// # Examples
/// ```no_run
/// use code_agent_sdk::CodeIntelligence;
/// use std::path::PathBuf;
///
/// # async fn example() {
/// // Build with specific language
/// let client = CodeIntelligence::builder()
///     .workspace_root(PathBuf::from("/path/to/project"))
///     .add_language("typescript")
///     .add_language("rust")
///     .build().expect("Failed to build");
///
/// // Build with auto-detection
/// let client = CodeIntelligence::builder()
///     .workspace_root(PathBuf::from("."))
///     .auto_detect_languages()
///     .build().expect("Failed to build");
/// 
/// # }
/// ```ignore
pub struct CodeIntelligenceBuilder {
    workspace_root: Option<PathBuf>,
    languages: Vec<String>,
    auto_detect: bool,
}

impl CodeIntelligenceBuilder {
    /// Create a new builder instance
    pub fn new() -> Self {
        Self {
            workspace_root: None,
            languages: Vec::new(),
            auto_detect: false,
        }
    }

    /// Set the workspace root directory
    pub fn workspace_root(mut self, root: PathBuf) -> Self {
        self.workspace_root = Some(root);
        self
    }

    /// Add support for a specific programming language
    pub fn add_language(mut self, language: &str) -> Self {
        self.languages.push(language.to_string());
        self
    }

    /// Enable automatic language detection based on workspace files
    pub fn auto_detect_languages(mut self) -> Self {
        self.auto_detect = true;
        self
    }

    /// Build the CodeIntelligence instance
    pub fn build(self) -> anyhow::Result<CodeIntelligence, String> {
        let workspace_root = self
            .workspace_root
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let mut client = CodeIntelligence::new(workspace_root);

        if self.auto_detect {
            let workspace_info = client
                .detect_workspace()
                .map_err(|e| format!("Failed to detect workspace: {}", e))?;

            for language in workspace_info.detected_languages {
                if let Ok(config) = ConfigManager::get_config_by_language(&language) {
                    client.add_language_server(config);
                }
            }
        } else {
            for language in self.languages {
                let config = ConfigManager::get_config_by_language(&language)?;
                client.add_language_server(config);
            }
        }
        Ok(client)
    }
}

impl Default for CodeIntelligenceBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_builder_new() {
        let builder = CodeIntelligenceBuilder::new();
        assert!(builder.workspace_root.is_none());
        assert_eq!(builder.languages.len(), 0);
        assert!(!builder.auto_detect);
    }

    #[test]
    fn test_builder_default() {
        let builder = CodeIntelligenceBuilder::default();
        assert!(builder.workspace_root.is_none());
        assert_eq!(builder.languages.len(), 0);
        assert!(!builder.auto_detect);
    }

    #[test]
    fn test_builder_workspace_root() {
        let root = PathBuf::from("/test/workspace");
        let builder = CodeIntelligenceBuilder::new().workspace_root(root.clone());
        assert_eq!(builder.workspace_root, Some(root));
    }

    #[test]
    fn test_builder_add_language() {
        let builder = CodeIntelligenceBuilder::new()
            .add_language("rust")
            .add_language("typescript");
        assert_eq!(builder.languages, vec!["rust", "typescript"]);
    }

    #[test]
    fn test_builder_add_multiple_languages() {
        let builder = CodeIntelligenceBuilder::new()
            .add_language("rust")
            .add_language("python")
            .add_language("javascript");
        assert_eq!(builder.languages, vec!["rust", "python", "javascript"]);
    }

    #[test]
    fn test_builder_auto_detect_languages() {
        let builder = CodeIntelligenceBuilder::new().auto_detect_languages();
        assert!(builder.auto_detect);
    }

    #[test]
    fn test_builder_chaining() {
        let root = PathBuf::from("/project");
        let builder = CodeIntelligenceBuilder::new()
            .workspace_root(root.clone())
            .add_language("rust")
            .add_language("typescript")
            .auto_detect_languages();
        
        assert_eq!(builder.workspace_root, Some(root));
        assert_eq!(builder.languages, vec!["rust", "typescript"]);
        assert!(builder.auto_detect);
    }

    #[test]
    fn test_builder_build_with_invalid_language() {
        let builder = CodeIntelligenceBuilder::new()
            .add_language("nonexistent_language");
        
        let result = builder.build();
        assert!(result.is_err());
    }
}
