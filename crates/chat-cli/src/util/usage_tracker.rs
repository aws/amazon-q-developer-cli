use std::collections::HashMap;
use std::sync::{
    Arc,
    Mutex,
};

use chrono::{
    DateTime,
    Utc,
};

use crate::util::command_analytics::{
    AnalyticsConfig,
    AnalyticsError,
    CommandUsageStats,
    ExecutionMetrics,
    SystemMetrics,
};

/// Trait for analytics storage backends
pub trait StatsStorage: Send + Sync {
    fn store_metrics(&self, metrics: &[ExecutionMetrics]) -> Result<(), AnalyticsError>;
    fn get_command_stats(&self, command_name: &str) -> Result<CommandUsageStats, AnalyticsError>;
    #[allow(dead_code)]
    fn get_system_metrics(&self) -> Result<SystemMetrics, AnalyticsError>;
    fn update_command_stats(&self, stats: &CommandUsageStats) -> Result<(), AnalyticsError>;
    #[allow(dead_code)]
    fn cleanup_old_metrics(&self, older_than: DateTime<Utc>) -> Result<(), AnalyticsError>;
    #[allow(dead_code)]
    fn list_all_command_stats(&self) -> Result<Vec<CommandUsageStats>, AnalyticsError>;
}

/// Main usage tracker for collecting command analytics
pub struct UsageTracker {
    storage: Arc<dyn StatsStorage>,
    config: AnalyticsConfig,
    #[allow(dead_code)]
    active_executions: Arc<Mutex<HashMap<String, ExecutionMetrics>>>,
    metrics_buffer: Arc<Mutex<Vec<ExecutionMetrics>>>,
}

