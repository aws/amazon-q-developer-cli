use lsp_types::*;
use serde_json::Value;
use url::Url;

/// Configuration for LSP client initialization
pub struct LspConfig;

#[allow(deprecated)]
impl LspConfig {
    /// Build initialization parameters for LSP server
    pub fn build_initialize_params(
        root_uri: Url,
        initialization_options: Option<Value>,
    ) -> InitializeParams {
        InitializeParams {
            process_id: None,
            root_path: None,
            root_uri: None,
            initialization_options,
            capabilities: ClientCapabilities {
                general: Some(GeneralClientCapabilities {
                    position_encodings: Some(vec![
                        PositionEncodingKind::UTF8,
                        PositionEncodingKind::UTF16,
                    ]),
                    ..Default::default()
                }),
                text_document: Some(TextDocumentClientCapabilities {
                    definition: Some(GotoCapability {
                        dynamic_registration: Some(true),
                        link_support: Some(true),
                    }),
                    references: Some(ReferenceClientCapabilities {
                        dynamic_registration: Some(true),
                    }),
                    type_definition: Some(GotoCapability {
                        dynamic_registration: Some(true),
                        link_support: Some(true),
                    }),
                    rename: Some(RenameClientCapabilities {
                        dynamic_registration: Some(true),
                        ..Default::default()
                    }),
                    synchronization: Some(TextDocumentSyncClientCapabilities {
                        dynamic_registration: Some(true),
                        will_save: Some(true),
                        will_save_wait_until: Some(true),
                        did_save: Some(true),
                    }),
                    document_symbol: Some(DocumentSymbolClientCapabilities {
                        dynamic_registration: Some(true),
                        ..Default::default()
                    }),
                    diagnostic: Some(DiagnosticClientCapabilities {
                        dynamic_registration: Some(true),
                        related_document_support: Some(true),
                    }),
                    ..Default::default()
                }),
                workspace: Some(WorkspaceClientCapabilities {
                    symbol: Some(WorkspaceSymbolClientCapabilities {
                        dynamic_registration: Some(true),
                        resolve_support: Some(WorkspaceSymbolResolveSupportCapability {
                            properties: vec!["location.range".to_string()],
                        }),
                        ..Default::default()
                    }),
                    //Important, workspace symbol search.
                    workspace_folders: Some(true),
                    diagnostic: Some(DiagnosticWorkspaceClientCapabilities {
                        refresh_support: Some(true),
                    }),
                    did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
                        dynamic_registration: Some(true),
                        relative_pattern_support: Some(true),
                    }),
                    file_operations: Some(WorkspaceFileOperationsClientCapabilities {
                        dynamic_registration: Some(true),
                        will_rename: Some(true),
                        will_delete: Some(true),
                        did_create: Some(true),
                        will_create: Some(true),
                        did_rename: Some(true),
                        did_delete: Some(true),
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
            trace: Some(TraceValue::Verbose),
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: root_uri,
                name: "workspace".to_string(),
            }]),
            client_info: None,
            locale: None,
            work_done_progress_params: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_build_initialize_params_basic() {
        let root_uri = Url::parse("file:///workspace").unwrap();
        let params = LspConfig::build_initialize_params(root_uri.clone(), None);
        
        assert!(params.workspace_folders.is_some());
        let workspace_folders = params.workspace_folders.unwrap();
        assert_eq!(workspace_folders.len(), 1);
        assert_eq!(workspace_folders[0].uri, root_uri);
        assert!(params.initialization_options.is_none());
        assert!(params.capabilities.text_document.is_some());
        assert!(params.capabilities.workspace.is_some());
        assert_eq!(params.trace, Some(TraceValue::Verbose));
    }

    #[test]
    fn test_build_initialize_params_with_options() {
        let root_uri = Url::parse("file:///project").unwrap();
        let init_options = json!({"custom": "value", "debug": true});
        
        let params = LspConfig::build_initialize_params(root_uri.clone(), Some(init_options.clone()));
        
        assert!(params.workspace_folders.is_some());
        let workspace_folders = params.workspace_folders.unwrap();
        assert_eq!(workspace_folders.len(), 1);
        assert_eq!(workspace_folders[0].uri, root_uri);
        assert_eq!(params.initialization_options, Some(init_options));
    }

    #[test]
    fn test_build_initialize_params_capabilities() {
        let root_uri = Url::parse("file:///test").unwrap();
        let params = LspConfig::build_initialize_params(root_uri, None);
        
        let text_doc_caps = params.capabilities.text_document.unwrap();
        assert!(text_doc_caps.definition.is_some());
        assert!(text_doc_caps.references.is_some());
        assert!(text_doc_caps.rename.is_some());
        assert!(text_doc_caps.document_symbol.is_some());
        
        let workspace_caps = params.capabilities.workspace.unwrap();
        assert!(workspace_caps.symbol.is_some());
        assert_eq!(workspace_caps.workspace_folders, Some(true));
    }

    #[test]
    fn test_build_initialize_params_workspace_folders() {
        let root_uri = Url::parse("file:///my-project").unwrap();
        let params = LspConfig::build_initialize_params(root_uri.clone(), None);
        
        let workspace_folders = params.workspace_folders.unwrap();
        assert_eq!(workspace_folders.len(), 1);
        assert_eq!(workspace_folders[0].uri, root_uri);
        assert_eq!(workspace_folders[0].name, "workspace");
    }

    #[test]
    fn test_build_initialize_params_position_encodings() {
        let root_uri = Url::parse("file:///test").unwrap();
        let params = LspConfig::build_initialize_params(root_uri, None);
        
        let general_caps = params.capabilities.general.unwrap();
        let encodings = general_caps.position_encodings.unwrap();
        assert!(encodings.contains(&PositionEncodingKind::UTF8));
        assert!(encodings.contains(&PositionEncodingKind::UTF16));
    }
}
