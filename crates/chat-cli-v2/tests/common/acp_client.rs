//! ACP test client using actor pattern.
//!
//! The `agent_client_protocol` crate requires a `LocalSet` for spawning internal tasks,
//! which conflicts with tokio's default multi-threaded test runtime. Rather than forcing
//! tests to use `#[tokio::test(flavor = "current_thread")]` and managing a `LocalSet`,
//! we spawn the ACP connection in a dedicated thread with its own single-threaded runtime.
//!
//! The `AcpTestClient` handle communicates with this actor via channels, providing a
//! simple async API that works from any tokio runtime.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use agent_client_protocol::{
    self as acp,
    Agent as _,
    PromptResponse,
};
use tokio::process::{
    ChildStdin,
    ChildStdout,
};
use tokio::sync::{
    Mutex,
    mpsc,
    oneshot,
};
use tokio_util::compat::{
    TokioAsyncReadCompatExt,
    TokioAsyncWriteCompatExt,
};

/// Helper to create a text content block.
pub fn text_content(text: &str) -> acp::ContentBlock {
    acp::ContentBlock::Text(acp::TextContent {
        text: text.to_string(),
        annotations: None,
        meta: None,
    })
}

/// Captured notifications from the agent.
#[derive(Debug, Clone, Default)]
pub struct CapturedNotifications {
    pub session_updates: Vec<acp::SessionUpdate>,
    pub permission_requests: Vec<acp::RequestPermissionRequest>,
    pub ext_notifications: Vec<acp::ExtNotification>,
}

/// Queued permission response for testing.
#[derive(Debug, Clone)]
pub enum PermissionResponse {
    /// Select an option by its ID
    Select(String),
    /// Cancel the permission request
    Cancel,
}

/// Commands sent to the ACP actor.
enum Command {
    Initialize {
        reply: oneshot::Sender<acp::Result<acp::InitializeResponse>>,
    },
    NewSession {
        cwd: PathBuf,
        mcp_servers: Vec<acp::McpServer>,
        reply: oneshot::Sender<acp::Result<acp::NewSessionResponse>>,
    },
    LoadSession {
        session_id: acp::SessionId,
        cwd: PathBuf,
        reply: oneshot::Sender<acp::Result<acp::LoadSessionResponse>>,
    },
    Prompt {
        session_id: acp::SessionId,
        content: Vec<acp::ContentBlock>,
        reply: oneshot::Sender<acp::Result<acp::PromptResponse>>,
    },
    SetSessionMode {
        session_id: acp::SessionId,
        mode_id: String,
        reply: oneshot::Sender<acp::Result<acp::SetSessionModeResponse>>,
    },
    SetSessionModel {
        session_id: acp::SessionId,
        model_id: String,
        reply: oneshot::Sender<acp::Result<acp::SetSessionModelResponse>>,
    },
    Cancel {
        session_id: acp::SessionId,
        reply: oneshot::Sender<acp::Result<()>>,
    },
    GetCaptured {
        reply: oneshot::Sender<CapturedNotifications>,
    },
    ClearCaptured {
        reply: oneshot::Sender<()>,
    },
    QueuePermissionResponse {
        response: PermissionResponse,
        reply: oneshot::Sender<()>,
    },
}

/// Test client that captures all notifications for assertions.
struct TestAcpClient {
    captured: Arc<Mutex<CapturedNotifications>>,
    permission_responses: Arc<Mutex<VecDeque<PermissionResponse>>>,
    trust_all: bool,
}

impl TestAcpClient {
    fn new(
        captured: Arc<Mutex<CapturedNotifications>>,
        permission_responses: Arc<Mutex<VecDeque<PermissionResponse>>>,
        trust_all: bool,
    ) -> Self {
        Self {
            captured,
            permission_responses,
            trust_all,
        }
    }
}

#[async_trait::async_trait(?Send)]
impl acp::Client for TestAcpClient {
    async fn session_notification(&self, args: acp::SessionNotification) -> acp::Result<()> {
        self.captured.lock().await.session_updates.push(args.update);
        Ok(())
    }

    async fn request_permission(
        &self,
        args: acp::RequestPermissionRequest,
    ) -> acp::Result<acp::RequestPermissionResponse> {
        self.captured.lock().await.permission_requests.push(args.clone());

        if self.trust_all {
            // Auto-approve with AllowOnce
            return Ok(acp::RequestPermissionResponse {
                outcome: acp::RequestPermissionOutcome::Selected {
                    option_id: acp::PermissionOptionId(
                        agent::protocol::PermissionOptionId::AllowOnce.to_string().into(),
                    ),
                },
                meta: None,
            });
        }

        // Pop from pre-queued responses
        let response = self
            .permission_responses
            .lock()
            .await
            .pop_front()
            .expect("No permission response queued - use queue_permission_response() before prompt");

        let outcome = match response {
            PermissionResponse::Select(id) => acp::RequestPermissionOutcome::Selected {
                option_id: acp::PermissionOptionId(id.into()),
            },
            PermissionResponse::Cancel => acp::RequestPermissionOutcome::Cancelled,
        };

        Ok(acp::RequestPermissionResponse { outcome, meta: None })
    }