#[allow(dead_code)]
impl UsageTracker {
    #[allow(dead_code)]
    pub fn new(storage: Arc<dyn StatsStorage>, config: AnalyticsConfig) -> Self {
        Self {
            storage,
            config,
            active_executions: Arc::new(Mutex::new(HashMap::new())),
            metrics_buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Start tracking a command execution
    pub fn track_execution_start(
        &self,
        command_name: &str,
        parameters: &HashMap<String, serde_json::Value>,
    ) -> Result<String, AnalyticsError> {
        if !self.config.enabled {
            return Err(AnalyticsError::Disabled);
        }

        let anonymized_params = if self.config.privacy_mode {
            Self::anonymize_parameters(parameters)
        } else {
            parameters.clone()
        };

        let metrics = ExecutionMetrics::new(command_name.to_string(), anonymized_params);
        let execution_id = metrics.execution_id.clone();

        // Store in active executions
        {
            let mut active = self
                .active_executions
                .lock()
                .map_err(|e| AnalyticsError::Storage(format!("Failed to acquire lock: {}", e)))?;
            active.insert(execution_id.clone(), metrics);
        }

        Ok(execution_id)
    }

    /// Complete tracking for a command execution
    pub fn track_execution_end(
        &self,
        execution_id: &str,
        success: bool,
        error_type: Option<String>,
    ) -> Result<(), AnalyticsError> {
        if !self.config.enabled {
            return Ok(()); // Silently ignore if disabled
        }

        // Remove from active executions and complete the metrics
        let completed_metrics = {
            let mut active = self
                .active_executions
                .lock()
                .map_err(|e| AnalyticsError::Storage(format!("Failed to acquire lock: {}", e)))?;

            if let Some(mut metrics) = active.remove(execution_id) {
                metrics.complete(success, error_type);
                Some(metrics)
            } else {
                None
            }
        };

        if let Some(metrics) = completed_metrics {
            // Add to buffer
            {
                let mut buffer = self
                    .metrics_buffer
                    .lock()
                    .map_err(|e| AnalyticsError::Storage(format!("Failed to acquire lock: {}", e)))?;
                buffer.push(metrics);

                // Flush if buffer is full
                if buffer.len() >= self.config.buffer_size {
                    let metrics_to_flush = buffer.drain(..).collect::<Vec<_>>();
                    drop(buffer); // Release lock before storage operation
                    self.flush_metrics_internal(&metrics_to_flush)?;
                }
            }
        }

        Ok(())
    }

    /// Get usage statistics for a specific command
    pub fn get_command_stats(&self, command_name: &str) -> Result<CommandUsageStats, AnalyticsError> {
        if !self.config.enabled {
            return Err(AnalyticsError::Disabled);
        }

        self.storage.get_command_stats(command_name)
    }

    /// Get system-wide metrics
    pub fn get_system_metrics(&self) -> Result<SystemMetrics, AnalyticsError> {
        if !self.config.enabled {
            return Err(AnalyticsError::Disabled);
        }

        self.storage.get_system_metrics()
    }

    /// Get all command statistics
    pub fn get_all_command_stats(&self) -> Result<Vec<CommandUsageStats>, AnalyticsError> {
        if !self.config.enabled {
            return Err(AnalyticsError::Disabled);
        }

        self.storage.list_all_command_stats()
    }

    /// Manually flush all buffered metrics
    pub fn flush_metrics(&self) -> Result<(), AnalyticsError> {
        if !self.config.enabled {
            return Ok(());
        }

        let metrics_to_flush = {
            let mut buffer = self
                .metrics_buffer
                .lock()
                .map_err(|e| AnalyticsError::Storage(format!("Failed to acquire lock: {}", e)))?;
            buffer.drain(..).collect::<Vec<_>>()
        };

        if !metrics_to_flush.is_empty() {
            self.flush_metrics_internal(&metrics_to_flush)?;
        }

        Ok(())
    }

    /// Clean up old metrics based on retention policy
    pub fn cleanup_old_metrics(&self) -> Result<(), AnalyticsError> {
        if !self.config.enabled {
            return Ok(());
        }

        let cutoff_date = Utc::now() - chrono::Duration::days(self.config.retention_days as i64);
        self.storage.cleanup_old_metrics(cutoff_date)
    }

    /// Update analytics configuration
    pub fn update_config(&mut self, config: AnalyticsConfig) {
        self.config = config;
    }

    /// Check if analytics is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Check if privacy mode is enabled
    pub fn is_privacy_mode(&self) -> bool {
        self.config.privacy_mode
    }

    /// Anonymize parameters for privacy protection
    fn anonymize_parameters(parameters: &HashMap<String, serde_json::Value>) -> HashMap<String, serde_json::Value> {
        parameters
            .iter()
            .map(|(key, value)| {
                let anonymized_value = match value {
                    serde_json::Value::String(_) => serde_json::Value::String("<string>".to_string()),
                    serde_json::Value::Number(_) => serde_json::Value::String("<number>".to_string()),
                    serde_json::Value::Bool(_) => serde_json::Value::String("<boolean>".to_string()),
                    serde_json::Value::Array(_) => serde_json::Value::String("<array>".to_string()),
                    serde_json::Value::Object(_) => serde_json::Value::String("<object>".to_string()),
                    serde_json::Value::Null => serde_json::Value::String("<null>".to_string()),
                };
                (key.clone(), anonymized_value)
            })
            .collect()
    }

    /// Internal method to flush metrics to storage
    fn flush_metrics_internal(&self, metrics: &[ExecutionMetrics]) -> Result<(), AnalyticsError> {
        // Store raw metrics
        self.storage.store_metrics(metrics)?;

        // Update aggregated command statistics
        for metric in metrics {
            if let Some(duration) = metric.duration {
                // Try to get existing stats or create new ones
                let mut stats = self
                    .storage
                    .get_command_stats(&metric.command_name)
                    .unwrap_or_else(|_| {
                        use crate::util::command_types::CommandScope;
                        CommandUsageStats::new(metric.command_name.clone(), CommandScope::Project)
                    });

                stats.update_execution(duration, metric.success, &metric.parameters_used);
                self.storage.update_command_stats(&stats)?;
            }
        }

        Ok(())
    }
}

impl Drop for UsageTracker {
    fn drop(&mut self) {
        // Attempt to flush remaining metrics on drop
        if let Err(e) = self.flush_metrics() {
            eprintln!("Warning: Failed to flush analytics metrics on drop: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::util::command_analytics::{
        CommandUsageStats,
        SystemMetrics,
    };

    // Mock storage for testing
    struct MockStorage {
        command_stats: Arc<Mutex<HashMap<String, CommandUsageStats>>>,
        metrics: Arc<Mutex<Vec<ExecutionMetrics>>>,
    }

    impl MockStorage {
        fn new() -> Self {
            Self {
                command_stats: Arc::new(Mutex::new(HashMap::new())),
                metrics: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl StatsStorage for MockStorage {
        fn store_metrics(&self, metrics: &[ExecutionMetrics]) -> Result<(), AnalyticsError> {
            let mut stored_metrics = self.metrics.lock().unwrap();
            stored_metrics.extend_from_slice(metrics);
            Ok(())
        }

        fn get_command_stats(&self, command_name: &str) -> Result<CommandUsageStats, AnalyticsError> {
            let stats = self.command_stats.lock().unwrap();
            stats
                .get(command_name)
                .cloned()
                .ok_or_else(|| AnalyticsError::Storage("Command not found".to_string()))
        }

        fn get_system_metrics(&self) -> Result<SystemMetrics, AnalyticsError> {
            Ok(SystemMetrics::new())
        }

        fn update_command_stats(&self, stats: &CommandUsageStats) -> Result<(), AnalyticsError> {
            let mut command_stats = self.command_stats.lock().unwrap();
            command_stats.insert(stats.command_name.clone(), stats.clone());
            Ok(())
        }

        fn cleanup_old_metrics(&self, _older_than: DateTime<Utc>) -> Result<(), AnalyticsError> {
            Ok(())
        }

        fn list_all_command_stats(&self) -> Result<Vec<CommandUsageStats>, AnalyticsError> {
            let stats = self.command_stats.lock().unwrap();
            Ok(stats.values().cloned().collect())
        }
    }

    #[test]
    fn test_usage_tracker_creation() {
        let storage = Arc::new(MockStorage::new());
        let config = AnalyticsConfig::default();
        let tracker = UsageTracker::new(storage, config);

        assert!(tracker.is_enabled());
        assert!(tracker.is_privacy_mode());
    }

    #[test]
    fn test_track_execution_lifecycle() {
        let storage = Arc::new(MockStorage::new());
        let config = AnalyticsConfig::default();
        let tracker = UsageTracker::new(storage, config);

        let parameters = HashMap::from([("param1".to_string(), serde_json::json!("value1"))]);

        // Start tracking
        let execution_id = tracker.track_execution_start("test-command", &parameters).unwrap();
        assert!(!execution_id.is_empty());

        // End tracking
        tracker.track_execution_end(&execution_id, true, None).unwrap();

        // Flush to ensure metrics are processed
        tracker.flush_metrics().unwrap();
    }

    #[test]
    fn test_disabled_analytics() {
        let storage = Arc::new(MockStorage::new());
        let config = AnalyticsConfig {
            enabled: false,
            ..Default::default()
        };
        let tracker = UsageTracker::new(storage, config);

        let parameters = HashMap::new();

        // Should return error when disabled
        assert!(tracker.track_execution_start("test-command", &parameters).is_err());
        assert!(tracker.get_system_metrics().is_err());
    }

    #[test]
    fn test_parameter_anonymization() {
        let storage = Arc::new(MockStorage::new());
        let config = AnalyticsConfig::default(); // privacy_mode = true by default
        let _tracker = UsageTracker::new(storage, config);

        let parameters = HashMap::from([
            ("string_param".to_string(), serde_json::json!("sensitive_data")),
            ("number_param".to_string(), serde_json::json!(42)),
            ("bool_param".to_string(), serde_json::json!(true)),
        ]);

        let anonymized = UsageTracker::anonymize_parameters(&parameters);

        assert_eq!(anonymized.get("string_param"), Some(&serde_json::json!("<string>")));
        assert_eq!(anonymized.get("number_param"), Some(&serde_json::json!("<number>")));
        assert_eq!(anonymized.get("bool_param"), Some(&serde_json::json!("<boolean>")));
    }

    #[test]
    fn test_buffer_flushing() {
        let storage = Arc::new(MockStorage::new());
        let config = AnalyticsConfig {
            buffer_size: 2, // Small buffer for testing
            ..Default::default()
        };
        let tracker = UsageTracker::new(storage.clone(), config);

        let parameters = HashMap::new();

        // Execute multiple commands to trigger buffer flush
        for i in 0..3 {
            let execution_id = tracker
                .track_execution_start(&format!("command-{}", i), &parameters)
                .unwrap();
            tracker.track_execution_end(&execution_id, true, None).unwrap();
        }

        // Check that metrics were stored
        let stored_metrics = storage.metrics.lock().unwrap();
        assert!(!stored_metrics.is_empty());
    }
}
