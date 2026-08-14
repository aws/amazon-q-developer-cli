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
use std::collections::{
    HashMap,
    VecDeque,
};
use std::path::{
    Path,
    PathBuf,
};
use std::rc::Rc;
use std::sync::atomic::{
    AtomicU64,
    Ordering,
};
use std::sync::{
    Arc,
    Mutex,
};
use std::time::Duration;

use acp::Agent as _;
use agent_client_protocol as acp;
use tokio::io::{
    AsyncBufReadExt,
    BufReader,
};
use tokio::sync::{
    mpsc,
    oneshot,
};
use tokio::time::Instant;
use tokio_util::compat::{
    TokioAsyncReadCompatExt,
    TokioAsyncWriteCompatExt,
};
use tracing::{
    debug,
    info,
    warn,
};

use super::attachment_read::{
    AttachmentReadAuthorizer,
    AttachmentReadDecision,
};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// How the bot handles tool permission requests from the agent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalPolicy {
    /// Deny all tool requests.
    Deny,
    /// Auto-approve only exact bot-owned reads; ask for every other tool.
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
    pub attachment_reads: Arc<AttachmentReadAuthorizer>,
}

pub const PROMPT_TIMEOUT_ENV: &str = "KIRO_BOT_PROMPT_TIMEOUT_SECS";
pub(crate) const CANCEL_CONFIRM_TIMEOUT: Duration = Duration::from_secs(12);
const DEFAULT_PROMPT_TIMEOUT_SECS: u64 = 15 * 60;
const PROMPT_CANCEL_GRACE: Duration = Duration::from_secs(5);
const STOP_PROMPT_TIMEOUT: Duration = Duration::from_secs(8);
const CANCEL_NOTIFY_TIMEOUT: Duration = Duration::from_secs(2);
const CANCEL_SETTLE_TIMEOUT: Duration = Duration::from_secs(3);
const PROMPT_TASK_STOP_TIMEOUT: Duration = Duration::from_secs(1);
const PROGRESS_UPDATE_INTERVAL: Duration = Duration::from_secs(1);
const APPROVAL_QUEUE_LIMIT: usize = 32;
const STDERR_HISTORY_LIMIT: usize = 20;
const DISABLE_DEFAULT_RESOURCES_KEY: &str = "chat.disableInheritingDefaultResources";
const AGENT_CONFIG_DIR_ENV: &str = "KIRO_AGENT_CONFIG_DIR";
const KIRO_HOME_ENV: &str = "KIRO_HOME";
const PINNED_AGENT_NAME: &str = "kiro-help";
const SETTINGS_PATH_ENV: &str = "KIRO_TEST_SETTINGS_PATH";
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_SETTINGS_OVERLAY_ID: AtomicU64 = AtomicU64::new(1);

fn next_request_id() -> String {
    format!(
        "acp-{}-{}",
        std::process::id(),
        NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed)
    )
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AcpRuntimeState {
    #[default]
    Starting,
    Running,
    Draining,
    Stopped,
    Failed,
}

