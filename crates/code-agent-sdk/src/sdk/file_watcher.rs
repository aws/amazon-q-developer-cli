use crate::model::{FsEvent, FsEventKind};
use anyhow::Result;
use globset::{Glob, GlobSetBuilder};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::sync::mpsc;
use url::Url;

/// Configuration for file watching with glob patterns
#[derive(Debug, Clone)]
pub(crate) struct FileWatcherConfig {
    /// Patterns to include (e.g., ["**/*.rs", "**/*.ts", "**/*.py"])
    pub include_patterns: Vec<String>,
    /// Patterns to exclude (e.g., ["**/target/**", "**/node_modules/**", "**/.git/**"])
    pub exclude_patterns: Vec<String>,
    /// Whether to respect .gitignore files (default: true)
    pub respect_gitignore: bool,
}

/// Multi-level gitignore matcher that handles nested .gitignore files
struct GitignoreMatcher {
    gitignores: Vec<(PathBuf, Gitignore)>,
}

impl GitignoreMatcher {
    /// Build gitignore matcher by scanning workspace for .gitignore files
    fn new(workspace_root: &Path) -> Result<Self> {
        let mut gitignores = Vec::new();
        
        // Walk the directory tree to find all .gitignore files
        for entry in ignore::WalkBuilder::new(workspace_root)
            .hidden(false) // We want to see .gitignore files
            .git_ignore(false) // Don't apply gitignore while searching for gitignore files
            .build()
        {
            let entry = entry?;
            let path = entry.path();
            
            if path.file_name() == Some(std::ffi::OsStr::new(".gitignore")) {
                let parent_dir = path.parent().unwrap_or(workspace_root);
                
                let mut builder = GitignoreBuilder::new(parent_dir);
                if let Some(err) = builder.add(path) {
                    tracing::warn!("Failed to parse .gitignore at {:?}: {}", path, err);
                    continue;
                }
                
                match builder.build() {
                    Ok(gitignore) => {
                        tracing::trace!("Loaded .gitignore from {:?}", path);
                        gitignores.push((parent_dir.to_path_buf(), gitignore));
                    }
                    Err(e) => {
                        tracing::warn!("Failed to build gitignore for {:?}: {}", path, e);
                    }
                }
            }
        }
        
        // Sort by depth (deepest first) for proper precedence
        gitignores.sort_by(|a, b| {
            b.0.components().count().cmp(&a.0.components().count())
        });
        
        tracing::trace!("Loaded {} .gitignore files", gitignores.len());
        Ok(Self { gitignores })
    }

    /// Check if a path should be ignored according to gitignore rules
    fn is_ignored(&self, path: &Path) -> bool {
        for (gitignore_dir, gitignore) in &self.gitignores {
            // Check if this path is under this gitignore's directory
            if let Ok(relative_path) = path.strip_prefix(gitignore_dir) {
                let matched = gitignore.matched(relative_path, path.is_dir());
                if matched.is_ignore() {
                    tracing::trace!("Path {:?} ignored by .gitignore in {:?}", path, gitignore_dir);
                    return true;
                }
            }
        }
        false
    }
}

/// Non-blocking file watcher with glob filtering and gitignore support
#[derive(Debug)]
pub(crate) struct FileWatcher {
    _watcher: RecommendedWatcher,
}

impl FileWatcher {
    /// Create a new file watcher with mandatory glob patterns and gitignore support
    pub fn new(
        workspace_root: PathBuf,
        event_tx: mpsc::UnboundedSender<FsEvent>,
        config: FileWatcherConfig,
    ) -> Result<Self> {
        let tx = event_tx.clone();
        let workspace_root_clone = workspace_root.clone();
        
        // Build glob matchers
        let mut include_builder = GlobSetBuilder::new();
        for pattern in &config.include_patterns {
            include_builder.add(Glob::new(pattern)?);
        }
        let include_matcher = include_builder.build()?;
        
        let mut exclude_builder = GlobSetBuilder::new();
        for pattern in &config.exclude_patterns {
            exclude_builder.add(Glob::new(pattern)?);
        }
        let exclude_matcher = exclude_builder.build()?;
        
        // Build gitignore matcher if enabled
        let gitignore_matcher = if config.respect_gitignore {
            Some(GitignoreMatcher::new(&workspace_root)?)
        } else {
            None
        };
        
        let watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            match res {
                Ok(event) => {
                    tracing::trace!("Raw file system event: {:?}", event);
                    
                    if let Some(fs_event) = convert_notify_event(event, &workspace_root_clone) {
                        // Apply filtering
                        if should_process_event(&fs_event, &include_matcher, &exclude_matcher, &gitignore_matcher, &workspace_root_clone) {
                            tracing::trace!("Accepted file event: {:?}", fs_event);
                            if let Err(e) = tx.send(fs_event) {
                                tracing::error!("Failed to send file system event: {}", e);
                            }
                        } else {
                            tracing::trace!("Filtered out file event: {:?}", fs_event.uri);
                        }
                    }
                }
                Err(e) => tracing::error!("File watcher error: {:?}", e),
            }
        })?;

        let mut file_watcher = Self {
            _watcher: watcher,
        };

        file_watcher._watcher.watch(&workspace_root, RecursiveMode::Recursive)?;
        tracing::trace!("Started watching directory: {:?} with patterns include={:?}, exclude={:?}, gitignore={}", 
                       workspace_root, config.include_patterns, config.exclude_patterns, config.respect_gitignore);

        Ok(file_watcher)
    }
}

