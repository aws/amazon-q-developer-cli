use anyhow::Result;
use lsp_types::*;
use url::Url;

use super::workspace_service::WorkspaceService;
use crate::model::types::*;
use crate::sdk::workspace_manager::WorkspaceManager;
use crate::utils::file::canonicalize_path;

/// Service for code manipulation operations
#[async_trait::async_trait]
pub trait CodingService: Send + Sync {
    /// Rename symbol at specific location
    async fn rename_symbol(
        &self,
        workspace_manager: &mut WorkspaceManager,
        request: RenameSymbolRequest,
    ) -> Result<Option<WorkspaceEdit>>;

    /// Format code in a file or workspace
    async fn format_code(
        &self,
        workspace_manager: &mut WorkspaceManager,
        request: FormatCodeRequest,
    ) -> Result<Vec<TextEdit>>;
}

/// LSP-based implementation of CodingService
pub struct LspCodingService {
    workspace_service: Box<dyn WorkspaceService>,
}

impl LspCodingService {
    pub fn new(workspace_service: Box<dyn WorkspaceService>) -> Self {
        Self { workspace_service }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk::services::workspace_service::LspWorkspaceService;

    #[test]
    fn test_new() {
        let workspace_service = Box::new(LspWorkspaceService::new());
        let coding_service = LspCodingService::new(workspace_service);
        // Just verify it constructs successfully
        assert!(std::ptr::addr_of!(coding_service.workspace_service) as *const _ != std::ptr::null());
    }
}

#[async_trait::async_trait]
impl CodingService for LspCodingService {
    async fn rename_symbol(
        &self,
        workspace_manager: &mut WorkspaceManager,
        request: RenameSymbolRequest,
    ) -> Result<Option<WorkspaceEdit>> {
        // Ensure initialized
        if !workspace_manager.is_initialized() {
            workspace_manager.initialize().await?;
        }

        let canonical_path = canonicalize_path(&request.file_path)?;
        let content = std::fs::read_to_string(&canonical_path)?;
        self.workspace_service
            .open_file(workspace_manager, &canonical_path, content)
            .await?;

        let client = workspace_manager
            .get_client_for_file(&canonical_path)
            .await?
            .ok_or_else(|| anyhow::anyhow!("No language server for file"))?;

        let uri = Url::from_file_path(&canonical_path)
            .map_err(|_| anyhow::anyhow!("Invalid file path"))?;

        let params = RenameParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: crate::utils::to_lsp_position(request.row, request.column),
            },
            new_name: request.new_name.clone(),
            work_done_progress_params: Default::default(),
        };

        let workspace_edit = client.rename(params).await?;

        // Apply workspace edit using LSP batch operations if not dry-run
        if let Some(ref edit) = workspace_edit {
            if !request.dry_run {
                client.apply_workspace_edit(edit).await?;
            }
        }

        Ok(workspace_edit)
    }

    async fn format_code(
        &self,
        workspace_manager: &mut WorkspaceManager,
        request: FormatCodeRequest,
    ) -> Result<Vec<TextEdit>> {
        // Ensure initialized
        if !workspace_manager.is_initialized() {
            workspace_manager.initialize().await?;
        }

        if let Some(file_path) = &request.file_path {
            // Format specific file
            let canonical_path = canonicalize_path(file_path)?;
            let content = std::fs::read_to_string(&canonical_path)?;
            self.workspace_service
                .open_file(workspace_manager, &canonical_path, content)
                .await?;

            let client = workspace_manager
                .get_client_for_file(&canonical_path)
                .await?
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "No language server available for file: {}",
                        canonical_path.display()
                    )
                })?;

            let uri = Url::from_file_path(&canonical_path).map_err(|_| {
                anyhow::anyhow!("Invalid file path: {}", canonical_path.display())
            })?;

            let params = DocumentFormattingParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                options: FormattingOptions {
                    tab_size: request.tab_size,
                    insert_spaces: request.insert_spaces,
                    properties: Default::default(),
                    trim_trailing_whitespace: Some(true),
                    insert_final_newline: Some(true),
                    trim_final_newlines: Some(true),
                },
                work_done_progress_params: Default::default(),
            };

            let edits = client
                .format_document(params)
                .await?
                .unwrap_or_default();

            // Apply formatting edits using LSP batch operations
            if !edits.is_empty() {
                let mut changes = std::collections::HashMap::new();
                changes.insert(uri, edits.clone());
                
                let workspace_edit = WorkspaceEdit {
                    changes: Some(changes),
                    document_changes: None,
                    change_annotations: None,
                };

                client.apply_workspace_edit(&workspace_edit).await?;
            }

            Ok(edits)
        } else {
            // Format workspace - not commonly supported by LSPs
            // Return empty edits for now
            Ok(Vec::new())
        }
    }
}
