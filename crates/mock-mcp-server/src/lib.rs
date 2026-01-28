//! Mock MCP server library for testing ACP MCP integration.

use std::collections::HashMap;
use std::io::Write;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{
    Child,
    Command,
    Stdio,
};
use std::sync::{
    Arc,
    Mutex,
};

use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;

/// Tool definition in JSONL config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub input_schema: Value,
}

/// Mock response mapping in JSONL config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockResponse {
    pub tool: String,
    #[serde(default)]
    pub input_match: Option<Value>,
    pub response: Value,
}

/// Config entry (either tool or response)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ConfigEntry {
    #[serde(rename = "tool")]
    Tool(ToolDef),
    #[serde(rename = "response")]
    Response(MockResponse),
}

/// Parse config content from JSONL format
/// Type alias for config parsing result
pub type ConfigParseResult = (Vec<ToolDef>, HashMap<String, Vec<MockResponse>>);

pub fn parse_config(content: &str) -> std::io::Result<ConfigParseResult> {
    let mut tools = Vec::new();
    let mut responses: HashMap<String, Vec<MockResponse>> = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let entry: ConfigEntry =
            serde_json::from_str(line).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        match entry {
            ConfigEntry::Tool(t) => tools.push(t),
            ConfigEntry::Response(r) => {
                responses.entry(r.tool.clone()).or_default().push(r);
            },
        }
    }

    Ok((tools, responses))
}

/// Find matching response for a tool call
pub fn find_response(
    responses: &HashMap<String, Vec<MockResponse>>,
    tool_name: &str,
    args: &Option<serde_json::Map<String, Value>>,
) -> Option<Value> {
    responses.get(tool_name).and_then(|resps| {
        // Try to find a response with matching input_match
        if let Some(args) = args {
            for resp in resps {
                if let Some(ref input_match) = resp.input_match {
                    // Check if all fields in input_match are present and equal in args
                    if let Some(match_obj) = input_match.as_object() {
                        let matches = match_obj.iter().all(|(key, value)| args.get(key) == Some(value));
                        if matches {
                            return Some(resp.response.clone());
                        }
                    }
                }
            }
        }

        // Fall back to first response without input_match, or first response overall
        resps
            .iter()
            .find(|r| r.input_match.is_none())
            .or_else(|| resps.first())
            .map(|r| r.response.clone())
    })
}

/// Builder for creating mock MCP servers
#[derive(Default)]
pub struct MockMcpServerBuilder {
    tools: Vec<ToolDef>,
    responses: Vec<MockResponse>,
    probe_status: Option<u16>,
}

impl MockMcpServerBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_tool(mut self, tool: ToolDef) -> Self {
        self.tools.push(tool);
        self
    }

    pub fn add_response(mut self, response: MockResponse) -> Self {
        self.responses.push(response);
        self
    }

    /// Set the HTTP status code to return for probe requests (initial POST without valid MCP
    /// payload). Use 401 or 403 to trigger OAuth flow.
    pub fn probe_status(mut self, status: u16) -> Self {
        self.probe_status = Some(status);
        self
    }

    /// Spawn an HTTP mock MCP server on an automatically assigned port.
    pub fn spawn_http(self) -> std::io::Result<MockMcpServerHandle> {
        let temp_dir = tempfile::tempdir()?;
        let config_path = temp_dir.path().join("config.jsonl");

        // Write config
        let mut file = std::fs::File::create(&config_path)?;
        for tool in &self.tools {
            let entry = ConfigEntry::Tool(tool.clone());
            writeln!(file, "{}", serde_json::to_string(&entry).unwrap())?;
        }
        for response in &self.responses {
            let entry = ConfigEntry::Response(response.clone());
            writeln!(file, "{}", serde_json::to_string(&entry).unwrap())?;
        }
        drop(file);

        let port = find_available_port()?;
        let port_as_str = port.to_string();
        let binary_path: String;
        let manifest_dir: PathBuf;

        // Try to find pre-built binary first, fall back to cargo run
        let (program, mut args) = if let Some(path) = find_binary() {
            binary_path = path.to_string_lossy().to_string();
            (binary_path.as_str(), vec![
                "--config",
                config_path.to_str().unwrap(),
                "--transport",
                "http",
                "--port",
                &port_as_str,
            ])
        } else {
            manifest_dir = find_cargo_manifest_dir()
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "Could not find mock-mcp-server crate directory or binary",
                    )
                })?
                .join("Cargo.toml");
            ("cargo", vec![
                "run",
                "--quiet",
                "--manifest-path",
                manifest_dir.to_str().unwrap(),
                "--",
                "--config",
                config_path.to_str().unwrap(),
                "--transport",
                "http",
                "--port",
                &port_as_str,
            ])
        };

        // Store probe_status as string for lifetime
        let probe_status_str = self.probe_status.map(|s| s.to_string());
        if let Some(ref status) = probe_status_str {
            args.push("--probe-status");
            args.push(status);
        }

        let child = Command::new(program)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        Ok(MockMcpServerHandle {
            inner: Arc::new(HandleInner {
                _temp_dir: temp_dir,
                child: Mutex::new(Some(child)),
                port,
            }),
        })
    }
}

