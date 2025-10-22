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
    ) -> Result<usize>;
}

/// LSP-based implementation of CodingService
pub struct LspCodingService {
    workspace_service: Box<dyn WorkspaceService>,
}

impl LspCodingService {
    pub fn new(workspace_service: Box<dyn WorkspaceService>) -> Self {
        Self { workspace_service }
    }

    /// Send proper LSP notifications for file changes
    async fn notify_file_changes(
        &self,
        workspace_manager: &mut WorkspaceManager,
        workspace_edit: &WorkspaceEdit,
    ) -> Result<()> {
        use lsp_types::{DidChangeTextDocumentParams, VersionedTextDocumentIdentifier, TextDocumentContentChangeEvent};
        use std::collections::HashSet;

        let mut changed_files = HashSet::new();

        // Collect unique files from changes field
        if let Some(changes) = &workspace_edit.changes {
            for uri in changes.keys() {
                if let Ok(file_path) = uri.to_file_path() {
                    // Only notify for files that are actually opened in LSP
                    if workspace_manager.is_file_opened(&file_path) {
                        changed_files.insert((uri.clone(), file_path));
                    }
                }
            }
        }

        // Collect unique files from document_changes field
        if let Some(document_changes) = &workspace_edit.document_changes {
            if let lsp_types::DocumentChanges::Edits(edits) = document_changes {
                for edit in edits {
                    if let Ok(file_path) = edit.text_document.uri.to_file_path() {
                        // Only notify for files that are actually opened in LSP
                        if workspace_manager.is_file_opened(&file_path) {
                            changed_files.insert((edit.text_document.uri.clone(), file_path));
                        }
                    }
                }
            }
        }

        // Send didChange with full content for each opened file
        for (uri, file_path) in changed_files {
            // Get next version number first
            let version = workspace_manager.get_next_version(&file_path);
            
            if let Ok(Some(client)) = workspace_manager.get_client_for_file(&file_path).await {
                // Read current file content
                if let Ok(content) = std::fs::read_to_string(&file_path) {
                    let params = DidChangeTextDocumentParams {
                        text_document: VersionedTextDocumentIdentifier {
                            uri: uri.clone(),
                            version,
                        },
                        content_changes: vec![TextDocumentContentChangeEvent {
                            range: None, // Full document update (safer)
                            range_length: None,
                            text: content,
                        }],
                    };
                    
                    tracing::trace!("Sending didChange for opened file: {:?}, version: {}", file_path, version);
                    let _ = client.did_change(params).await;
                }
            }
        }

        Ok(())
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
        tracing::trace!("Starting rename_symbol: file={:?}, row={}, col={}, new_name={}", 
            request.file_path, request.row, request.column, request.new_name);

        // Ensure initialized
        if !workspace_manager.is_initialized() {
            tracing::trace!("Workspace not initialized, initializing...");
            workspace_manager.initialize().await?;
        }

        let canonical_path = canonicalize_path(&request.file_path)?;
        tracing::trace!("Canonical path: {:?}", canonical_path);
        
        let content = std::fs::read_to_string(&canonical_path)?;
        tracing::trace!("File content length: {} bytes", content.len());
        
        self.workspace_service
            .open_file(workspace_manager, &canonical_path, content)
            .await?;
        tracing::trace!("File opened in workspace");

        let client = workspace_manager
            .get_client_for_file(&canonical_path)
            .await?
            .ok_or_else(|| anyhow::anyhow!("No language server for file"))?;
        tracing::trace!("Got LSP client for file");

        let uri = Url::from_file_path(&canonical_path)
            .map_err(|_| anyhow::anyhow!("Invalid file path"))?;
        tracing::trace!("File URI: {}", uri);

        let params = RenameParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: crate::utils::position::to_lsp_position(request.row, request.column),
            },
            new_name: request.new_name.clone(),
            work_done_progress_params: Default::default(),
        };
        tracing::trace!("Sending rename request to LSP: {:?}", params);

        let result = client.rename(params).await;
        tracing::trace!("LSP rename result: {:?}", result);

        // Apply edits if not dry_run
        if !request.dry_run {
            if let Ok(Some(ref workspace_edit)) = result {
                tracing::trace!("Applying workspace edit (not dry-run)");
                use crate::utils::apply_workspace_edit;
                if let Err(e) = apply_workspace_edit(workspace_edit) {
                    tracing::trace!("Failed to apply workspace edit: {}", e);
                } else {
                    // Send workspace change notifications. 
                    // TODO: This needs to be improved to have a propper fileWatcher for the repository.
                    self.notify_file_changes(workspace_manager, workspace_edit).await?;
                }
            }
        } else {
            tracing::trace!("Dry-run mode, not applying edits");
        }

        result
    }

    async fn format_code(
        &self,
        workspace_manager: &mut WorkspaceManager,
        request: FormatCodeRequest,
    ) -> Result<usize> {
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

            let params = DocumentFormattingParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::from_file_path(&canonical_path).map_err(|_| {
                        anyhow::anyhow!("Invalid file path: {}", canonical_path.display())
                    })?,
                },
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

            let edit_count = edits.len();

            // Apply formatting edits to the actual file
            if !edits.is_empty() {
                use crate::utils::apply_text_edits;
                apply_text_edits(&canonical_path, &edits)?;
            }

            Ok(edit_count)
        } else {
            // Format workspace - not commonly supported by LSPs
            Ok(0)
        }
    }
}
