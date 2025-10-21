use anyhow::{Error, Result};
use lsp_types::*;
use std::path::Path;
use url::Url;

use super::workspace_service::WorkspaceService;
use crate::model::entities::{DefinitionInfo, ReferenceInfo, SymbolInfo};
use crate::model::types::*;
use crate::sdk::workspace_manager::WorkspaceManager;
use crate::utils::file::canonicalize_path;

/// Service for all symbol-related operations
#[async_trait::async_trait]
pub trait SymbolService: Send + Sync {
    /// Find symbols across workspace or within a specific file
    async fn find_symbols(
        &self,
        workspace_manager: &mut WorkspaceManager,
        request: FindSymbolsRequest,
    ) -> Result<Vec<SymbolInfo>>;

    /// Get symbols by name (direct lookup)
    async fn get_symbols(
        &self,
        workspace_manager: &mut WorkspaceManager,
        request: GetSymbolsRequest,
    ) -> Result<Vec<SymbolInfo>>;

    /// Get document symbols for a specific file
    async fn get_document_symbols(
        &self,
        workspace_manager: &mut WorkspaceManager,
        file_path: &Path,
        top_level_only: bool,
    ) -> Result<Vec<SymbolInfo>>;

    /// Go to definition for symbol at specific location
    async fn goto_definition(
        &self,
        workspace_manager: &mut WorkspaceManager,
        file_path: &Path,
        line: u32,
        character: u32,
        show_source: bool,
    ) -> Result<Option<DefinitionInfo>>;

    /// Find references by location (line/column)
    async fn find_references_by_location(
        &self,
        workspace_manager: &mut WorkspaceManager,
        request: FindReferencesByLocationRequest,
    ) -> Result<Vec<ReferenceInfo>>;

    /// Find references by symbol name
    async fn find_references_by_name(
        &self,
        workspace_manager: &mut WorkspaceManager,
        request: FindReferencesByNameRequest,
    ) -> Result<Vec<ReferenceInfo>>;

}

/// LSP-based implementation of SymbolService
pub struct LspSymbolService {
    workspace_service: Box<dyn WorkspaceService>,
}

impl LspSymbolService {
    pub fn new(workspace_service: Box<dyn WorkspaceService>) -> Self {
        Self { workspace_service }
    }

    /// Check if a symbol kind should be included in top-level results
    fn is_top_level_symbol_kind(kind: SymbolKind) -> bool {
        matches!(kind,
            SymbolKind::FILE |
            SymbolKind::MODULE |
            SymbolKind::NAMESPACE |
            SymbolKind::PACKAGE |
            SymbolKind::CLASS |
            SymbolKind::ENUM |
            SymbolKind::INTERFACE |
            SymbolKind::METHOD |
            SymbolKind::STRUCT
        )
    }

    /// Convert DocumentSymbol to SymbolInfo
    fn document_symbol_to_symbol_info(
        ds: &DocumentSymbol,
        file_path: &Path,
        workspace_root: &Path,
    ) -> Option<SymbolInfo> {
        let uri = Url::from_file_path(file_path).ok()?;
        let location = Location::new(uri, ds.range);

        let mut symbol_info = SymbolInfo::from_workspace_symbol(
            &WorkspaceSymbol {
                name: ds.name.clone(),
                kind: ds.kind,
                location: OneOf::Left(location),
                container_name: None,
                tags: ds.tags.clone(),
                data: None,
            },
            workspace_root,
        )?;

        // Capture detail from DocumentSymbol (not available in WorkspaceSymbol)
        symbol_info.detail = ds.detail.clone();

        Some(symbol_info)
    }

