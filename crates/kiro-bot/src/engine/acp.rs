//! ACP worker pool and session management.
//!
//! Manages a pool of `kiro-cli acp` subprocesses, each running an independent
//! ACP session. Workers are spawned on demand, reused per conversation, and
//! reaped after idle timeout.
//!
//! ## Architecture
//!
//! ```text
//! BotCore → Work channel → ACP thread (single-threaded tokio LocalSet)
//!                              └─ AcpPool
//!                                   ├─ Worker "dm:alice"  → kiro-cli acp process
//!                                   ├─ Worker "thread:C1:ts" → kiro-cli acp process
//!                                   └─ (idle workers reaped every 60s)
//! ```

use std::cell::{
    Cell,
    RefCell,
};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{
    Arc,
    Mutex,
};
use std::time::{
    Duration,
    Instant,
};

use acp::Agent as _;
use agent_client_protocol as acp;
use tokio::sync::{
    mpsc,
    oneshot,
};
use tokio_util::compat::{
    TokioAsyncReadCompatExt,
    TokioAsyncWriteCompatExt,
};
use tracing::info;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// How the bot handles tool permission requests from the agent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalPolicy {
    /// Deny all tool requests.
    Deny,
    /// Auto-approve all tool requests.
    #[default]
    Approve,
    /// Post to Slack and wait for user reaction.
    Ask,
}

/// Configuration for the ACP worker pool.
pub struct AcpConfig {
    pub command: String,
    pub model_id: String,
    pub bot_user: String,
    pub mcp_wait_ms: u64,
    pub default_mode: Option<String>,
    pub max_workers: usize,
    pub idle_timeout_secs: u64,
    pub approval_policy: ApprovalPolicy,
    pub approval_tx: Option<mpsc::UnboundedSender<ApprovalRequest>>,
}

/// A permission request sent from the ACP thread to the Slack frontend.
pub struct ApprovalRequest {
    pub tool_name: String,
    pub tool_call_id: String,
    pub options: Vec<(String, String)>,
    pub channel: String,
    pub thread_ts: Option<String>,
    pub slack_user_id: String,
    pub reply_tx: oneshot::Sender<ApprovalResponse>,
}

pub enum ApprovalResponse {
    Selected(String),
    Denied,
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ModeInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Default)]
pub struct AcpInfo {
    pub available_modes: Vec<ModeInfo>,
    pub session_modes: HashMap<String, String>,
    pub model_id: String,
}

// ---------------------------------------------------------------------------
// Work items
// ---------------------------------------------------------------------------

/// A unit of work dispatched from the bot core to the ACP thread.
pub enum Work {
    Prompt {
        text: String,
        context: Vec<String>,
        conversation: String,
        channel: String,
        thread_ts: Option<String>,
        user: String,
        slack_user_id: String,
        reply_tx: oneshot::Sender<String>,
        progress_tx: mpsc::UnboundedSender<String>,
    },
    NewSession {
        conversation: String,
        reply_tx: oneshot::Sender<String>,
    },
    SetMode {
        conversation: String,
        mode: String,
        reply_tx: oneshot::Sender<String>,
    },
    SetModel {
        conversation: String,
        model: String,
        reply_tx: oneshot::Sender<String>,
    },
    Cancel {
        conversation: String,
    },
    Status {
        conversation: String,
        reply_tx: oneshot::Sender<String>,
    },
}

// ---------------------------------------------------------------------------
// Worker and pool traits
// ---------------------------------------------------------------------------

#[async_trait::async_trait(?Send)]
pub trait Worker {
    fn session_id(&self) -> String;
    fn touch(&self);
    fn last_active(&self) -> Instant;
    fn set_conv(&self, channel: String, thread_ts: Option<String>, slack_user_id: String);
    async fn prompt(&self, messages: Vec<String>, progress_tx: mpsc::UnboundedSender<String>) -> String;
    async fn cancel(&self);
    async fn set_mode(&self, mode: String) -> Result<String, String>;
    async fn kill(&self);
}

