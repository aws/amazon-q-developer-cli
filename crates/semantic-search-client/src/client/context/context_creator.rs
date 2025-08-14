use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::super::operation::OperationManager;
use super::context_manager::ContextManager;
use super::{
    BM25Context,
    SemanticContext,
};
use crate::embedding::{
    EmbeddingType,
    TextEmbedderTrait,
};
use crate::error::Result;
use crate::types::{
    BM25DataPoint,
    DataPoint,
};

/// Context creator utility
pub struct ContextCreator;

impl Default for ContextCreator {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextCreator {
    /// Create new context creator
    pub fn new() -> Self {
        Self
    }

    /// Create context
    #[allow(clippy::too_many_arguments)]
    pub async fn create_context(
        &self,
        context_dir: &Path,
        items: &[serde_json::Value],
        embedding_type: EmbeddingType,
        operation_id: Uuid,
        cancel_token: &CancellationToken,
        operation_manager: &OperationManager,
        embedder: &dyn TextEmbedderTrait,
        context_manager: &ContextManager,
    ) -> std::result::Result<(), String> {
        if embedding_type.is_bm25() {
            self.create_bm25_context(
                context_dir,
                items,
                operation_id,
                cancel_token,
                operation_manager,
                context_manager,
            )
            .await
        } else {
            self.create_semantic_context(
                context_dir,
                items,
                operation_id,
                cancel_token,
                operation_manager,
                embedder,
                context_manager,
            )
            .await
        }
    }

    async fn create_bm25_context(
        &self,
        context_dir: &Path,
        items: &[serde_json::Value],
        operation_id: Uuid,
        cancel_token: &CancellationToken,
        operation_manager: &OperationManager,
        context_manager: &ContextManager,
    ) -> std::result::Result<(), String> {
        self.update_operation_status(operation_manager, operation_id, "Creating BM25 context...".to_string())
            .await;

        if cancel_token.is_cancelled() {
            return Err("Operation was cancelled during BM25 context creation".to_string());
        }

        let mut bm25_context = BM25Context::new(context_dir.join("data.bm25.json"), 5.0)
            .map_err(|e| format!("Failed to create BM25 context: {}", e))?;

        let mut data_points = Vec::new();
        let total_items = items.len();

        for (i, item) in items.iter().enumerate() {
            if cancel_token.is_cancelled() {
                return Err("Operation was cancelled during BM25 data point creation".to_string());
            }

            if i % 10 == 0 {
                self.update_operation_progress(
                    operation_manager,
                    operation_id,
                    i as u64,
                    total_items as u64,
                    format!("Creating BM25 data points ({}/{})", i, total_items),
                )
                .await;
            }

            let data_point = Self::create_bm25_data_point_from_item(item, i)
                .map_err(|e| format!("Failed to create BM25 data point: {}", e))?;
            data_points.push(data_point);
        }

        if cancel_token.is_cancelled() {
            return Err("Operation was cancelled before building BM25 index".to_string());
        }

        self.update_operation_status(operation_manager, operation_id, "Building BM25 index...".to_string())
            .await;

        bm25_context
            .add_data_points(data_points)
            .map_err(|e| format!("Failed to add BM25 data points: {}", e))?;

        let _ = bm25_context.save();

        // Store the BM25 context
        let context_id = context_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        {
            let mut bm25_contexts = context_manager.get_bm25_contexts_ref().write().await;
            bm25_contexts.insert(context_id, Arc::new(Mutex::new(bm25_context)));
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn create_semantic_context(
        &self,
        context_dir: &Path,
        items: &[serde_json::Value],
        operation_id: Uuid,
        cancel_token: &CancellationToken,
        operation_manager: &OperationManager,
        embedder: &dyn TextEmbedderTrait,
        context_manager: &ContextManager,
    ) -> std::result::Result<(), String> {
        self.update_operation_status(
            operation_manager,
            operation_id,
            "Creating semantic context...".to_string(),
        )
        .await;

        if cancel_token.is_cancelled() {
            return Err("Operation was cancelled during semantic context creation".to_string());
        }

        let mut semantic_context = SemanticContext::new(context_dir.join("data.json"))
            .map_err(|e| format!("Failed to create semantic context: {}", e))?;

        let mut data_points = Vec::new();
        let total_items = items.len();

        for (i, item) in items.iter().enumerate() {
            if cancel_token.is_cancelled() {
                return Err("Operation was cancelled during embedding generation".to_string());
            }

            if i % 10 == 0 {
                self.update_operation_progress(
                    operation_manager,
                    operation_id,
                    i as u64,
                    total_items as u64,
                    format!("Generating embeddings ({}/{})", i, total_items),
                )
                .await;
            }

            let data_point = Self::create_data_point_from_item(item, i, embedder)
                .map_err(|e| format!("Failed to create data point: {}", e))?;
            data_points.push(data_point);
        }

        if cancel_token.is_cancelled() {
            return Err("Operation was cancelled before building index".to_string());
        }

        self.update_operation_status(operation_manager, operation_id, "Building vector index...".to_string())
            .await;

        semantic_context
            .add_data_points(data_points)
            .map_err(|e| format!("Failed to add data points: {}", e))?;

        //Persist context.
        let _ = semantic_context.save();

        // Store the semantic context
        let context_id = context_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        {
            let mut volatile_contexts = context_manager.get_volatile_contexts_ref().write().await;
            volatile_contexts.insert(context_id, Arc::new(Mutex::new(semantic_context)));
        }

        Ok(())
    }

    fn create_bm25_data_point_from_item(item: &serde_json::Value, id: usize) -> Result<BM25DataPoint> {
        let text = item.get("text").and_then(|v| v.as_str()).unwrap_or("");

        let payload: HashMap<String, serde_json::Value> = if let serde_json::Value::Object(map) = item {
            map.clone().into_iter().collect()
        } else {
            let mut map = HashMap::new();
            map.insert("text".to_string(), serde_json::Value::String(text.to_string()));
            map
        };

        Ok(BM25DataPoint {
            id,
            payload,
            content: text.to_string(),
        })
    }

    fn create_data_point_from_item(
        item: &serde_json::Value,
        id: usize,
        embedder: &dyn TextEmbedderTrait,
    ) -> Result<DataPoint> {
        let text = item.get("text").and_then(|v| v.as_str()).unwrap_or("");
        let vector = embedder.embed(text)?;

        let payload: HashMap<String, serde_json::Value> = if let serde_json::Value::Object(map) = item {
            map.clone().into_iter().collect()
        } else {
            let mut map = HashMap::new();
            map.insert("text".to_string(), item.clone());
            map
        };

        Ok(DataPoint { id, payload, vector })
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
