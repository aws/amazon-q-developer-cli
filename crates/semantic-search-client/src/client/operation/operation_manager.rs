use std::collections::HashMap;
use std::sync::Arc;
use std::time::{
    Duration,
    SystemTime,
};

use tokio::sync::{
    Mutex,
    RwLock,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::super::context::ContextManager;
use crate::error::{
    Result,
    SemanticSearchError,
};
use crate::types::*;

const MAX_CONCURRENT_OPERATIONS: usize = 3;

#[derive(Clone)]
/// Operation manager for tracking operations
pub struct OperationManager {
    active_operations: Arc<RwLock<HashMap<Uuid, OperationHandle>>>,
}

impl Default for OperationManager {
    fn default() -> Self {
        Self::new()
    }
}

impl OperationManager {
    /// Create new operation manager
    pub fn new() -> Self {
        Self {
            active_operations: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get active operations (for checking duplicates)
    pub fn get_active_operations(&self) -> &Arc<RwLock<HashMap<Uuid, OperationHandle>>> {
        &self.active_operations
    }

    /// Register operation
    pub async fn register_operation(
        &self,
        operation_id: Uuid,
        operation_type: OperationType,
        cancel_token: CancellationToken,
    ) {
        let handle = OperationHandle {
            operation_type,
            started_at: SystemTime::now(),
            progress: Arc::new(Mutex::new(ProgressInfo::new())),
            cancel_token,
            task_handle: None,
        };

        let mut operations = self.active_operations.write().await;
        operations.insert(operation_id, handle);
    }

    /// Cancel operation
    pub async fn cancel_operation(&self, operation_id: Uuid) -> Result<String> {
        let mut operations = self.active_operations.write().await;

        if let Some(handle) = operations.get_mut(&operation_id) {
            handle.cancel_token.cancel();

            if let Some(task_handle) = &handle.task_handle {
                task_handle.abort();
            }

            let op_type = handle.operation_type.display_name();
            let id_display = &operation_id.to_string()[..8];

            if let Ok(mut progress) = handle.progress.try_lock() {
                progress.message = "Operation cancelled by user".to_string();
            }

            Ok(format!("✅ Cancelled operation: {} (ID: {})", op_type, id_display))
        } else {
            Err(SemanticSearchError::OperationFailed(format!(
                "Operation not found: {}",
                &operation_id.to_string()[..8]
            )))
        }
    }

    /// Cancel the most recent operation
    pub async fn cancel_most_recent_operation(&self) -> Result<String> {
        let operations = self.active_operations.read().await;

        if operations.is_empty() {
            return Ok("No active operations to cancel".to_string());
        }

        // Find the most recent operation (highest started_at time)
        let most_recent = operations
            .iter()
            .max_by_key(|(_, handle)| handle.started_at)
            .map(|(id, _)| *id);

        drop(operations); // Release the read lock

        if let Some(operation_id) = most_recent {
            self.cancel_operation(operation_id).await
        } else {
            Err(SemanticSearchError::OperationFailed(
                "No active operations found".to_string(),
            ))
        }
    }

    /// Cancel all operations
    pub async fn cancel_all_operations(&self) -> Result<String> {
        let mut operations = self.active_operations.write().await;
        let count = operations.len();

        if count == 0 {
            return Ok("No active operations to cancel".to_string());
        }

        for handle in operations.values_mut() {
            handle.cancel_token.cancel();

            if let Some(task_handle) = &handle.task_handle {
                task_handle.abort();
            }

            if let Ok(mut progress) = handle.progress.try_lock() {
                progress.message = "Operation cancelled by user".to_string();
                progress.current = 0;
                progress.total = 0;
            }
        }

        Ok(format!("✅ Cancelled {} active operations", count))
    }

    /// Find operation by short ID
    pub async fn find_operation_by_short_id(&self, short_id: &str) -> Option<Uuid> {
        let operations = self.active_operations.read().await;
        operations
            .iter()
            .find(|(id, _)| id.to_string().starts_with(short_id))
            .map(|(id, _)| *id)
    }

    /// List operation IDs
    pub async fn list_operation_ids(&self) -> Vec<String> {
        let operations = self.active_operations.read().await;
        operations
            .iter()
            .map(|(id, _)| format!("{} (short: {})", id, &id.to_string()[..8]))
            .collect()
    }

    /// Get status data
    pub async fn get_status_data(&self, context_manager: &ContextManager) -> Result<SystemStatus> {
        let mut operations = self.active_operations.write().await;
        let contexts = context_manager.get_contexts_ref().read().await;

        // Clean up old cancelled operations
        let now = SystemTime::now();
        let cleanup_threshold = Duration::from_secs(30);

        operations.retain(|_, handle| {
            if let Ok(progress) = handle.progress.try_lock() {
                let is_cancelled = progress.message.to_lowercase().contains("cancelled");
                let is_failed = progress.message.to_lowercase().contains("failed");
                if is_cancelled || is_failed {
                    now.duration_since(handle.started_at).unwrap_or_default() < cleanup_threshold
                } else {
                    true
                }
            } else {
                true
            }
        });

        // Collect context information
        let total_contexts = contexts.len();
        let persistent_contexts = contexts.values().filter(|c| c.persistent).count();
        let volatile_contexts = total_contexts - persistent_contexts;

        // Collect operation information
        let mut operation_statuses = Vec::new();
        let mut active_count = 0;
        let mut waiting_count = 0;

        for (id, handle) in operations.iter() {
            if let Ok(progress) = handle.progress.try_lock() {
                let is_failed = progress.message.to_lowercase().contains("failed");
                let is_cancelled = progress.message.to_lowercase().contains("cancelled");
                let is_waiting = Self::is_operation_waiting(&progress);

                if is_cancelled {
                    // Don't count cancelled operations
                } else if is_failed || is_waiting {
                    waiting_count += 1;
                } else {
                    active_count += 1;
                }

                let operation_status = OperationStatus {
                    id: id.to_string(),
                    short_id: id.to_string()[..8].to_string(),
                    operation_type: handle.operation_type.clone(),
                    started_at: handle.started_at,
                    current: progress.current,
                    total: progress.total,
                    message: progress.message.clone(),
                    is_cancelled,
                    is_failed,
                    is_waiting,
                    eta: progress.calculate_eta(),
                };

                operation_statuses.push(operation_status);
            }
        }

        Ok(SystemStatus {
            total_contexts,
            persistent_contexts,
            volatile_contexts,
            operations: operation_statuses,
            active_count,
            waiting_count,
            max_concurrent: MAX_CONCURRENT_OPERATIONS,
        })
    }

    fn is_operation_waiting(progress: &ProgressInfo) -> bool {
        progress.message.contains("Waiting")
            || progress.message.contains("queue")
            || progress.message.contains("slot")
            || progress.message.contains("write access")
            || progress.message.contains("Initializing")
            || progress.message.contains("Starting")
            || (progress.current == 0 && progress.total == 0 && !progress.message.contains("complete"))
    }

    /// Get active operations reference
    pub fn get_active_operations_ref(&self) -> &Arc<RwLock<HashMap<Uuid, OperationHandle>>> {
        &self.active_operations
    }
}
