use std::collections::HashMap;
use std::pin::Pin;
use std::process::Stdio;
use std::time::{
    Duration,
    Instant,
};

use bstr::ByteSlice as _;
use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::{
    mpsc,
    oneshot,
};
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::agent::agent_config::definitions::{
    CommandHook,
    HookConfig,
    HookTrigger,
};
use crate::agent::agent_loop::types::ToolUseBlock;
use crate::agent::tools::{
    Tool,
    ToolExecutionOutput,
    ToolExecutionResult,
    ToolState,
};
use crate::agent::util::truncate_safe;

#[derive(Debug, Clone)]
pub struct ToolExecutorHandle {}

pub type ToolFuture = Pin<Box<dyn Future<Output = ToolExecutionResult> + Send>>;

/// An abstraction around executing tools and hooks in parallel on separate tasks.
///
/// `TaskExecutor` is required to avoid blocking the primary session task on tool and hook
/// execution.
#[derive(Debug)]
pub struct TaskExecutor {
    /// Buffer to hold executor events
    event_buf: Vec<TaskExecutorEvent>,

    execute_request_tx: mpsc::Sender<ExecuteRequest>,
    execute_request_rx: mpsc::Receiver<ExecuteRequest>,
    execute_result_tx: mpsc::Sender<ExecutorResult>,
    execute_result_rx: mpsc::Receiver<ExecutorResult>,
    executing_tools: HashMap<ToolExecutionId, ExecutingTool>,
    executing_hooks: HashMap<HookExecutionId, ExecutingHook>,

    hooks_cache: HashMap<Hook, CachedHook>,
}

impl TaskExecutor {
    pub fn new() -> Self {
        let (execute_request_tx, execute_request_rx) = mpsc::channel(32);
        let (execute_result_tx, execute_result_rx) = mpsc::channel(32);
        Self {
            event_buf: Vec::new(),
            execute_request_tx,
            execute_request_rx,
            execute_result_tx,
            execute_result_rx,
            executing_tools: HashMap::new(),
            executing_hooks: HashMap::new(),
            hooks_cache: HashMap::new(),
        }
    }

    pub async fn recv_next(&mut self, event_buf: &mut Vec<TaskExecutorEvent>) {
        tokio::select! {
            req = self.execute_request_rx.recv() => {
                let Some(req) = req else {
                    return;
                };
                self.handle_execute_request(req);
            },
            res = self.execute_result_rx.recv() => {
                let Some(res) = res else {
                    return;
                };
                self.handle_execute_result(res).await;
            }
        }
        event_buf.append(&mut self.event_buf);
    }

    /// Begins executing the tool future, identified by an id
    ///
    /// Generally, the id would just be the tool_use_id returned by the model.
    pub async fn start_tool_execution(&mut self, req: StartToolExecution) {
        // this will never fail - ToolExecutor owns both tx and rx
        let _ = self.execute_request_tx.send(ExecuteRequest::Tool(req)).await;
    }

    /// Begins executing the provided hook config.
    ///
    /// Note that [HookExecutionId] actually contains the hook config itself.
    pub async fn start_hook_execution(&mut self, req: StartHookExecution) {
        let _ = self.execute_request_tx.send(ExecuteRequest::Hook(req)).await;
    }

    /// Cancels an executing tool
    pub fn cancel_tool_execution(&self, id: &ToolExecutionId) {
        // Removing the executing tool will be done on the result handler.
        if let Some(v) = self.executing_tools.get(id) {
            v.cancel_token.cancel();
        }
    }

    /// Cancels an executing tool
    pub fn cancel_hook_execution(&self, id: &HookExecutionId) {
        // Removing the executing hook will be done on the result handler.
        if let Some(v) = self.executing_hooks.get(id) {
            v.cancel_token.cancel();
        }
    }

    fn handle_execute_request(&mut self, req: ExecuteRequest) {
        debug!(?req, "background executor received new request");
        match req {
            ExecuteRequest::Tool(t) => self.handle_tool_execute_request(t),
            ExecuteRequest::Hook(h) => self.handle_hook_execute_request(h),
        };
    }

