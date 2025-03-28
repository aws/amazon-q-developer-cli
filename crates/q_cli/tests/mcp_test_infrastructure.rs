// Test Infrastructure Foundation for MCP Integration Tests
// This module provides the foundation for all MCP integration tests

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use tracing::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::fs;
use tokio::net::TcpListener;
use tokio::sync::RwLock;

// Re-export commonly used testing crates
pub use assert_cmd::prelude::*;
pub use assert_fs::prelude::*;
pub use predicates::prelude::*;
pub use tokio::test;

/// Configuration for test environment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConfig {
    /// Log level for tests
    pub log_level: String,
    /// Whether to keep temporary files after tests
    pub keep_temp_files: bool,
    /// Default timeout for async operations in milliseconds
    pub default_timeout_ms: u64,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            log_level: "debug".to_string(),
            keep_temp_files: false,
            default_timeout_ms: 5000,
        }
    }
}

/// Base test fixture that will be used by all tests
pub struct TestFixture {
    /// Temporary directory for test files
    pub temp_dir: TempDir,
    /// Test configuration
    pub config: TestConfig,
    /// Shared state for tests
    pub state: Arc<RwLock<HashMap<String, Value>>>,
}

impl TestFixture {
    /// Create a new test fixture with default configuration
    pub async fn new() -> Result<Self> {
        Self::with_config(TestConfig::default()).await
    }

    /// Create a new test fixture with custom configuration
    pub async fn with_config(config: TestConfig) -> Result<Self> {
        // Initialize logging based on config
        if std::env::var("RUST_LOG").is_err() {
            std::env::set_var("RUST_LOG", &config.log_level);
        }
        env_logger::try_init().ok(); // Ignore if already initialized

        // Create temporary directory
        let temp_dir = TempDir::new().context("Failed to create temporary directory")?;
        
        info!("Created test fixture with temp directory: {:?}", temp_dir.path());
        
        Ok(Self {
            temp_dir,
            config,
            state: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Get a path within the temporary directory
    pub fn temp_path(&self, relative_path: &str) -> PathBuf {
        self.temp_dir.path().join(relative_path)
    }

    /// Create a file in the temporary directory with the given content
    pub async fn create_file(&self, relative_path: &str, content: &str) -> Result<PathBuf> {
        let path = self.temp_path(relative_path);
        
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.context("Failed to create parent directories")?;
        }
        
        fs::write(&path, content).await.context("Failed to write file")?;
        debug!("Created file at {:?} with content length {}", path, content.len());
        
        Ok(path)
    }

    /// Create a JSON file in the temporary directory
    pub async fn create_json_file<T: Serialize>(&self, relative_path: &str, content: &T) -> Result<PathBuf> {
        let json_content = serde_json::to_string_pretty(content).context("Failed to serialize to JSON")?;
        self.create_file(relative_path, &json_content).await
    }

    /// Find an available port for testing servers
    pub async fn find_available_port(&self) -> Result<u16> {
        // Bind to port 0 to let the OS assign an available port
        let listener = TcpListener::bind("127.0.0.1:0").await.context("Failed to bind to port")?;
        let addr = listener.local_addr().context("Failed to get local address")?;
        
        // We don't need to keep the listener open
        drop(listener);
        
        Ok(addr.port())
    }

    /// Store a value in the shared test state
    pub async fn set_state<T: Serialize>(&self, key: &str, value: T) -> Result<()> {
        let value = serde_json::to_value(value).context("Failed to convert to JSON value")?;
        let mut state = self.state.write().await;
        state.insert(key.to_string(), value);
        Ok(())
    }

    /// Get a value from the shared test state
    pub async fn get_state<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Result<Option<T>> {
        let state = self.state.read().await;
        if let Some(value) = state.get(key) {
            let typed_value = serde_json::from_value(value.clone()).context("Failed to deserialize state value")?;
            Ok(Some(typed_value))
        } else {
            Ok(None)
        }
    }
}

/// Trait for test components that need cleanup
#[async_trait]
pub trait TestCleanup {
    /// Clean up resources used by this component
    async fn cleanup(&self) -> Result<()>;
}

/// Helper function to create a temporary file with content
pub async fn create_temp_file(content: &str) -> Result<(TempDir, PathBuf)> {
    let dir = TempDir::new().context("Failed to create temporary directory")?;
    let file_path = dir.path().join("test_file");
    fs::write(&file_path, content).await.context("Failed to write temporary file")?;
    Ok((dir, file_path))
}

/// Helper function to read a file to string
pub async fn read_file_to_string(path: impl AsRef<Path>) -> Result<String> {
    fs::read_to_string(path).await.context("Failed to read file")
}

/// Helper function to wait for a condition with timeout
pub async fn wait_for_condition<F>(
    condition: F, 
    timeout_ms: u64, 
    check_interval_ms: u64
) -> Result<bool>
where
    F: Fn() -> bool,
{
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_millis(timeout_ms);
    let interval = std::time::Duration::from_millis(check_interval_ms);
    
    while start.elapsed() < timeout {
        if condition() {
            return Ok(true);
        }
        tokio::time::sleep(interval).await;
    }
    
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fixture_creation() {
        let fixture = TestFixture::new().await.expect("Failed to create test fixture");
        assert!(fixture.temp_dir.path().exists());
    }

    #[tokio::test]
    async fn test_file_creation() {
        let fixture = TestFixture::new().await.expect("Failed to create test fixture");
        let content = "test content";
        let path = fixture.create_file("test.txt", content).await.expect("Failed to create file");
        
        let read_content = fs::read_to_string(path).await.expect("Failed to read file");
        assert_eq!(read_content, content);
    }

    #[tokio::test]
    async fn test_json_file_creation() {
        let fixture = TestFixture::new().await.expect("Failed to create test fixture");
        let data = json!({
            "name": "test",
            "value": 42
        });
        
        let path = fixture.create_json_file("test.json", &data).await.expect("Failed to create JSON file");
        
        let read_content = fs::read_to_string(path).await.expect("Failed to read file");
        let read_json: Value = serde_json::from_str(&read_content).expect("Failed to parse JSON");
        
        assert_eq!(read_json["name"], "test");
        assert_eq!(read_json["value"], 42);
    }

    #[tokio::test]
    async fn test_shared_state() {
        let fixture = TestFixture::new().await.expect("Failed to create test fixture");
        
        fixture.set_state("test_key", "test_value").await.expect("Failed to set state");
        let value: Option<String> = fixture.get_state("test_key").await.expect("Failed to get state");
        
        assert_eq!(value, Some("test_value".to_string()));
    }

    #[tokio::test]
    async fn test_available_port() {
        let fixture = TestFixture::new().await.expect("Failed to create test fixture");
        let port = fixture.find_available_port().await.expect("Failed to find available port");
        
        assert!(port > 0);
    }
}
