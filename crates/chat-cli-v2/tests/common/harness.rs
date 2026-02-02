//! ACP test harness for spawning and communicating with the agent subprocess.

use std::path::Path;
use std::sync::atomic::{
    AtomicU64,
    Ordering,
};
use std::time::Duration;

use chat_cli_v2::agent::ipc_server::{
    MessageKind,
    TestCommand,
    TestMessageCommand,
    TestMessageResponse,
    TestResponse,
};
use chat_cli_v2::api_client::model::ConversationState;
use chat_cli_v2::api_client::send_message_output::MockStreamItem;
use tokio::io::{
    AsyncBufReadExt,
    AsyncWriteExt,
    BufReader,
    BufWriter,
};
use tokio::net::{
    UnixListener,
    UnixStream,
};
use tokio::process::{
    Child,
    Command,
};

use super::paths::{
    TestPaths,
    create_test_dir,
};

/// Builder for configuring and spawning an ACP test harness.
pub struct AcpTestHarnessBuilder {
    test_name: String,
    agent_configs: Vec<(String, serde_json::Value)>,
    settings: serde_json::Map<String, serde_json::Value>,
    trust_all: bool,
}

impl AcpTestHarnessBuilder {
    pub fn new(test_name: &str) -> Self {
        Self {
            test_name: test_name.to_string(),
            agent_configs: Vec::new(),
            settings: serde_json::Map::new(),
            trust_all: false,
        }
    }

    /// Add an agent config to be created before spawning.
    pub fn with_agent_config(mut self, name: &str, config: &impl serde::Serialize) -> Self {
        let value = serde_json::to_value(config).expect("failed to serialize agent config");
        self.agent_configs.push((name.to_string(), value));
        self
    }

    /// Add a single setting entry.
    pub fn with_setting(mut self, key: &str, value: impl serde::Serialize) -> Self {
        let value = serde_json::to_value(value).expect("failed to serialize setting");
        self.settings.insert(key.to_string(), value);
        self
    }

    /// Replace all settings with the provided map.
    pub fn with_settings(mut self, settings: serde_json::Map<String, serde_json::Value>) -> Self {
        self.settings = settings;
        self
    }

    /// Set whether to auto-approve all permission requests.
    pub fn with_trust_all(mut self, trust_all: bool) -> Self {
        self.trust_all = trust_all;
        self
    }

    /// Build and spawn the harness, returning harness + initialized client.
    pub async fn build(self) -> (AcpTestHarness, super::AcpTestClient) {
        let paths = create_test_dir(&self.test_name);

        for (name, config) in &self.agent_configs {
            let path = paths.agents_dir.join(format!("{}.json", name));
            let json = serde_json::to_string_pretty(config).expect("failed to serialize agent config");
            std::fs::write(&path, json).expect("failed to write agent config");
        }

        if !self.settings.is_empty() {
            let content = std::fs::read_to_string(&paths.settings_path).expect("failed to read settings");
            let mut existing: serde_json::Map<String, serde_json::Value> =
                serde_json::from_str(&content).expect("failed to parse settings");
            for (key, value) in self.settings {
                existing.insert(key, value);
            }
            let json = serde_json::to_string_pretty(&existing).expect("failed to serialize settings");
            std::fs::write(&paths.settings_path, json).expect("failed to write settings");
        }

        let mut harness = AcpTestHarness::spawn(paths).await;
        let (stdin, stdout) = harness.take_stdio();
        let client = super::AcpTestClient::spawn(stdin, stdout, self.trust_all);
        client.initialize().await.expect("initialize failed");
        harness.wait_for_ipc().await;
        (harness, client)
    }

    /// Build harness + client + new session.
    pub async fn build_with_session(
        self,
    ) -> (
        AcpTestHarness,
        super::AcpTestClient,
        agent_client_protocol::SessionId,
        std::path::PathBuf,
    ) {
        let (harness, client) = self.build().await;
        let cwd = harness.paths.cwd.clone();
        let resp = client.new_session(cwd.clone()).await.expect("new_session failed");
        (harness, client, resp.session_id, cwd)
    }
}