    async fn write_text_file(&self, _: acp::WriteTextFileRequest) -> acp::Result<acp::WriteTextFileResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn read_text_file(&self, _: acp::ReadTextFileRequest) -> acp::Result<acp::ReadTextFileResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn create_terminal(&self, _: acp::CreateTerminalRequest) -> acp::Result<acp::CreateTerminalResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn terminal_output(&self, _: acp::TerminalOutputRequest) -> acp::Result<acp::TerminalOutputResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn release_terminal(&self, _: acp::ReleaseTerminalRequest) -> acp::Result<acp::ReleaseTerminalResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn wait_for_terminal_exit(
        &self,
        _: acp::WaitForTerminalExitRequest,
    ) -> acp::Result<acp::WaitForTerminalExitResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn kill_terminal_command(
        &self,
        _: acp::KillTerminalCommandRequest,
    ) -> acp::Result<acp::KillTerminalCommandResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn ext_method(&self, _: acp::ExtRequest) -> acp::Result<acp::ExtResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn ext_notification(&self, args: acp::ExtNotification) -> acp::Result<()> {
        self.captured.lock().await.ext_notifications.push(args);
        Ok(())
    }
}

/// Handle to communicate with the ACP actor.
#[derive(Clone)]
pub struct AcpTestClient {
    tx: mpsc::Sender<Command>,
}

impl AcpTestClient {
    /// Spawn the ACP client actor in a separate thread with its own runtime.
    pub fn spawn(stdin: ChildStdin, stdout: ChildStdout, trust_all: bool) -> Self {
        let (tx, rx) = mpsc::channel(32);

        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            let local = tokio::task::LocalSet::new();
            local.block_on(&rt, run_actor(stdin, stdout, rx, trust_all));
        });

        Self { tx }
    }

    /// Queue a permission response for the next permission request.
    pub async fn queue_permission_response(&self, response: PermissionResponse) {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(Command::QueuePermissionResponse { response, reply })
            .await
            .ok();
        rx.await.ok();
    }

    pub async fn initialize(&self) -> acp::Result<acp::InitializeResponse> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(Command::Initialize { reply }).await.ok();
        rx.await.unwrap()
    }

    pub async fn new_session(&self, cwd: PathBuf) -> acp::Result<acp::NewSessionResponse> {
        self.new_session_with_mcp(cwd, Vec::new()).await
    }

    pub async fn new_session_with_mcp(
        &self,
        cwd: PathBuf,
        mcp_servers: Vec<sacp::schema::McpServer>,
    ) -> acp::Result<acp::NewSessionResponse> {
        // Convert sacp::schema::McpServer to acp::McpServer via JSON
        let mcp_servers: Vec<acp::McpServer> = mcp_servers
            .into_iter()
            .map(|s| {
                let json = serde_json::to_value(&s).unwrap();
                serde_json::from_value(json).unwrap()
            })
            .collect();
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(Command::NewSession {
                cwd,
                mcp_servers,
                reply,
            })
            .await
            .ok();
        rx.await.unwrap()
    }

    pub async fn load_session(
        &self,
        session_id: acp::SessionId,
        cwd: PathBuf,
    ) -> acp::Result<acp::LoadSessionResponse> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(Command::LoadSession { session_id, cwd, reply }).await.ok();
        rx.await.unwrap()
    }

    pub async fn prompt_text(&self, session_id: acp::SessionId, text: &str) -> acp::Result<PromptResponse> {
        self.prompt(session_id, vec![text_content(text)]).await
    }

    /// Sends prompt and waits for completion.
    /// In ACP the prompt() call only returns when all the session updates have been sent out.
    pub async fn prompt(
        &self,
        session_id: acp::SessionId,
        content: Vec<acp::ContentBlock>,
    ) -> acp::Result<PromptResponse> {
        let rx = self.prompt_async(session_id, content).await;
        rx.await.unwrap()
    }

    /// Sends prompt and returns receiver immediately
    pub async fn prompt_async(
        &self,
        session_id: acp::SessionId,
        content: Vec<acp::ContentBlock>,
    ) -> tokio::sync::oneshot::Receiver<acp::Result<PromptResponse>> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(Command::Prompt {
                session_id,
                content,
                reply,
            })
            .await
            .ok();

        rx
    }

    pub async fn prompt_text_async(
        &self,
        session_id: acp::SessionId,
        text: &str,
    ) -> tokio::sync::oneshot::Receiver<acp::Result<PromptResponse>> {
        self.prompt_async(session_id, vec![text_content(text)]).await
    }

    pub async fn set_session_mode(
        &self,
        session_id: acp::SessionId,
        mode_id: String,
    ) -> acp::Result<acp::SetSessionModeResponse> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(Command::SetSessionMode {
                session_id,
                mode_id,
                reply,
            })
            .await
            .ok();
        rx.await.unwrap()
    }

    pub async fn cancel(&self, session_id: acp::SessionId) -> acp::Result<()> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(Command::Cancel { session_id, reply }).await.ok();
        rx.await.unwrap()
    }

    pub async fn set_session_model(
        &self,
        session_id: acp::SessionId,
        model_id: String,
    ) -> acp::Result<acp::SetSessionModelResponse> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(Command::SetSessionModel {
                session_id,
                model_id,
                reply,
            })
            .await
            .ok();
        rx.await.unwrap()
    }

    pub async fn captured(&self) -> CapturedNotifications {
        let (reply, rx) = oneshot::channel();
        self.tx.send(Command::GetCaptured { reply }).await.ok();
        rx.await.unwrap()
    }

    pub async fn clear_captured(&self) {
        let (reply, rx) = oneshot::channel();
        self.tx.send(Command::ClearCaptured { reply }).await.ok();
        rx.await.ok();
    }

    /// Poll until the predicate returns true, sleeping between checks.
    pub async fn wait_for<F>(&self, predicate: F)
    where
        F: Fn(&CapturedNotifications) -> bool,
    {
        loop {
            let captured = self.captured().await;
            if predicate(&captured) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }
}

