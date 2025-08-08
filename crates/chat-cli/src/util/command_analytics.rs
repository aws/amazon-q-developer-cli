use std::collections::HashMap;

use chrono::{
    DateTime,
    Duration,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};
use uuid::Uuid;

use crate::util::command_types::CommandScope;

/// Usage statistics for a specific command
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandUsageStats {
    pub command_name: String,
    pub command_scope: CommandScope,
    pub execution_count: u64,
    pub total_execution_time: Duration,
    pub average_execution_time: Duration,
    pub last_executed: Option<DateTime<Utc>>,
    pub success_count: u64,
    pub error_count: u64,
    pub parameter_usage: HashMap<String, ParameterUsageStats>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl CommandUsageStats {
    #[allow(dead_code)]
    pub fn new(command_name: String, command_scope: CommandScope) -> Self {
        let now = Utc::now();
        Self {
            command_name,
            command_scope,
            execution_count: 0,
            total_execution_time: Duration::zero(),
            average_execution_time: Duration::zero(),
            last_executed: None,
            success_count: 0,
            error_count: 0,
            parameter_usage: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn update_execution(
        &mut self,
        duration: Duration,
        success: bool,
        parameters: &HashMap<String, serde_json::Value>,
    ) {
        self.execution_count += 1;
        self.total_execution_time += duration;
        self.average_execution_time = Duration::nanoseconds(
            self.total_execution_time.num_nanoseconds().unwrap_or(0) / self.execution_count as i64,
        );
        self.last_executed = Some(Utc::now());
        self.updated_at = Utc::now();

        if success {
            self.success_count += 1;
        } else {
            self.error_count += 1;
        }

        // Update parameter usage statistics
        for (param_name, param_value) in parameters {
            let param_stats = self
                .parameter_usage
                .entry(param_name.clone())
                .or_insert_with(|| ParameterUsageStats::new(param_name.clone()));
            param_stats.update_usage(param_value);
        }
    }

    #[allow(dead_code)]
    pub fn success_rate(&self) -> f64 {
        if self.execution_count == 0 {
            0.0
        } else {
            self.success_count as f64 / self.execution_count as f64
        }
    }
}

/// Usage statistics for a specific parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterUsageStats {
    pub parameter_name: String,
    pub usage_count: u64,
    pub common_values: HashMap<String, u64>,
    pub default_usage_count: u64,
}

impl ParameterUsageStats {
    pub fn new(parameter_name: String) -> Self {
        Self {
            parameter_name,
            usage_count: 0,
            common_values: HashMap::new(),
            default_usage_count: 0,
        }
    }

    pub fn update_usage(&mut self, value: &serde_json::Value) {
        self.usage_count += 1;

        // Track anonymized value types for privacy
        let value_type = match value {
            serde_json::Value::String(_) => "<string>",
            serde_json::Value::Number(_) => "<number>",
            serde_json::Value::Bool(_) => "<boolean>",
            serde_json::Value::Array(_) => "<array>",
            serde_json::Value::Object(_) => "<object>",
            serde_json::Value::Null => "<null>",
        };

        *self.common_values.entry(value_type.to_string()).or_insert(0) += 1;
    }
}

/// Metrics for a single command execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetrics {
    pub command_name: String,
    pub execution_id: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration: Option<Duration>,
    pub success: bool,
    pub error_type: Option<String>,
    pub parameters_used: HashMap<String, serde_json::Value>,
    pub user_id: Option<String>, // Always None for privacy
}

impl ExecutionMetrics {
    #[allow(dead_code)]
    pub fn new(command_name: String, parameters: HashMap<String, serde_json::Value>) -> Self {
        Self {
            command_name,
            execution_id: Uuid::new_v4().to_string(),
            started_at: Utc::now(),
            completed_at: None,
            duration: None,
            success: false,
            error_type: None,
            parameters_used: parameters,
            user_id: None, // Never track user IDs for privacy
        }
    }

    #[allow(dead_code)]
    pub fn complete(&mut self, success: bool, error_type: Option<String>) {
        self.completed_at = Some(Utc::now());
        self.duration = Some(self.completed_at.unwrap() - self.started_at);
        self.success = success;
        self.error_type = error_type;
    }
}

/// System-wide metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub total_commands: u64,
    pub total_executions: u64,
    pub average_execution_time: Duration,
    pub most_used_commands: Vec<(String, u64)>,
    pub error_rate: f64,
    pub uptime: Duration,
    pub cache_hit_rate: f64,
}