    fn handle_tool_execute_request(&mut self, req: StartToolExecution) {
        let result_tx = self.execute_result_tx.clone();
        let cancel_token = CancellationToken::new();

        let id_clone = req.id.clone();
        let cancel_token_clone = cancel_token.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = cancel_token_clone.cancelled() => {
                    let _ = result_tx.send(ExecutorResult::Tool(ToolExecutorResult::Cancelled { id: id_clone })).await;
                }
                result = req.fut => {
                    let _ = result_tx.send(ExecutorResult::Tool(ToolExecutorResult::Completed { id: id_clone, result })).await;
                }
            }
        });

        let start_time = Utc::now();
        self.event_buf
            .push(TaskExecutorEvent::ToolExecutionStart(ToolExecutionStartEvent {
                id: req.id.clone(),
                tool: req.tool.clone(),
                start_time,
            }));
        self.executing_tools.insert(req.id, ExecutingTool {
            tool: req.tool,
            cancel_token,
            start_instant: Instant::now(),
            start_time,
            context_rx: req.context_rx,
        });
    }

    fn handle_hook_execute_request(&mut self, req: StartHookExecution) {
        // Handle cached hooks immediately.
        if let Some(cached) = self.get_cached_hook(&req.id.hook) {
            debug!(?cached, "found cached hook");
            self.event_buf
                .push(TaskExecutorEvent::CachedHookRun(CachedHookRunEvent {
                    id: req.id,
                    result: cached,
                }));
            return;
        }

        let req_id = req.id.clone();

        // Otherwise, run the hook on another task.
        let result_tx = self.execute_result_tx.clone();
        let cancel_token = CancellationToken::new();
        let id_clone = req.id.clone();
        let cancel_token_clone = cancel_token.clone();

        match req.id.hook.config.clone() {
            HookConfig::ShellCommand(command) => {
                tokio::spawn(async move {
                    let cwd = std::env::current_dir()
                        .expect("current dir exists")
                        .to_string_lossy()
                        .to_string();
                    let fut = run_command_hook(
                        req.id.hook.trigger,
                        command.clone(),
                        &cwd,
                        req.prompt,
                        req.id.tool_context,
                    );
                    tokio::select! {
                        _ = cancel_token_clone.cancelled() => {
                            let _ = result_tx.send(ExecutorResult::Hook(HookExecutorResult::Cancelled { id: id_clone })).await;
                        }
                        result = fut => {
                            let _ = result_tx
                                .send(ExecutorResult::Hook(HookExecutorResult::Completed {
                                    id: id_clone,
                                    result: HookResult::Command(result.0),
                                    duration: result.1
                                }))
                                .await;
                        }
                    }
                });
            },
            HookConfig::Tool(_) => (),
        };

        let start_time = Utc::now();
        self.event_buf
            .push(TaskExecutorEvent::HookExecutionStart(HookExecutionStartEvent {
                id: req_id.clone(),
                start_time,
            }));
        self.executing_hooks.insert(req_id, ExecutingHook {
            cancel_token,
            start_instant: Instant::now(),
            start_time,
        });
    }

    fn get_cached_hook(&self, hook: &Hook) -> Option<HookResult> {
        self.hooks_cache.get(hook).and_then(|o| {
            if let Some(expiry) = o.expiry {
                if Instant::now() < expiry {
                    Some(o.result.clone())
                } else {
                    None
                }
            } else {
                Some(o.result.clone())
            }
        })
    }

    async fn handle_execute_result(&mut self, result: ExecutorResult) {
        match result {
            ExecutorResult::Tool(result) => {
                debug_assert!(self.executing_tools.contains_key(result.id()));
                if let Some(x) = self.executing_tools.remove(result.id()) {
                    // Get tool specific context, if it exists.
                    let context = (x.context_rx.await).ok();
                    self.event_buf
                        .push(TaskExecutorEvent::ToolExecutionEnd(ToolExecutionEndEvent {
                            id: result.id().clone(),
                            tool: x.tool,
                            result: result.clone(),
                            start_time: x.start_time,
                            end_time: Utc::now(),
                            duration: Instant::now().duration_since(x.start_instant),
                            context,
                        }));
                }
            },
            ExecutorResult::Hook(result) => {
                debug_assert!(self.executing_hooks.contains_key(result.id()));
                if let Some(x) = self.executing_hooks.remove(result.id()) {
                    self.event_buf
                        .push(TaskExecutorEvent::HookExecutionEnd(HookExecutionEndEvent {
                            id: result.id().clone(),
                            result: result.clone(),
                            start_time: x.start_time,
                            end_time: Utc::now(),
                            duration: Instant::now().duration_since(x.start_instant),
                        }));
                }
            },
        }
    }
}

