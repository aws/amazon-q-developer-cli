//! Mock MCP server library for testing ACP MCP integration.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{
    Child,
    Command,
    Stdio,
};
use std::sync::{
    Arc,
    Mutex,
    OnceLock,
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
    oauth: bool,
    oauth_token_ttl_secs: Option<u64>,
    oauth_no_refresh_token: bool,
    oauth_refresh_fails: bool,
    oauth_resource_origin_only: bool,
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

    /// Enable a fully working OAuth flow (discovery, registration, authorize, token,
    /// refresh). With this set, `/mcp` requires a valid bearer token, so the client
    /// must complete the OAuth handshake before it can call tools.
    pub fn oauth(mut self) -> Self {
        self.oauth = true;
        self
    }

    /// Set the lifetime (in seconds) of issued OAuth access tokens. Use a small value
    /// to force mid-session token expiry and exercise the refresh path. Implies `oauth()`.
    pub fn oauth_token_ttl_secs(mut self, secs: u64) -> Self {
        self.oauth = true;
        self.oauth_token_ttl_secs = Some(secs);
        self
    }

    /// Stop the server from issuing refresh tokens, forcing the client down the
    /// re-authorization path when the access token expires. Implies `oauth()`.
    pub fn oauth_no_refresh_token(mut self) -> Self {
        self.oauth = true;
        self.oauth_no_refresh_token = true;
        self
    }

    /// Make `grant_type=refresh_token` requests fail with HTTP 400, simulating a
    /// server that can no longer refresh the session. Implies `oauth()`.
    pub fn oauth_refresh_fails(mut self) -> Self {
        self.oauth = true;
        self.oauth_refresh_fails = true;
        self
    }

    /// Declare the RFC 9728 protected-resource-metadata `resource` as the server
    /// *origin* (`http://127.0.0.1:{port}`) instead of the full `/mcp` base URL.
    /// Both are valid RFC 8707 resource identifiers for the base URL, but they are
    /// different strings — which lets a test distinguish "honor the PRM-declared
    /// resource" (rmcp 3.0) from "derive resource from the base URL" (the rmcp 2.0
    /// bug that broke Microsoft Entra ID v2). Implies `oauth()`.
    pub fn oauth_resource_origin_only(mut self) -> Self {
        self.oauth = true;
        self.oauth_resource_origin_only = true;
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

        // The child binds an ephemeral port itself and reports it here; pre-assigning one
        // would leave it unowned between the probe and the child's bind.
        let port_file = temp_dir.path().join("port");
        let port_file_arg = port_file.to_str().unwrap().to_string();

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
                "0",
                "--port-file",
                &port_file_arg,
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
                "0",
                "--port-file",
                &port_file_arg,
            ])
        };

        // Store probe_status as string for lifetime
        let probe_status_str = self.probe_status.map(|s| s.to_string());
        if let Some(ref status) = probe_status_str {
            args.push("--probe-status");
            args.push(status);
        }

        // OAuth flags. `oauth_token_ttl_str` must outlive the args vec.
        let oauth_token_ttl_str = self.oauth_token_ttl_secs.map(|s| s.to_string());
        if self.oauth {
            args.push("--oauth");
        }
        if let Some(ref ttl) = oauth_token_ttl_str {
            args.push("--oauth-token-ttl-secs");
            args.push(ttl);
        }
        if self.oauth_no_refresh_token {
            args.push("--oauth-no-refresh-token");
        }
        if self.oauth_refresh_fails {
            args.push("--oauth-refresh-fails");
        }
        if self.oauth_resource_origin_only {
            args.push("--oauth-resource-origin-only");
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
                port_file,
                port: OnceLock::new(),
            }),
        })
    }
}

/// Build the mock server binary if it is missing or stale, at most once per process.
/// Every test calls this, and concurrent callers would each start their own `cargo
/// build` and then queue on cargo's lock instead of sharing a single build.
pub fn prebuild_bin() -> std::io::Result<PathBuf> {
    static BUILT: Mutex<Option<PathBuf>> = Mutex::new(None);

    // Recover from poisoning so a panicking build surfaces its own error to later
    // callers rather than a poison error.
    let mut built = BUILT.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(path) = built.as_ref() {
        return Ok(path.clone());
    }

    let path = build_bin()?;
    *built = Some(path.clone());
    Ok(path)
}

