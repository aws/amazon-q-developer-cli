use std::collections::HashMap;
use std::fs;
use std::path::{
    Path,
    PathBuf,
};

use chrono::{
    DateTime,
    Utc,
};
use serde_json;

use crate::util::command_analytics::{
    AnalyticsError,
    CommandUsageStats,
    ExecutionMetrics,
    SystemMetrics,
};
use crate::util::usage_tracker::StatsStorage;

/// File-based storage implementation for analytics data
pub struct FileStatsStorage {
    stats_dir: PathBuf,
    metrics_dir: PathBuf,
    system_metrics_file: PathBuf,
}

impl FileStatsStorage {
    #[allow(dead_code)]
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Result<Self, AnalyticsError> {
        let base_path = base_dir.as_ref();
        let stats_dir = base_path.join("command_stats");
        let metrics_dir = base_path.join("execution_metrics");
        let system_metrics_file = base_path.join("system_metrics.json");

        // Create directories if they don't exist
        fs::create_dir_all(&stats_dir)?;
        fs::create_dir_all(&metrics_dir)?;
        fs::create_dir_all(base_path)?;

        Ok(Self {
            stats_dir,
            metrics_dir,
            system_metrics_file,
        })
    }

    /// Get the file path for a command's statistics
    fn command_stats_file(&self, command_name: &str) -> PathBuf {
        // Sanitize command name for filesystem
        let safe_name = command_name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();
        self.stats_dir.join(format!("{}.json", safe_name))
    }

    /// Get the file path for execution metrics (organized by date)
    fn metrics_file(&self, date: DateTime<Utc>) -> PathBuf {
        let date_str = date.format("%Y-%m-%d").to_string();
        self.metrics_dir.join(format!("metrics_{}.jsonl", date_str))
    }

    /// Load command statistics from file
    fn load_command_stats(&self, command_name: &str) -> Result<CommandUsageStats, AnalyticsError> {
        let file_path = self.command_stats_file(command_name);

        if !file_path.exists() {
            return Err(AnalyticsError::Storage("Command stats not found".to_string()));
        }

        let content = fs::read_to_string(&file_path)?;
        let stats: CommandUsageStats = serde_json::from_str(&content)?;
        Ok(stats)
    }

    /// Save command statistics to file
    fn save_command_stats(&self, stats: &CommandUsageStats) -> Result<(), AnalyticsError> {
        let file_path = self.command_stats_file(&stats.command_name);
        let content = serde_json::to_string_pretty(stats)?;
        fs::write(&file_path, content)?;
        Ok(())
    }

    /// Append execution metrics to daily log file
    fn append_metrics(&self, metrics: &[ExecutionMetrics]) -> Result<(), AnalyticsError> {
        for metric in metrics {
            let file_path = self.metrics_file(metric.started_at);
            let line = serde_json::to_string(metric)?;

            // Append to file (create if doesn't exist)
            let mut content = if file_path.exists() {
                fs::read_to_string(&file_path)?
            } else {
                String::new()
            };

            if !content.is_empty() && !content.ends_with('\n') {
                content.push('\n');
            }
            content.push_str(&line);
            content.push('\n');

            fs::write(&file_path, content)?;
        }
        Ok(())
    }