async fn run_actor(stdin: ChildStdin, stdout: ChildStdout, mut rx: mpsc::Receiver<Command>, trust_all: bool) {
    let captured = Arc::new(Mutex::new(CapturedNotifications::default()));
    let permission_responses = Arc::new(Mutex::new(VecDeque::new()));
    let client = TestAcpClient::new(captured.clone(), permission_responses.clone(), trust_all);

    let outgoing = stdin.compat_write();
    let incoming = stdout.compat();

    let (conn, handle_io) = acp::ClientSideConnection::new(client, outgoing, incoming, |fut| {
        tokio::task::spawn_local(fut);
    });
    let conn = Arc::new(conn);

    tokio::task::spawn_local(handle_io);

    while let Some(cmd) = rx.recv().await {
        match cmd {
            Command::Initialize { reply } => {
                tokio::task::spawn_local({
                    let conn = conn.clone();
                    async move {
                        let result = conn
                            .initialize(acp::InitializeRequest {
                                protocol_version: acp::V1,
                                client_capabilities: acp::ClientCapabilities::default(),
                                client_info: Some(acp::Implementation {
                                    name: "test-client".to_string(),
                                    title: Some("Test Client".to_string()),
                                    version: "0.1.0".to_string(),
                                }),
                                meta: None,
                            })
                            .await;
                        let _ = reply.send(result);
                    }
                });
            },
            Command::NewSession {
                cwd,
                mcp_servers,
                reply,
            } => {
                tokio::task::spawn_local({
                    let conn = conn.clone();
                    async move {
                        let result = conn
                            .new_session(acp::NewSessionRequest {
                                mcp_servers,
                                cwd,
                                meta: None,
                            })
                            .await;
                        let _ = reply.send(result);
                    }
                });
            },
            Command::LoadSession { session_id, cwd, reply } => {
                let result = conn
                    .load_session(acp::LoadSessionRequest {
                        session_id,
                        cwd,
                        mcp_servers: Vec::new(),
                        meta: None,
                    })
                    .await;
                let _ = reply.send(result);
            },
            Command::Prompt {
                session_id,
                content,
                reply,
            } => {
                tokio::task::spawn_local({
                    let conn = conn.clone();
                    async move {
                        let result = conn
                            .prompt(acp::PromptRequest {
                                session_id,
                                prompt: content,
                                meta: None,
                            })
                            .await;
                        let _ = reply.send(result);
                    }
                });
            },
            Command::Cancel { session_id, reply } => {
                tokio::task::spawn_local({
                    let conn = conn.clone();
                    async move {
                        let result = conn.cancel(acp::CancelNotification { session_id, meta: None }).await;
                        let _ = reply.send(result);
                    }
                });
            },
            Command::SetSessionMode {
                session_id,
                mode_id,
                reply,
            } => {
                let result = conn
                    .set_session_mode(acp::SetSessionModeRequest {
                        session_id,
                        mode_id: acp::SessionModeId(mode_id.into()),
                        meta: None,
                    })
                    .await;
                let _ = reply.send(result);
            },
            Command::SetSessionModel {
                session_id,
                model_id,
                reply,
            } => {
                let result = conn
                    .set_session_model(acp::SetSessionModelRequest {
                        session_id,
                        model_id: acp::ModelId(model_id.into()),
                        meta: None,
                    })
                    .await;
                let _ = reply.send(result);
            },
            Command::GetCaptured { reply } => {
                let _ = reply.send(captured.lock().await.clone());
            },
            Command::ClearCaptured { reply } => {
                *captured.lock().await = CapturedNotifications::default();
                let _ = reply.send(());
            },
            Command::QueuePermissionResponse { response, reply } => {
                permission_responses.lock().await.push_back(response);
                let _ = reply.send(());
            },
        }
    }
}