/// Test harness for running ACP integration tests against the `chat_cli acp` subprocess.
///
/// Spawns the agent with sandboxed directories and establishes an IPC connection
/// for injecting mock API responses. The agent's stdin/stdout are available for
/// ACP protocol communication via `take_stdio()`.
pub struct AcpTestHarness {
    /// Isolated test directories (cwd, sessions, etc). Cleaned up on drop.
    pub paths: TestPaths,
    /// The spawned `chat_cli acp` subprocess.
    pub child: Child,
    /// IPC listener - agent connects after session is created.
    ipc_listener: UnixListener,
    /// IPC stream for mock response injection (set after wait_for_ipc).
    ipc_stream: Option<UnixStream>,
    /// Counter for generating unique IPC message IDs.
    msg_id: AtomicU64,
}

impl AcpTestHarness {
    /// Spawn the ACP agent subprocess with sandboxed directories.
    ///
    /// Creates isolated test directories, starts an IPC listener, and spawns the agent.
    /// The agent connects to IPC on startup (in SessionManager).
    /// Call `wait_for_ipc()` after spawning to establish the connection.
    pub async fn new(test_name: &str) -> Self {
        let paths = create_test_dir(test_name);
        Self::spawn(paths).await
    }

    /// Spawn the ACP agent subprocess with the given paths.
    async fn spawn(paths: TestPaths) -> Self {
        // Start IPC listener before spawning agent
        let ipc_listener = UnixListener::bind(&paths.ipc_socket).expect("failed to bind IPC socket");

        let binary = env!("CARGO_BIN_EXE_chat_cli_v2");

        let child = Command::new(binary)
            .arg("acp")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .env("KIRO_TEST_MODE", "1")
            .env("KIRO_TEST_SESSIONS_DIR", &paths.sessions_dir)
            .env("KIRO_TEST_AGENTS_DIR", &paths.agents_dir)
            .env("KIRO_TEST_SETTINGS_PATH", &paths.settings_path)
            .env("KIRO_TEST_CHAT_IPC_SOCKET_PATH", &paths.ipc_socket)
            .env("KIRO_CHAT_LOG_FILE", &paths.log_file)
            .env("KIRO_LOG_LEVEL", "chat_cli=debug,agent=debug")
            .kill_on_drop(true)
            .spawn()
            .unwrap_or_else(|e| panic!("failed to spawn {}: {}", binary, e));

        eprintln!("Agent log file: {:?}", paths.log_file);

        Self {
            paths,
            child,
            ipc_listener,
            ipc_stream: None,
            msg_id: AtomicU64::new(0),
        }
    }

    /// Wait for the agent to connect to IPC.
    /// Call this after ACP initialize to ensure SessionManager has started.
    pub async fn wait_for_ipc(&mut self) {
        let (stream, _) = tokio::time::timeout(Duration::from_secs(10), self.ipc_listener.accept())
            .await
            .expect("timeout waiting for agent IPC connection")
            .expect("failed to accept IPC connection");
        self.ipc_stream = Some(stream);
    }

    /// Take ownership of the child's stdin/stdout for ACP protocol communication.
    pub fn take_stdio(&mut self) -> (tokio::process::ChildStdin, tokio::process::ChildStdout) {
        (
            self.child.stdin.take().expect("stdin already taken"),
            self.child.stdout.take().expect("stdout already taken"),
        )
    }

    /// Push mock response items for a specific session.
    /// Pass `None` to signal end of response stream.
    pub async fn push_mock_response(&mut self, session_id: &str, events: Option<Vec<MockStreamItem>>) {
        self.send_ipc_command(TestCommand::PushSendMessageResponse {
            session_id: session_id.to_string(),
            events,
        })
        .await;
    }