/// Check if an event should be processed based on all filtering rules
fn should_process_event(
    event: &FsEvent,
    include_matcher: &globset::GlobSet,
    exclude_matcher: &globset::GlobSet,
    gitignore_matcher: &Option<GitignoreMatcher>,
    workspace_root: &Path,
) -> bool {
    let path = match event.uri.to_file_path() {
        Ok(path) => path,
        Err(_) => return false,
    };
    
    // Check gitignore first (most restrictive)
    if let Some(gitignore) = gitignore_matcher {
        if gitignore.is_ignored(&path) {
            return false;
        }
    }
    
    // Use relative path for glob matching (better pattern matching)
    let match_path = if let Ok(relative) = path.strip_prefix(workspace_root) {
        relative
    } else {
        &path
    };
    
    // Must match include patterns
    if !include_matcher.is_match(match_path) {
        return false;
    }
    
    // Must not match exclude patterns
    if exclude_matcher.is_match(match_path) {
        return false;
    }
    
    true
}

/// Convert notify::Event to our internal FsEvent
fn convert_notify_event(event: Event, workspace_root: &PathBuf) -> Option<FsEvent> {
    use notify::EventKind;

    let timestamp = Instant::now();
    
    // Get the first path from the event
    let path = event.paths.first()?;
    
    // Use absolute path for URL creation
    let uri = Url::from_file_path(path).ok()?;

    let kind = match event.kind {
        EventKind::Create(_) => FsEventKind::Created,
        EventKind::Modify(_) => FsEventKind::Modified,
        EventKind::Remove(_) => FsEventKind::Deleted,
        EventKind::Other => {
            // Handle rename events if we have two paths
            if event.paths.len() == 2 {
                let from_path = &event.paths[0];
                let from_relative = if let Ok(rel) = from_path.strip_prefix(workspace_root) {
                    rel.to_path_buf()
                } else {
                    from_path.clone()
                };
                
                if let Ok(from_uri) = Url::from_file_path(&from_relative) {
                    FsEventKind::Renamed { from: from_uri }
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }
        _ => {
            tracing::trace!("Ignoring event kind: {:?} for path: {:?}", event.kind, path);
            return None;
        }
    };

    Some(FsEvent {
        uri,
        kind,
        timestamp,
    })
}

/// Event processor that handles file system events and sends LSP notifications
pub(crate) struct EventProcessor {
    event_rx: mpsc::UnboundedReceiver<FsEvent>,
    workspace_manager: *mut crate::sdk::WorkspaceManager,
    workspace_root: PathBuf,
}

unsafe impl Send for EventProcessor {}

impl EventProcessor {
    pub fn new(event_rx: mpsc::UnboundedReceiver<FsEvent>, workspace_manager: *mut crate::sdk::WorkspaceManager, workspace_root: PathBuf) -> Self {
        Self { 
            event_rx, 
            workspace_manager,
            workspace_root,
        }
    }

    /// Run the event processing loop
    pub async fn run(mut self) {
        tracing::trace!("Starting file event processor");
        
        while let Some(event) = self.event_rx.recv().await {
            let age = event.timestamp.elapsed();
            tracing::trace!("Processing file event: {:?} (age: {:?})", event, age);
            
            if let Err(e) = self.handle_file_event(&event).await {
                tracing::error!("Failed to handle file event: {}", e);
            }
        }
        
        tracing::trace!("File event processor stopped");
    }

    async fn handle_file_event(&mut self, event: &FsEvent) -> Result<()> {
        use lsp_types::{DidChangeTextDocumentParams, VersionedTextDocumentIdentifier, TextDocumentContentChangeEvent, DidChangeWatchedFilesParams, FileEvent, FileChangeType};
        
        // Convert relative URI to absolute path
        if let Ok(relative_path) = event.uri.to_file_path() {
            let absolute_path = self.workspace_root.join(&relative_path);
            let absolute_uri = match Url::from_file_path(&absolute_path) {
                Ok(uri) => uri,
                Err(_) => return Ok(()),
            };
            
            match event.kind {
                FsEventKind::Modified => {
                    // SAFETY: We know workspace_manager is valid during EventProcessor lifetime
                    unsafe {
                        let workspace_manager = &mut *self.workspace_manager;
                        
                        if workspace_manager.is_file_opened(&absolute_path) {
                            // Send didChange for opened files
                            let version = workspace_manager.get_next_version(&absolute_path);
                            
                            if let Ok(Some(client)) = workspace_manager.get_client_for_file(&absolute_path).await {
                                if let Ok(content) = std::fs::read_to_string(&absolute_path) {
                                    let params = DidChangeTextDocumentParams {
                                        text_document: VersionedTextDocumentIdentifier {
                                            uri: absolute_uri,
                                            version,
                                        },
                                        content_changes: vec![TextDocumentContentChangeEvent {
                                            range: None,
                                            range_length: None,
                                            text: content,
                                        }],
                                    };
                                    
                                    tracing::info!("📝 Sending didChange for opened file: {:?}, version: {}", absolute_path, version);
                                    let _ = client.did_change(params).await;
                                }
                            }
                        } else {
                            // Send workspace/didChangeWatchedFiles for closed files
                            if let Ok(Some(client)) = workspace_manager.get_client_for_file(&absolute_path).await {
                                let params = DidChangeWatchedFilesParams {
                                    changes: vec![FileEvent {
                                        uri: absolute_uri,
                                        typ: FileChangeType::CHANGED,
                                    }],
                                };
                                
                                tracing::info!("📁 Sending didChangeWatchedFiles for closed file: {:?}", absolute_path);
                                let _ = client.did_change_watched_files(params).await;
                            }
                        }
                    }
                }
                FsEventKind::Created => tracing::info!("📄 File created: {:?}", absolute_path),
                FsEventKind::Deleted => tracing::info!("🗑️ File deleted: {:?}", absolute_path),
                FsEventKind::Renamed { ref from } => {
                    if let Ok(from_path) = from.to_file_path() {
                        let from_absolute = self.workspace_root.join(&from_path);
                        tracing::info!("📋 File renamed: {:?} -> {:?}", from_absolute, absolute_path);
                    }
                }
            }
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_config() -> FileWatcherConfig {
        FileWatcherConfig {
            include_patterns: vec!["**/*.ts".to_string(), "**/*.js".to_string()],
            exclude_patterns: vec!["**/node_modules/**".to_string(), "**/target/**".to_string()],
            respect_gitignore: false,
        }
    }

    #[test]
    fn test_convert_notify_event_create() {
        let workspace_root = PathBuf::from("/test/workspace");
        let file_path = workspace_root.join("src/test.ts");
        
        let event = notify::Event {
            kind: notify::EventKind::Create(notify::event::CreateKind::File),
            paths: vec![file_path.clone()],
            attrs: Default::default(),
        };

        let fs_event = convert_notify_event(event, &workspace_root).unwrap();
        
        assert_eq!(fs_event.kind, FsEventKind::Created);
        assert_eq!(fs_event.uri.path(), file_path.to_str().unwrap());
    }

    #[test]
    fn test_convert_notify_event_modify() {
        let workspace_root = PathBuf::from("/test/workspace");
        let file_path = workspace_root.join("src/test.ts");
        
        let event = notify::Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(notify::event::DataChange::Content)),
            paths: vec![file_path.clone()],
            attrs: Default::default(),
        };

        let fs_event = convert_notify_event(event, &workspace_root).unwrap();
        
        assert_eq!(fs_event.kind, FsEventKind::Modified);
        assert_eq!(fs_event.uri.path(), file_path.to_str().unwrap());
    }

    #[test]
    fn test_convert_notify_event_delete() {
        let workspace_root = PathBuf::from("/test/workspace");
        let file_path = workspace_root.join("src/test.ts");
        
        let event = notify::Event {
            kind: notify::EventKind::Remove(notify::event::RemoveKind::File),
            paths: vec![file_path.clone()],
            attrs: Default::default(),
        };

        let fs_event = convert_notify_event(event, &workspace_root).unwrap();
        
        assert_eq!(fs_event.kind, FsEventKind::Deleted);
        assert_eq!(fs_event.uri.path(), file_path.to_str().unwrap());
    }

    #[test]
    fn test_should_process_event_include_patterns() {
        let workspace_root = PathBuf::from("/test/workspace");
        let config = create_test_config();
        
        let mut include_builder = globset::GlobSetBuilder::new();
        for pattern in &config.include_patterns {
            include_builder.add(globset::Glob::new(pattern).unwrap());
        }
        let include_matcher = include_builder.build().unwrap();
        
        let exclude_matcher = globset::GlobSetBuilder::new().build().unwrap();

        // TypeScript file should be included
        let ts_file = workspace_root.join("src/test.ts");
        let ts_event = FsEvent {
            uri: Url::from_file_path(&ts_file).unwrap(),
            kind: FsEventKind::Modified,
            timestamp: Instant::now(),
        };
        
        assert!(should_process_event(&ts_event, &include_matcher, &exclude_matcher, &None, &workspace_root));

        // Non-matching file should be excluded
        let txt_file = workspace_root.join("src/test.txt");
        let txt_event = FsEvent {
            uri: Url::from_file_path(&txt_file).unwrap(),
            kind: FsEventKind::Modified,
            timestamp: Instant::now(),
        };
        
        assert!(!should_process_event(&txt_event, &include_matcher, &exclude_matcher, &None, &workspace_root));
    }

    #[test]
    fn test_should_process_event_exclude_patterns() {
        let workspace_root = PathBuf::from("/test/workspace");
        let config = create_test_config();
        
        let mut include_builder = globset::GlobSetBuilder::new();
        for pattern in &config.include_patterns {
            include_builder.add(globset::Glob::new(pattern).unwrap());
        }
        let include_matcher = include_builder.build().unwrap();
        
        let mut exclude_builder = globset::GlobSetBuilder::new();
        for pattern in &config.exclude_patterns {
            exclude_builder.add(globset::Glob::new(pattern).unwrap());
        }
        let exclude_matcher = exclude_builder.build().unwrap();

        // File in node_modules should be excluded
        let node_modules_file = workspace_root.join("node_modules/package/index.ts");
        let excluded_event = FsEvent {
            uri: Url::from_file_path(&node_modules_file).unwrap(),
            kind: FsEventKind::Modified,
            timestamp: Instant::now(),
        };
        
        assert!(!should_process_event(&excluded_event, &include_matcher, &exclude_matcher, &None, &workspace_root));

        // Regular TypeScript file should be included
        let regular_file = workspace_root.join("src/test.ts");
        let included_event = FsEvent {
            uri: Url::from_file_path(&regular_file).unwrap(),
            kind: FsEventKind::Modified,
            timestamp: Instant::now(),
        };
        
        assert!(should_process_event(&included_event, &include_matcher, &exclude_matcher, &None, &workspace_root));
    }

    #[test]
    fn test_convert_notify_event_empty_paths() {
        let workspace_root = PathBuf::from("/test/workspace");
        
        let event = notify::Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Data(notify::event::DataChange::Content)),
            paths: vec![],
            attrs: Default::default(),
        };
        
        assert!(convert_notify_event(event, &workspace_root).is_none());
    }

    #[test]
    fn test_convert_notify_event_unsupported_kind() {
        let workspace_root = PathBuf::from("/test/workspace");
        let file_path = workspace_root.join("src/test.ts");
        
        let event = notify::Event {
            kind: notify::EventKind::Access(notify::event::AccessKind::Read),
            paths: vec![file_path],
            attrs: Default::default(),
        };
        
        assert!(convert_notify_event(event, &workspace_root).is_none());
    }

    #[test]
    fn test_gitignore_matcher() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_root = temp_dir.path().to_path_buf();
        
        let gitignore_content = "*.log\n";
        std::fs::write(workspace_root.join(".gitignore"), gitignore_content).unwrap();
        
        let matcher = GitignoreMatcher::new(&workspace_root).unwrap();
        
        // Test file extension matching
        let log_file = workspace_root.join("test.log");
        assert!(matcher.is_ignored(&log_file));
        
        // Test non-matching file
        let regular_file = workspace_root.join("test.ts");
        assert!(!matcher.is_ignored(&regular_file));
    }
}
