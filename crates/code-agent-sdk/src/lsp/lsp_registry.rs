use crate::lsp::LspClient;
use crate::model::types::LanguageServerConfig;
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

/// Registry for managing LSP client instances
#[derive(Default)]
pub struct LspRegistry {
    clients: HashMap<String, LspClient>,
    configs: HashMap<String, LanguageServerConfig>,
}

impl std::fmt::Debug for LspRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LspRegistry")
            .field("client_count", &self.clients.len())
            .field("config_count", &self.configs.len())
            .finish()
    }
}

impl LspRegistry {
    /// Create new LSP registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a language server configuration
    pub fn register_config(&mut self, config: LanguageServerConfig) {
        self.configs.insert(config.name.clone(), config);
    }

    /// Get or create LSP client for a language server
    pub async fn get_client(
        &mut self,
        server_name: &str,
        _workspace_root: &Path,
    ) -> Result<&mut LspClient> {
        if !self.clients.contains_key(server_name) {
            let config = self.configs.get(server_name).ok_or_else(|| {
                anyhow::anyhow!("Language server '{}' not registered", server_name)
            })?;

            let client = LspClient::new(config.clone()).await?;
            self.clients.insert(server_name.to_string(), client);
        }

        Ok(self.clients.get_mut(server_name).unwrap())
    }

    /// Get client for file extension
    pub async fn get_client_for_extension(
        &mut self,
        extension: &str,
        workspace_root: &Path,
    ) -> Result<Option<&mut LspClient>> {
        let server_name = self
            .configs
            .iter()
            .find(|(_, config)| config.file_extensions.contains(&extension.to_string()))
            .map(|(name, _)| name.clone());

        if let Some(name) = server_name {
            return Ok(Some(self.get_client(&name, workspace_root).await?));
        }
        Ok(None)
    }

    /// Check if language server is available
    pub fn is_available(&self, server_name: &str) -> bool {
        self.configs.contains_key(server_name)
    }

    /// Get all registered language server names
    pub fn registered_servers(&self) -> Vec<&String> {
        self.configs.keys().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::types::LanguageServerConfig;

    fn create_test_config(name: &str, extensions: Vec<&str>) -> LanguageServerConfig {
        LanguageServerConfig {
            name: name.to_string(),
            command: format!("{}-lsp", name),
            args: vec!["--stdio".to_string()],
            file_extensions: extensions.iter().map(|s| s.to_string()).collect(),
            exclude_patterns: vec!["**/test/**".to_string()],
            initialization_options: None,
        }
    }

    #[test]
    fn test_new_registry() {
        let registry = LspRegistry::new();
        assert_eq!(registry.registered_servers().len(), 0);
    }

    #[test]
    fn test_register_config() {
        let mut registry = LspRegistry::new();
        let config = create_test_config("rust-analyzer", vec!["rs"]);
        
        registry.register_config(config);
        
        assert_eq!(registry.registered_servers().len(), 1);
        assert!(registry.is_available("rust-analyzer"));
    }

    #[test]
    fn test_register_multiple_configs() {
        let mut registry = LspRegistry::new();
        let rust_config = create_test_config("rust-analyzer", vec!["rs"]);
        let ts_config = create_test_config("typescript-language-server", vec!["ts", "js"]);
        
        registry.register_config(rust_config);
        registry.register_config(ts_config);
        
        assert_eq!(registry.registered_servers().len(), 2);
        assert!(registry.is_available("rust-analyzer"));
        assert!(registry.is_available("typescript-language-server"));
    }

    #[test]
    fn test_is_available_false_for_unregistered() {
        let registry = LspRegistry::new();
        assert!(!registry.is_available("nonexistent-server"));
    }

    #[test]
    fn test_registered_servers_returns_correct_names() {
        let mut registry = LspRegistry::new();
        let config1 = create_test_config("server1", vec!["ext1"]);
        let config2 = create_test_config("server2", vec!["ext2"]);
        
        registry.register_config(config1);
        registry.register_config(config2);
        
        let servers = registry.registered_servers();
        assert_eq!(servers.len(), 2);
        assert!(servers.contains(&&"server1".to_string()));
        assert!(servers.contains(&&"server2".to_string()));
    }

    #[test]
    fn test_register_config_overwrites_existing() {
        let mut registry = LspRegistry::new();
        let config1 = create_test_config("rust-analyzer", vec!["rs"]);
        let mut config2 = create_test_config("rust-analyzer", vec!["rs", "toml"]);
        config2.command = "new-rust-analyzer".to_string();
        
        registry.register_config(config1);
        registry.register_config(config2);
        
        assert_eq!(registry.registered_servers().len(), 1);
        assert!(registry.is_available("rust-analyzer"));
    }
}