impl SystemMetrics {
    pub fn new() -> Self {
        Self {
            total_commands: 0,
            total_executions: 0,
            average_execution_time: Duration::zero(),
            most_used_commands: Vec::new(),
            error_rate: 0.0,
            uptime: Duration::zero(),
            cache_hit_rate: 0.0,
        }
    }
}

impl Default for SystemMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Analytics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsConfig {
    pub enabled: bool,
    pub privacy_mode: bool,
    pub buffer_size: usize,
    pub retention_days: u32,
    pub storage_path: std::path::PathBuf,
}

impl Default for AnalyticsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            privacy_mode: true,
            buffer_size: 100,
            retention_days: 30,
            storage_path: std::path::PathBuf::from(".amazonq/analytics"),
        }
    }
}

/// Analytics errors
#[allow(dead_code)]
#[derive(Debug, thiserror::Error)]
pub enum AnalyticsError {
    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Analytics disabled")]
    Disabled,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_command_usage_stats_creation() {
        let stats = CommandUsageStats::new("test-command".to_string(), CommandScope::Project);

        assert_eq!(stats.command_name, "test-command");
        assert_eq!(stats.execution_count, 0);
        assert_eq!(stats.success_count, 0);
        assert_eq!(stats.error_count, 0);
        assert!(stats.parameter_usage.is_empty());
    }

    #[test]
    fn test_command_usage_stats_update() {
        let mut stats = CommandUsageStats::new("test-command".to_string(), CommandScope::Project);
        let parameters = HashMap::from([
            ("param1".to_string(), json!("value1")),
            ("param2".to_string(), json!(42)),
        ]);

        stats.update_execution(Duration::milliseconds(100), true, &parameters);

        assert_eq!(stats.execution_count, 1);
        assert_eq!(stats.success_count, 1);
        assert_eq!(stats.error_count, 0);
        assert_eq!(stats.total_execution_time, Duration::milliseconds(100));
        assert_eq!(stats.average_execution_time, Duration::milliseconds(100));
        assert!(stats.last_executed.is_some());
        assert_eq!(stats.parameter_usage.len(), 2);
    }

    #[test]
    fn test_success_rate_calculation() {
        let mut stats = CommandUsageStats::new("test-command".to_string(), CommandScope::Project);
        let parameters = HashMap::new();

        // No executions
        assert_eq!(stats.success_rate(), 0.0);

        // One success
        stats.update_execution(Duration::milliseconds(100), true, &parameters);
        assert_eq!(stats.success_rate(), 1.0);

        // One failure
        stats.update_execution(Duration::milliseconds(100), false, &parameters);
        assert_eq!(stats.success_rate(), 0.5);
    }

    #[test]
    fn test_parameter_usage_stats() {
        let mut param_stats = ParameterUsageStats::new("test-param".to_string());

        param_stats.update_usage(&json!("string_value"));
        param_stats.update_usage(&json!(42));
        param_stats.update_usage(&json!("another_string"));

        assert_eq!(param_stats.usage_count, 3);
        assert_eq!(param_stats.common_values.get("<string>"), Some(&2));
        assert_eq!(param_stats.common_values.get("<number>"), Some(&1));
    }

    #[test]
    fn test_execution_metrics() {
        let parameters = HashMap::from([("param1".to_string(), json!("value1"))]);

        let mut metrics = ExecutionMetrics::new("test-command".to_string(), parameters);

        assert_eq!(metrics.command_name, "test-command");
        assert!(metrics.completed_at.is_none());
        assert!(metrics.duration.is_none());
        assert!(!metrics.success);
        assert!(metrics.user_id.is_none()); // Privacy check

        metrics.complete(true, None);

        assert!(metrics.completed_at.is_some());
        assert!(metrics.duration.is_some());
        assert!(metrics.success);
    }

    #[test]
    fn test_analytics_config_default() {
        let config = AnalyticsConfig::default();

        assert!(config.enabled);
        assert!(config.privacy_mode);
        assert_eq!(config.buffer_size, 100);
        assert_eq!(config.retention_days, 30);
    }
}
