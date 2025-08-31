use std::path::Path;

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::super::operation::OperationManager;
use crate::config::SemanticSearchConfig;
use crate::processing::process_file_with_config;

/// File processor for handling directory operations
pub struct FileProcessor {
    config: SemanticSearchConfig,
}

impl FileProcessor {
    /// Create new file processor
    pub fn new(config: SemanticSearchConfig) -> Self {
        Self { config }
    }

    /// Count files in directory
    pub async fn count_files_in_directory(
        &self,
        dir_path: &Path,
        operation_id: Uuid,
        include_patterns: &Option<Vec<String>>,
        exclude_patterns: &Option<Vec<String>>,
        operation_manager: &OperationManager,
    ) -> std::result::Result<usize, String> {
        self.update_operation_status(operation_manager, operation_id, "Counting files...".to_string())
            .await;

        let dir_path = dir_path.to_path_buf();
        let active_operations = operation_manager.get_active_operations_ref().clone();
        let pattern_filter = Self::create_pattern_filter(include_patterns, exclude_patterns)?;
        let max_files = self.config.max_files;

        let count_result = tokio::task::spawn_blocking(move || {
            let mut count = 0;
            let mut checked = 0;

            for _entry in walkdir::WalkDir::new(&dir_path)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
                .filter(|e| {
                    !e.path()
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|s| s.starts_with('.'))
                })
                .filter(|e| {
                    pattern_filter
                        .as_ref()
                        .is_none_or(|filter| filter.should_include(e.path()))
                })
            {
                count += 1;
                checked += 1;

                if checked % 100 == 0 {
                    if let Ok(operations) = active_operations.try_read() {
                        if let Some(handle) = operations.get(&operation_id) {
                            if handle.cancel_token.is_cancelled() {
                                return Err("Operation cancelled during file counting".to_string());
                            }
                            if let Ok(progress) = handle.progress.try_lock() {
                                if progress.message.contains("cancelled") {
                                    return Err("Operation cancelled during file counting".to_string());
                                }
                            }
                        }
                    }
                }

                if count > max_files {
                    break;
                }
            }

            Ok(count)
        })
        .await;

        match count_result {
            Ok(Ok(count)) => Ok(count),
            Ok(Err(e)) => Err(e),
            Err(e) => Err(format!("File counting task failed: {}", e)),
        }
    }

    /// Process directory files
    #[allow(clippy::too_many_arguments)]
    pub async fn process_directory_files(
        &self,
        dir_path: &Path,
        file_count: usize,
        operation_id: Uuid,
        cancel_token: &CancellationToken,
        include_patterns: &Option<Vec<String>>,
        exclude_patterns: &Option<Vec<String>>,
        operation_manager: &OperationManager,
    ) -> std::result::Result<Vec<serde_json::Value>, String> {
        self.update_operation_status(
            operation_manager,
            operation_id,
            format!("Starting indexing ({} files)", file_count),
        )
        .await;

        let pattern_filter = Self::create_pattern_filter(include_patterns, exclude_patterns)?;
        let mut processed_files = 0;
        let mut items = Vec::new();

        for entry in walkdir::WalkDir::new(dir_path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| {
                pattern_filter
                    .as_ref()
                    .is_none_or(|filter| filter.should_include(e.path()))
            })
        {
            if cancel_token.is_cancelled() {
                return Err("Operation was cancelled during file processing".to_string());
            }

            let path = entry.path();

            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|s| s.starts_with('.'))
            {
                continue;
            }

            match process_file_with_config(path, Some(self.config.chunk_size), Some(self.config.chunk_overlap)) {
                Ok(mut file_items) => items.append(&mut file_items),
                Err(_) => continue,
            }

            processed_files += 1;

            if processed_files % 10 == 0 {
                self.update_operation_progress(
                    operation_manager,
                    operation_id,
                    processed_files as u64,
                    file_count as u64,
                    format!("Indexing files ({}/{})", processed_files, file_count),
                )
                .await;
            }
        }

        Ok(items)
    }

    fn create_pattern_filter(
        include_patterns: &Option<Vec<String>>,
        exclude_patterns: &Option<Vec<String>>,
    ) -> std::result::Result<Option<crate::pattern_filter::PatternFilter>, String> {
        if include_patterns.is_some() || exclude_patterns.is_some() {
            let inc = include_patterns.as_deref().unwrap_or(&[]);
            let exc = exclude_patterns.as_deref().unwrap_or(&[]);
            Ok(Some(
                crate::pattern_filter::PatternFilter::new(inc, exc).map_err(|e| format!("Invalid patterns: {}", e))?,
            ))
        } else {
            Ok(None)
        }
    }

    async fn update_operation_status(&self, operation_manager: &OperationManager, operation_id: Uuid, message: String) {
        if let Ok(mut operations) = operation_manager.get_active_operations_ref().try_write() {
            if let Some(operation) = operations.get_mut(&operation_id) {
                if let Ok(mut progress) = operation.progress.try_lock() {
                    progress.message = message;
                }
            }
        }
    }

    async fn update_operation_progress(
        &self,
        operation_manager: &OperationManager,
        operation_id: Uuid,
        current: u64,
        total: u64,
        message: String,
    ) {
        if let Ok(mut operations) = operation_manager.get_active_operations_ref().try_write() {
            if let Some(operation) = operations.get_mut(&operation_id) {
                if let Ok(mut progress) = operation.progress.try_lock() {
                    progress.update(current, total, message);
                }
            }
        }
    }
}