    /// Calculate system metrics from all stored data
    fn calculate_system_metrics(&self) -> Result<SystemMetrics, AnalyticsError> {
        let mut total_commands = 0u64;
        let mut total_executions = 0u64;
        let mut total_duration_ms = 0i64;
        let mut total_errors = 0u64;
        let mut command_counts: HashMap<String, u64> = HashMap::new();

        // Read all command stats files
        if self.stats_dir.exists() {
            for entry in fs::read_dir(&self.stats_dir)? {
                let entry = entry?;
                if entry.path().extension().and_then(|s| s.to_str()) == Some("json") {
                    if let Ok(content) = fs::read_to_string(entry.path()) {
                        if let Ok(stats) = serde_json::from_str::<CommandUsageStats>(&content) {
                            total_commands += 1;
                            total_executions += stats.execution_count;
                            total_duration_ms += stats.total_execution_time.num_milliseconds();
                            total_errors += stats.error_count;
                            command_counts.insert(stats.command_name.clone(), stats.execution_count);
                        }
                    }
                }
            }
        }

        // Sort commands by usage count
        let mut most_used: Vec<(String, u64)> = command_counts.into_iter().collect();
        most_used.sort_by(|a, b| b.1.cmp(&a.1));
        most_used.truncate(10); // Top 10

        let average_execution_time = if total_executions > 0 {
            chrono::Duration::milliseconds(total_duration_ms / total_executions as i64)
        } else {
            chrono::Duration::zero()
        };

        let error_rate = if total_executions > 0 {
            total_errors as f64 / total_executions as f64
        } else {
            0.0
        };

        Ok(SystemMetrics {
            total_commands,
            total_executions,
            average_execution_time,
            most_used_commands: most_used,
            error_rate,
            uptime: chrono::Duration::zero(), // Would need to track application uptime separately
            cache_hit_rate: 0.0,              // Would need to track cache metrics separately
        })
    }