impl Default for TaskExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub enum ExecuteRequest {
    Tool(StartToolExecution),
    Hook(StartHookExecution),
}

/// A request to start executing a tool
pub struct StartToolExecution {
    /// Id for the tool execution. Uniquely identified by an agent id and tool use id.
    pub id: ToolExecutionId,
    /// The tool to execute
    pub tool: Tool,
    /// The future containing the tool execution
    pub fut: ToolFuture,
    /// A receiver for tool state
    pub context_rx: oneshot::Receiver<ToolState>,
}

impl std::fmt::Debug for StartToolExecution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StartToolExecution")
            .field("id", &self.id)
            .field("tool", &self.tool)
            .field("fut", &"<ToolFuture>")
            .field("context_rx", &self.context_rx)
            .finish()
    }
}

/// A request to start executing a hook
#[derive(Debug)]
pub struct StartHookExecution {
    pub id: HookExecutionId,
    /// The user prompt. Passed to the hook as context if available.
    pub prompt: Option<String>,
}

#[derive(Debug)]
struct ExecutingTool {
    tool: Tool,
    cancel_token: CancellationToken,
    start_instant: Instant,
    start_time: DateTime<Utc>,
    context_rx: oneshot::Receiver<ToolState>,
}

#[derive(Debug)]
struct ExecutingHook {
    cancel_token: CancellationToken,
    start_instant: Instant,
    start_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum TaskExecutorEvent {
    /// A tool has started executing
    ToolExecutionStart(ToolExecutionStartEvent),
    /// A tool completed executing
    ToolExecutionEnd(ToolExecutionEndEvent),

    HookExecutionStart(HookExecutionStartEvent),
    HookExecutionEnd(HookExecutionEndEvent),
    /// A hook was not executed because it was already in the cache.
    CachedHookRun(CachedHookRunEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionStartEvent {
    /// Identifier for the tool execution
    pub id: ToolExecutionId,
    pub tool: Tool,
    pub start_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionEndEvent {
    /// Identifier for the tool execution
    pub id: ToolExecutionId,
    pub tool: Tool,
    pub result: ToolExecutorResult,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub duration: Duration,
    /// Optional context that was updated as part of the execution.
    pub context: Option<ToolState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookExecutionStartEvent {
    pub id: HookExecutionId,
    pub start_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookExecutionEndEvent {
    pub id: HookExecutionId,
    pub result: HookExecutorResult,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub duration: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedHookRunEvent {
    pub id: HookExecutionId,
    pub result: HookResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToolExecutionId {
    tool_use_id: String,
}

impl ToolExecutionId {
    pub fn new(tool_use_id: String) -> Self {
        Self { tool_use_id }
    }

    pub fn tool_use_id(&self) -> &str {
        &self.tool_use_id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum ExecutorResult {
    Tool(ToolExecutorResult),
    Hook(HookExecutorResult),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolExecutorResult {
    /// Tool execution completed and returned a result
    Completed {
        /// Identifier for the tool execution
        id: ToolExecutionId,
        result: ToolExecutionResult,
    },
    /// Tool execution was cancelled before a result could be returned
    Cancelled {
        /// Identifier for the tool execution
        id: ToolExecutionId,
    },
}

impl ToolExecutorResult {
    fn id(&self) -> &ToolExecutionId {
        match self {
            ToolExecutorResult::Completed { id, .. } => id,
            ToolExecutorResult::Cancelled { id } => id,
        }
    }

    /// The output of the tool execution, if it completed successfully.
    pub fn tool_execution_output(&self) -> Option<&ToolExecutionOutput> {
        match self {
            ToolExecutorResult::Completed { result: Ok(res), .. } => Some(res),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HookExecutorResult {
    Completed {
        id: HookExecutionId,
        result: HookResult,
        duration: Duration,
    },
    Cancelled {
        id: HookExecutionId,
    },
}

impl HookExecutorResult {
    fn id(&self) -> &HookExecutionId {
        match self {
            HookExecutorResult::Completed { id, .. } => id,
            HookExecutorResult::Cancelled { id } => id,
        }
    }
}

/// Unique identifier for a hook execution
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookExecutionId {
    pub hook: Hook,
    pub tool_context: Option<ToolContext>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hook {
    pub trigger: HookTrigger,
    pub config: HookConfig,
}

#[derive(Debug, Clone)]
struct CachedHook {
    result: HookResult,
    expiry: Option<Instant>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HookResult {
    /// Result for command hooks
    Command(Result<CommandResult, String>),
    /// Result for tool hooks (unimplemented)
    Tool { output: String },
}

impl HookResult {
    /// Returns the exit code of the hook if it was a command hook that ran to completion.
    pub fn exit_code(&self) -> Option<i32> {
        match self {
            HookResult::Command(Ok(CommandResult { exit_code, .. })) => Some(*exit_code),
            _ => None,
        }
    }

    pub fn is_success(&self) -> bool {
        match self {
            HookResult::Command(res) => res.as_ref().is_ok_and(|r| r.exit_code == 0),
            HookResult::Tool { .. } => panic!("unimplemented"),
        }
    }

    /// Returns the hook output, if it exists.
    ///
    /// Note that this includes hooks that have output but are not successful, e.g. command hooks
    /// that have a nonzero exit code.
    pub fn output(&self) -> Option<&str> {
        match self {
            HookResult::Command(Ok(CommandResult { output, .. })) => Some(output),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    /// The command's process exit code. 0 for success, nonzero for error.
    exit_code: i32,
    /// Contains stdout if exit_code is 0, otherwise stderr.
    output: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToolContext {
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub tool_response: Option<serde_json::Value>,
}

impl From<(&ToolUseBlock, &Tool)> for ToolContext {
    fn from(value: (&ToolUseBlock, &Tool)) -> Self {
        Self {
            tool_name: value.1.canonical_tool_name().as_full_name().to_string(),
            tool_input: value.0.input.clone(),
            tool_response: None,
        }
    }
}

impl From<(&ToolUseBlock, &Tool, &serde_json::Value)> for ToolContext {
    fn from(value: (&ToolUseBlock, &Tool, &serde_json::Value)) -> Self {
        Self {
            tool_name: value.1.canonical_tool_name().as_full_name().to_string(),
            tool_input: value.0.input.clone(),
            tool_response: Some(value.2.clone()),
        }
    }
}

async fn run_command_hook(
    trigger: HookTrigger,
    config: CommandHook,
    cwd: &str,
    prompt: Option<String>,
    tool_context: Option<ToolContext>,
) -> (Result<CommandResult, String>, Duration) {
    let start_time = Instant::now();

    let command = &config.command;

    #[cfg(unix)]
    let mut cmd = tokio::process::Command::new("bash");
    #[cfg(unix)]
    let cmd = cmd
        .arg("-c")
        .arg(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    let mut cmd = tokio::process::Command::new("cmd");
    #[cfg(windows)]
    let cmd = cmd
        .arg("/C")
        .arg(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let timeout = Duration::from_millis(config.opts.timeout_ms);

    // Generate hook command input in JSON format
    let mut hook_input = serde_json::json!({
        "hook_event_name": trigger.to_string(),
        "cwd": cwd
    });

    // Set USER_PROMPT environment variable and add to JSON input if provided
    if let Some(prompt) = prompt {
        // Sanitize the prompt to avoid issues with special characters
        let sanitized_prompt = sanitize_user_prompt(prompt.as_str());
        cmd.env("USER_PROMPT", sanitized_prompt);
        hook_input["prompt"] = serde_json::Value::String(prompt);
    }

    // ToolUse specific input
    if let Some(tool_ctx) = tool_context {
        hook_input["tool_name"] = serde_json::Value::String(tool_ctx.tool_name);
        hook_input["tool_input"] = tool_ctx.tool_input;
        if let Some(response) = tool_ctx.tool_response {
            hook_input["tool_response"] = response;
        }
    }
    let json_input = serde_json::to_string(&hook_input).unwrap_or_default();

    // Build a future for hook command w/ the JSON input passed in through STDIN
    let command_future = async move {
        let mut child = cmd.spawn()?;
        if let Some(stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            let mut stdin = stdin;
            let _ = stdin.write_all(json_input.as_bytes()).await;
            let _ = stdin.shutdown().await;
        }
        child.wait_with_output().await
    };

    // Run with timeout
    let result = match tokio::time::timeout(timeout, command_future).await {
        Ok(Ok(output)) => {
            let exit_code = output.status.code().unwrap_or(-1);
            let raw_output = if exit_code == 0 {
                output.stdout.to_str_lossy()
            } else {
                output.stderr.to_str_lossy()
            };
            let formatted_output = format!(
                "{}{}",
                truncate_safe(&raw_output, config.opts.max_output_size),
                if raw_output.len() > config.opts.max_output_size {
                    " ... truncated"
                } else {
                    ""
                }
            );
            Ok(CommandResult {
                exit_code,
                output: formatted_output,
            })
        },
        Ok(Err(err)) => Err(format!("failed to execute command: {}", err)),
        Err(_) => Err(format!("command timed out after {} ms", timeout.as_millis())),
    };

    (result, start_time.elapsed())
}

/// Sanitizes a string value to be used as an environment variable
fn sanitize_user_prompt(input: &str) -> String {
    // Limit the size of input to first 4096 characters
    let truncated = if input.len() > 4096 { &input[0..4096] } else { input };

    // Remove any potentially problematic characters
    truncated.replace(|c: char| c.is_control() && c != '\n' && c != '\r' && c != '\t', "")
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_COMMAND_HOOK: &str = r#"
{
    "command": "echo hello world"
}
"#;

    async fn run_with_timeout<T: Future>(timeout: Duration, fut: T) {
        match tokio::time::timeout(timeout, fut).await {
            Ok(_) => (),
            Err(e) => panic!("Future failed to resolve within timeout: {}", e),
        }
    }

    #[tokio::test]
    async fn test_hook_execution() {
        let mut executor = TaskExecutor::new();

        executor
            .start_hook_execution(StartHookExecution {
                id: HookExecutionId {
                    hook: Hook {
                        trigger: HookTrigger::UserPromptSubmit,
                        config: serde_json::from_str(TEST_COMMAND_HOOK).unwrap(),
                    },
                    tool_context: None,
                },
                prompt: None,
            })
            .await;

        run_with_timeout(Duration::from_millis(100), async move {
            let mut event_buf = Vec::new();
            loop {
                executor.recv_next(&mut event_buf).await;
                // Check if we get a "hello world" successful hook execution.
                if event_buf.iter().any(|ev| match ev {
                    TaskExecutorEvent::HookExecutionEnd(HookExecutionEndEvent { result, .. }) => {
                        let HookExecutorResult::Completed { result, .. } = result else {
                            return false;
                        };
                        let HookResult::Command(result) = result else {
                            return false;
                        };
                        result
                            .as_ref()
                            .is_ok_and(|output| output.output.contains("hello world"))
                    },
                    _ => false,
                }) {
                    // Hook succeeded with expected output, break.
                    break;
                }
                event_buf.drain(..);
            }
        })
        .await;
    }
}