pub fn prebuild_bin() -> std::io::Result<PathBuf> {
    const NOT_FOUND_MSG: &str = "Could not find mock-mcp-server crate directory or binary";

    let manifest_parent_dir =
        find_cargo_manifest_dir().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, NOT_FOUND_MSG))?;

    let manifest_dir = manifest_parent_dir.join("Cargo.toml");

    'find_bin: {
        if let Some(bin_path) = find_binary() {
            let bin_mtime = bin_path
                .metadata()
                .map_err(|_e| std::io::Error::new(std::io::ErrorKind::NotFound, "Could not extract bin metadata"))?
                .modified()
                .map_err(|_e| std::io::Error::new(std::io::ErrorKind::NotFound, "Could not extract bin mtime"))?;

            let walker = walkdir::WalkDir::new(manifest_parent_dir);
            for entry in walker.into_iter().filter_map(|e| e.ok()) {
                let Ok(md) = entry.metadata() else {
                    continue;
                };
                if md.is_file() {
                    let Ok(mtime) = md.modified() else {
                        continue;
                    };

                    if mtime > bin_mtime {
                        break 'find_bin;
                    }
                }
            }

            return Ok(bin_path);
        }
    }

    let exit_status = Command::new("cargo")
        .args(vec![
            "build",
            "--quiet",
            "--manifest-path",
            manifest_dir.to_str().unwrap(),
        ])
        .spawn()?
        .wait()?;

    if !exit_status.success() {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, NOT_FOUND_MSG));
    }

    find_binary().ok_or(std::io::Error::new(std::io::ErrorKind::NotFound, NOT_FOUND_MSG))
}

fn find_binary() -> Option<PathBuf> {
    // Check CARGO_BIN_EXE env var (set by cargo test)
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_mock-mcp-server") {
        let p = PathBuf::from(&path);
        if p.exists() {
            return Some(p);
        }
    }

    // Check target/debug relative to workspace root
    if let Some(manifest_dir) = find_cargo_manifest_dir() {
        if let Some(workspace_root) = manifest_dir.parent().and_then(|p| p.parent()) {
            let debug_binary = workspace_root.join("target/debug/mock-mcp-server");
            if debug_binary.exists() {
                return Some(debug_binary);
            }
        }
    }

    None
}

/// Handle to a running HTTP mock MCP server process.
#[derive(Clone)]
pub struct MockMcpServerHandle {
    inner: Arc<HandleInner>,
}

struct HandleInner {
    _temp_dir: tempfile::TempDir,
    child: Mutex<Option<Child>>,
    port: u16,
}

impl Drop for HandleInner {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl MockMcpServerHandle {
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/mcp", self.inner.port)
    }

    pub fn port(&self) -> u16 {
        self.inner.port
    }

    pub fn is_running(&self) -> bool {
        let mut child_guard = self.inner.child.lock().unwrap();
        if let Some(ref mut child) = *child_guard {
            matches!(child.try_wait(), Ok(None))
        } else {
            false
        }
    }

    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.inner)
    }

    /// Wait for the server to be ready to accept connections.
    /// Polls the server until it responds or timeout is reached.
    pub fn wait_ready(&self, timeout: std::time::Duration) -> std::io::Result<()> {
        let start = std::time::Instant::now();
        let url = self.url();

        while start.elapsed() < timeout {
            if let Ok(stream) = std::net::TcpStream::connect(format!("127.0.0.1:{}", self.inner.port)) {
                drop(stream);
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("Server at {} not ready after {:?}", url, timeout),
        ))
    }
}

fn find_available_port() -> std::io::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

fn find_cargo_manifest_dir() -> Option<PathBuf> {
    // Find the mock-mcp-server crate directory
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        // If we're in mock-mcp-server crate, use it directly
        if manifest_dir.ends_with("mock-mcp-server") {
            return Some(PathBuf::from(manifest_dir));
        }
        // Otherwise, navigate from workspace root
        let path = PathBuf::from(&manifest_dir);
        for ancestor in path.ancestors() {
            if ancestor.join("Cargo.toml").exists() && ancestor.join("crates").exists() {
                let mock_server_dir = ancestor.join("crates/mock-mcp-server");
                if mock_server_dir.exists() {
                    return Some(mock_server_dir);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_pattern() {
        let handle = MockMcpServerBuilder::new()
            .add_tool(ToolDef {
                name: "test".to_string(),
                description: "Test tool".to_string(),
                input_schema: serde_json::json!({"type": "object"}),
            })
            .add_response(MockResponse {
                tool: "test".to_string(),
                input_match: None,
                response: serde_json::json!({"result": "ok"}),
            })
            .spawn_http()
            .unwrap();

        assert!(handle.port() > 0);
        assert!(handle.url().contains(&handle.port().to_string()));
    }

    #[test]
    fn test_handle_clone_and_strong_count() {
        let handle1 = MockMcpServerBuilder::new().spawn_http().unwrap();
        assert_eq!(handle1.strong_count(), 1);

        let handle2 = handle1.clone();
        assert_eq!(handle1.strong_count(), 2);
        assert_eq!(handle2.strong_count(), 2);

        drop(handle2);
        assert_eq!(handle1.strong_count(), 1);
    }

    #[test]
    fn test_automatic_port_assignment() {
        let handle1 = MockMcpServerBuilder::new().spawn_http().unwrap();
        let handle2 = MockMcpServerBuilder::new().spawn_http().unwrap();

        assert_ne!(handle1.port(), handle2.port());
    }
}