    /// Clean up metrics files older than the specified date
    fn cleanup_metrics_files(&self, older_than: DateTime<Utc>) -> Result<(), AnalyticsError> {
        if !self.metrics_dir.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(&self.metrics_dir)? {
            let entry = entry?;
            let path = entry.path();

            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                if file_name.starts_with("metrics_") && file_name.ends_with(".jsonl") {
                    // Extract date from filename
                    if let Some(date_part) = file_name
                        .strip_prefix("metrics_")
                        .and_then(|s| s.strip_suffix(".jsonl"))
                    {
                        if let Ok(file_date) = chrono::NaiveDate::parse_from_str(date_part, "%Y-%m-%d") {
                            let file_datetime = file_date.and_hms_opt(0, 0, 0).unwrap().and_utc();
                            if file_datetime < older_than {
                                if let Err(e) = fs::remove_file(&path) {
                                    eprintln!("Warning: Failed to remove old metrics file {:?}: {}", path, e);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

impl StatsStorage for FileStatsStorage {
    fn store_metrics(&self, metrics: &[ExecutionMetrics]) -> Result<(), AnalyticsError> {
        self.append_metrics(metrics)
    }

    fn get_command_stats(&self, command_name: &str) -> Result<CommandUsageStats, AnalyticsError> {
        self.load_command_stats(command_name)
    }

    fn get_system_metrics(&self) -> Result<SystemMetrics, AnalyticsError> {
        // Try to load cached system metrics first
        if self.system_metrics_file.exists() {
            if let Ok(content) = fs::read_to_string(&self.system_metrics_file) {
                if let Ok(cached_metrics) = serde_json::from_str::<SystemMetrics>(&content) {
                    return Ok(cached_metrics);
                }
            }
        }

        // Calculate and cache system metrics
        let metrics = self.calculate_system_metrics()?;

        // Cache the calculated metrics
        if let Ok(content) = serde_json::to_string_pretty(&metrics) {
            let _ = fs::write(&self.system_metrics_file, content);
        }

        Ok(metrics)
    }

    fn update_command_stats(&self, stats: &CommandUsageStats) -> Result<(), AnalyticsError> {
        self.save_command_stats(stats)?;

        // Invalidate system metrics cache
        let _ = fs::remove_file(&self.system_metrics_file);

        Ok(())
    }

    fn cleanup_old_metrics(&self, older_than: DateTime<Utc>) -> Result<(), AnalyticsError> {
        self.cleanup_metrics_files(older_than)
    }

    fn list_all_command_stats(&self) -> Result<Vec<CommandUsageStats>, AnalyticsError> {
        let mut all_stats = Vec::new();

        if !self.stats_dir.exists() {
            return Ok(all_stats);
        }

        for entry in fs::read_dir(&self.stats_dir)? {
            let entry = entry?;
            if entry.path().extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    if let Ok(stats) = serde_json::from_str::<CommandUsageStats>(&content) {
                        all_stats.push(stats);
                    }
                }
            }
        }

        // Sort by execution count (most used first)
        all_stats.sort_by(|a, b| b.execution_count.cmp(&a.execution_count));

        Ok(all_stats)
    }
}

/// Analytics manager that combines usage tracking with storage
#[allow(dead_code)]
pub struct AnalyticsManager {
    usage_tracker: crate::util::usage_tracker::UsageTracker,
}

#[allow(dead_code)]
impl AnalyticsManager {
    #[allow(dead_code)]
    pub fn new(
        storage_path: PathBuf,
        config: crate::util::command_analytics::AnalyticsConfig,
    ) -> Result<Self, AnalyticsError> {
        let storage = std::sync::Arc::new(FileStatsStorage::new(storage_path)?);
        let usage_tracker = crate::util::usage_tracker::UsageTracker::new(storage, config);

        Ok(Self { usage_tracker })
    }

    pub fn track_execution_start(
        &self,
        command_name: &str,
        parameters: &HashMap<String, serde_json::Value>,
    ) -> Result<String, AnalyticsError> {
        self.usage_tracker.track_execution_start(command_name, parameters)
    }

    pub fn track_execution_end(
        &self,
        execution_id: &str,
        success: bool,
        error_type: Option<String>,
    ) -> Result<(), AnalyticsError> {
        self.usage_tracker
            .track_execution_end(execution_id, success, error_type)
    }

    pub fn get_command_stats(&self, command_name: &str) -> Result<CommandUsageStats, AnalyticsError> {
        self.usage_tracker.get_command_stats(command_name)
    }

    pub fn get_system_metrics(&self) -> Result<SystemMetrics, AnalyticsError> {
        self.usage_tracker.get_system_metrics()
    }

    pub fn get_all_command_stats(&self) -> Result<Vec<CommandUsageStats>, AnalyticsError> {
        self.usage_tracker.get_all_command_stats()
    }

    pub fn flush_metrics(&self) -> Result<(), AnalyticsError> {
        self.usage_tracker.flush_metrics()
    }

    pub fn cleanup_old_metrics(&self) -> Result<(), AnalyticsError> {
        self.usage_tracker.cleanup_old_metrics()
    }

    pub fn is_enabled(&self) -> bool {
        self.usage_tracker.is_enabled()
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::util::command_types::CommandScope;

    #[test]
    fn test_file_stats_storage_creation() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStatsStorage::new(temp_dir.path()).unwrap();

        assert!(storage.stats_dir.exists());
        assert!(storage.metrics_dir.exists());
    }

    #[test]
    fn test_command_stats_file_path() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStatsStorage::new(temp_dir.path()).unwrap();

        let path = storage.command_stats_file("test-command");
        assert!(path.to_string_lossy().contains("test-command.json"));

        // Test sanitization
        let path = storage.command_stats_file("test/command:with*special");
        assert!(path.to_string_lossy().contains("test_command_with_special.json"));
    }

    #[test]
    fn test_save_and_load_command_stats() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStatsStorage::new(temp_dir.path()).unwrap();

        let stats = CommandUsageStats::new("test-command".to_string(), CommandScope::Project);

        // Save stats
        storage.save_command_stats(&stats).unwrap();

        // Load stats
        let loaded_stats = storage.load_command_stats("test-command").unwrap();
        assert_eq!(loaded_stats.command_name, "test-command");
    }

    #[test]
    fn test_metrics_storage() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStatsStorage::new(temp_dir.path()).unwrap();

        let metrics = vec![ExecutionMetrics::new("test-command".to_string(), HashMap::new())];

        storage.store_metrics(&metrics).unwrap();

        // Check that metrics file was created
        let metrics_files: Vec<_> = fs::read_dir(&storage.metrics_dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .collect();

        assert!(!metrics_files.is_empty());
    }

    #[test]
    fn test_analytics_manager() {
        let temp_dir = TempDir::new().unwrap();
        let config = crate::util::command_analytics::AnalyticsConfig::default();
        let manager = AnalyticsManager::new(temp_dir.path().to_path_buf(), config).unwrap();

        assert!(manager.is_enabled());

        let parameters = HashMap::new();
        let execution_id = manager.track_execution_start("test-command", &parameters).unwrap();
        manager.track_execution_end(&execution_id, true, None).unwrap();
        manager.flush_metrics().unwrap();
    }
}