#[derive(Debug, Default)]
pub struct AcpInfo {
    pub available_modes: Vec<ModeInfo>,
    pub session_modes: HashMap<String, String>,
    pub model_id: String,
    pub runtime_state: AcpRuntimeState,
    pub workers: usize,
    pub busy_workers: usize,
    pub worker_restarts: u64,
    pub prompt_timeouts: u64,
    pub overload_rejections: u64,
    pub dropped_progress_updates: u64,
    pub pending_approvals: usize,
    pub last_failure: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressStatus {
    Pending,
    InProgress,
    Complete,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressUpdate {
    pub id: String,
    pub title: String,
    pub status: ProgressStatus,
}

// ---------------------------------------------------------------------------
// Work items
// ---------------------------------------------------------------------------

/// Prompt fields that can be prepared after the work item is queued.
pub struct PromptPayload {
    pub text: String,
    pub context: Vec<String>,
    pub channel: String,
    pub thread_ts: Option<String>,
    pub user: String,
    pub slack_user_id: String,
}

pub enum PromptInput {
    Ready(PromptPayload),
    Deferred(oneshot::Receiver<PromptPayload>),
}

impl PromptInput {
    pub fn ready(
        text: String,
        context: Vec<String>,
        channel: String,
        thread_ts: Option<String>,
        user: String,
        slack_user_id: String,
    ) -> Self {
        Self::Ready(PromptPayload {
            text,
            context,
            channel,
            thread_ts,
            user,
            slack_user_id,
        })
    }

    async fn resolve(self) -> Result<PromptPayload, String> {
        match self {
            Self::Ready(payload) => Ok(payload),
            Self::Deferred(receiver) => receiver
                .await
                .map_err(|_| "prompt preparation was cancelled".to_string()),
        }
    }
}

/// A unit of work dispatched from the bot core to the ACP thread.
pub enum Work {
    Prompt {
        input: PromptInput,
        conversation: String,
        reply_tx: oneshot::Sender<String>,
        progress_tx: mpsc::UnboundedSender<ProgressUpdate>,
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
    CancelAndWait {
        conversation: String,
        reply_tx: oneshot::Sender<()>,
    },
    Status {
        conversation: String,
        reply_tx: oneshot::Sender<String>,
    },
    Shutdown {
        grace: Duration,
        reply_tx: oneshot::Sender<()>,
    },
}

impl Work {
    pub fn conversation_id(&self) -> Option<&str> {
        match self {
            Self::Prompt { conversation, .. }
            | Self::NewSession { conversation, .. }
            | Self::SetMode { conversation, .. }
            | Self::SetModel { conversation, .. }
            | Self::Cancel { conversation }
            | Self::CancelAndWait { conversation, .. }
            | Self::Status { conversation, .. } => Some(conversation),
            Self::Shutdown { .. } => None,
        }
    }

    pub fn reject(self, message: impl Into<String>) {
        let message = message.into();
        match self {
            Self::Prompt { reply_tx, .. }
            | Self::NewSession { reply_tx, .. }
            | Self::SetMode { reply_tx, .. }
            | Self::SetModel { reply_tx, .. }
            | Self::Status { reply_tx, .. } => {
                let _ = reply_tx.send(message);
            },
            Self::Cancel { .. } | Self::CancelAndWait { .. } | Self::Shutdown { .. } => {},
        }
    }
}

// ---------------------------------------------------------------------------
// Worker and pool traits
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerHealth {
    Healthy,
    Dead(String),
}

#[derive(Debug, Clone, Copy)]
enum WorkerActivityState {
    IdleSince(Instant),
    Busy,
}

pub struct WorkerActivity {
    state: Rc<Cell<WorkerActivityState>>,
    acp_info: Arc<Mutex<AcpInfo>>,
}

impl Drop for WorkerActivity {
    fn drop(&mut self) {
        if matches!(self.state.get(), WorkerActivityState::Busy) {
            self.state.set(WorkerActivityState::IdleSince(Instant::now()));
            let mut info = self.acp_info.lock().unwrap();
            info.busy_workers = info.busy_workers.saturating_sub(1);
        }
    }
}

#[async_trait::async_trait(?Send)]
pub trait Worker {
    fn session_id(&self) -> String;
    fn request_id(&self) -> String;
    fn begin_activity(&self) -> Result<WorkerActivity, String>;
    fn idle_since(&self) -> Option<Instant>;
    fn health(&self) -> WorkerHealth;
    fn set_request_context(&self, request_id: String, conversation_id: String);
    fn set_conv(&self, channel: String, thread_ts: Option<String>, slack_user_id: String);
    async fn prompt(
        &self,
        messages: Vec<String>,
        progress_tx: mpsc::UnboundedSender<ProgressUpdate>,
    ) -> Result<String, String>;
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

fn should_emit_progress(last_progress_update: &Cell<Option<Instant>>) -> bool {
    let now = Instant::now();
    if last_progress_update
        .get()
        .is_some_and(|last| now.duration_since(last) < PROGRESS_UPDATE_INTERVAL)
    {
        return false;
    }
    last_progress_update.set(Some(now));
    true
}

struct AcpClient {
    chunks: Rc<RefCell<Vec<String>>>,
    progress: Rc<RefCell<Option<mpsc::UnboundedSender<ProgressUpdate>>>>,
    progress_titles: Rc<RefCell<HashMap<String, String>>>,
    last_progress_update: Rc<Cell<Option<Instant>>>,
    mcp_ready_count: Rc<RefCell<u32>>,
    mcp_notify: Rc<tokio::sync::Notify>,
    acp_info: Arc<Mutex<AcpInfo>>,
    approval_policy: ApprovalPolicy,
    approval_tx: Option<mpsc::UnboundedSender<ApprovalRequest>>,
    attachment_reads: Arc<AttachmentReadAuthorizer>,
    current_request: Rc<RefCell<(String, String)>>,
    current_conv: Rc<RefCell<(String, Option<String>, String)>>,
}

struct ApprovalCapacityGuard {
    acp_info: Arc<Mutex<AcpInfo>>,
}

impl Drop for ApprovalCapacityGuard {
    fn drop(&mut self) {
        let mut info = self.acp_info.lock().unwrap();
        info.pending_approvals = info.pending_approvals.saturating_sub(1);
    }
}

fn mcp_tool_identity_from_meta(meta: Option<&serde_json::Map<String, serde_json::Value>>) -> Option<(&str, &str)> {
    let identity = meta?.get("mcpToolIdentity")?;
    Some((
        identity.get("serverName")?.as_str()?,
        identity.get("toolName")?.as_str()?,
    ))
}

fn is_auto_approved_mcp_read(meta: Option<&serde_json::Map<String, serde_json::Value>>) -> bool {
    let Some(identity) = mcp_tool_identity_from_meta(meta) else {
        return false;
    };
    crate::agents::AUTO_APPROVED_MCP_READS.contains(&identity)
}

fn fs_read_paths_from_meta(
    meta: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<Option<Vec<String>>, ()> {
    let Some(value) = meta.and_then(|meta| meta.get("fsReadPaths")) else {
        return Ok(None);
    };
    let values = value.as_array().ok_or(())?;
    if values.is_empty() {
        return Err(());
    }
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|path| !path.is_empty())
                .map(str::to_string)
                .ok_or(())
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn permission_option(args: &acp::RequestPermissionRequest, option_id: &str) -> Option<acp::PermissionOptionId> {
    args.options
        .iter()
        .find(|option| option.option_id.0.as_ref() == option_id)
        .map(|option| option.option_id.clone())
}

async fn request_slack_approval(
    client: &AcpClient,
    args: &acp::RequestPermissionRequest,
) -> acp::Result<acp::RequestPermissionResponse> {
    let cancelled = || acp::RequestPermissionResponse::new(acp::RequestPermissionOutcome::Cancelled);
    let Some(tx) = &client.approval_tx else {
        return Ok(cancelled());
    };

    {
        let mut info = client.acp_info.lock().unwrap();
        if info.pending_approvals >= APPROVAL_QUEUE_LIMIT {
            info.overload_rejections += 1;
            warn!("approval queue is full; denying permission request");
            return Ok(cancelled());
        }
        info.pending_approvals += 1;
    }
    let _capacity = ApprovalCapacityGuard {
        acp_info: client.acp_info.clone(),
    };
    let options: Vec<(String, String)> = args
        .options
        .iter()
        .map(|option| (option.option_id.to_string(), option.name.clone()))
        .collect();
    let title = args.tool_call.fields.title.clone().unwrap_or_default();
    let (reply_tx, reply_rx) = oneshot::channel();
    let (channel, thread_ts, slack_user_id) = client.current_conv.borrow().clone();
    let req = ApprovalRequest {
        tool_name: title,
        tool_call_id: args.tool_call.tool_call_id.to_string(),
        options,
        channel,
        thread_ts,
        slack_user_id,
        reply_tx,
    };
    if tx.send(req).is_err() {
        return Ok(cancelled());
    }
    match tokio::time::timeout(Duration::from_secs(600), reply_rx).await {
        Ok(Ok(ApprovalResponse::Selected(option_id))) => Ok(acp::RequestPermissionResponse::new(
            acp::RequestPermissionOutcome::Selected(acp::SelectedPermissionOutcome::new(acp::PermissionOptionId::new(
                option_id,
            ))),
        )),
        _ => Ok(cancelled()),
    }
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
            .map(|o| o.option_id.clone())
            .ok_or_else(acp::Error::method_not_found)?;

        let cancelled = || acp::RequestPermissionResponse::new(acp::RequestPermissionOutcome::Cancelled);
        let selected = |id: acp::PermissionOptionId| {
            acp::RequestPermissionResponse::new(acp::RequestPermissionOutcome::Selected(
                acp::SelectedPermissionOutcome::new(id),
            ))
        };

        match self.approval_policy {
            ApprovalPolicy::Deny => Ok(cancelled()),
            ApprovalPolicy::Approve | ApprovalPolicy::Ask => {
                let fs_read_paths = match fs_read_paths_from_meta(args.meta.as_ref()) {
                    Ok(paths) => paths,
                    Err(()) => return Ok(cancelled()),
                };
                if let Some(paths) = fs_read_paths {
                    let conversation = self.current_request.borrow().1.clone();
                    return match self.attachment_reads.evaluate(&conversation, &paths) {
                        AttachmentReadDecision::Allow => Ok(permission_option(&args, "allow_once")
                            .map(selected)
                            .unwrap_or_else(cancelled)),
                        AttachmentReadDecision::Deny => Ok(cancelled()),
                        AttachmentReadDecision::Unmanaged => request_slack_approval(self, &args).await,
                    };
                }
                if is_auto_approved_mcp_read(args.meta.as_ref()) {
                    Ok(selected(first_option))
                } else {
                    request_slack_approval(self, &args).await
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

    async fn kill_terminal(&self, _: acp::KillTerminalRequest) -> acp::Result<acp::KillTerminalResponse> {
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
                let id = tool_call.tool_call_id.to_string();
                let title = format!("{} {}", tool_emoji(&tool_call.kind), tool_call.title);
                self.progress_titles.borrow_mut().insert(id.clone(), title.clone());
                let status = match tool_call.status {
                    acp::ToolCallStatus::Pending => ProgressStatus::Pending,
                    acp::ToolCallStatus::InProgress => ProgressStatus::InProgress,
                    acp::ToolCallStatus::Completed => ProgressStatus::Complete,
                    acp::ToolCallStatus::Failed => ProgressStatus::Error,
                    _ => ProgressStatus::InProgress,
                };
                let terminal = matches!(status, ProgressStatus::Complete | ProgressStatus::Error);
                if let Some(sender) = self.progress.borrow().as_ref() {
                    if terminal || should_emit_progress(&self.last_progress_update) {
                        let _ = sender.send(ProgressUpdate { id, title, status });
                    } else {
                        self.acp_info.lock().unwrap().dropped_progress_updates += 1;
                    }
                }
                if terminal {
                    self.progress_titles
                        .borrow_mut()
                        .remove(&tool_call.tool_call_id.to_string());
                }
            },
            acp::SessionUpdate::ToolCallUpdate(tool_call) => {
                let id = tool_call.tool_call_id.to_string();
                if let Some(title) = tool_call.fields.title {
                    self.progress_titles.borrow_mut().insert(id.clone(), title);
                }
                let Some(status) = tool_call.fields.status else {
                    return Ok(());
                };
                let status = match status {
                    acp::ToolCallStatus::Pending => ProgressStatus::Pending,
                    acp::ToolCallStatus::InProgress => ProgressStatus::InProgress,
                    acp::ToolCallStatus::Completed => ProgressStatus::Complete,
                    acp::ToolCallStatus::Failed => ProgressStatus::Error,
                    _ => ProgressStatus::InProgress,
                };
                let title = self
                    .progress_titles
                    .borrow()
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| "Tool call".into());
                let terminal = matches!(status, ProgressStatus::Complete | ProgressStatus::Error);
                if let Some(sender) = self.progress.borrow().as_ref() {
                    if terminal || should_emit_progress(&self.last_progress_update) {
                        let _ = sender.send(ProgressUpdate {
                            id: id.clone(),
                            title,
                            status,
                        });
                    } else {
                        self.acp_info.lock().unwrap().dropped_progress_updates += 1;
                    }
                }
                if terminal {
                    self.progress_titles.borrow_mut().remove(&id);
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
    activity: Rc<Cell<WorkerActivityState>>,
    acp_info: Arc<Mutex<AcpInfo>>,
    child: Rc<RefCell<tokio::process::Child>>,
    io_alive: Rc<Cell<bool>>,
    failure: Rc<RefCell<Option<String>>>,
    stderr: Rc<RefCell<VecDeque<String>>>,
    log_context: Rc<RefCell<(String, String)>>,
    chunks: Rc<RefCell<Vec<String>>>,
    progress: Rc<RefCell<Option<mpsc::UnboundedSender<ProgressUpdate>>>>,
    progress_titles: Rc<RefCell<HashMap<String, String>>>,
    last_progress_update: Rc<Cell<Option<Instant>>>,
    current_conv: Rc<RefCell<(String, Option<String>, String)>>,
    _settings_overlay: Option<SettingsOverlay>,
}

struct PromptProgressGuard {
    progress: Rc<RefCell<Option<mpsc::UnboundedSender<ProgressUpdate>>>>,
}

struct SettingsOverlay {
    path: PathBuf,
}

impl SettingsOverlay {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for SettingsOverlay {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let mut lock_path = self.path.clone().into_os_string();
        lock_path.push(".lock");
        let _ = std::fs::remove_file(PathBuf::from(lock_path));
    }
}

impl Drop for PromptProgressGuard {
    fn drop(&mut self) {
        *self.progress.borrow_mut() = None;
    }
}

async fn shutdown_child(child: &Rc<RefCell<tokio::process::Child>>) {
    #[cfg(unix)]
    {
        let child_pid = { child.borrow().id() };
        if let Some(pid) = child_pid {
            let pgid = pid as libc::pid_t;
            unsafe { libc::killpg(pgid, libc::SIGTERM) };
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            if matches!(child.borrow_mut().try_wait(), Ok(None)) {
                unsafe { libc::killpg(pgid, libc::SIGKILL) };
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.borrow_mut().start_kill();
    }
}

#[async_trait::async_trait(?Send)]
impl Worker for AcpWorker {
    fn session_id(&self) -> String {
        self.session.to_string()
    }

    fn request_id(&self) -> String {
        self.log_context.borrow().0.clone()
    }

    fn begin_activity(&self) -> Result<WorkerActivity, String> {
        if matches!(self.activity.get(), WorkerActivityState::Busy) {
            return Err("Worker is already processing a request".into());
        }
        self.activity.set(WorkerActivityState::Busy);
        self.acp_info.lock().unwrap().busy_workers += 1;
        Ok(WorkerActivity {
            state: self.activity.clone(),
            acp_info: self.acp_info.clone(),
        })
    }

    fn idle_since(&self) -> Option<Instant> {
        match self.activity.get() {
            WorkerActivityState::IdleSince(since) => Some(since),
            WorkerActivityState::Busy => None,
        }
    }

    fn health(&self) -> WorkerHealth {
        match self.child.borrow_mut().try_wait() {
            Ok(Some(status)) => {
                let detail = format!("ACP process exited with {status}");
                self.io_alive.set(false);
                *self.failure.borrow_mut() = Some(detail);
            },
            Ok(None) => {},
            Err(error) => {
                self.io_alive.set(false);
                *self.failure.borrow_mut() = Some(format!("failed to inspect ACP process: {error}"));
            },
        }

        if self.io_alive.get() {
            WorkerHealth::Healthy
        } else {
            let mut detail = self
                .failure
                .borrow()
                .clone()
                .unwrap_or_else(|| "ACP IO loop stopped".into());
            if let Some(line) = self.stderr.borrow().back() {
                detail.push_str(&format!("; stderr: {line}"));
            }
            WorkerHealth::Dead(detail)
        }
    }

    fn set_request_context(&self, request_id: String, conversation_id: String) {
        *self.log_context.borrow_mut() = (request_id, conversation_id);
    }

    fn set_conv(&self, channel: String, thread_ts: Option<String>, slack_user_id: String) {
        *self.current_conv.borrow_mut() = (channel, thread_ts, slack_user_id);
    }

    async fn prompt(
        &self,
        messages: Vec<String>,
        progress_tx: mpsc::UnboundedSender<ProgressUpdate>,
    ) -> Result<String, String> {
        self.chunks.borrow_mut().clear();
        self.progress_titles.borrow_mut().clear();
        self.last_progress_update.set(None);
        *self.progress.borrow_mut() = Some(progress_tx);
        let _progress_guard = PromptProgressGuard {
            progress: self.progress.clone(),
        };
        let acp_messages: Vec<acp::ContentBlock> = messages
            .into_iter()
            .map(|s| acp::ContentBlock::Text(acp::TextContent::new(s)))
            .collect();
        match self
            .connection
            .prompt(acp::PromptRequest::new(self.session.clone(), acp_messages))
            .await
        {
            Ok(r) if r.stop_reason == acp::StopReason::Cancelled => Ok("❌ Cancelled".into()),
            Ok(_) => Ok(self.chunks.borrow().join("")),
            Err(error) => Err(error.to_string()),
        }
    }

    async fn cancel(&self) {
        let _ = self
            .connection
            .cancel(acp::CancelNotification::new(self.session.clone()))
            .await;
    }

    async fn set_mode(&self, mode: String) -> Result<String, String> {
        self.connection
            .set_session_mode(acp::SetSessionModeRequest::new(
                self.session.clone(),
                acp::SessionModeId::new(mode.clone()),
            ))
            .await
            .map(|_| format!("→ agent: {mode}"))
            .map_err(|e| format!("Error: {e}"))
    }

    async fn kill(&self) {
        shutdown_child(&self.child).await;
    }
}

fn source_settings_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var(SETTINGS_PATH_ENV)
        && !path.is_empty()
    {
        return Some(path.into());
    }
    let kiro_home = std::env::var("KIRO_HOME")
        .ok()
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".kiro")))?;
    Some(kiro_home.join("settings").join("cli.json"))
}

fn source_kiro_home() -> Option<PathBuf> {
    std::env::var(KIRO_HOME_ENV)
        .ok()
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".kiro")))
}

fn authoritative_agent_dir_in(required_mode: Option<&str>, kiro_home: &Path) -> Result<Option<PathBuf>, String> {
    if required_mode != Some(PINNED_AGENT_NAME) {
        return Ok(None);
    }
    let dir = kiro_home.join("agents");
    let config_path = dir.join(format!("{PINNED_AGENT_NAME}.json"));
    let config = std::fs::read_to_string(&config_path).map_err(|error| {
        format!(
            "Failed to read packaged ACP agent config {}: {error}",
            config_path.display()
        )
    })?;
    let config: serde_json::Value = serde_json::from_str(&config).map_err(|error| {
        format!(
            "Failed to parse packaged ACP agent config {}: {error}",
            config_path.display()
        )
    })?;
    if config.get("name").and_then(serde_json::Value::as_str) != Some(PINNED_AGENT_NAME) {
        return Err(format!(
            "Packaged ACP agent config {} must declare name '{PINNED_AGENT_NAME}'",
            config_path.display()
        ));
    }
    for entry in std::fs::read_dir(&dir).map_err(|error| {
        format!(
            "Failed to inspect packaged ACP agent directory {}: {error}",
            dir.display()
        )
    })? {
        let path = entry
            .map_err(|error| {
                format!(
                    "Failed to inspect packaged ACP agent directory {}: {error}",
                    dir.display()
                )
            })?
            .path();
        if path == config_path || path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let Ok(candidate) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(candidate) = serde_json::from_str::<serde_json::Value>(&candidate) else {
            continue;
        };
        if candidate.get("name").and_then(serde_json::Value::as_str) == Some(PINNED_AGENT_NAME) {
            return Err(format!(
                "Packaged ACP agent directory {} contains duplicate agent name '{PINNED_AGENT_NAME}' in {}",
                dir.display(),
                path.display()
            ));
        }
    }
    let prompt_uri = config
        .get("prompt")
        .and_then(serde_json::Value::as_str)
        .and_then(|prompt| prompt.strip_prefix("file://"))
        .filter(|path| !path.is_empty())
        .ok_or_else(|| {
            format!(
                "Packaged ACP agent config {} must declare a file prompt",
                config_path.display()
            )
        })?;
    let prompt_path = Path::new(prompt_uri);
    if prompt_path.is_absolute() || prompt_uri.starts_with('~') || prompt_uri.starts_with('$') {
        return Err(format!("Packaged ACP agent prompt must stay inside {}", dir.display()));
    }
    let resolved_dir = std::fs::canonicalize(&dir).map_err(|error| {
        format!(
            "Failed to resolve packaged ACP agent directory {}: {error}",
            dir.display()
        )
    })?;
    let prompt_path = std::fs::canonicalize(dir.join(prompt_path))
        .map_err(|error| format!("Failed to resolve packaged ACP agent prompt {prompt_uri}: {error}"))?;
    if !prompt_path.starts_with(&resolved_dir) {
        return Err(format!("Packaged ACP agent prompt must stay inside {}", dir.display()));
    }
    let prompt = std::fs::read_to_string(&prompt_path).map_err(|error| {
        format!(
            "Failed to read packaged ACP agent prompt {}: {error}",
            prompt_path.display()
        )
    })?;
    if prompt.trim().is_empty() {
        return Err(format!("Packaged ACP agent prompt is empty: {}", prompt_path.display()));
    }
    Ok(Some(dir))
}

fn authoritative_agent_dir(required_mode: Option<&str>) -> Result<Option<PathBuf>, String> {
    if required_mode != Some(PINNED_AGENT_NAME) {
        return Ok(None);
    }
    let kiro_home =
        source_kiro_home().ok_or_else(|| "Cannot locate the packaged kiro-help agent directory".to_string())?;
    authoritative_agent_dir_in(required_mode, &kiro_home)
}

fn write_settings_overlay(
    writer: &mut impl std::io::Write,
    settings: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    serde_json::to_writer_pretty(&mut *writer, settings)
        .map_err(|error| format!("Failed to write ACP settings overlay: {error}"))?;
    writer
        .write_all(b"\n")
        .and_then(|()| writer.flush())
        .map_err(|error| format!("Failed to flush ACP settings overlay: {error}"))
}

fn create_settings_overlay(source: Option<&Path>, temp_dir: &Path) -> Result<SettingsOverlay, String> {
    let mut settings = match source.map(std::fs::read).transpose() {
        Ok(Some(contents)) => serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(&contents)
            .map_err(|error| format!("Failed to parse ACP settings: {error}"))?,
        Ok(None) => serde_json::Map::new(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => serde_json::Map::new(),
        Err(error) => return Err(format!("Failed to read ACP settings: {error}")),
    };
    settings.insert(DISABLE_DEFAULT_RESOURCES_KEY.into(), serde_json::Value::Bool(true));

    let (path, mut file) = loop {
        let id = NEXT_SETTINGS_OVERLAY_ID.fetch_add(1, Ordering::Relaxed);
        let path = temp_dir.join(format!("kiro-bot-settings-{}-{id}.json", std::process::id()));
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(file) => break (path, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("Failed to create ACP settings overlay: {error}")),
        }
    };
    let overlay = SettingsOverlay { path };
    write_settings_overlay(&mut file, &settings)?;
    file.sync_all()
        .map_err(|error| format!("Failed to flush ACP settings overlay: {error}"))?;
    Ok(overlay)
}

fn acp_command(command: &str, required_mode: Option<&str>) -> Result<(String, Vec<String>), String> {
    let mut parts = command.split_whitespace();
    let executable = parts.next().ok_or_else(|| "ACP command is empty".to_string())?;
    let mut args = parts.map(str::to_string).collect::<Vec<_>>();

    if let Some(required_mode) = required_mode {
        let configured_mode = args
            .iter()
            .position(|arg| arg == "--agent" || arg.starts_with("--agent="))
            .map(|index| {
                let arg = &args[index];
                let value = arg
                    .strip_prefix("--agent=")
                    .map(str::to_string)
                    .or_else(|| args.get(index + 1).filter(|value| !value.starts_with('-')).cloned());
                value.ok_or_else(|| "ACP command has an --agent flag without a value".to_string())
            })
            .transpose()?;
        match configured_mode {
            Some(configured_mode) if configured_mode != required_mode => {
                return Err(format!(
                    "ACP command selects agent '{configured_mode}', but bot requires '{required_mode}'"
                ));
            },
            Some(_) => {},
            None => {
                args.push("--agent".into());
                args.push(required_mode.into());
            },
        }
    }

    Ok((executable.into(), args))
}

fn required_mode_change(modes: Option<&acp::SessionModeState>, required_mode: &str) -> Result<bool, String> {
    let modes = modes.ok_or_else(|| {
        format!("Configured ACP mode '{required_mode}' cannot be verified because the server returned no modes")
    })?;
    if !modes
        .available_modes
        .iter()
        .any(|mode| mode.id.to_string() == required_mode)
    {
        let available = modes
            .available_modes
            .iter()
            .map(|mode| mode.id.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "Configured ACP mode '{required_mode}' is unavailable; available modes: {available}"
        ));
    }
    Ok(modes.current_mode_id.to_string() != required_mode)
}

fn verify_default_resources_disabled(response: &acp::ExtResponse) -> Result<(), String> {
    let settings = serde_json::from_str::<serde_json::Value>(response.0.get())
        .map_err(|error| format!("Failed to inspect effective ACP settings: {error}"))?;
    if settings.get(DISABLE_DEFAULT_RESOURCES_KEY) == Some(&serde_json::Value::Bool(true)) {
        Ok(())
    } else {
        Err(format!(
            "Effective ACP setting '{DISABLE_DEFAULT_RESOURCES_KEY}' must be true for a pinned bot agent"
        ))
    }
}

async fn spawn_acp_worker(cfg: &AcpConfig, acp_info: &Arc<Mutex<AcpInfo>>) -> Result<AcpWorker, String> {
    let (executable, args) = acp_command(&cfg.command, cfg.default_mode.as_deref())?;
    let authoritative_agent_dir = authoritative_agent_dir(cfg.default_mode.as_deref())?;
    let settings_overlay = cfg
        .default_mode
        .as_ref()
        .map(|_| create_settings_overlay(source_settings_path().as_deref(), &std::env::temp_dir()))
        .transpose()?;
    let mut cmd = tokio::process::Command::new(executable);
    cmd.args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    if let Some(settings_overlay) = &settings_overlay {
        cmd.env(SETTINGS_PATH_ENV, settings_overlay.path());
    }
    if let Some(authoritative_agent_dir) = &authoritative_agent_dir {
        cmd.env(AGENT_CONFIG_DIR_ENV, authoritative_agent_dir);
        let kiro_home = authoritative_agent_dir.parent().ok_or_else(|| {
            format!(
                "Packaged ACP agent directory has no parent: {}",
                authoritative_agent_dir.display()
            )
        })?;
        cmd.env(KIRO_HOME_ENV, kiro_home);
    }
    #[cfg(unix)]
    cmd.process_group(0);
    let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn ACP: {e}"))?;

    let chunks: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let progress: Rc<RefCell<Option<mpsc::UnboundedSender<ProgressUpdate>>>> = Rc::new(RefCell::new(None));
    let progress_titles = Rc::new(RefCell::new(HashMap::new()));
    let last_progress_update = Rc::new(Cell::new(None));
    let mcp_ready_count: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    let mcp_notify = Rc::new(tokio::sync::Notify::new());
    let current_conv: Rc<RefCell<(String, Option<String>, String)>> =
        Rc::new(RefCell::new((String::new(), None, String::new())));
    let io_alive = Rc::new(Cell::new(true));
    let failure = Rc::new(RefCell::new(None));
    let stderr = Rc::new(RefCell::new(VecDeque::with_capacity(STDERR_HISTORY_LIMIT)));
    let log_context = Rc::new(RefCell::new(("startup".into(), "unassigned".into())));

    let child_stdin = child
        .stdin
        .take()
        .ok_or_else(|| "ACP stdin was not piped".to_string())?;
    let child_stdout = child
        .stdout
        .take()
        .ok_or_else(|| "ACP stdout was not piped".to_string())?;
    let child_stderr = child
        .stderr
        .take()
        .ok_or_else(|| "ACP stderr was not piped".to_string())?;

    let (connection, handle_io) = acp::ClientSideConnection::new(
        AcpClient {
            chunks: chunks.clone(),
            progress: progress.clone(),
            progress_titles: progress_titles.clone(),
            last_progress_update: last_progress_update.clone(),
            mcp_ready_count: mcp_ready_count.clone(),
            mcp_notify: mcp_notify.clone(),
            acp_info: acp_info.clone(),
            approval_policy: cfg.approval_policy,
            approval_tx: cfg.approval_tx.clone(),
            attachment_reads: cfg.attachment_reads.clone(),
            current_request: log_context.clone(),
            current_conv: current_conv.clone(),
        },
        child_stdin.compat_write(),
        child_stdout.compat(),
        |fut| {
            tokio::task::spawn_local(fut);
        },
    );
    {
        let io_alive = io_alive.clone();
        let failure = failure.clone();
        let log_context = log_context.clone();
        tokio::task::spawn_local(async move {
            let result = handle_io.await;
            io_alive.set(false);
            let detail = format!("ACP IO loop exited: {result:?}");
            *failure.borrow_mut() = Some(detail.clone());
            let (request_id, conversation_id) = log_context.borrow().clone();
            warn!(%request_id, %conversation_id, %detail);
        });
    }
    {
        let stderr = stderr.clone();
        let log_context = log_context.clone();
        tokio::task::spawn_local(async move {
            let mut lines = BufReader::new(child_stderr).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        let (request_id, conversation_id) = log_context.borrow().clone();
                        debug!(
                            target: "kiro_bot::acp_stderr",
                            %request_id,
                            %conversation_id,
                            %line,
                            "ACP stderr"
                        );
                        let mut recent = stderr.borrow_mut();
                        if recent.len() == STDERR_HISTORY_LIMIT {
                            recent.pop_front();
                        }
                        recent.push_back(line);
                    },
                    Ok(None) => break,
                    Err(error) => {
                        let (request_id, conversation_id) = log_context.borrow().clone();
                        warn!(%request_id, %conversation_id, %error, "failed to read ACP stderr");
                        break;
                    },
                }
            }
        });
    }
    let connection = Rc::new(connection);