fn build_bin() -> std::io::Result<PathBuf> {
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

    if let Ok(target_dir) = std::env::var("CARGO_TARGET_DIR") {
        let debug_binary = PathBuf::from(target_dir).join("debug/mock-mcp-server");
        if debug_binary.exists() {
            return Some(debug_binary);
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
    port_file: PathBuf,
    port: OnceLock<u16>,
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
        format!("http://127.0.0.1:{}/mcp", self.port())
    }

    /// # Panics
    ///
    /// Panics unless `wait_ready` has returned `Ok`, since only the server itself
    /// knows which ephemeral port it bound.
    pub fn port(&self) -> u16 {
        *self
            .inner
            .port
            .get()
            .expect("mock server port is unknown until wait_ready succeeds")
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

    /// Wait for the server to report the port it bound and to accept a connection on it.
    /// This must succeed before `port` or `url` may be called.
    pub fn wait_ready(&self, timeout: std::time::Duration) -> std::io::Result<()> {
        let start = std::time::Instant::now();

        while start.elapsed() < timeout {
            if let Some(status) = self.exit_status() {
                // stderr is piped and nothing else ever reads it, so the line saying
                // why the child died would be discarded when the pipe closes.
                let stderr = self.drain_stderr();
                let detail = if stderr.is_empty() {
                    "no stderr output".to_string()
                } else {
                    stderr
                };
                return Err(std::io::Error::other(format!(
                    "Server exited before it was ready: {status}: {detail}"
                )));
            }

            // Publish the port only once a connection has succeeded, so `port` and `url`
            // stay unreachable when this call goes on to time out.
            if let Some(port) = self.reported_port() {
                if let Ok(stream) = std::net::TcpStream::connect(("127.0.0.1", port)) {
                    drop(stream);
                    let _ = self.inner.port.set(port);
                    return Ok(());
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        // Never reporting a port (still building, or dead before binding) and binding
        // one that then refuses connections have different causes, so the timeout
        // message has to tell them apart.
        let progress = match self.reported_port() {
            Some(port) => format!("bound port {port} but refused connections"),
            None => "child never reported a port".to_string(),
        };
        Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("Server not ready after {timeout:?}: {progress}"),
        ))
    }

    /// Reads whatever the child wrote to `stderr`. Only call this once the child
    /// has exited: the pipe stays open while it runs, so the read would block.
    fn drain_stderr(&self) -> String {
        use std::io::Read;

        let mut child_guard = self.inner.child.lock().unwrap();
        let Some(child) = child_guard.as_mut() else {
            return String::new();
        };
        let Some(mut stderr) = child.stderr.take() else {
            return String::new();
        };
        let mut buf = String::new();
        let _ = stderr.read_to_string(&mut buf);
        buf.trim().to_string()
    }

    fn exit_status(&self) -> Option<std::process::ExitStatus> {
        let mut child_guard = self.inner.child.lock().unwrap();
        child_guard.as_mut().and_then(|child| child.try_wait().ok().flatten())
    }

    fn reported_port(&self) -> Option<u16> {
        std::fs::read_to_string(&self.inner.port_file).ok()?.trim().parse().ok()
    }
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

    const READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

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
        handle.wait_ready(READY_TIMEOUT).unwrap();

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
        handle1.wait_ready(READY_TIMEOUT).unwrap();
        handle2.wait_ready(READY_TIMEOUT).unwrap();

        assert_ne!(handle1.port(), handle2.port());
    }

    #[test]
    fn test_timed_out_wait_ready_leaves_port_unpublished() {
        let temp_dir = tempfile::tempdir().unwrap();
        let port_file = temp_dir.path().join("port");
        // Nothing listens on port 1, so the reported port is never connectable.
        std::fs::write(&port_file, "1").unwrap();

        let handle = MockMcpServerHandle {
            inner: Arc::new(HandleInner {
                _temp_dir: temp_dir,
                child: Mutex::new(None),
                port_file,
                port: OnceLock::new(),
            }),
        };

        let err = handle
            .wait_ready(std::time::Duration::from_millis(100))
            .expect_err("wait_ready must fail when the reported port refuses connections");
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
        assert!(
            handle.inner.port.get().is_none(),
            "port must stay unpublished so port() upholds its documented panic"
        );
    }
}