#[async_trait::async_trait(?Send)]
pub trait WorkerPool {
    fn get(&self, conversation: &str) -> Option<Rc<dyn Worker>>;
    async fn get_or_spawn(&self, conversation: &str) -> Result<Rc<dyn Worker>, String>;
    async fn remove(&self, conversation: &str) -> bool;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn max_workers(&self) -> usize;
}

#[async_trait::async_trait(?Send)]
trait WorkerFactory {
    async fn spawn(&self) -> Result<Rc<dyn Worker>, String>;
}

// ---------------------------------------------------------------------------
// ACP Client (per-worker, !Send)
// ---------------------------------------------------------------------------

fn tool_emoji(kind: &acp::ToolKind) -> &'static str {
    match kind {
        acp::ToolKind::Read => "📖",
        acp::ToolKind::Edit => "✏️",
        acp::ToolKind::Delete => "🗑️",
        acp::ToolKind::Move => "📦",
        acp::ToolKind::Search => "🔍",
        acp::ToolKind::Execute => "⚡",
        acp::ToolKind::Think => "💭",
        acp::ToolKind::Fetch => "🌐",
        acp::ToolKind::SwitchMode => "🔄",
        _ => "🔧",
    }
}

struct AcpClient {
    chunks: Rc<RefCell<Vec<String>>>,
    progress: Rc<RefCell<Option<mpsc::UnboundedSender<String>>>>,
    mcp_ready_count: Rc<RefCell<u32>>,
    mcp_notify: Rc<tokio::sync::Notify>,
    acp_info: Arc<Mutex<AcpInfo>>,
    approval_policy: ApprovalPolicy,
    approval_tx: Option<mpsc::UnboundedSender<ApprovalRequest>>,
    current_conv: Rc<RefCell<(String, Option<String>, String)>>,
}

#[async_trait::async_trait(?Send)]
impl acp::Client for AcpClient {
    async fn request_permission(
        &self,
        args: acp::RequestPermissionRequest,
    ) -> acp::Result<acp::RequestPermissionResponse> {
        let first_option = args
            .options
            .first()
            .map(|o| o.id.clone())
            .ok_or_else(acp::Error::method_not_found)?;

        let cancelled = || acp::RequestPermissionResponse {
            outcome: acp::RequestPermissionOutcome::Cancelled,
            meta: None,
        };
        let selected = |id: acp::PermissionOptionId| acp::RequestPermissionResponse {
            outcome: acp::RequestPermissionOutcome::Selected { option_id: id },
            meta: None,
        };

        match self.approval_policy {
            ApprovalPolicy::Deny => Ok(cancelled()),
            ApprovalPolicy::Approve => Ok(selected(first_option)),
            ApprovalPolicy::Ask => {
                if let Some(tx) = &self.approval_tx {
                    let options: Vec<(String, String)> = args
                        .options
                        .iter()
                        .map(|o| (o.id.to_string(), o.name.clone()))
                        .collect();
                    let title = args.tool_call.fields.title.clone().unwrap_or_default();
                    let (reply_tx, reply_rx) = oneshot::channel();
                    let (channel, thread_ts, slack_user_id) = self.current_conv.borrow().clone();
                    let req = ApprovalRequest {
                        tool_name: title,
                        tool_call_id: args.tool_call.id.to_string(),
                        options,
                        channel,
                        thread_ts,
                        slack_user_id,
                        reply_tx,
                    };
                    if tx.send(req).is_err() {
                        return Ok(cancelled());
                    }
                    match reply_rx.await {
                        Ok(ApprovalResponse::Selected(option_id)) => {
                            Ok(selected(acp::PermissionOptionId(option_id.into())))
                        },
                        _ => Ok(cancelled()),
                    }
                } else {
                    Ok(cancelled())
                }
            },
        }
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
        match args.method.as_ref() {
            "kiro.dev/mcp/server_initialized" | "kiro.dev/mcp/server_init_failure" => {
                *self.mcp_ready_count.borrow_mut() += 1;
                self.mcp_notify.notify_one();
            },
            _ => {},
        }
        Ok(())
    }

