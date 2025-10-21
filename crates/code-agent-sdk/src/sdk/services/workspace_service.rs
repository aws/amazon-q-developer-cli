use crate::config::ConfigManager;
use crate::sdk::workspace_manager::WorkspaceManager;
use anyhow::Result;
use lsp_types::*;
use std::path::Path;
use url::Url;

/// Service for shared workspace and file operations
#[async_trait::async_trait]
pub trait WorkspaceService: Send + Sync {
    /// Open a file in the appropriate language server
    async fn open_file(
        &self,
        workspace_manager: &mut WorkspaceManager,
        file_path: &Path,
        content: String,
    ) -> Result<()>;
}

/// Implementation of WorkspaceService using WorkspaceManager
pub struct LspWorkspaceService;

impl Default for LspWorkspaceService {
    fn default() -> Self {
        Self::new()
    }
}

impl LspWorkspaceService {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl WorkspaceService for LspWorkspaceService {
    async fn open_file(
        &self,
        workspace_manager: &mut WorkspaceManager,
        file_path: &Path,
        content: String,
    ) -> Result<()> {
        // Ensure initialized
        if !workspace_manager.is_initialized() {
            workspace_manager.initialize().await?;
        }

        // Check if file is already opened
        if workspace_manager.is_file_opened(file_path) {
            return Ok(()); // File already opened, no need to wait
        }

        let client = workspace_manager
            .get_client_for_file(file_path)
            .await?
            .ok_or_else(|| anyhow::anyhow!("No language server for file"))?;

        let uri =
            Url::from_file_path(file_path).map_err(|_| anyhow::anyhow!("Invalid file path"))?;

        // Determine language ID from file extension using ConfigManager
        let language_id = if let Some(ext) = file_path.extension().and_then(|ext| ext.to_str()) {
            ConfigManager::get_language_for_extension(ext)
                .unwrap_or_else(|| "plaintext".to_string())
        } else {
            "plaintext".to_string()
        };

        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri,
                language_id,
                version: 1,
                text: content,
            },
        };

        client.did_open(params).await?;

        // Mark file as opened
        workspace_manager.mark_file_opened(file_path.to_path_buf());
        //tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

        Ok(())
    }
}