    connection
        .initialize(acp::InitializeRequest::new(acp::ProtocolVersion::V1).client_info(Some(
            acp::Implementation::new("kiro-bot", env!("CARGO_PKG_VERSION")).title(Some("Kiro Bot".to_string())),
        )))
        .await
        .map_err(|e| format!("ACP init failed: {e}"))?;

    if settings_overlay.is_some() {
        let params = acp::RawValue::from_string("{}".into())
            .map_err(|error| format!("Failed to encode ACP settings request: {error}"))?;
        let settings = connection
            .ext_method(acp::ExtRequest::new("kiro.dev/settings/list", params.into()))
            .await
            .map_err(|error| format!("Failed to inspect effective ACP settings: {error}"))?;
        verify_default_resources_disabled(&settings)?;
    }

    let resp = connection
        .new_session(acp::NewSessionRequest::new(std::env::current_dir().unwrap()))
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
    }

    let active_mode = if let Some(agent) = &cfg.default_mode {
        if required_mode_change(resp.modes.as_ref(), agent)?
            && let Err(error) = connection
                .set_session_mode(acp::SetSessionModeRequest::new(
                    resp.session_id.clone(),
                    acp::SessionModeId::new(agent.clone()),
                ))
                .await
        {
            return Err(format!("Failed to select configured ACP mode '{agent}': {error}"));
        }
        Some(agent.clone())
    } else {
        resp.modes
            .as_ref()
            .map(|mode_state| mode_state.current_mode_id.to_string())
    };
    if let Some(active_mode) = active_mode {
        acp_info
            .lock()
            .unwrap()
            .session_modes
            .insert(resp.session_id.to_string(), active_mode);
    }

    // Wait for the selected mode's MCP servers to initialize.
    while tokio::time::timeout(Duration::from_millis(cfg.mcp_wait_ms), mcp_notify.notified())
        .await
        .is_ok()
    {}

    let child = Rc::new(RefCell::new(child));
    {
        let child = child.clone();
        let io_alive = io_alive.clone();
        let failure = failure.clone();
        let log_context = log_context.clone();
        tokio::task::spawn_local(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                match child.borrow_mut().try_wait() {
                    Ok(Some(status)) => {
                        io_alive.set(false);
                        let detail = format!("ACP process exited with {status}");
                        *failure.borrow_mut() = Some(detail.clone());
                        let (request_id, conversation_id) = log_context.borrow().clone();
                        warn!(%request_id, %conversation_id, %detail);
                        break;
                    },
                    Ok(None) => {},
                    Err(error) => {
                        io_alive.set(false);
                        let detail = format!("failed to inspect ACP process: {error}");
                        *failure.borrow_mut() = Some(detail.clone());
                        let (request_id, conversation_id) = log_context.borrow().clone();
                        warn!(%request_id, %conversation_id, %detail);
                        break;
                    },
                }
            }
        });
    }

    Ok(AcpWorker {
        connection,
        session: resp.session_id,
        activity: Rc::new(Cell::new(WorkerActivityState::IdleSince(Instant::now()))),
        acp_info: acp_info.clone(),
        child,
        io_alive,
        failure,
        stderr,
        log_context,
        chunks,
        progress,
        progress_titles,
        last_progress_update,
        current_conv,
        _settings_overlay: settings_overlay,
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
    spawning: Rc<Cell<usize>>,
    acp_info: Arc<Mutex<AcpInfo>>,
}

struct SpawnReservation {
    spawning: Rc<Cell<usize>>,
}

impl Drop for SpawnReservation {
    fn drop(&mut self) {
        self.spawning.set(self.spawning.get().saturating_sub(1));
    }
}

impl AcpPool {
    fn new(
        factory: Rc<dyn WorkerFactory>,
        default_worker: Option<Rc<dyn Worker>>,
        max: usize,
        acp_info: Arc<Mutex<AcpInfo>>,
    ) -> Self {
        Self {
            workers: RefCell::new(HashMap::new()),
            default_worker: RefCell::new(default_worker),
            factory,
            max,
            spawning: Rc::new(Cell::new(0)),
            acp_info,
        }
    }

    #[allow(clippy::await_holding_refcell_ref)]
    async fn reap_idle(&self, timeout: Duration) {
        let candidates: Vec<(String, Rc<dyn Worker>)> = self
            .workers
            .borrow()
            .iter()
            .map(|(key, worker)| (key.clone(), worker.clone()))
            .collect();
        for (key, expected) in candidates {
            let Some((worker, failure)) = self.take_reapable(&key, &expected, timeout) else {
                continue;
            };
            let session_id = worker.session_id();
            let request_id = worker.request_id();
            if let Some(failure) = failure {
                warn!(
                    %request_id,
                    conversation_id = %key,
                    %session_id,
                    %failure,
                    "Evicting dead ACP worker"
                );
                let mut info = self.acp_info.lock().unwrap();
                info.worker_restarts += 1;
                info.last_failure = Some(failure);
            } else {
                info!(
                    %request_id,
                    conversation_id = %key,
                    %session_id,
                    "Reaping idle worker"
                );
            }
            worker.kill().await;
            let mut info = self.acp_info.lock().unwrap();
            info.workers = info.workers.saturating_sub(1);
        }
    }

    fn take_reapable(
        &self,
        conversation: &str,
        expected: &Rc<dyn Worker>,
        timeout: Duration,
    ) -> Option<(Rc<dyn Worker>, Option<String>)> {
        let worker = self.workers.borrow().get(conversation).cloned()?;
        if !Rc::ptr_eq(&worker, expected) {
            return None;
        }
        let failure = match worker.health() {
            WorkerHealth::Dead(reason) => Some(reason),
            WorkerHealth::Healthy => {
                let since = worker.idle_since()?;
                if Instant::now().duration_since(since) <= timeout {
                    return None;
                }
                None
            },
        };
        self.workers
            .borrow_mut()
            .remove(conversation)
            .map(|worker| (worker, failure))
    }

    async fn shutdown(&self) {
        let keys: Vec<String> = self.workers.borrow().keys().cloned().collect();
        for key in &keys {
            self.remove(key).await;
        }
        let default = self.default_worker.borrow_mut().take();
        if let Some(w) = default {
            w.kill().await;
            let mut info = self.acp_info.lock().unwrap();
            info.workers = info.workers.saturating_sub(1);
        }
    }

    async fn remove_if(&self, conversation: &str, expected: &Rc<dyn Worker>) -> bool {
        let worker = {
            let mut workers = self.workers.borrow_mut();
            match workers.get(conversation) {
                Some(current) if Rc::ptr_eq(current, expected) => workers.remove(conversation),
                _ => None,
            }
        };
        if let Some(worker) = worker {
            worker.kill().await;
            let mut info = self.acp_info.lock().unwrap();
            info.workers = info.workers.saturating_sub(1);
            true
        } else {
            false
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
        let existing_worker = self.workers.borrow().get(conversation).cloned();
        if let Some(worker) = existing_worker {
            match worker.health() {
                WorkerHealth::Healthy => return Ok(worker),
                WorkerHealth::Dead(failure) => {
                    warn!(
                        request_id = %worker.request_id(),
                        conversation_id = %conversation,
                        session_id = %worker.session_id(),
                        %failure,
                        "Replacing dead ACP worker"
                    );
                    {
                        let mut info = self.acp_info.lock().unwrap();
                        info.worker_restarts += 1;
                        info.last_failure = Some(failure);
                    }
                    self.remove(conversation).await;
                },
            }
        }
        let default_worker = self.default_worker.borrow_mut().take();
        if let Some(worker) = default_worker {
            match worker.health() {
                WorkerHealth::Healthy => {
                    self.workers
                        .borrow_mut()
                        .insert(conversation.to_string(), worker.clone());
                    return Ok(worker);
                },
                WorkerHealth::Dead(failure) => {
                    warn!(
                        request_id = %worker.request_id(),
                        conversation_id = "unassigned",
                        %failure,
                        "Replacing dead warm ACP worker"
                    );
                    worker.kill().await;
                    let mut info = self.acp_info.lock().unwrap();
                    info.worker_restarts += 1;
                    info.workers = info.workers.saturating_sub(1);
                    info.last_failure = Some(failure);
                },
            }
        }
        if self.workers.borrow().len() + self.spawning.get() >= self.max {
            return Err("⏳ All workers busy — try again shortly".into());
        }
        self.spawning.set(self.spawning.get() + 1);
        let _reservation = SpawnReservation {
            spawning: self.spawning.clone(),
        };
        let w = self.factory.spawn().await?;
        self.workers.borrow_mut().insert(conversation.to_string(), w.clone());
        Ok(w)
    }

    async fn remove(&self, conversation: &str) -> bool {
        let worker = self.workers.borrow_mut().remove(conversation);
        if let Some(w) = worker {
            w.kill().await;
            let mut info = self.acp_info.lock().unwrap();
            info.workers = info.workers.saturating_sub(1);
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
        let worker: Rc<dyn Worker> = Rc::new(spawn_acp_worker(&self.cfg, &self.acp_info).await?);
        self.acp_info.lock().unwrap().workers += 1;
        Ok(worker)
    }
}

// ---------------------------------------------------------------------------
// Work loop
// ---------------------------------------------------------------------------

fn prompt_timeout_from_env() -> Duration {
    match std::env::var(PROMPT_TIMEOUT_ENV) {
        Ok(raw) => match raw.parse::<u64>() {
            Ok(seconds) if seconds > 0 => Duration::from_secs(seconds),
            _ => {
                warn!(
                    env = PROMPT_TIMEOUT_ENV,
                    %raw,
                    default_secs = DEFAULT_PROMPT_TIMEOUT_SECS,
                    "invalid prompt timeout; using default"
                );
                Duration::from_secs(DEFAULT_PROMPT_TIMEOUT_SECS)
            },
        },
        Err(_) => Duration::from_secs(DEFAULT_PROMPT_TIMEOUT_SECS),
    }
}

#[allow(clippy::too_many_arguments)]
async fn execute_prompt(
    pool: Rc<AcpPool>,
    request_id: String,
    conversation: String,
    channel: String,
    thread_ts: Option<String>,
    slack_user_id: String,
    messages: Vec<String>,
    progress_tx: mpsc::UnboundedSender<ProgressUpdate>,
    prompt_timeout: Duration,
    acp_info: Arc<Mutex<AcpInfo>>,
) -> String {
    let started_at = Instant::now();
    info!(
        %request_id,
        conversation_id = %conversation,
        timeout_secs = prompt_timeout.as_secs(),
        "ACP prompt started"
    );
    let mut preflight_retry_available = true;
    loop {
        let worker = match pool.get_or_spawn(&conversation).await {
            Ok(worker) => worker,
            Err(error) => {
                warn!(
                    %request_id,
                    conversation_id = %conversation,
                    %error,
                    "failed to allocate ACP worker"
                );
                return error;
            },
        };
        worker.set_request_context(request_id.clone(), conversation.clone());
        worker.set_conv(channel.clone(), thread_ts.clone(), slack_user_id.clone());
        let activity = match worker.begin_activity() {
            Ok(activity) => activity,
            Err(error) => {
                warn!(
                    %request_id,
                    conversation_id = %conversation,
                    session_id = %worker.session_id(),
                    %error,
                    "failed to start ACP worker activity"
                );
                return format!("Error: {error}");
            },
        };

        if let WorkerHealth::Dead(failure) = worker.health() {
            drop(activity);
            warn!(
                %request_id,
                conversation_id = %conversation,
                session_id = %worker.session_id(),
                %failure,
                "ACP worker died before prompt dispatch"
            );
            {
                let mut info = acp_info.lock().unwrap();
                info.worker_restarts += 1;
                info.last_failure = Some(failure);
            }
            pool.remove(&conversation).await;
            if preflight_retry_available {
                preflight_retry_available = false;
                continue;
            }
            return "Error: ACP worker was unavailable after one restart".into();
        }

        let mut prompt = Box::pin(worker.prompt(messages.clone(), progress_tx.clone()));
        let result = match tokio::time::timeout(prompt_timeout, prompt.as_mut()).await {
            Ok(result) => result,
            Err(_) => {
                {
                    let mut info = acp_info.lock().unwrap();
                    info.prompt_timeouts += 1;
                    info.last_failure = Some(format!("prompt exceeded {}s deadline", prompt_timeout.as_secs()));
                }
                warn!(
                    %request_id,
                    conversation_id = %conversation,
                    session_id = %worker.session_id(),
                    timeout_secs = prompt_timeout.as_secs(),
                    "prompt deadline exceeded; cancelling ACP request"
                );
                let cancel_sent = tokio::time::timeout(PROMPT_CANCEL_GRACE, worker.cancel()).await.is_ok();
                if !cancel_sent
                    || tokio::time::timeout(PROMPT_CANCEL_GRACE, prompt.as_mut())
                        .await
                        .is_err()
                {
                    warn!(
                        %request_id,
                        conversation_id = %conversation,
                        session_id = %worker.session_id(),
                        "ACP request did not stop after cancellation; evicting worker"
                    );
                    pool.remove(&conversation).await;
                }
                drop(activity);
                info!(
                    %request_id,
                    conversation_id = %conversation,
                    elapsed_ms = started_at.elapsed().as_millis(),
                    outcome = "timeout",
                    "ACP prompt finished"
                );
                return format!(
                    "Error: Request exceeded the {} second deadline and was cancelled",
                    prompt_timeout.as_secs()
                );
            },
        };

        drop(activity);
        let reply = match result {
            Ok(reply) => reply,
            Err(error) => match worker.health() {
                WorkerHealth::Healthy => format!("Error: {error}"),
                WorkerHealth::Dead(failure) => {
                    warn!(
                        %request_id,
                        conversation_id = %conversation,
                        session_id = %worker.session_id(),
                        %failure,
                        "ACP worker died during prompt; evicting without retry"
                    );
                    {
                        let mut info = acp_info.lock().unwrap();
                        info.last_failure = Some(failure.clone());
                    }
                    pool.remove(&conversation).await;
                    "Error: ACP worker stopped during the request; please retry".into()
                },
            },
        };
        info!(
            %request_id,
            conversation_id = %conversation,
            elapsed_ms = started_at.elapsed().as_millis(),
            outcome = if reply.starts_with("Error:") { "error" } else { "completed" },
            "ACP prompt finished"
        );
        return reply;
    }
}

struct PromptCompletion {
    conversation: String,
    generation: u64,
    reply: String,
    reply_tx: oneshot::Sender<String>,
}

struct ActivePrompt {
    generation: u64,
    task: tokio::task::AbortHandle,
    stopping: bool,
    task_finished: bool,
    stop_finished: bool,
    pending_completion: Option<PromptCompletion>,
    cancel_waiters: Vec<oneshot::Sender<()>>,
}

struct StopPromptResult {
    conversation: String,
    generation: u64,
}

async fn wait_for_task_stop(task: &tokio::task::AbortHandle) -> bool {
    tokio::time::timeout(PROMPT_TASK_STOP_TIMEOUT, async {
        while !task.is_finished() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .is_ok()
}

async fn stop_prompt_inner(pool: &AcpPool, conversation: &str, prompt_task: &tokio::task::AbortHandle) -> bool {
    if let Some(worker) = pool.get(conversation) {
        let cancel_sent = tokio::time::timeout(CANCEL_NOTIFY_TIMEOUT, worker.cancel())
            .await
            .is_ok();
        let settled = cancel_sent
            && tokio::time::timeout(CANCEL_SETTLE_TIMEOUT, async {
                while worker.idle_since().is_none() {
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            })
            .await
            .is_ok();
        if !settled {
            warn!(
                request_id = %worker.request_id(),
                conversation_id = %conversation,
                session_id = %worker.session_id(),
                "ACP cancellation did not stop the prompt; evicting worker"
            );
            pool.remove_if(conversation, &worker).await;
        }
    } else {
        prompt_task.abort();
    }

    if wait_for_task_stop(prompt_task).await {
        return true;
    }
    prompt_task.abort();
    wait_for_task_stop(prompt_task).await
}

async fn stop_prompt(
    pool: Rc<AcpPool>,
    conversation: String,
    generation: u64,
    prompt_task: tokio::task::AbortHandle,
) -> StopPromptResult {
    let fallback_task = prompt_task.clone();
    let stopped = match tokio::time::timeout(
        STOP_PROMPT_TIMEOUT,
        stop_prompt_inner(&pool, &conversation, &prompt_task),
    )
    .await
    {
        Ok(stopped) => stopped,
        Err(_) => {
            fallback_task.abort();
            wait_for_task_stop(&fallback_task).await
        },
    };
    if !stopped {
        warn!(
            conversation_id = %conversation,
            generation,
            timeout_secs = STOP_PROMPT_TIMEOUT.as_secs(),
            "ACP prompt stop deadline exceeded"
        );
    }
    StopPromptResult {
        conversation,
        generation,
    }
}

fn begin_prompt_stop(
    pool: Rc<AcpPool>,
    active_prompts: &mut HashMap<String, ActivePrompt>,
    stops: &mut tokio::task::JoinSet<StopPromptResult>,
    conversation: String,
    waiter: Option<oneshot::Sender<()>>,
) {
    let Some(active) = active_prompts.get_mut(&conversation) else {
        if let Some(waiter) = waiter {
            let _ = waiter.send(());
        }
        return;
    };
    if let Some(waiter) = waiter {
        active.cancel_waiters.push(waiter);
    }
    if active.stopping {
        return;
    }
    active.stopping = true;
    let generation = active.generation;
    let prompt_task = active.task.clone();
    stops.spawn_local(stop_prompt(pool, conversation, generation, prompt_task));
}

fn try_finish_prompt_stop(active_prompts: &mut HashMap<String, ActivePrompt>, conversation: &str, generation: u64) {
    let ready = active_prompts.get(conversation).is_some_and(|active| {
        active.generation == generation && active.stopping && active.task_finished && active.stop_finished
    });
    if !ready {
        return;
    }

    let active = active_prompts.remove(conversation).unwrap();
    for waiter in active.cancel_waiters {
        let _ = waiter.send(());
    }
    if let Some(completion) = active.pending_completion {
        let _ = completion.reply_tx.send(completion.reply);
    }
}

fn publish_prompt_completion(active_prompts: &mut HashMap<String, ActivePrompt>, completion: PromptCompletion) {
    let conversation = completion.conversation.clone();
    let generation = completion.generation;
    let is_current = active_prompts
        .get(&conversation)
        .is_some_and(|active| active.generation == generation);
    if !is_current {
        return;
    }
    if active_prompts.get(&conversation).is_some_and(|active| active.stopping) {
        let active = active_prompts.get_mut(&conversation).unwrap();
        active.task_finished = true;
        active.pending_completion = Some(completion);
        try_finish_prompt_stop(active_prompts, &conversation, generation);
        return;
    }

    active_prompts.remove(&conversation);
    let _ = completion.reply_tx.send(completion.reply);
}

fn finish_prompt_stop(active_prompts: &mut HashMap<String, ActivePrompt>, result: StopPromptResult) {
    let is_current = active_prompts
        .get(&result.conversation)
        .is_some_and(|active| active.generation == result.generation);
    if !is_current {
        return;
    }

    active_prompts.get_mut(&result.conversation).unwrap().stop_finished = true;
    try_finish_prompt_stop(active_prompts, &result.conversation, result.generation);
}

fn finish_failed_prompt_task(
    active_prompts: &mut HashMap<String, ActivePrompt>,
    task_keys: &mut HashMap<tokio::task::Id, (String, u64)>,
    task_id: tokio::task::Id,
) {
    let Some((conversation, generation)) = task_keys.remove(&task_id) else {
        return;
    };
    let is_current = active_prompts
        .get(&conversation)
        .is_some_and(|active| active.generation == generation);
    if !is_current {
        return;
    }
    if active_prompts.get(&conversation).is_some_and(|active| active.stopping) {
        active_prompts.get_mut(&conversation).unwrap().task_finished = true;
        try_finish_prompt_stop(active_prompts, &conversation, generation);
    } else {
        active_prompts.remove(&conversation);
    }
}

async fn run_work_loop(
    pool: Rc<AcpPool>,
    mut work_receiver: mpsc::UnboundedReceiver<Work>,
    acp_info: Arc<Mutex<AcpInfo>>,
    idle_timeout: Duration,
    prompt_timeout: Duration,
) {
    let mut prompts = tokio::task::JoinSet::new();
    let mut prompt_stops = tokio::task::JoinSet::new();
    let mut reap_tasks = tokio::task::JoinSet::new();
    let mut active_prompts: HashMap<String, ActivePrompt> = HashMap::new();
    let mut task_keys = HashMap::new();
    let mut next_generation = 0_u64;
    let mut reaper = tokio::time::interval(Duration::from_secs(60));
    reaper.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let (grace, reply_tx) = loop {
        let work = tokio::select! {
            joined = prompts.join_next_with_id(), if !prompts.is_empty() => {
                match joined {
                    Some(Ok((task_id, completion))) => {
                        task_keys.remove(&task_id);
                        publish_prompt_completion(&mut active_prompts, completion);
                    },
                    Some(Err(error)) => {
                        finish_failed_prompt_task(&mut active_prompts, &mut task_keys, error.id());
                        if !error.is_cancelled() {
                            warn!(%error, "ACP prompt task failed");
                            let mut info = acp_info.lock().unwrap();
                            info.last_failure = Some(format!("prompt task failed: {error}"));
                        }
                    },
                    None => {},
                }
                continue;
            },
            stopped = prompt_stops.join_next(), if !prompt_stops.is_empty() => {
                match stopped {
                    Some(Ok(result)) => finish_prompt_stop(&mut active_prompts, result),
                    Some(Err(error)) => warn!(%error, "ACP prompt stop task failed"),
                    None => {},
                }
                continue;
            },
            reaped = reap_tasks.join_next(), if !reap_tasks.is_empty() => {
                if let Some(Err(error)) = reaped {
                    warn!(%error, "ACP idle reaper task failed");
                }
                continue;
            },
            _ = reaper.tick(), if reap_tasks.is_empty() => {
                let pool = pool.clone();
                reap_tasks.spawn_local(async move {
                    pool.reap_idle(idle_timeout).await;
                });
                continue;
            },
            work = work_receiver.recv() => work,
        };
        let Some(work) = work else {
            break (Duration::from_secs(30), None);
        };
        match work {
            Work::Prompt {
                input,
                conversation,
                reply_tx,
                progress_tx,
            } => {
                if active_prompts.contains_key(&conversation) {
                    let _ = reply_tx.send("Error: A request is already active for this conversation".into());
                    continue;
                }
                next_generation = next_generation.wrapping_add(1).max(1);
                let generation = next_generation;
                let request_id = next_request_id();
                let pool = pool.clone();
                let acp_info = acp_info.clone();
                let task_conversation = conversation.clone();
                let completion_conversation = conversation.clone();
                let prompt = prompts.spawn_local(async move {
                    let reply = match input.resolve().await {
                        Ok(PromptPayload {
                            text,
                            context,
                            channel,
                            thread_ts,
                            user,
                            slack_user_id,
                        }) => {
                            let mut messages = vec![];
                            if let Some(ctx) = crate::engine::core::format_context(&context) {
                                messages.push(ctx);
                            }
                            messages.push(format!("Slack message from {user} ({user}): {text}"));
                            execute_prompt(
                                pool,
                                request_id,
                                task_conversation,
                                channel,
                                thread_ts,
                                slack_user_id,
                                messages,
                                progress_tx,
                                prompt_timeout,
                                acp_info,
                            )
                            .await
                        },
                        Err(error) => format!("Error: {error}"),
                    };
                    PromptCompletion {
                        conversation: completion_conversation,
                        generation,
                        reply,
                        reply_tx,
                    }
                });
                task_keys.insert(prompt.id(), (conversation.clone(), generation));
                active_prompts.insert(conversation, ActivePrompt {
                    generation,
                    task: prompt,
                    stopping: false,
                    task_finished: false,
                    stop_finished: false,
                    pending_completion: None,
                    cancel_waiters: Vec::new(),
                });
            },
            Work::NewSession { conversation, reply_tx } => {
                let msg = if active_prompts.contains_key(&conversation) {
                    "⏳ A request is in progress — cancel it before resetting the session"
                } else {
                    match pool.get(&conversation) {
                        Some(worker) if worker.idle_since().is_none() => {
                            "⏳ A request is in progress — cancel it before resetting the session"
                        },
                        Some(_) => {
                            pool.remove(&conversation).await;
                            "✨ Session reset — next message will start fresh"
                        },
                        _ => "No active session",
                    }
                };
                let _ = reply_tx.send(msg.into());
            },
            Work::SetMode {
                conversation,
                mode,
                reply_tx,
            } => {
                let msg = if active_prompts.contains_key(&conversation) {
                    "⏳ A request is in progress — cancel it before changing agents".into()
                } else if let Some(w) = pool.get(&conversation) {
                    if w.idle_since().is_none() {
                        "⏳ A request is in progress — cancel it before changing agents".into()
                    } else {
                        let activity = w.begin_activity();
                        match activity {
                            Ok(_activity) => match tokio::time::timeout(PROMPT_CANCEL_GRACE, w.set_mode(mode)).await {
                                Ok(result) => result.unwrap_or_else(|error| error),
                                Err(_) => {
                                    pool.remove(&conversation).await;
                                    "Error: Agent change timed out; the session was reset".into()
                                },
                            },
                            Err(error) => format!("Error: {error}"),
                        }
                    }
                } else {
                    "No session — send a message first".into()
                };
                let _ = reply_tx.send(msg);
            },
            Work::SetModel {
                conversation, reply_tx, ..
            } => {
                let busy = active_prompts.contains_key(&conversation)
                    || pool
                        .get(&conversation)
                        .is_some_and(|worker| worker.idle_since().is_none());
                let message = if busy {
                    "⏳ A request is in progress — cancel it before changing models"
                } else {
                    "set_session_model not available"
                };
                let _ = reply_tx.send(message.into());
            },
            Work::Cancel { conversation } => {
                begin_prompt_stop(pool.clone(), &mut active_prompts, &mut prompt_stops, conversation, None);
            },
            Work::CancelAndWait { conversation, reply_tx } => {
                begin_prompt_stop(
                    pool.clone(),
                    &mut active_prompts,
                    &mut prompt_stops,
                    conversation,
                    Some(reply_tx),
                );
            },
            Work::Status { conversation, reply_tx } => {
                let prompt_state = if active_prompts.contains_key(&conversation) {
                    if pool.get(&conversation).is_some() {
                        "active"
                    } else {
                        "starting"
                    }
                } else {
                    "idle"
                };
                let info = acp_info.lock().unwrap();
                let message = if let Some(w) = pool.get(&conversation) {
                    let mode = info
                        .session_modes
                        .get(&w.session_id())
                        .map(|s| s.as_str())
                        .unwrap_or("unknown");
                    format!(
                        "**Agent:** {mode}\n**Model:** {}\n**Session:** {}\n**Request:** {prompt_state}\n**Runtime:** {:?}\n**Workers:** {}/{} ({} busy)\n**Restarts:** {}\n**Timeouts:** {}\n**Overload rejections:** {}\n**Dropped progress:** {}\n**Pending approvals:** {}\n**Last failure:** {}",
                        info.model_id,
                        w.session_id(),
                        info.runtime_state,
                        pool.len(),
                        pool.max_workers(),
                        info.busy_workers,
                        info.worker_restarts,
                        info.prompt_timeouts,
                        info.overload_rejections,
                        info.dropped_progress_updates,
                        info.pending_approvals,
                        info.last_failure.as_deref().unwrap_or("none"),
                    )
                } else {
                    let session = if prompt_state == "starting" {
                        "Session is starting"
                    } else {
                        "No active session — send a message first"
                    };
                    format!(
                        "**Model:** {}\n**Request:** {prompt_state}\n**Runtime:** {:?}\n**Workers:** {}/{} ({} busy)\n**Restarts:** {}\n**Timeouts:** {}\n**Overload rejections:** {}\n**Dropped progress:** {}\n**Pending approvals:** {}\n**Last failure:** {}\n{session}",
                        info.model_id,
                        info.runtime_state,
                        pool.len(),
                        pool.max_workers(),
                        info.busy_workers,
                        info.worker_restarts,
                        info.prompt_timeouts,
                        info.overload_rejections,
                        info.dropped_progress_updates,
                        info.pending_approvals,
                        info.last_failure.as_deref().unwrap_or("none"),
                    )
                };
                let _ = reply_tx.send(message);
            },
            Work::Shutdown { grace, reply_tx } => {
                work_receiver.close();
                break (grace, Some(reply_tx));
            },
        }
    };

    acp_info.lock().unwrap().runtime_state = AcpRuntimeState::Draining;
    reap_tasks.abort_all();
    while reap_tasks.join_next().await.is_some() {}
    prompt_stops.abort_all();
    while prompt_stops.join_next().await.is_some() {}
    for active in active_prompts.values_mut() {
        active.stopping = true;
        active.stop_finished = true;
    }
    let ready_prompts: Vec<(String, u64)> = active_prompts
        .iter()
        .map(|(conversation, active)| (conversation.clone(), active.generation))
        .collect();
    for (conversation, generation) in ready_prompts {
        try_finish_prompt_stop(&mut active_prompts, &conversation, generation);
    }

    if tokio::time::timeout(grace, async {
        while !prompts.is_empty() {
            match prompts.join_next_with_id().await {
                Some(Ok((task_id, completion))) => {
                    task_keys.remove(&task_id);
                    publish_prompt_completion(&mut active_prompts, completion);
                },
                Some(Err(error)) => {
                    finish_failed_prompt_task(&mut active_prompts, &mut task_keys, error.id());
                    if !error.is_cancelled() {
                        warn!(%error, "ACP prompt task failed while draining");
                    }
                },
                None => {},
            }
        }
    })
    .await
    .is_err()
    {
        warn!(
            grace_secs = grace.as_secs(),
            "ACP drain deadline exceeded; terminating workers"
        );
        prompts.abort_all();
        while let Some(result) = prompts.join_next_with_id().await {
            match result {
                Ok((task_id, completion)) => {
                    task_keys.remove(&task_id);
                    publish_prompt_completion(&mut active_prompts, completion);
                },
                Err(error) => {
                    finish_failed_prompt_task(&mut active_prompts, &mut task_keys, error.id());
                },
            }
        }
    }
    active_prompts.clear();
    pool.shutdown().await;
    if let Some(reply_tx) = reply_tx {
        let _ = reply_tx.send(());
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
        let failure_state = acp_info_clone.clone();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let local = tokio::task::LocalSet::new();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            local.block_on(&runtime, async move {
                let max_workers = cfg.max_workers;
                let idle_timeout = Duration::from_secs(cfg.idle_timeout_secs);
                let prompt_timeout = prompt_timeout_from_env();
                {
                    let mut info = acp_info_clone.lock().unwrap();
                    info.model_id = cfg.model_id.clone();
                    info.runtime_state = AcpRuntimeState::Starting;
                }

                let cfg = Rc::new(cfg);
                let factory: Rc<dyn WorkerFactory> = Rc::new(AcpWorkerFactory {
                    cfg: cfg.clone(),
                    acp_info: acp_info_clone.clone(),
                });

                info!(command = %cfg.command, "Spawning initial ACP worker");
                let warmup = match factory.spawn().await {
                    Ok(worker) => worker,
                    Err(error) => {
                        let mut info = acp_info_clone.lock().unwrap();
                        info.runtime_state = AcpRuntimeState::Failed;
                        info.last_failure = Some(error.clone());
                        warn!(
                            request_id = "startup",
                            conversation_id = "unassigned",
                            %error,
                            "failed to spawn initial ACP worker"
                        );
                        return;
                    },
                };
                let pool = Rc::new(AcpPool::new(factory, Some(warmup), max_workers, acp_info_clone.clone()));

                acp_info_clone.lock().unwrap().runtime_state = AcpRuntimeState::Running;
                let _ = ready_sender.send(());

                run_work_loop(
                    pool,
                    work_receiver,
                    acp_info_clone.clone(),
                    idle_timeout,
                    prompt_timeout,
                )
                .await;
                acp_info_clone.lock().unwrap().runtime_state = AcpRuntimeState::Stopped;
            });
        }));
        if result.is_err() {
            let mut info = failure_state.lock().unwrap();
            info.runtime_state = AcpRuntimeState::Failed;
            info.last_failure = Some("ACP runtime thread panicked".into());
            warn!("ACP runtime thread panicked");
        }
    });
    acp_info
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use acp::Client as _;
    use serde_json::json;

    use super::*;

    #[test]
    fn cancellation_confirmation_outlasts_stop_deadline() {
        assert!(CANCEL_CONFIRM_TIMEOUT > STOP_PROMPT_TIMEOUT + PROMPT_TASK_STOP_TIMEOUT);
    }

    #[test]
    fn settings_overlay_preserves_settings_and_disables_default_resources() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.json");
        std::fs::write(&source, r#"{"chat.defaultModel":"test-model"}"#).unwrap();

        let overlay = create_settings_overlay(Some(&source), temp.path()).unwrap();
        let settings: serde_json::Value = serde_json::from_slice(&std::fs::read(overlay.path()).unwrap()).unwrap();

        assert_eq!(settings["chat.defaultModel"], "test-model");
        assert_eq!(settings[DISABLE_DEFAULT_RESOURCES_KEY], true);
        let overlay_path = overlay.path().to_path_buf();
        drop(overlay);
        assert!(!overlay_path.exists());
    }

    #[test]
    fn partial_settings_overlay_is_removed_after_write_failure() {
        struct FailingWriter {
            file: std::fs::File,
            remaining: usize,
        }

        impl std::io::Write for FailingWriter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if self.remaining == 0 {
                    return Err(std::io::Error::other("injected write failure"));
                }
                let len = self.remaining.min(bytes.len());
                let written = std::io::Write::write(&mut self.file, &bytes[..len])?;
                self.remaining -= written;
                Ok(written)
            }

            fn flush(&mut self) -> std::io::Result<()> {
                std::io::Write::flush(&mut self.file)
            }
        }

        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("partial.json");
        let file = std::fs::File::create(&path).unwrap();
        let overlay = SettingsOverlay { path: path.clone() };
        let mut settings = serde_json::Map::new();
        settings.insert(DISABLE_DEFAULT_RESOURCES_KEY.into(), serde_json::Value::Bool(true));
        let mut writer = FailingWriter { file, remaining: 1 };

        assert!(write_settings_overlay(&mut writer, &settings).is_err());
        assert_ne!(std::fs::metadata(&path).unwrap().len(), 0);
        drop(writer);
        drop(overlay);
        assert!(!path.exists());
    }

    #[test]
    fn pinned_agent_requires_packaged_config() {
        let temp = tempfile::tempdir().unwrap();
        let agents = temp.path().join("agents");
        std::fs::create_dir(&agents).unwrap();

        assert!(authoritative_agent_dir_in(Some(PINNED_AGENT_NAME), temp.path()).is_err());
        std::fs::write(
            agents.join("kiro-help.json"),
            r#"{"name":"kiro-help","prompt":"file://kiro_help_prompt.md"}"#,
        )
        .unwrap();
        assert!(authoritative_agent_dir_in(Some(PINNED_AGENT_NAME), temp.path()).is_err());
        std::fs::write(agents.join("kiro_help_prompt.md"), "packaged prompt").unwrap();
        assert_eq!(
            authoritative_agent_dir_in(Some(PINNED_AGENT_NAME), temp.path()).unwrap(),
            Some(agents.clone())
        );
        assert_eq!(
            authoritative_agent_dir_in(Some("other-agent"), temp.path()).unwrap(),
            None
        );
        std::fs::write(
            agents.join("00-shadow.json"),
            r#"{"name":"kiro-help","prompt":"shadow"}"#,
        )
        .unwrap();
        let error = authoritative_agent_dir_in(Some(PINNED_AGENT_NAME), temp.path()).unwrap_err();
        assert!(error.contains("duplicate agent name 'kiro-help'"));
    }

    #[test]
    fn acp_command_pins_and_validates_required_agent() {
        assert_eq!(
            acp_command("kiro-cli acp", Some("kiro-help")).unwrap(),
            ("kiro-cli".to_string(), vec![
                "acp".to_string(),
                "--agent".to_string(),
                "kiro-help".to_string()
            ])
        );
        assert_eq!(
            acp_command("kiro-cli acp --agent=kiro-help", Some("kiro-help")).unwrap(),
            ("kiro-cli".to_string(), vec![
                "acp".to_string(),
                "--agent=kiro-help".to_string()
            ])
        );
        assert!(
            acp_command("kiro-cli acp --agent default", Some("kiro-help"))
                .unwrap_err()
                .contains("bot requires 'kiro-help'")
        );
        assert!(
            acp_command("kiro-cli acp --agent", Some("kiro-help"))
                .unwrap_err()
                .contains("without a value")
        );
    }

    #[test]
    fn required_mode_must_be_advertised_and_selected() {
        let available = vec![
            acp::SessionMode::new("default", "Default"),
            acp::SessionMode::new("kiro-help", "Kiro Help"),
        ];
        let default = acp::SessionModeState::new("default", available.clone());
        let selected = acp::SessionModeState::new("kiro-help", available);

        assert!(required_mode_change(Some(&default), "kiro-help").unwrap());
        assert!(!required_mode_change(Some(&selected), "kiro-help").unwrap());
        assert!(required_mode_change(Some(&selected), "missing").is_err());
        assert!(required_mode_change(None, "kiro-help").is_err());
    }

    #[test]
    fn effective_settings_must_confirm_resource_isolation() {
        let enabled = acp::RawValue::from_string(format!(r#"{{"{DISABLE_DEFAULT_RESOURCES_KEY}":true}}"#)).unwrap();
        let disabled = acp::RawValue::from_string(format!(r#"{{"{DISABLE_DEFAULT_RESOURCES_KEY}":false}}"#)).unwrap();

        assert!(verify_default_resources_disabled(&acp::ExtResponse::new(enabled.into())).is_ok());
        assert!(verify_default_resources_disabled(&acp::ExtResponse::new(disabled.into())).is_err());
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn child_shutdown_releases_refcell_borrow_before_waiting() {
        let mut command = tokio::process::Command::new("sleep");
        command.arg("60").process_group(0).kill_on_drop(true);
        let child = Rc::new(RefCell::new(command.spawn().unwrap()));

        shutdown_child(&child).await;

        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let exited = { child.borrow_mut().try_wait().unwrap().is_some() };
                if exited {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }

    /// Build a minimal `AcpClient` suitable for testing the
    /// `request_permission` decision path without spawning a real ACP child
    /// process.
    fn make_test_client(approval_policy: ApprovalPolicy) -> AcpClient {
        AcpClient {
            chunks: Rc::new(RefCell::new(Vec::new())),
            progress: Rc::new(RefCell::new(None)),
            progress_titles: Rc::new(RefCell::new(HashMap::new())),
            last_progress_update: Rc::new(Cell::new(None)),
            mcp_ready_count: Rc::new(RefCell::new(0)),
            mcp_notify: Rc::new(tokio::sync::Notify::new()),
            acp_info: Arc::new(Mutex::new(AcpInfo::default())),
            approval_policy,
            // `Ask` policy without an approval_tx falls through to Cancelled —
            // which is exactly the "fall-through" behaviour we want to assert
            // against in the negative tests.
            approval_tx: None,
            attachment_reads: Arc::new(AttachmentReadAuthorizer::default()),
            current_request: Rc::new(RefCell::new(("test-request".into(), "thread:C1:1".into()))),
            current_conv: Rc::new(RefCell::new((String::new(), None, String::new()))),
        }
    }

    /// Build a `RequestPermissionRequest` with one permission option
    /// (`allow_once`) and an optional `_meta` payload.
    fn make_permission_request(meta: Option<serde_json::Value>) -> acp::RequestPermissionRequest {
        let meta_map = meta.and_then(|v| v.as_object().cloned());
        acp::RequestPermissionRequest::new(
            acp::SessionId::new("test-session"),
            acp::ToolCallUpdate::new(
                acp::ToolCallId::new("tc-1"),
                acp::ToolCallUpdateFields::new().title(Some("Test tool".to_string())),
            ),
            vec![acp::PermissionOption::new(
                acp::PermissionOptionId::new("allow_once"),
                "Allow once",
                acp::PermissionOptionKind::AllowOnce,
            )],
        )
        .meta(meta_map)
    }

    fn run<F: std::future::Future>(fut: F) -> F::Output {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let local = tokio::task::LocalSet::new();
        local.block_on(&runtime, fut)
    }

    #[test]
    fn mcp_identity_requires_two_string_fields() {
        let complete = json!({
            "mcpToolIdentity": {
                "serverName": "kiro-github-read",
                "toolName": "search_github_issues"
            }
        });
        assert_eq!(
            mcp_tool_identity_from_meta(complete.as_object()),
            Some(("kiro-github-read", "search_github_issues"))
        );

        for malformed in [
            json!({}),
            json!({ "mcpToolIdentity": null }),
            json!({ "mcpToolIdentity": { "serverName": "kiro-github-read" } }),
            json!({
                "mcpToolIdentity": {
                    "serverName": "kiro-github-read",
                    "toolName": false
                }
            }),
        ] {
            assert_eq!(mcp_tool_identity_from_meta(malformed.as_object()), None);
        }
    }

    #[test]
    fn exact_host_owned_read_is_auto_approved() {
        for policy in [ApprovalPolicy::Approve, ApprovalPolicy::Ask] {
            let client = make_test_client(policy);
            let request = make_permission_request(Some(json!({
                "mcpToolIdentity": {
                    "serverName": "kiro-github-read",
                    "toolName": "search_github_issues"
                },
                "mcpAnnotations": { "readOnlyHint": false }
            })));
            let response = run(client.request_permission(request)).expect("request_permission ok");
            assert!(matches!(
                response.outcome,
                acp::RequestPermissionOutcome::Selected(ref outcome)
                    if outcome.option_id.to_string() == "allow_once"
            ));
        }
    }

    #[test]
    fn deny_policy_rejects_even_known_reads() {
        let client = make_test_client(ApprovalPolicy::Deny);
        let request = make_permission_request(Some(json!({
            "mcpToolIdentity": {
                "serverName": "kiro-github-read",
                "toolName": "search_github_issues"
            }
        })));
        let response = run(client.request_permission(request)).expect("request_permission ok");
        assert!(matches!(response.outcome, acp::RequestPermissionOutcome::Cancelled));
    }

    #[test]
    fn unknown_tool_routes_to_requester_approval_despite_read_only_hint() {
        run(async {
            let (approval_tx, mut approval_rx) = mpsc::unbounded_channel();
            let mut client = make_test_client(ApprovalPolicy::Approve);
            client.approval_tx = Some(approval_tx);
            let request = make_permission_request(Some(json!({
                "mcpToolIdentity": {
                    "serverName": "kiro-github-write",
                    "toolName": "create_github_issue"
                },
                "mcpAnnotations": { "readOnlyHint": true }
            })));

            let respond = async {
                let approval = approval_rx.recv().await.expect("approval request");
                assert_eq!(approval.options, vec![("allow_once".into(), "Allow once".into())]);
                assert!(
                    approval
                        .reply_tx
                        .send(ApprovalResponse::Selected("allow_once".into()))
                        .is_ok()
                );
            };
            let (response, ()) = tokio::join!(client.request_permission(request), respond);
            let response = response.expect("request_permission ok");
            assert!(matches!(
                response.outcome,
                acp::RequestPermissionOutcome::Selected(ref outcome)
                    if outcome.option_id.to_string() == "allow_once"
            ));
        });
    }

    #[test]
    fn deny_policy_does_not_enqueue_unknown_tools() {
        let (approval_tx, mut approval_rx) = mpsc::unbounded_channel();
        let mut client = make_test_client(ApprovalPolicy::Deny);
        client.approval_tx = Some(approval_tx);
        let request = make_permission_request(Some(json!({
            "mcpToolIdentity": {
                "serverName": "kiro-github-write",
                "toolName": "create_github_issue"
            }
        })));

        let response = run(client.request_permission(request)).expect("request_permission ok");
        assert!(matches!(response.outcome, acp::RequestPermissionOutcome::Cancelled));
        assert!(approval_rx.try_recv().is_err());
    }

    #[test]
    fn missing_malformed_and_unlisted_identity_fail_closed() {
        let client = make_test_client(ApprovalPolicy::Approve);
        for meta in [
            None,
            Some(json!({
                "mcpToolIdentity": {
                    "serverName": "kiro-github-read",
                    "toolName": 42
                }
            })),
            Some(json!({
                "mcpToolIdentity": {
                    "serverName": "kiro-github-read",
                    "toolName": "create_github_issue"
                }
            })),
            Some(json!({
                "mcpToolIdentity": {
                    "serverName": "kiro-github-write",
                    "toolName": "search_github_issues"
                }
            })),
        ] {
            let request = make_permission_request(meta);
            let response = run(client.request_permission(request)).expect("request_permission ok");
            assert!(matches!(response.outcome, acp::RequestPermissionOutcome::Cancelled));
        }
    }

    #[test]
    fn advisory_annotations_never_grant_host_approval() {
        let client = make_test_client(ApprovalPolicy::Deny);
        for hint in [json!(true), json!(false), json!("dishonest"), json!(null)] {
            let request = make_permission_request(Some(json!({
                "mcpAnnotations": { "readOnlyHint": hint }
            })));
            let response = run(client.request_permission(request)).expect("request_permission ok");
            assert!(matches!(response.outcome, acp::RequestPermissionOutcome::Cancelled));
        }
    }

    #[test]
    fn active_attachment_read_is_allow_once_and_cleanup_revokes_it() {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path().join("attachments");
        let root = base.join("request-active");
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("attachment.txt");
        std::fs::write(&file, b"private").unwrap();
        let authorizer = Arc::new(AttachmentReadAuthorizer::new(&base));
        let lease = authorizer.activate("thread:C1:1", &root).unwrap();
        let mut client = make_test_client(ApprovalPolicy::Approve);
        client.attachment_reads = authorizer;
        let request = || {
            make_permission_request(Some(json!({
                "fsReadPaths": [file.display().to_string()]
            })))
        };

        let response = run(client.request_permission(request())).expect("request_permission ok");
        assert!(matches!(
            response.outcome,
            acp::RequestPermissionOutcome::Selected(ref outcome)
                if outcome.option_id.to_string() == "allow_once"
        ));

        drop(lease);
        let response = run(client.request_permission(request())).expect("request_permission ok");
        assert!(matches!(response.outcome, acp::RequestPermissionOutcome::Cancelled));
    }

    #[test]
    fn cross_conversation_attachment_read_is_denied_without_requester_override() {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path().join("attachments");
        let root = base.join("request-other");
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("attachment.txt");
        std::fs::write(&file, b"private").unwrap();
        let authorizer = Arc::new(AttachmentReadAuthorizer::new(&base));
        let _lease = authorizer.activate("thread:C1:2", &root).unwrap();
        let (approval_tx, mut approval_rx) = mpsc::unbounded_channel();
        let mut client = make_test_client(ApprovalPolicy::Ask);
        client.attachment_reads = authorizer;
        client.approval_tx = Some(approval_tx);
        let request = make_permission_request(Some(json!({
            "fsReadPaths": [file.display().to_string()]
        })));

        let response = run(client.request_permission(request)).expect("request_permission ok");

        assert!(matches!(response.outcome, acp::RequestPermissionOutcome::Cancelled));
        assert!(approval_rx.try_recv().is_err());
    }

    #[test]
    fn malformed_fs_read_metadata_is_denied() {
        let client = make_test_client(ApprovalPolicy::Approve);
        for meta in [
            json!({ "fsReadPaths": null }),
            json!({ "fsReadPaths": [] }),
            json!({ "fsReadPaths": [42] }),
            json!({ "fsReadPaths": [""] }),
        ] {
            let request = make_permission_request(Some(meta));
            let response = run(client.request_permission(request)).expect("request_permission ok");
            assert!(matches!(response.outcome, acp::RequestPermissionOutcome::Cancelled));
        }
    }

    #[test]
    fn unmanaged_fs_read_routes_to_requester_approval() {
        run(async {
            let directory = tempfile::tempdir().unwrap();
            let base = directory.path().join("attachments");
            std::fs::create_dir(&base).unwrap();
            let ordinary_file = directory.path().join("ordinary.txt");
            std::fs::write(&ordinary_file, b"ordinary").unwrap();
            let (approval_tx, mut approval_rx) = mpsc::unbounded_channel();
            let mut client = make_test_client(ApprovalPolicy::Ask);
            client.attachment_reads = Arc::new(AttachmentReadAuthorizer::new(&base));
            client.approval_tx = Some(approval_tx);
            let request = make_permission_request(Some(json!({
                "fsReadPaths": [ordinary_file.display().to_string()]
            })));

            let respond = async {
                let approval = approval_rx.recv().await.expect("approval request");
                assert!(
                    approval
                        .reply_tx
                        .send(ApprovalResponse::Selected("allow_once".into()))
                        .is_ok()
                );
            };
            let (response, ()) = tokio::join!(client.request_permission(request), respond);
            let response = response.expect("request_permission ok");
            assert!(matches!(
                response.outcome,
                acp::RequestPermissionOutcome::Selected(ref outcome)
                    if outcome.option_id.to_string() == "allow_once"
            ));
        });
    }

    #[derive(Clone)]
    enum TestPrompt {
        Reply(Result<String, String>),
        Wait(Rc<tokio::sync::Notify>, Result<String, String>),
        Pending,
    }

    struct TestWorker {
        session_id: String,
        activity: Rc<Cell<WorkerActivityState>>,
        acp_info: Arc<Mutex<AcpInfo>>,
        health: RefCell<VecDeque<WorkerHealth>>,
        prompt: TestPrompt,
        cancellations: Cell<usize>,
        kills: Cell<usize>,
        mode_sets: Cell<usize>,
        log_context: RefCell<(String, String)>,
    }

    impl TestWorker {
        fn new(
            session_id: &str,
            acp_info: Arc<Mutex<AcpInfo>>,
            health: Vec<WorkerHealth>,
            prompt: TestPrompt,
        ) -> Rc<Self> {
            Rc::new(Self {
                session_id: session_id.into(),
                activity: Rc::new(Cell::new(WorkerActivityState::IdleSince(Instant::now()))),
                acp_info,
                health: RefCell::new(health.into()),
                prompt,
                cancellations: Cell::new(0),
                kills: Cell::new(0),
                mode_sets: Cell::new(0),
                log_context: RefCell::new(("test".into(), "test".into())),
            })
        }
    }

    #[async_trait::async_trait(?Send)]
    impl Worker for TestWorker {
        fn session_id(&self) -> String {
            self.session_id.clone()
        }

        fn request_id(&self) -> String {
            self.log_context.borrow().0.clone()
        }

        fn begin_activity(&self) -> Result<WorkerActivity, String> {
            if matches!(self.activity.get(), WorkerActivityState::Busy) {
                return Err("busy".into());
            }
            self.activity.set(WorkerActivityState::Busy);
            self.acp_info.lock().unwrap().busy_workers += 1;
            Ok(WorkerActivity {
                state: self.activity.clone(),
                acp_info: self.acp_info.clone(),
            })
        }

        fn idle_since(&self) -> Option<Instant> {
            match self.activity.get() {
                WorkerActivityState::IdleSince(since) => Some(since),
                WorkerActivityState::Busy => None,
            }
        }

        fn health(&self) -> WorkerHealth {
            let mut health = self.health.borrow_mut();
            if health.len() > 1 {
                health.pop_front().unwrap()
            } else {
                health.front().cloned().unwrap_or(WorkerHealth::Healthy)
            }
        }

        fn set_request_context(&self, request_id: String, conversation_id: String) {
            *self.log_context.borrow_mut() = (request_id, conversation_id);
        }

        fn set_conv(&self, _channel: String, _thread_ts: Option<String>, _slack_user_id: String) {}

        async fn prompt(
            &self,
            _messages: Vec<String>,
            _progress_tx: mpsc::UnboundedSender<ProgressUpdate>,
        ) -> Result<String, String> {
            match &self.prompt {
                TestPrompt::Reply(reply) => reply.clone(),
                TestPrompt::Wait(release, reply) => {
                    release.notified().await;
                    reply.clone()
                },
                TestPrompt::Pending => std::future::pending().await,
            }
        }

        async fn cancel(&self) {
            self.cancellations.set(self.cancellations.get() + 1);
        }

        async fn set_mode(&self, mode: String) -> Result<String, String> {
            self.mode_sets.set(self.mode_sets.get() + 1);
            Ok(mode)
        }

        async fn kill(&self) {
            self.kills.set(self.kills.get() + 1);
        }
    }

    struct TestFactory {
        workers: RefCell<VecDeque<Rc<dyn Worker>>>,
        acp_info: Arc<Mutex<AcpInfo>>,
        spawns: Cell<usize>,
    }

    #[async_trait::async_trait(?Send)]
    impl WorkerFactory for TestFactory {
        async fn spawn(&self) -> Result<Rc<dyn Worker>, String> {
            self.spawns.set(self.spawns.get() + 1);
            let worker = self
                .workers
                .borrow_mut()
                .pop_front()
                .ok_or_else(|| "no replacement worker".to_string())?;
            self.acp_info.lock().unwrap().workers += 1;
            Ok(worker)
        }
    }

    struct BlockingFactory {
        worker: RefCell<Option<Rc<dyn Worker>>>,
        acp_info: Arc<Mutex<AcpInfo>>,
        started: Rc<tokio::sync::Notify>,
        release: Rc<tokio::sync::Notify>,
    }

    #[async_trait::async_trait(?Send)]
    impl WorkerFactory for BlockingFactory {
        async fn spawn(&self) -> Result<Rc<dyn Worker>, String> {
            self.started.notify_one();
            self.release.notified().await;
            let worker = self
                .worker
                .borrow_mut()
                .take()
                .ok_or_else(|| "worker already spawned".to_string())?;
            self.acp_info.lock().unwrap().workers += 1;
            Ok(worker)
        }
    }

    fn test_pool(
        default_worker: Rc<dyn Worker>,
        replacements: Vec<Rc<dyn Worker>>,
        acp_info: Arc<Mutex<AcpInfo>>,
    ) -> (Rc<AcpPool>, Rc<TestFactory>) {
        acp_info.lock().unwrap().workers = 1;
        let factory = Rc::new(TestFactory {
            workers: RefCell::new(replacements.into()),
            acp_info: acp_info.clone(),
            spawns: Cell::new(0),
        });
        let pool = Rc::new(AcpPool::new(factory.clone(), Some(default_worker), 1, acp_info));
        (pool, factory)
    }

    #[tokio::test(start_paused = true)]
    async fn concurrent_spawns_reserve_worker_capacity_before_awaiting_factory() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let acp_info = Arc::new(Mutex::new(AcpInfo::default()));
                let worker = TestWorker::new(
                    "only-worker",
                    acp_info.clone(),
                    vec![WorkerHealth::Healthy],
                    TestPrompt::Reply(Ok("done".into())),
                );
                let started = Rc::new(tokio::sync::Notify::new());
                let release = Rc::new(tokio::sync::Notify::new());
                let factory = Rc::new(BlockingFactory {
                    worker: RefCell::new(Some(worker)),
                    acp_info: acp_info.clone(),
                    started: started.clone(),
                    release: release.clone(),
                });
                let pool = Rc::new(AcpPool::new(factory, None, 1, acp_info));

                let first_pool = pool.clone();
                let first = tokio::task::spawn_local(async move { first_pool.get_or_spawn("conversation-a").await });
                started.notified().await;

                let second = pool.get_or_spawn("conversation-b").await;
                match second {
                    Err(error) => assert_eq!(error, "⏳ All workers busy — try again shortly"),
                    Ok(_) => panic!("second spawn exceeded max_workers"),
                }

                release.notify_one();
                first.await.unwrap().unwrap();
                assert_eq!(pool.len(), 1);
                assert_eq!(pool.spawning.get(), 0);
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn spawn_window_rejects_session_controls_and_cancels_cleanly() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let acp_info = Arc::new(Mutex::new(AcpInfo::default()));
                let worker = TestWorker::new(
                    "unused-worker",
                    acp_info.clone(),
                    vec![WorkerHealth::Healthy],
                    TestPrompt::Reply(Ok("must not run".into())),
                );
                let started = Rc::new(tokio::sync::Notify::new());
                let factory = Rc::new(BlockingFactory {
                    worker: RefCell::new(Some(worker)),
                    acp_info: acp_info.clone(),
                    started: started.clone(),
                    release: Rc::new(tokio::sync::Notify::new()),
                });
                let pool = Rc::new(AcpPool::new(factory, None, 1, acp_info.clone()));
                let (work_tx, work_rx) = mpsc::unbounded_channel();
                let loop_task = tokio::task::spawn_local(run_work_loop(
                    pool.clone(),
                    work_rx,
                    acp_info,
                    Duration::from_secs(300),
                    Duration::from_secs(900),
                ));
                let (prompt_reply_tx, prompt_reply_rx) = oneshot::channel();
                let (progress_tx, _progress_rx) = mpsc::unbounded_channel();
                work_tx
                    .send(Work::Prompt {
                        input: PromptInput::ready(
                            "slow spawn".into(),
                            Vec::new(),
                            "channel".into(),
                            None,
                            "user".into(),
                            "U1".into(),
                        ),
                        conversation: "conversation-spawning".into(),
                        reply_tx: prompt_reply_tx,
                        progress_tx,
                    })
                    .unwrap();
                started.notified().await;

                let (reset_tx, reset_rx) = oneshot::channel();
                work_tx
                    .send(Work::NewSession {
                        conversation: "conversation-spawning".into(),
                        reply_tx: reset_tx,
                    })
                    .unwrap();
                assert_eq!(
                    reset_rx.await.unwrap(),
                    "⏳ A request is in progress — cancel it before resetting the session"
                );

                let (mode_tx, mode_rx) = oneshot::channel();
                work_tx
                    .send(Work::SetMode {
                        conversation: "conversation-spawning".into(),
                        mode: "other".into(),
                        reply_tx: mode_tx,
                    })
                    .unwrap();
                assert_eq!(
                    mode_rx.await.unwrap(),
                    "⏳ A request is in progress — cancel it before changing agents"
                );

                let (model_tx, model_rx) = oneshot::channel();
                work_tx
                    .send(Work::SetModel {
                        conversation: "conversation-spawning".into(),
                        model: "other".into(),
                        reply_tx: model_tx,
                    })
                    .unwrap();
                assert_eq!(
                    model_rx.await.unwrap(),
                    "⏳ A request is in progress — cancel it before changing models"
                );

                let (status_tx, status_rx) = oneshot::channel();
                work_tx
                    .send(Work::Status {
                        conversation: "conversation-spawning".into(),
                        reply_tx: status_tx,
                    })
                    .unwrap();
                let status = status_rx.await.unwrap();
                assert!(status.contains("**Request:** starting"));
                assert!(status.contains("Session is starting"));

                let (cancel_tx, cancel_rx) = oneshot::channel();
                work_tx
                    .send(Work::CancelAndWait {
                        conversation: "conversation-spawning".into(),
                        reply_tx: cancel_tx,
                    })
                    .unwrap();
                cancel_rx.await.unwrap();

                assert!(prompt_reply_rx.await.is_err());
                assert_eq!(pool.spawning.get(), 0);
                assert_eq!(pool.len(), 0);

                let (shutdown_tx, shutdown_rx) = oneshot::channel();
                work_tx
                    .send(Work::Shutdown {
                        grace: Duration::ZERO,
                        reply_tx: shutdown_tx,
                    })
                    .unwrap();
                shutdown_rx.await.unwrap();
                loop_task.await.unwrap();
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn cancellation_does_not_block_status_or_shutdown() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let acp_info = Arc::new(Mutex::new(AcpInfo::default()));
                let worker = TestWorker::new(
                    "cancelling-worker",
                    acp_info.clone(),
                    vec![WorkerHealth::Healthy],
                    TestPrompt::Pending,
                );
                let (pool, _) = test_pool(worker.clone(), Vec::new(), acp_info.clone());
                let (work_tx, work_rx) = mpsc::unbounded_channel();
                let loop_task = tokio::task::spawn_local(run_work_loop(
                    pool,
                    work_rx,
                    acp_info,
                    Duration::from_secs(300),
                    Duration::from_secs(900),
                ));
                let (prompt_reply_tx, prompt_reply_rx) = oneshot::channel();
                let (progress_tx, _progress_rx) = mpsc::unbounded_channel();
                work_tx
                    .send(Work::Prompt {
                        input: PromptInput::ready(
                            "slow".into(),
                            Vec::new(),
                            "channel".into(),
                            None,
                            "user".into(),
                            "U1".into(),
                        ),
                        conversation: "conversation-cancelling".into(),
                        reply_tx: prompt_reply_tx,
                        progress_tx,
                    })
                    .unwrap();
                while worker.idle_since().is_some() {
                    tokio::task::yield_now().await;
                }

                let started = Instant::now();
                work_tx
                    .send(Work::Cancel {
                        conversation: "conversation-cancelling".into(),
                    })
                    .unwrap();
                let (status_tx, status_rx) = oneshot::channel();
                work_tx
                    .send(Work::Status {
                        conversation: "conversation-cancelling".into(),
                        reply_tx: status_tx,
                    })
                    .unwrap();
                assert!(status_rx.await.unwrap().contains("**Request:** active"));
                assert_eq!(started.elapsed(), Duration::ZERO);

                let (shutdown_tx, shutdown_rx) = oneshot::channel();
                work_tx
                    .send(Work::Shutdown {
                        grace: Duration::ZERO,
                        reply_tx: shutdown_tx,
                    })
                    .unwrap();
                shutdown_rx.await.unwrap();
                assert_eq!(started.elapsed(), Duration::ZERO);
                assert!(prompt_reply_rx.await.is_err());
                loop_task.await.unwrap();
            })
            .await;
    }

    #[tokio::test]
    async fn drain_acknowledges_cancel_only_after_prompt_stops() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let acp_info = Arc::new(Mutex::new(AcpInfo::default()));
                let prompt_release = Rc::new(tokio::sync::Notify::new());
                let worker = TestWorker::new(
                    "draining-worker",
                    acp_info.clone(),
                    vec![WorkerHealth::Healthy],
                    TestPrompt::Wait(prompt_release.clone(), Ok("done".into())),
                );
                let (pool, _) = test_pool(worker.clone(), Vec::new(), acp_info.clone());
                let (work_tx, work_rx) = mpsc::unbounded_channel();
                let loop_task = tokio::task::spawn_local(run_work_loop(
                    pool,
                    work_rx,
                    acp_info.clone(),
                    Duration::from_secs(300),
                    Duration::from_secs(900),
                ));
                let (prompt_reply_tx, prompt_reply_rx) = oneshot::channel();
                let (progress_tx, _progress_rx) = mpsc::unbounded_channel();
                work_tx
                    .send(Work::Prompt {
                        input: PromptInput::ready(
                            "slow".into(),
                            Vec::new(),
                            "channel".into(),
                            None,
                            "user".into(),
                            "U1".into(),
                        ),
                        conversation: "conversation-draining".into(),
                        reply_tx: prompt_reply_tx,
                        progress_tx,
                    })
                    .unwrap();
                while worker.idle_since().is_some() {
                    tokio::task::yield_now().await;
                }

                let (cancel_tx, mut cancel_rx) = oneshot::channel();
                work_tx
                    .send(Work::CancelAndWait {
                        conversation: "conversation-draining".into(),
                        reply_tx: cancel_tx,
                    })
                    .unwrap();
                let (shutdown_tx, shutdown_rx) = oneshot::channel();
                work_tx
                    .send(Work::Shutdown {
                        grace: Duration::from_secs(30),
                        reply_tx: shutdown_tx,
                    })
                    .unwrap();
                while acp_info.lock().unwrap().runtime_state != AcpRuntimeState::Draining {
                    tokio::task::yield_now().await;
                }

                assert!(matches!(
                    cancel_rx.try_recv(),
                    Err(tokio::sync::oneshot::error::TryRecvError::Empty)
                ));
                prompt_release.notify_one();
                cancel_rx.await.unwrap();
                assert_eq!(prompt_reply_rx.await.unwrap(), "done");
                shutdown_rx.await.unwrap();
                loop_task.await.unwrap();
            })
            .await;
    }

    #[tokio::test]
    async fn completion_is_cleared_before_reply_allows_immediate_citation_retry() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let acp_info = Arc::new(Mutex::new(AcpInfo::default()));
                let worker = TestWorker::new(
                    "retry-worker",
                    acp_info.clone(),
                    vec![WorkerHealth::Healthy],
                    TestPrompt::Reply(Ok("done".into())),
                );
                let (pool, _) = test_pool(worker, Vec::new(), acp_info.clone());
                let (work_tx, work_rx) = mpsc::unbounded_channel();
                let loop_task = tokio::task::spawn_local(run_work_loop(
                    pool,
                    work_rx,
                    acp_info,
                    Duration::from_secs(300),
                    Duration::from_secs(900),
                ));

                for text in ["initial", "citation retry"] {
                    let (reply_tx, reply_rx) = oneshot::channel();
                    let (progress_tx, _progress_rx) = mpsc::unbounded_channel();
                    work_tx
                        .send(Work::Prompt {
                            input: PromptInput::ready(
                                text.into(),
                                Vec::new(),
                                "channel".into(),
                                None,
                                "user".into(),
                                "U1".into(),
                            ),
                            conversation: "conversation-retry".into(),
                            reply_tx,
                            progress_tx,
                        })
                        .unwrap();
                    assert_eq!(reply_rx.await.unwrap(), "done");
                }

                let (shutdown_tx, shutdown_rx) = oneshot::channel();
                work_tx
                    .send(Work::Shutdown {
                        grace: Duration::ZERO,
                        reply_tx: shutdown_tx,
                    })
                    .unwrap();
                shutdown_rx.await.unwrap();
                loop_task.await.unwrap();
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stale_task_completion_cannot_clear_newer_prompt_generation() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let stale_task = tokio::task::spawn_local(std::future::pending::<()>());
                let current_task = tokio::task::spawn_local(std::future::pending::<()>());
                let mut task_keys = HashMap::from([(stale_task.id(), ("same-conversation".to_string(), 1_u64))]);
                let mut active_prompts = HashMap::from([("same-conversation".to_string(), ActivePrompt {
                    generation: 2,
                    task: current_task.abort_handle(),
                    stopping: false,
                    task_finished: false,
                    stop_finished: false,
                    pending_completion: None,
                    cancel_waiters: Vec::new(),
                })]);
                let (stale_reply_tx, stale_reply_rx) = oneshot::channel();

                publish_prompt_completion(&mut active_prompts, PromptCompletion {
                    conversation: "same-conversation".into(),
                    generation: 1,
                    reply: "stale".into(),
                    reply_tx: stale_reply_tx,
                });

                finish_failed_prompt_task(&mut active_prompts, &mut task_keys, stale_task.id());

                assert_eq!(active_prompts["same-conversation"].generation, 2);
                assert!(stale_reply_rx.await.is_err());
                stale_task.abort();
                current_task.abort();
                let _ = stale_task.await;
                let _ = current_task.await;
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn session_reset_and_mode_change_reject_while_prompt_is_busy() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let acp_info = Arc::new(Mutex::new(AcpInfo::default()));
                let worker = TestWorker::new(
                    "busy-worker",
                    acp_info.clone(),
                    vec![WorkerHealth::Healthy],
                    TestPrompt::Pending,
                );
                let (pool, _) = test_pool(worker.clone(), Vec::new(), acp_info.clone());
                let (work_tx, work_rx) = mpsc::unbounded_channel();
                let loop_task = tokio::task::spawn_local(run_work_loop(
                    pool,
                    work_rx,
                    acp_info,
                    Duration::from_secs(300),
                    Duration::from_secs(900),
                ));
                let (prompt_reply_tx, _prompt_reply_rx) = oneshot::channel();
                let (progress_tx, _progress_rx) = mpsc::unbounded_channel();
                work_tx
                    .send(Work::Prompt {
                        input: PromptInput::ready(
                            "slow".into(),
                            Vec::new(),
                            "channel".into(),
                            None,
                            "user".into(),
                            "U1".into(),
                        ),
                        conversation: "conversation-busy".into(),
                        reply_tx: prompt_reply_tx,
                        progress_tx,
                    })
                    .unwrap();
                for _ in 0..10 {
                    if worker.idle_since().is_none() {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
                assert!(worker.idle_since().is_none());

                let (reset_tx, reset_rx) = oneshot::channel();
                work_tx
                    .send(Work::NewSession {
                        conversation: "conversation-busy".into(),
                        reply_tx: reset_tx,
                    })
                    .unwrap();
                assert_eq!(
                    reset_rx.await.unwrap(),
                    "⏳ A request is in progress — cancel it before resetting the session"
                );

                let (mode_tx, mode_rx) = oneshot::channel();
                work_tx
                    .send(Work::SetMode {
                        conversation: "conversation-busy".into(),
                        mode: "other".into(),
                        reply_tx: mode_tx,
                    })
                    .unwrap();
                assert_eq!(
                    mode_rx.await.unwrap(),
                    "⏳ A request is in progress — cancel it before changing agents"
                );
                assert_eq!(worker.mode_sets.get(), 0);
                assert_eq!(worker.kills.get(), 0);

                let (model_tx, model_rx) = oneshot::channel();
                work_tx
                    .send(Work::SetModel {
                        conversation: "conversation-busy".into(),
                        model: "other".into(),
                        reply_tx: model_tx,
                    })
                    .unwrap();
                assert_eq!(
                    model_rx.await.unwrap(),
                    "⏳ A request is in progress — cancel it before changing models"
                );

                let (status_tx, status_rx) = oneshot::channel();
                work_tx
                    .send(Work::Status {
                        conversation: "conversation-busy".into(),
                        reply_tx: status_tx,
                    })
                    .unwrap();
                assert!(status_rx.await.unwrap().contains("**Request:** active"));

                let (shutdown_tx, shutdown_rx) = oneshot::channel();
                work_tx
                    .send(Work::Shutdown {
                        grace: Duration::ZERO,
                        reply_tx: shutdown_tx,
                    })
                    .unwrap();
                shutdown_rx.await.unwrap();
                loop_task.await.unwrap();
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn active_prompt_is_not_reaped_after_300_or_600_seconds() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let acp_info = Arc::new(Mutex::new(AcpInfo::default()));
                let worker = TestWorker::new(
                    "active",
                    acp_info.clone(),
                    vec![WorkerHealth::Healthy],
                    TestPrompt::Pending,
                );
                let (pool, _) = test_pool(worker.clone(), Vec::new(), acp_info.clone());
                let (progress_tx, _progress_rx) = mpsc::unbounded_channel();
                let task = tokio::task::spawn_local(execute_prompt(
                    pool.clone(),
                    "request-active".into(),
                    "conversation-active".into(),
                    "channel".into(),
                    None,
                    "user".into(),
                    vec!["hello".into()],
                    progress_tx,
                    Duration::from_secs(900),
                    acp_info.clone(),
                ));
                tokio::task::yield_now().await;

                tokio::time::advance(Duration::from_secs(301)).await;
                pool.reap_idle(Duration::from_secs(300)).await;
                assert_eq!(pool.len(), 1);
                assert_eq!(worker.kills.get(), 0);

                tokio::time::advance(Duration::from_secs(300)).await;
                pool.reap_idle(Duration::from_secs(300)).await;
                assert_eq!(pool.len(), 1);
                assert_eq!(worker.kills.get(), 0);

                tokio::time::advance(Duration::from_secs(299)).await;
                tokio::task::yield_now().await;
                tokio::time::advance(Duration::from_secs(6)).await;
                let reply = task.await.unwrap();
                assert!(reply.contains("900 second deadline"));
                assert_eq!(worker.cancellations.get(), 1);
                assert_eq!(worker.kills.get(), 1);
                let info = acp_info.lock().unwrap();
                assert_eq!(info.busy_workers, 0);
                assert_eq!(info.prompt_timeouts, 1);
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn reaper_revalidates_candidate_before_removal() {
        let acp_info = Arc::new(Mutex::new(AcpInfo::default()));
        let worker = TestWorker::new(
            "revalidated",
            acp_info.clone(),
            vec![WorkerHealth::Healthy],
            TestPrompt::Pending,
        );
        let factory = Rc::new(TestFactory {
            workers: RefCell::new(VecDeque::new()),
            acp_info: acp_info.clone(),
            spawns: Cell::new(0),
        });
        let pool = AcpPool::new(factory, None, 1, acp_info);
        let candidate: Rc<dyn Worker> = worker.clone();
        pool.workers
            .borrow_mut()
            .insert("conversation-revalidated".into(), candidate.clone());
        tokio::time::advance(Duration::from_secs(301)).await;

        let _activity = worker.begin_activity().unwrap();
        assert!(
            pool.take_reapable("conversation-revalidated", &candidate, Duration::from_secs(300))
                .is_none()
        );
        assert_eq!(pool.len(), 1);
        assert_eq!(worker.kills.get(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn progress_updates_resume_after_throttle_interval() {
        let last_progress_update = Cell::new(None);
        assert!(should_emit_progress(&last_progress_update));
        assert!(!should_emit_progress(&last_progress_update));

        tokio::time::advance(PROGRESS_UPDATE_INTERVAL).await;
        assert!(should_emit_progress(&last_progress_update));
    }

    #[tokio::test(start_paused = true)]
    async fn idle_timeout_starts_when_prompt_completes() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let acp_info = Arc::new(Mutex::new(AcpInfo::default()));
                let worker = TestWorker::new(
                    "completed",
                    acp_info.clone(),
                    vec![WorkerHealth::Healthy],
                    TestPrompt::Reply(Ok("done".into())),
                );
                let (pool, _) = test_pool(worker.clone(), Vec::new(), acp_info.clone());
                let (progress_tx, _progress_rx) = mpsc::unbounded_channel();
                let reply = execute_prompt(
                    pool.clone(),
                    "request-completed".into(),
                    "conversation-completed".into(),
                    "channel".into(),
                    None,
                    "user".into(),
                    vec!["hello".into()],
                    progress_tx,
                    Duration::from_secs(900),
                    acp_info,
                )
                .await;
                assert_eq!(reply, "done");

                tokio::time::advance(Duration::from_secs(300)).await;
                pool.reap_idle(Duration::from_secs(300)).await;
                assert_eq!(pool.len(), 1);

                tokio::time::advance(Duration::from_secs(1)).await;
                pool.reap_idle(Duration::from_secs(300)).await;
                assert_eq!(pool.len(), 0);
                assert_eq!(worker.kills.get(), 1);
            })
            .await;
    }

    #[tokio::test]
    async fn dead_worker_is_replaced_once_before_prompt_dispatch() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let acp_info = Arc::new(Mutex::new(AcpInfo::default()));
                let dead = TestWorker::new(
                    "dead",
                    acp_info.clone(),
                    vec![
                        WorkerHealth::Healthy,
                        WorkerHealth::Dead("exited before dispatch".into()),
                    ],
                    TestPrompt::Pending,
                );
                let replacement = TestWorker::new(
                    "replacement",
                    acp_info.clone(),
                    vec![WorkerHealth::Healthy],
                    TestPrompt::Reply(Ok("recovered".into())),
                );
                let (pool, factory) = test_pool(dead.clone(), vec![replacement], acp_info.clone());
                let (progress_tx, _progress_rx) = mpsc::unbounded_channel();
                let reply = execute_prompt(
                    pool,
                    "request-recovery".into(),
                    "conversation-recovery".into(),
                    "channel".into(),
                    None,
                    "user".into(),
                    vec!["hello".into()],
                    progress_tx,
                    Duration::from_secs(30),
                    acp_info,
                )
                .await;

                assert_eq!(reply, "recovered");
                assert_eq!(dead.kills.get(), 1);
                assert_eq!(factory.spawns.get(), 1);
            })
            .await;
    }

    #[tokio::test]
    async fn worker_death_during_prompt_is_not_retried() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let acp_info = Arc::new(Mutex::new(AcpInfo::default()));
                let dead = TestWorker::new(
                    "dead-inflight",
                    acp_info.clone(),
                    vec![
                        WorkerHealth::Healthy,
                        WorkerHealth::Healthy,
                        WorkerHealth::Dead("exited during prompt".into()),
                    ],
                    TestPrompt::Reply(Err("server shut down unexpectedly".into())),
                );
                let replacement = TestWorker::new(
                    "unused",
                    acp_info.clone(),
                    vec![WorkerHealth::Healthy],
                    TestPrompt::Reply(Ok("must not run".into())),
                );
                let (pool, factory) = test_pool(dead.clone(), vec![replacement], acp_info.clone());
                let (progress_tx, _progress_rx) = mpsc::unbounded_channel();
                let reply = execute_prompt(
                    pool,
                    "request-no-retry".into(),
                    "conversation-no-retry".into(),
                    "channel".into(),
                    None,
                    "user".into(),
                    vec!["hello".into()],
                    progress_tx,
                    Duration::from_secs(30),
                    acp_info,
                )
                .await;

                assert!(reply.contains("stopped during the request"));
                assert_eq!(dead.kills.get(), 1);
                assert_eq!(factory.spawns.get(), 0);
            })
            .await;
    }
}