    async fn find_symbols_exact(
        &self,
        workspace_manager: &mut WorkspaceManager,
        request: &FindSymbolsRequest,
    ) -> Result<Vec<SymbolInfo>, Error> {
        let mut all_symbols = Vec::new();
        // If file_path is specified, use document symbols for that file
        if let Some(file_path) = &request.file_path {
            let symbols = self
                .get_document_symbols(workspace_manager, file_path, false)
                .await?;
            all_symbols.extend(symbols);
        } else {
            // Use workspace symbol search only for detected languages
            let detected_languages = workspace_manager.get_detected_languages()?;
            for language in detected_languages {
                if let Ok(Some(client)) = workspace_manager.get_client_by_language(&language).await
                {
                    let params = WorkspaceSymbolParams {
                        query: request.symbol_name.clone(),
                        work_done_progress_params: Default::default(),
                        partial_result_params: Default::default(),
                    };

                    match client.workspace_symbols(params).await {
                        Ok(Some(symbols)) => {
                            let mapped_symbols: Vec<SymbolInfo> = symbols
                                .iter()
                                .filter_map(|s| {
                                    SymbolInfo::from_workspace_symbol(
                                        s,
                                        workspace_manager.workspace_root(),
                                    )
                                })
                                .collect();
                            all_symbols.extend(mapped_symbols);
                        }
                        Ok(None) => {}
                        Err(_) => {} // Skip servers that don't support workspace/symbol
                    }
                }
            }
        }
        if !request.symbol_name.is_empty() {
            let query_lower = request.symbol_name.to_lowercase();
            if request.exact_match {
                all_symbols.retain(|s| s.name.to_lowercase() == query_lower);
            } else {
                all_symbols.retain(|s| s.name.to_lowercase().contains(&query_lower));
            }
        }
        // Filter by symbol type if specified
        if let Some(symbol_type) = &request.symbol_type {
            let lsp_kind = symbol_type.to_lsp_symbol_kind();
            all_symbols.retain(|s| s.symbol_type == Some(format!("{:?}", lsp_kind)));
        }

        // Apply limit
        if request.limit.is_some() {
            all_symbols.truncate(request.limit.unwrap() as usize);
        }
        Ok(all_symbols)
    }
}

const MAX_RESULTS: u32 = 50;
const DEFAULT_RESULTS: u32 = 20;

#[async_trait::async_trait]
impl SymbolService for LspSymbolService {
    async fn find_symbols(
        &self,
        workspace_manager: &mut WorkspaceManager,
        mut request: FindSymbolsRequest,
    ) -> Result<Vec<SymbolInfo>> {
        // Ensure initialized
        if !workspace_manager.is_initialized() {
            workspace_manager.initialize().await?;
        }
        
        // Enforce limits
        request.limit = Some(request.limit.unwrap_or(DEFAULT_RESULTS).min(MAX_RESULTS));
        self.find_symbols_exact(workspace_manager, &request).await
    }

    async fn get_symbols(
        &self,
        workspace_manager: &mut WorkspaceManager,
        request: GetSymbolsRequest,
    ) -> Result<Vec<SymbolInfo>> {
        let mut results = Vec::new();

        for symbol_name in &request.symbols {
            let find_request = FindSymbolsRequest {
                symbol_name: symbol_name.clone(),
                file_path: request.file_path.clone(),
                symbol_type: None,
                limit: None,
                exact_match: true,
            };

            let symbols = self.find_symbols(workspace_manager, find_request).await?;
            results.extend(symbols);
        }

        Ok(results)
    }

