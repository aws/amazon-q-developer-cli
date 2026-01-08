use std::path::Path;

use globset::{
    Glob,
    GlobSet,
    GlobSetBuilder,
};
use lsp_types::*;
use serde_json::{
    Value,
    json,
};
use url::Url;

use crate::model::types::LanguageServerConfig;

/// Configuration for LSP client initialization
pub struct LspConfig;

#[allow(deprecated)]
impl LspConfig {
    /// Build initialization parameters for LSP server
    pub fn build_initialize_params(
        root_uri: Url,
        language_config: Option<&LanguageServerConfig>,
        initialization_options: Option<Value>,
    ) -> InitializeParams {
        let workspace_folders = if let Some(config) = language_config {
            if config.multi_workspace {
                Self::discover_workspaces(&root_uri, &config.project_patterns, &config.exclude_patterns)
            } else {
                vec![WorkspaceFolder {
                    uri: root_uri.clone(),
                    name: "workspace".to_string(),
                }]
            }
        } else {
            vec![WorkspaceFolder {
                uri: root_uri.clone(),
                name: "workspace".to_string(),
            }]
        };

        tracing::trace!(
            "LSP init workspace_folders for root_uri={}: {:?}",
            root_uri,
            workspace_folders.iter().map(|f| f.uri.as_str()).collect::<Vec<_>>()
        );

        InitializeParams {
            process_id: None,
            root_path: None,
            root_uri: Some(root_uri.clone()),
            initialization_options: initialization_options.or(Some(json!({}))),
            workspace_folders: Some(workspace_folders),
            capabilities: ClientCapabilities {
                general: Some(GeneralClientCapabilities {
                    position_encodings: Some(vec![PositionEncodingKind::UTF8, PositionEncodingKind::UTF16]),
                    ..Default::default()
                }),
                text_document: Some(TextDocumentClientCapabilities {
                    publish_diagnostics: Some(PublishDiagnosticsClientCapabilities {
                        related_information: Some(true),
                        version_support: Some(true),
                        code_description_support: Some(true),
                        data_support: Some(true),
                        ..Default::default()
                    }),
                    diagnostic: Some(DiagnosticClientCapabilities {
                        dynamic_registration: Some(true),
                        related_document_support: Some(true),
                    }),
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
            client_info: None,
            locale: None,
            work_done_progress_params: Default::default(),
        }
    }

    /// Discover workspace folders based on project patterns
    pub fn discover_workspaces(
        root_uri: &Url,
        project_patterns: &[String],
        exclude_patterns: &[String],
    ) -> Vec<WorkspaceFolder> {
        let Ok(root_path) = root_uri.to_file_path() else {
            tracing::error!("Invalid root URI for workspace discovery");
            return vec![WorkspaceFolder {
                uri: root_uri.clone(),
                name: "workspace".to_string(),
            }];
        };

        // Build exclude matcher once
        let exclude_matcher = Self::build_glob_set(exclude_patterns);

        let mut folders = vec![];
        const MAX_DEPTH: usize = 3;

        if let Err(e) = Self::scan_for_workspaces(
            &root_path,
            &root_path,
            project_patterns,
            &exclude_matcher,
            &mut folders,
            0,
            MAX_DEPTH,
        ) {
            tracing::error!("Error during workspace discovery: {}", e);
        }

        if folders.is_empty() {
            tracing::info!("No workspaces discovered, using root as single workspace");
            folders.push(WorkspaceFolder {
                uri: root_uri.clone(),
                name: "workspace".to_string(),
            });
        } else {
            tracing::info!("Discovered {} workspace(s)", folders.len());
        }

        folders
    }

    fn build_glob_set(patterns: &[String]) -> GlobSet {
        let mut builder = GlobSetBuilder::new();
        for pattern in patterns {
            if let Ok(glob) = Glob::new(pattern) {
                builder.add(glob);
            }
        }
        builder.build().unwrap_or_else(|_| GlobSet::empty())
    }

    fn scan_for_workspaces(
        current_path: &Path,
        root_path: &Path,
        project_patterns: &[String],
        exclude_matcher: &GlobSet,
        folders: &mut Vec<WorkspaceFolder>,
        depth: usize,
        max_depth: usize,
    ) -> std::io::Result<()> {
        if depth > max_depth {
            return Ok(());
        }

        // Check if current path should be excluded
        if Self::should_exclude(current_path, root_path, exclude_matcher) {
            return Ok(());
        }

        // Check if current path matches any project pattern
        let has_project_marker = project_patterns
            .iter()
            .any(|pattern| current_path.join(pattern).exists());

        if has_project_marker && let Ok(folder_uri) = Url::from_file_path(current_path) {
            let folder_name = current_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("workspace")
                .to_string();

            folders.push(WorkspaceFolder {
                uri: folder_uri,
                name: folder_name.clone(),
            });

            tracing::info!("Discovered workspace: {} at {}", folder_name, current_path.display());
            // Continue scanning for nested workspaces (monorepo support)
            // but only if we haven't hit max depth
        }

        // Recurse into subdirectories
        if current_path.is_dir() {
            for entry in std::fs::read_dir(current_path)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    Self::scan_for_workspaces(
                        &path,
                        root_path,
                        project_patterns,
                        exclude_matcher,
                        folders,
                        depth + 1,
                        max_depth,
                    )?;
                }
            }
        }

        Ok(())
    }

    fn should_exclude(path: &Path, root_path: &Path, exclude_matcher: &GlobSet) -> bool {
        let relative_path = path.strip_prefix(root_path).unwrap_or(path);
        exclude_matcher.is_match(relative_path)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_build_initialize_params_basic() {
        let root_uri = Url::parse("file:///workspace").unwrap();
        let params = LspConfig::build_initialize_params(root_uri.clone(), None, None);

        assert!(params.workspace_folders.is_some());
        let workspace_folders = params.workspace_folders.unwrap();
        assert_eq!(workspace_folders.len(), 1);
        assert_eq!(workspace_folders[0].uri, root_uri);
        assert!(params.initialization_options.is_some());
        assert_eq!(params.initialization_options.unwrap(), json!({}));
        assert!(params.capabilities.text_document.is_some());
        assert!(params.capabilities.workspace.is_some());
        assert_eq!(params.trace, Some(TraceValue::Verbose));
    }

    #[test]
    fn test_build_initialize_params_with_options() {
        let root_uri = Url::parse("file:///project").unwrap();
        let init_options = json!({"custom": "value", "debug": true});

        let params = LspConfig::build_initialize_params(root_uri.clone(), None, Some(init_options.clone()));

        assert!(params.workspace_folders.is_some());
        let workspace_folders = params.workspace_folders.unwrap();
        assert_eq!(workspace_folders.len(), 1);
        assert_eq!(workspace_folders[0].uri, root_uri);
        assert_eq!(params.initialization_options, Some(init_options));
    }

    #[test]
    fn test_build_initialize_params_capabilities() {
        let root_uri = Url::parse("file:///test").unwrap();
        let params = LspConfig::build_initialize_params(root_uri, None, None);

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
        let params = LspConfig::build_initialize_params(root_uri.clone(), None, None);

        let workspace_folders = params.workspace_folders.unwrap();
        assert_eq!(workspace_folders.len(), 1);
        assert_eq!(workspace_folders[0].uri, root_uri);
        assert_eq!(workspace_folders[0].name, "workspace");
    }

    #[test]
    fn test_build_initialize_params_position_encodings() {
        let root_uri = Url::parse("file:///test").unwrap();
        let params = LspConfig::build_initialize_params(root_uri, None, None);

        let general_caps = params.capabilities.general.unwrap();
        let encodings = general_caps.position_encodings.unwrap();
        assert!(encodings.contains(&PositionEncodingKind::UTF8));
        assert!(encodings.contains(&PositionEncodingKind::UTF16));
    }

    #[test]
    fn test_should_exclude() {
        use std::path::PathBuf;

        let root = PathBuf::from("/workspace");
        let patterns = vec![
            "**/node_modules/**".to_string(),
            "**/target/**".to_string(),
            "**/*.log".to_string(),
        ];
        let exclude_matcher = LspConfig::build_glob_set(&patterns);

        // Should exclude
        assert!(LspConfig::should_exclude(
            &root.join("node_modules/pkg"),
            &root,
            &exclude_matcher
        ));
        assert!(LspConfig::should_exclude(
            &root.join("src/node_modules/pkg"),
            &root,
            &exclude_matcher
        ));
        assert!(LspConfig::should_exclude(
            &root.join("target/debug"),
            &root,
            &exclude_matcher
        ));
        assert!(LspConfig::should_exclude(
            &root.join("app.log"),
            &root,
            &exclude_matcher
        ));

        // Should not exclude
        assert!(!LspConfig::should_exclude(
            &root.join("src/main.rs"),
            &root,
            &exclude_matcher
        ));
        assert!(!LspConfig::should_exclude(
            &root.join("src/lib"),
            &root,
            &exclude_matcher
        ));
    }
}