    #[allow(clippy::await_holding_refcell_ref)]
    async fn session_notification(&self, args: acp::SessionNotification) -> acp::Result<()> {
        match args.update {
            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk {
                content: acp::ContentBlock::Text(text_block),
                ..
            }) => {
                self.chunks.borrow_mut().push(text_block.text);
            },
            acp::SessionUpdate::ToolCall(tool_call) => {
                self.chunks.borrow_mut().clear();
                let status = format!("{} {}...", tool_emoji(&tool_call.kind), tool_call.title);
                if let Some(sender) = self.progress.borrow().as_ref() {
                    let _ = sender.send(status);
                }
            },
            acp::SessionUpdate::CurrentModeUpdate(mode_update) => {
                self.acp_info
                    .lock()
                    .unwrap()
                    .session_modes
                    .insert(args.session_id.to_string(), mode_update.current_mode_id.to_string());
            },
            _ => {},
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// AcpWorker
// ---------------------------------------------------------------------------

struct AcpWorker {
    connection: Rc<acp::ClientSideConnection>,
    session: acp::SessionId,
    last_active_ts: Rc<Cell<Instant>>,
    child: Rc<RefCell<tokio::process::Child>>,
    chunks: Rc<RefCell<Vec<String>>>,
    progress: Rc<RefCell<Option<mpsc::UnboundedSender<String>>>>,
    current_conv: Rc<RefCell<(String, Option<String>, String)>>,
}

#[async_trait::async_trait(?Send)]
impl Worker for AcpWorker {
    fn session_id(&self) -> String {
        self.session.to_string()
    }

    fn touch(&self) {
        self.last_active_ts.set(Instant::now());
    }

    fn last_active(&self) -> Instant {
        self.last_active_ts.get()
    }

    fn set_conv(&self, channel: String, thread_ts: Option<String>, slack_user_id: String) {
        *self.current_conv.borrow_mut() = (channel, thread_ts, slack_user_id);
    }

    async fn prompt(&self, messages: Vec<String>, progress_tx: mpsc::UnboundedSender<String>) -> String {
        self.chunks.borrow_mut().clear();
        *self.progress.borrow_mut() = Some(progress_tx);
        let acp_messages: Vec<acp::ContentBlock> = messages
            .into_iter()
            .map(|s| {
                acp::ContentBlock::Text(acp::TextContent {
                    text: s,
                    annotations: None,
                    meta: None,
                })
            })
            .collect();
        let reply = match self
            .connection
            .prompt(acp::PromptRequest {
                session_id: self.session.clone(),
                prompt: acp_messages,
                meta: None,
            })
            .await
        {
            Ok(r) if r.stop_reason == acp::StopReason::Cancelled => "❌ Cancelled".into(),
            Ok(_) => self.chunks.borrow().join(""),
            Err(e) => format!("Error: {e}"),
        };
        *self.progress.borrow_mut() = None;
        reply
    }

    async fn cancel(&self) {
        let _ = self
            .connection
            .cancel(acp::CancelNotification {
                session_id: self.session.clone(),
                meta: None,
            })
            .await;
    }

    async fn set_mode(&self, mode: String) -> Result<String, String> {
        self.connection
            .set_session_mode(acp::SetSessionModeRequest {
                session_id: self.session.clone(),
                mode_id: acp::SessionModeId(mode.clone().into()),
                meta: None,
            })
            .await
            .map(|_| format!("→ agent: {mode}"))
            .map_err(|e| format!("Error: {e}"))
    }

    #[allow(clippy::await_holding_refcell_ref)]
    async fn kill(&self) {
        let _ = self.child.borrow_mut().kill().await;
    }
}

async fn spawn_acp_worker(cfg: &AcpConfig, acp_info: &Arc<Mutex<AcpInfo>>) -> Result<AcpWorker, String> {
    let parts: Vec<&str> = cfg.command.split_whitespace().collect();
    let mut child = tokio::process::Command::new(parts[0])
        .args(&parts[1..])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Failed to spawn ACP: {e}"))?;

    let chunks: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let progress: Rc<RefCell<Option<mpsc::UnboundedSender<String>>>> = Rc::new(RefCell::new(None));
    let mcp_ready_count: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    let mcp_notify = Rc::new(tokio::sync::Notify::new());
    let current_conv: Rc<RefCell<(String, Option<String>, String)>> =
        Rc::new(RefCell::new((String::new(), None, String::new())));

    let (connection, handle_io) = acp::ClientSideConnection::new(
        AcpClient {
            chunks: chunks.clone(),
            progress: progress.clone(),
            mcp_ready_count: mcp_ready_count.clone(),
            mcp_notify: mcp_notify.clone(),
            acp_info: acp_info.clone(),
            approval_policy: cfg.approval_policy,
            approval_tx: cfg.approval_tx.clone(),
            current_conv: current_conv.clone(),
        },
        child.stdin.take().unwrap().compat_write(),
        child.stdout.take().unwrap().compat(),
        |fut| {
            tokio::task::spawn_local(fut);
        },
    );
    tokio::task::spawn_local(handle_io);
    let connection = Rc::new(connection);

    connection
        .initialize(acp::InitializeRequest {
            protocol_version: acp::V1,
            client_capabilities: acp::ClientCapabilities::default(),
            client_info: Some(acp::Implementation {
                name: "kiro-bot".to_string(),
                title: Some("Kiro Bot".to_string()),
                version: env!("CARGO_PKG_VERSION").to_string(),
            }),
            meta: None,
        })
        .await
        .map_err(|e| format!("ACP init failed: {e}"))?;

    let resp = connection
        .new_session(acp::NewSessionRequest {
            mcp_servers: Vec::new(),
            cwd: std::env::current_dir().unwrap(),
            meta: None,
        })
        .await
        .map_err(|e| format!("New session failed: {e}"))?;

    if let Some(mode_state) = &resp.modes {
        let mut info = acp_info.lock().unwrap();
        if info.available_modes.is_empty() {
            info.available_modes = mode_state
                .available_modes
                .iter()
                .map(|m| ModeInfo {
                    id: m.id.to_string(),
                    name: m.name.clone(),
                    description: m.description.clone(),
                })
                .collect();
        }
        info.session_modes
            .insert(resp.session_id.to_string(), mode_state.current_mode_id.to_string());
    }

    // Wait for MCP servers to initialize
    while tokio::time::timeout(Duration::from_millis(cfg.mcp_wait_ms), mcp_notify.notified())
        .await
        .is_ok()
    {}

    if let Some(agent) = &cfg.default_mode
        && connection
            .set_session_mode(acp::SetSessionModeRequest {
                session_id: resp.session_id.clone(),
                mode_id: acp::SessionModeId(agent.clone().into()),
                meta: None,
            })
            .await
            .is_ok()
    {
        acp_info
            .lock()
            .unwrap()
            .session_modes
            .insert(resp.session_id.to_string(), agent.clone());
    }

    Ok(AcpWorker {
        connection,
        session: resp.session_id,
        last_active_ts: Rc::new(Cell::new(Instant::now())),
        child: Rc::new(RefCell::new(child)),
        chunks,
        progress,
        current_conv,
    })
}

// ---------------------------------------------------------------------------
// AcpPool
// ---------------------------------------------------------------------------

struct AcpPool {
    workers: RefCell<HashMap<String, Rc<dyn Worker>>>,
    default_worker: RefCell<Option<Rc<dyn Worker>>>,
    factory: Rc<dyn WorkerFactory>,
    max: usize,
}

impl AcpPool {
    fn new(factory: Rc<dyn WorkerFactory>, default_worker: Option<Rc<dyn Worker>>, max: usize) -> Self {
        Self {
            workers: RefCell::new(HashMap::new()),
            default_worker: RefCell::new(default_worker),
            factory,
            max,
        }
    }

    #[allow(clippy::await_holding_refcell_ref)]
    async fn reap_idle(&self, timeout: Duration) {
        let now = Instant::now();
        let expired: Vec<String> = self
            .workers
            .borrow()
            .iter()
            .filter(|(_, w)| now.duration_since(w.last_active()) > timeout)
            .map(|(k, _)| k.clone())
            .collect();
        for key in &expired {
            info!(conversation = %key, "Reaping idle worker");
            self.remove(key).await;
        }
    }

    async fn shutdown(&self) {
        let keys: Vec<String> = self.workers.borrow().keys().cloned().collect();
        for key in &keys {
            self.remove(key).await;
        }
        let default = self.default_worker.borrow_mut().take();
        if let Some(w) = default {
            w.kill().await;
        }
    }
}

#[allow(clippy::await_holding_refcell_ref)]
#[async_trait::async_trait(?Send)]
impl WorkerPool for AcpPool {
    fn get(&self, conversation: &str) -> Option<Rc<dyn Worker>> {
        self.workers.borrow().get(conversation).cloned()
    }

    async fn get_or_spawn(&self, conversation: &str) -> Result<Rc<dyn Worker>, String> {
        if let Some(w) = self.workers.borrow().get(conversation) {
            return Ok(w.clone());
        }
        if let Some(w) = self.default_worker.borrow_mut().take() {
            self.workers.borrow_mut().insert(conversation.to_string(), w.clone());
            return Ok(w);
        }
        if self.workers.borrow().len() >= self.max {
            return Err("⏳ All workers busy — try again shortly".into());
        }
        let w = self.factory.spawn().await?;
        self.workers.borrow_mut().insert(conversation.to_string(), w.clone());
        Ok(w)
    }

    async fn remove(&self, conversation: &str) -> bool {
        let worker = self.workers.borrow_mut().remove(conversation);
        if let Some(w) = worker {
            w.kill().await;
            true
        } else {
            false
        }
    }

    fn len(&self) -> usize {
        self.workers.borrow().len()
    }

    fn max_workers(&self) -> usize {
        self.max
    }
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

struct AcpWorkerFactory {
    cfg: Rc<AcpConfig>,
    acp_info: Arc<Mutex<AcpInfo>>,
}

#[async_trait::async_trait(?Send)]
impl WorkerFactory for AcpWorkerFactory {
    async fn spawn(&self) -> Result<Rc<dyn Worker>, String> {
        Ok(Rc::new(spawn_acp_worker(&self.cfg, &self.acp_info).await?))
    }
}

// ---------------------------------------------------------------------------
// Work loop
// ---------------------------------------------------------------------------

async fn run_work_loop(
    pool: &dyn WorkerPool,
    mut work_receiver: mpsc::UnboundedReceiver<Work>,
    _bot_user: &str,
    acp_info: &Arc<Mutex<AcpInfo>>,
) {
    while let Some(work) = work_receiver.recv().await {
        match work {
            Work::Prompt {
                text,
                context,
                conversation,
                channel,
                thread_ts,
                user,
                slack_user_id,
                reply_tx,
                progress_tx,
            } => {
                let pool_worker = match pool.get_or_spawn(&conversation).await {
                    Ok(w) => w,
                    Err(e) => {
                        let _ = reply_tx.send(e);
                        continue;
                    },
                };
                pool_worker.touch();
                pool_worker.set_conv(channel, thread_ts, slack_user_id);

                tokio::task::spawn_local(async move {
                    let mut messages = vec![];
                    if let Some(ctx) = crate::engine::core::format_context(&context) {
                        messages.push(ctx);
                    }
                    messages.push(format!("Slack message from {user} ({user}): {text}"));
                    let reply = pool_worker.prompt(messages, progress_tx).await;
                    let _ = reply_tx.send(reply);
                });
            },
            Work::NewSession { conversation, reply_tx } => {
                let msg = if pool.remove(&conversation).await {
                    "✨ Session reset — next message will start fresh"
                } else {
                    "No active session"
                };
                let _ = reply_tx.send(msg.into());
            },
            Work::SetMode {
                conversation,
                mode,
                reply_tx,
            } => {
                let msg = if let Some(w) = pool.get(&conversation) {
                    w.set_mode(mode).await.unwrap_or_else(|e| e)
                } else {
                    "No session — send a message first".into()
                };
                let _ = reply_tx.send(msg);
            },
            Work::SetModel { reply_tx, .. } => {
                let _ = reply_tx.send("set_session_model not available".into());
            },
            Work::Cancel { conversation } => {
                if let Some(w) = pool.get(&conversation) {
                    w.cancel().await;
                }
            },
            Work::Status { conversation, reply_tx } => {
                let info = acp_info.lock().unwrap();
                let message = if let Some(w) = pool.get(&conversation) {
                    let mode = info
                        .session_modes
                        .get(&w.session_id())
                        .map(|s| s.as_str())
                        .unwrap_or("unknown");
                    format!(
                        "*Agent:* {mode}\n*Model:* {}\n*Session:* {}\n*Workers:* {}/{}",
                        info.model_id,
                        w.session_id(),
                        pool.len(),
                        pool.max_workers()
                    )
                } else {
                    format!(
                        "*Model:* {}\n*Workers:* {}/{}\nNo active session — send a message first",
                        info.model_id,
                        pool.len(),
                        pool.max_workers()
                    )
                };
                let _ = reply_tx.send(message);
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Pool thread
// ---------------------------------------------------------------------------

/// Spawn the ACP worker pool on a dedicated thread. Returns shared [`AcpInfo`].
pub fn spawn_acp_thread(
    work_receiver: mpsc::UnboundedReceiver<Work>,
    ready_sender: oneshot::Sender<()>,
    cfg: AcpConfig,
) -> Arc<Mutex<AcpInfo>> {
    let acp_info = Arc::new(Mutex::new(AcpInfo::default()));
    let acp_info_clone = acp_info.clone();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let local = tokio::task::LocalSet::new();
        local.block_on(&runtime, async move {
            let max_workers = cfg.max_workers;
            let idle_timeout = Duration::from_secs(cfg.idle_timeout_secs);
            let bot_user = cfg.bot_user.clone();
            {
                acp_info_clone.lock().unwrap().model_id = cfg.model_id.clone();
            }

            let cfg = Rc::new(cfg);
            let factory: Rc<dyn WorkerFactory> = Rc::new(AcpWorkerFactory {
                cfg: cfg.clone(),
                acp_info: acp_info_clone.clone(),
            });

            info!(command = %cfg.command, "Spawning initial ACP worker");
            let warmup = factory.spawn().await.expect("Failed to spawn initial ACP worker");
            let pool = Rc::new(AcpPool::new(factory, Some(warmup), max_workers));

            let _ = ready_sender.send(());

            // Idle reaper
            {
                let pool = pool.clone();
                tokio::task::spawn_local(async move {
                    let mut interval = tokio::time::interval(Duration::from_secs(60));
                    loop {
                        interval.tick().await;
                        pool.reap_idle(idle_timeout).await;
                    }
                });
            }

            run_work_loop(pool.as_ref(), work_receiver, &bot_user, &acp_info_clone).await;
            pool.shutdown().await;
        });
    });
    acp_info
}