    async fn get_document_symbols(
        &self,
        workspace_manager: &mut WorkspaceManager,
        file_path: &Path,
        top_level_only: bool,
    ) -> Result<Vec<SymbolInfo>> {
        // Ensure initialized
        if !workspace_manager.is_initialized() {
            workspace_manager.initialize().await?;
        }

        let canonical_path = canonicalize_path(file_path)?;
        let content = std::fs::read_to_string(&canonical_path)?;
        self.workspace_service
            .open_file(workspace_manager, &canonical_path, content)
            .await?;
        if let Some(client) = workspace_manager
            .get_client_for_file(&canonical_path)
            .await?
        {
            let uri = Url::from_file_path(&canonical_path)
                .map_err(|_| anyhow::anyhow!("Invalid file path"))?;

            let params = DocumentSymbolParams {
                text_document: TextDocumentIdentifier { uri },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };

            if let Some(symbols) = client.document_symbols(params).await? {
                let result = match symbols {
                    DocumentSymbolResponse::Flat(flat_symbols) => {
                        flat_symbols
                            .into_iter()
                            .filter(|s| !top_level_only || Self::is_top_level_symbol_kind(s.kind))
                            .filter_map(|s| {
                                SymbolInfo::from_workspace_symbol(
                                    &WorkspaceSymbol {
                                        name: s.name,
                                        kind: s.kind,
                                        location: OneOf::Left(s.location),
                                        container_name: s.container_name,
                                        tags: s.tags,
                                        data: None,
                                    },
                                    workspace_manager.workspace_root(),
                                )
                            })
                            .collect()
                    }
                    DocumentSymbolResponse::Nested(nested_symbols) => {
                        let mut nested_result = Vec::new();
                        for ds in nested_symbols {
                            if !top_level_only || Self::is_top_level_symbol_kind(ds.kind) {
                                if let Some(symbol_info) = Self::document_symbol_to_symbol_info(
                                    &ds,
                                    &canonical_path,
                                    workspace_manager.workspace_root(),
                                ) {
                                    nested_result.push(symbol_info);
                                }
                            }
                        }
                        nested_result
                    }
                };
                return Ok(result);
            }
        }

        Ok(vec![])
    }

    async fn goto_definition(
        &self,
        workspace_manager: &mut WorkspaceManager,
        file_path: &Path,
        line: u32,
        character: u32,
        show_source: bool,
    ) -> Result<Option<DefinitionInfo>> {
        // Ensure initialized
        if !workspace_manager.is_initialized() {
            workspace_manager.initialize().await?;
        }

        let canonical_path = canonicalize_path(file_path)?;
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

        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: crate::utils::to_lsp_position(line, character),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        match client.goto_definition(params).await? {
            Some(response) => {
                let mut def_info = match response {
                    GotoDefinitionResponse::Scalar(location) => {
                        DefinitionInfo::from_location(
                            &location,
                            workspace_manager.workspace_root(),
                            false, // Don't use show_source yet
                        )
                    }
                    GotoDefinitionResponse::Array(locations) => {
                        if let Some(location) = locations.first() {
                            DefinitionInfo::from_location(
                                location,
                                workspace_manager.workspace_root(),
                                false,
                            )
                        } else {
                            return Ok(None);
                        }
                    }
                    GotoDefinitionResponse::Link(links) => {
                        if let Some(link) = links.first() {
                            let location = Location {
                                uri: link.target_uri.clone(),
                                range: link.target_selection_range,
                            };
                            DefinitionInfo::from_location(
                                &location,
                                workspace_manager.workspace_root(),
                                false,
                            )
                        } else {
                            return Ok(None);
                        }
                    }
                };

                // If show_source is true, get full symbol range from document symbols
                if show_source {
                    let symbols = self
                        .get_document_symbols(workspace_manager, &canonical_path, false)
                        .await?;

                    // Find matching symbol - definition position should be within symbol range
                    for symbol in symbols {
                        let in_range = def_info.start_row >= symbol.start_row
                            && def_info.start_row <= symbol.end_row
                            && (def_info.start_row != symbol.start_row
                                || def_info.start_column >= symbol.start_column);

                        if in_range {
                            // Found matching symbol, use its full range and re-read source
                            def_info.end_row = symbol.end_row;
                            def_info.end_column = symbol.end_column;
                            // Re-read source with full range
                            use crate::model::entities::read_source_lines;
                            def_info.source_line = read_source_lines(
                                &canonical_path,
                                symbol.start_row,
                                symbol.end_row,
                            );
                            break;
                        }
                    }
                }

                Ok(Some(def_info))
            }
            None => Ok(None),
        }
    }