    /// Load mock responses from a JSONL file and push them for a session.
    /// File format: JSON items separated by newlines, blank lines separate response streams.
    /// Lines starting with `//` are treated as comments.
    pub async fn push_mock_responses_from_file(&mut self, session_id: &str, path: impl AsRef<Path>) {
        let content = std::fs::read_to_string(path.as_ref())
            .unwrap_or_else(|e| panic!("failed to read mock file {:?}: {}", path.as_ref(), e));

        for stream in parse_mock_response_streams(&content) {
            self.push_mock_response(session_id, Some(stream)).await;
            // Each stream needs its own None terminator
            self.push_mock_response(session_id, None).await;
        }
    }

    async fn send_ipc_command(&mut self, data: TestCommand) -> TestResponse {
        let stream = self
            .ipc_stream
            .as_mut()
            .expect("IPC not connected - call wait_for_ipc() first");

        let id = self.msg_id.fetch_add(1, Ordering::Relaxed).to_string();
        let cmd = TestMessageCommand {
            id,
            msg_kind: MessageKind::Command,
            data,
        };
        let json = serde_json::to_string(&cmd).expect("failed to serialize IPC command");

        let (reader, writer) = stream.split();
        let mut writer = BufWriter::new(writer);
        let mut reader = BufReader::new(reader);

        writer
            .write_all(format!("{}\n", json).as_bytes())
            .await
            .expect("failed to write IPC command");
        writer.flush().await.expect("failed to flush IPC command");

        let mut line = String::new();
        reader.read_line(&mut line).await.expect("failed to read IPC response");

        let resp: TestMessageResponse = serde_json::from_str(&line).expect("failed to parse IPC response");
        if let TestResponse::Error { error } = &resp.data {
            panic!("IPC error: {}", error);
        }
        resp.data
    }

    /// Get captured LLM requests for a session.
    pub async fn get_captured_requests(&mut self, session_id: &str) -> Vec<ConversationState> {
        match self
            .send_ipc_command(TestCommand::GetCapturedRequests {
                session_id: session_id.to_string(),
            })
            .await
        {
            TestResponse::GetCapturedRequests { requests } => requests,
            other => panic!("unexpected response: {:?}", other),
        }
    }

    /// Create an agent config file in the test agents directory.
    /// Must be called before spawning the agent (i.e., before `new()`).
    pub fn create_agent_config(agents_dir: &Path, name: &str, config: &impl serde::Serialize) {
        let path = agents_dir.join(format!("{}.json", name));
        let json = serde_json::to_string_pretty(config).expect("failed to serialize agent config");
        std::fs::write(&path, json).expect("failed to write agent config");
    }

    /// Set a setting in the test settings file.
    /// Must be called after harness creation but settings are read when agent initializes.
    pub fn set_setting(settings_path: &Path, key: &str, value: impl serde::Serialize) {
        let content = std::fs::read_to_string(settings_path).expect("failed to read settings");
        let mut settings: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&content).expect("failed to parse settings");
        settings.insert(key.to_string(), serde_json::to_value(value).unwrap());
        let json = serde_json::to_string_pretty(&settings).expect("failed to serialize settings");
        std::fs::write(settings_path, json).expect("failed to write settings");
    }
}

/// Parse mock response streams from JSONL content.
/// Blank lines separate response streams. Lines starting with `//` are comments.
fn parse_mock_response_streams(content: &str) -> Vec<Vec<MockStreamItem>> {
    let mut streams = Vec::new();
    let mut current = Vec::new();

    for line in content.lines() {
        // Skip comments
        if line.starts_with("//") {
            continue;
        }
        // Blank line ends current stream
        if line.is_empty() {
            if !current.is_empty() {
                streams.push(std::mem::take(&mut current));
            }
            continue;
        }
        // Parse JSON item
        current.push(serde_json::from_str(line).unwrap_or_else(|e| panic!("invalid JSON: {}: {}", line, e)));
    }
    // Don't forget trailing stream without blank line
    if !current.is_empty() {
        streams.push(current);
    }
    streams
}
