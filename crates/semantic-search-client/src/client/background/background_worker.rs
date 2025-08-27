use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{
    Semaphore,
    mpsc,
};
use tokio_util::sync::CancellationToken;
use tracing::debug;
use uuid::Uuid;

use super::super::context::{
    ContextCreator,
    ContextManager,
};
use super::super::operation::OperationManager;
use super::file_processor::FileProcessor;
use crate::client::{
    embedder_factory,
    utils,
};
use crate::config::SemanticSearchConfig;
use crate::embedding::TextEmbedderTrait;
use crate::types::*;

const MAX_CONCURRENT_OPERATIONS: usize = 3;

/// Background worker for processing indexing jobs
pub struct BackgroundWorker {
    job_rx: mpsc::UnboundedReceiver<IndexingJob>,
    context_manager: ContextManager,
    operation_manager: OperationManager,
    embedder: Box<dyn TextEmbedderTrait>,
    config: SemanticSearchConfig,
    base_dir: PathBuf,
    indexing_semaphore: Arc<Semaphore>,
    file_processor: FileProcessor,
    context_creator: ContextCreator,
}

impl BackgroundWorker {
    /// Create new background worker
    pub async fn new(
        job_rx: mpsc::UnboundedReceiver<IndexingJob>,
        context_manager: ContextManager,
        operation_manager: OperationManager,
        config: SemanticSearchConfig,
        base_dir: PathBuf,
    ) -> crate::error::Result<Self> {
        let embedder = embedder_factory::create_embedder(config.embedding_type)?;
        let file_processor = FileProcessor::new(config.clone());
        let context_creator = ContextCreator::new();

        Ok(Self {
            job_rx,
            context_manager,
            operation_manager,
            embedder,
            config,
            base_dir,
            indexing_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_OPERATIONS)),
            file_processor,
            context_creator,
        })
    }

    /// Run the background worker
    pub async fn run(mut self) {
        debug!("Background worker started for async semantic search client");

        while let Some(job) = self.job_rx.recv().await {
            match job {
                IndexingJob::AddDirectory {
                    id,
                    cancel,
                    path,
                    name,
                    description,
                    persistent,
                    include_patterns,
                    exclude_patterns,
                    embedding_type,
                } => {
                    let params = IndexingParams {
                        path,
                        name,
                        description,
                        persistent,
                        include_patterns,
                        exclude_patterns,
                        embedding_type,
                    };

                    self.process_add_directory(id, params, cancel).await;
                },
                IndexingJob::Clear { id, cancel } => {
                    self.process_clear(id, cancel).await;
                },
            }
        }

        debug!("Background worker stopped");
    }

    async fn process_add_directory(&self, operation_id: Uuid, params: IndexingParams, cancel_token: CancellationToken) {
        debug!(
            "Processing AddDirectory job: {} -> {}",
            params.name,
            params.path.display()
        );

        if cancel_token.is_cancelled() {
            self.mark_operation_cancelled(operation_id).await;
            return;
        }

        self.update_operation_status(operation_id, "Waiting in queue...".to_string())
            .await;

        let _permit = match self.indexing_semaphore.try_acquire() {
            Ok(permit) => {
                self.update_operation_status(operation_id, "Acquired slot, starting indexing...".to_string())
                    .await;
                permit
            },
            Err(_) => {
                self.update_operation_status(
                    operation_id,
                    format!(
                        "Waiting for available slot (max {} concurrent)...",
                        MAX_CONCURRENT_OPERATIONS
                    ),
                )
                .await;
                match self.indexing_semaphore.acquire().await {
                    Ok(permit) => {
                        self.update_operation_status(operation_id, "Acquired slot, starting indexing...".to_string())
                            .await;
                        permit
                    },
                    Err(_) => {
                        self.mark_operation_failed(operation_id, "Semaphore unavailable".to_string())
                            .await;
                        return;
                    },
                }
            },
        };

        let result = self.perform_indexing(operation_id, params, cancel_token).await;

        match result {
            Ok(context_id) => {
                debug!("Successfully indexed context: {}", context_id);
                self.mark_operation_completed(operation_id).await;
            },
            Err(e) => {
                tracing::error!("Indexing failed: {}", e);
                self.mark_operation_failed(operation_id, e).await;
            },
        }
    }

    async fn perform_indexing(
        &self,
        operation_id: Uuid,
        params: IndexingParams,
        cancel_token: CancellationToken,
    ) -> std::result::Result<String, String> {
        if !params.path.exists() {
            return Err(format!("Path '{}' does not exist", params.path.display()));
        }

        if cancel_token.is_cancelled() {
            return Err("Operation was cancelled".to_string());
        }

        let context_id = utils::generate_context_id();
        let context_dir = if params.persistent {
            self.base_dir.join(&context_id)
        } else {
            std::env::temp_dir().join("semantic_search").join(&context_id)
        };

        tokio::fs::create_dir_all(&context_dir)
            .await
            .map_err(|e| format!("Failed to create context directory: {}", e))?;

        if cancel_token.is_cancelled() {
            return Err("Operation was cancelled during setup".to_string());
        }

        let file_count = self
            .file_processor
            .count_files_in_directory(
                &params.path,
                operation_id,
                &params.include_patterns,
                &params.exclude_patterns,
                &self.operation_manager,
            )
            .await?;

        if file_count > self.config.max_files {
            self.update_operation_status(
                operation_id,
                format!(
                    "Failed: Directory contains {} files, which exceeds the maximum limit of {} files",
                    file_count, self.config.max_files
                ),
            )
            .await;
            cancel_token.cancel();
            return Err(format!(
                "Failed: Directory contains {} files, which exceeds the maximum limit of {} files",
                file_count, self.config.max_files
            ));
        }

        if cancel_token.is_cancelled() {
            return Err("Failed: Operation was cancelled before file processing".to_string());
        }

        let items = self
            .file_processor
            .process_directory_files(
                &params.path,
                file_count,
                operation_id,
                &cancel_token,
                &params.include_patterns,
                &params.exclude_patterns,
                &self.operation_manager,
            )
            .await?;

        if cancel_token.is_cancelled() {
            return Err("Failed: Operation was cancelled before semantic context creation".to_string());
        }

        let effective_embedding_type = params.embedding_type.unwrap_or(self.config.embedding_type);

        self.context_creator
            .create_context(
                &context_dir,
                &items,
                effective_embedding_type,
                operation_id,
                &cancel_token,
                &self.operation_manager,
                &*self.embedder,
                &self.context_manager,
            )
            .await?;

        self.store_context_metadata(
            &context_id,
            &params.name,
            &params.description,
            params.persistent,
            Some(params.path.to_string_lossy().to_string()),
            &params.include_patterns,
            &params.exclude_patterns,
            file_count,
            effective_embedding_type,
        )
        .await?;

        Ok(context_id)
    }

    async fn process_clear(&self, operation_id: Uuid, cancel_token: CancellationToken) {
        debug!("Processing Clear job");

        if cancel_token.is_cancelled() {
            self.mark_operation_cancelled(operation_id).await;
            return;
        }

        self.update_operation_status(operation_id, "Starting clear operation...".to_string())
            .await;

        let contexts = {
            let contexts_guard = self.context_manager.get_contexts_ref().read().await;
            contexts_guard.values().cloned().collect::<Vec<_>>()
        };

        if cancel_token.is_cancelled() {
            self.mark_operation_cancelled(operation_id).await;
            return;
        }

        self.update_operation_status(operation_id, format!("Clearing {} contexts...", contexts.len()))
            .await;

        let mut removed = 0;

        for (index, context) in contexts.iter().enumerate() {
            if cancel_token.is_cancelled() {
                self.update_operation_status(
                    operation_id,
                    format!(
                        "Operation was cancelled after clearing {} of {} contexts",
                        removed,
                        contexts.len()
                    ),
                )
                .await;
                self.mark_operation_cancelled(operation_id).await;
                return;
            }

            self.update_operation_progress(
                operation_id,
                (index + 1) as u64,
                contexts.len() as u64,
                format!(
                    "Clearing context {} of {} ({})...",
                    index + 1,
                    contexts.len(),
                    context.name
                ),
            )
            .await;

            {
                let mut contexts_guard = self.context_manager.get_contexts_ref().write().await;
                contexts_guard.remove(&context.id);
            }

            {
                let mut volatile_contexts = self.context_manager.get_volatile_contexts_ref().write().await;
                volatile_contexts.remove(&context.id);
            }

            if context.persistent {
                let context_dir = self.base_dir.join(&context.id);
                if context_dir.exists() {
                    if let Err(e) = tokio::fs::remove_dir_all(&context_dir).await {
                        tracing::warn!("Failed to remove context directory {}: {}", context_dir.display(), e);
                    }
                }
            }

            removed += 1;
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        if let Err(e) = self.context_manager.save_contexts_metadata(&self.base_dir).await {
            tracing::error!("Failed to save contexts metadata after clear: {}", e);
        }

        if cancel_token.is_cancelled() {
            self.mark_operation_cancelled(operation_id).await;
        } else {
            self.update_operation_status(operation_id, format!("Successfully cleared {} contexts", removed))
                .await;
            self.mark_operation_completed(operation_id).await;
        }
    }

    async fn update_operation_status(&self, operation_id: Uuid, message: String) {
        if let Ok(mut operations) = self.operation_manager.get_active_operations_ref().try_write() {
            if let Some(operation) = operations.get_mut(&operation_id) {
                if let Ok(mut progress) = operation.progress.try_lock() {
                    progress.message = message;
                }
            }
        }
    }

    async fn update_operation_progress(&self, operation_id: Uuid, current: u64, total: u64, message: String) {
        if let Ok(mut operations) = self.operation_manager.get_active_operations_ref().try_write() {
            if let Some(operation) = operations.get_mut(&operation_id) {
                if let Ok(mut progress) = operation.progress.try_lock() {
                    progress.update(current, total, message);
                }
            }
        }
    }

    async fn mark_operation_completed(&self, operation_id: Uuid) {
        if let Ok(mut operations) = self.operation_manager.get_active_operations_ref().try_write() {
            operations.remove(&operation_id);
        }
        debug!("Operation {} completed", operation_id);
    }

    async fn mark_operation_failed(&self, operation_id: Uuid, error: String) {
        if let Ok(mut operations) = self.operation_manager.get_active_operations_ref().try_write() {
            if let Some(operation) = operations.get_mut(&operation_id) {
                if let Ok(mut progress) = operation.progress.try_lock() {
                    progress.message = error.clone();
                }
            }
        }
        tracing::error!("Operation {} failed: {}", operation_id, error);
    }

    async fn mark_operation_cancelled(&self, operation_id: Uuid) {
        if let Ok(mut operations) = self.operation_manager.get_active_operations_ref().try_write() {
            if let Some(operation) = operations.get_mut(&operation_id) {
                if let Ok(mut progress) = operation.progress.try_lock() {
                    progress.message = "Operation cancelled by user".to_string();
                    progress.current = 0;
                    progress.total = 0;
                }
            }
        }
        debug!("Operation {} cancelled", operation_id);
    }

    #[allow(clippy::too_many_arguments)]
    async fn store_context_metadata(
        &self,
        context_id: &str,
        name: &str,
        description: &str,
        persistent: bool,
        source_path: Option<String>,
        include_patterns: &Option<Vec<String>>,
        exclude_patterns: &Option<Vec<String>>,
        item_count: usize,
        embedding_type: crate::embedding::EmbeddingType,
    ) -> std::result::Result<(), String> {
        let context = KnowledgeContext::new(
            context_id.to_string(),
            name,
            description,
            persistent,
            source_path,
            (
                include_patterns.as_deref().unwrap_or(&[]).to_vec(),
                exclude_patterns.as_deref().unwrap_or(&[]).to_vec(),
            ),
            item_count,
            embedding_type,
        );

        {
            let mut contexts = self.context_manager.get_contexts_ref().write().await;
            contexts.insert(context_id.to_string(), context);
        }

        if persistent {
            self.context_manager
                .save_contexts_metadata(&self.base_dir)
                .await
                .map_err(|e| format!("Failed to save contexts metadata: {}", e))?;
        }

        Ok(())
    }
}