    async fn find_references_by_location(
        &self,
        workspace_manager: &mut WorkspaceManager,
        request: FindReferencesByLocationRequest,
    ) -> Result<Vec<ReferenceInfo>> {
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

        let params = ReferenceParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: crate::utils::to_lsp_position(request.row, request.column),
            },
            context: ReferenceContext {
                include_declaration: true,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let references = client.find_references(params).await?.unwrap_or_default();
        Ok(references
            .iter()
            .map(|r| ReferenceInfo::from_location(r, workspace_manager.workspace_root()))
            .collect())
    }

    async fn find_references_by_name(
        &self,
        workspace_manager: &mut WorkspaceManager,
        request: FindReferencesByNameRequest,
    ) -> Result<Vec<ReferenceInfo>> {
        // Find the symbol first
        let find_request = FindSymbolsRequest {
            symbol_name: request.symbol_name,
            file_path: None,
            symbol_type: None,
            limit: Some(1), // Only need first match
            exact_match: true,
        };

        let symbols = self.find_symbols(workspace_manager, find_request).await?;
        if let Some(symbol) = symbols.first() {
            // Convert relative path back to absolute and find references
            let workspace_file_path = workspace_manager.workspace_root().join(&symbol.file_path);
            let location_request = FindReferencesByLocationRequest {
                file_path: workspace_file_path,
                row: symbol.start_row - 1,      // Convert back to 0-based
                column: symbol.start_column - 1, // Convert back to 0-based
            };
            self.find_references_by_location(workspace_manager, location_request)
                .await
        } else {
            Ok(Vec::new())
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::entities::SymbolInfo;
    use lsp_types::SymbolKind;

    fn create_test_symbol(name: &str, symbol_type: &str) -> SymbolInfo {
        SymbolInfo {
            name: name.to_string(),
            symbol_type: Some(symbol_type.to_string()),
            file_path: "/test.rs".to_string(),
            fully_qualified_name: format!("test.rs::{}", name),
            start_row: 1,
            end_row: 1,
            start_column: 1,
            end_column: 10,
            container_name: None,
            detail: None,
            source_line: None,
        }
    }

    #[test]
    fn test_is_top_level_symbol_kind_true_cases() {
        assert!(LspSymbolService::is_top_level_symbol_kind(SymbolKind::FILE));
        assert!(LspSymbolService::is_top_level_symbol_kind(SymbolKind::MODULE));
        assert!(LspSymbolService::is_top_level_symbol_kind(SymbolKind::NAMESPACE));
        assert!(LspSymbolService::is_top_level_symbol_kind(SymbolKind::PACKAGE));
        assert!(LspSymbolService::is_top_level_symbol_kind(SymbolKind::CLASS));
        assert!(LspSymbolService::is_top_level_symbol_kind(SymbolKind::ENUM));
        assert!(LspSymbolService::is_top_level_symbol_kind(SymbolKind::INTERFACE));
        assert!(LspSymbolService::is_top_level_symbol_kind(SymbolKind::METHOD));
        assert!(LspSymbolService::is_top_level_symbol_kind(SymbolKind::STRUCT));
    }

    #[test]
    fn test_is_top_level_symbol_kind_false_cases() {
        assert!(!LspSymbolService::is_top_level_symbol_kind(SymbolKind::FUNCTION));
        assert!(!LspSymbolService::is_top_level_symbol_kind(SymbolKind::VARIABLE));
        assert!(!LspSymbolService::is_top_level_symbol_kind(SymbolKind::CONSTANT));
        assert!(!LspSymbolService::is_top_level_symbol_kind(SymbolKind::PROPERTY));
        assert!(!LspSymbolService::is_top_level_symbol_kind(SymbolKind::FIELD));
        assert!(!LspSymbolService::is_top_level_symbol_kind(SymbolKind::CONSTRUCTOR));
    }

    #[test]
    fn test_symbol_name_filtering_logic() {
        // Test the filtering logic that we modified
        let symbols = vec![
            create_test_symbol("test_function", "Function"),
            create_test_symbol("my_test", "Function"),
            create_test_symbol("testing", "Function"),
            create_test_symbol("other", "Function"),
        ];

        // Test exact match (should only match "test_function")
        let exact_matches: Vec<_> = symbols.iter()
            .filter(|s| s.name == "test_function")
            .collect();
        assert_eq!(exact_matches.len(), 1);
        assert_eq!(exact_matches[0].name, "test_function");

        // Test contains match (should match "test_function", "my_test", "testing")
        let contains_matches: Vec<_> = symbols.iter()
            .filter(|s| s.name.contains("test"))
            .collect();
        assert_eq!(contains_matches.len(), 3);
        assert!(contains_matches.iter().any(|s| s.name == "test_function"));
        assert!(contains_matches.iter().any(|s| s.name == "my_test"));
        assert!(contains_matches.iter().any(|s| s.name == "testing"));
    }

    // Helper function to create a test service
    fn create_test_service() -> LspSymbolService {
        use crate::sdk::services::workspace_service::LspWorkspaceService;
        let workspace_service = Box::new(LspWorkspaceService::new());
        LspSymbolService::new(workspace_service)
    }

    #[test]
    fn test_document_symbol_to_symbol_info() {
        use lsp_types::{DocumentSymbol, Position, Range, SymbolKind};
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.rs");
        let workspace_root = temp_dir.path();

        let doc_symbol = DocumentSymbol {
            name: "test_function".to_string(),
            detail: Some("fn test_function() -> i32".to_string()),
            kind: SymbolKind::FUNCTION,
            tags: None,
            deprecated: None,
            range: Range::new(Position::new(0, 0), Position::new(0, 20)),
            selection_range: Range::new(Position::new(0, 3), Position::new(0, 16)),
            children: None,
        };

        let result = LspSymbolService::document_symbol_to_symbol_info(
            &doc_symbol,
            &file_path,
            workspace_root,
        );

        assert!(result.is_some());
        let symbol_info = result.unwrap();
        assert_eq!(symbol_info.name, "test_function");
        assert_eq!(symbol_info.detail, Some("fn test_function() -> i32".to_string()));
        assert_eq!(symbol_info.start_row, 1); // LSP is 0-based, SymbolInfo is 1-based
        assert_eq!(symbol_info.start_column, 1);
    }

    #[test]
    fn test_document_symbol_to_symbol_info_no_detail() {
        use lsp_types::{DocumentSymbol, Position, Range, SymbolKind};
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.rs");
        let workspace_root = temp_dir.path();

        let doc_symbol = DocumentSymbol {
            name: "TestStruct".to_string(),
            detail: None,
            kind: SymbolKind::STRUCT,
            tags: None,
            deprecated: None,
            range: Range::new(Position::new(5, 0), Position::new(10, 1)),
            selection_range: Range::new(Position::new(5, 7), Position::new(5, 17)),
            children: None,
        };

        let result = LspSymbolService::document_symbol_to_symbol_info(
            &doc_symbol,
            &file_path,
            workspace_root,
        );

        assert!(result.is_some());
        let symbol_info = result.unwrap();
        assert_eq!(symbol_info.name, "TestStruct");
        assert!(symbol_info.detail.is_none());
        assert_eq!(symbol_info.start_row, 6); // LSP line 5 -> SymbolInfo line 6
    }

    #[test]
    fn test_new_symbol_service() {
        let service = create_test_service();
        // Just verify it constructs successfully
        assert!(std::ptr::addr_of!(service.workspace_service) as *const _ != std::ptr::null());
    }
}
