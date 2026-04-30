//! A Unix implementation of ExecuteCmd that uses bash as the shell.

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;

use bstr::ByteSlice as _;
use schemars::{
    JsonSchema,
    schema_for,
};
use serde::{
    Deserialize,
    Serialize,
};
use tokio::io::{
    AsyncBufReadExt,
    AsyncReadExt,
};
use tokio::process::Command;
use tokio::sync::broadcast;
use tokio::time::Duration;

use crate::agent::protocol::{
    AgentEvent,
    ContentChunk,
    UpdateEvent,
};
use crate::agent::tools::{
    BuiltInToolName,
    BuiltInToolTrait,
    ToolExecutionError,
    ToolExecutionOutput,
    ToolExecutionOutputItem,
    ToolExecutionResult,
};
use crate::agent::util::consts::{
    USER_AGENT_APP_NAME,
    USER_AGENT_ENV_VAR,
    USER_AGENT_VERSION_KEY,
    USER_AGENT_VERSION_VALUE,
};
use crate::agent::util::path::canonicalize_path_sys;
use crate::agent::util::shell::wrap_cmd_with_fd_limit;
use crate::agent::util::truncate_safe;
use crate::util::providers::SystemProvider;

/// Maximum tool response size in bytes (actual service limit is 800_000).
const MAX_TOOL_RESPONSE_SIZE: usize = 400_000;

/// Maximum size for command output (stdout/stderr) in bytes.
/// This is a third of MAX_TOOL_RESPONSE_SIZE to allow room for both stdout and stderr
/// plus the JSON structure in the response.
const MAX_COMMAND_OUTPUT_SIZE: usize = MAX_TOOL_RESPONSE_SIZE / 3;

const EXECUTE_CMD_TOOL_DESCRIPTION: &str = r#"
A tool for executing bash commands.

WHEN TO USE THIS TOOL:
- Use only as a last-resort when no other available tool can accomplish the task

HOW TO USE:
- Provide the command to execute
- Use the `working_dir` argument to execute a command in another directory (defaults to current working directory)

LIMITATIONS:
- Does not respect user's bash profile or aliases

TIPS:
- Use the fileRead and fileWrite tools for reading and modifying files
- NEVER prefix commands with cd to execute a command in another directory. Use the `working_dir` argument instead
"#;

const EXECUTE_CMD_SCHEMA: &str = r#"
{
    "type": "object",
    "properties": {
        "command": {
            "type": "string",
            "description": "Command to execute"
        },
        "working_dir": {
            "type": "string",
            "description": "Optional working directory for command execution. If not specified, uses the current working directory."
        }
    },
    "required": [
        "command"
    ]
}
"#;

impl BuiltInToolTrait for ExecuteCmd {
    fn name() -> BuiltInToolName {
        BuiltInToolName::ExecuteCmd
    }

    fn description() -> std::borrow::Cow<'static, str> {
        EXECUTE_CMD_TOOL_DESCRIPTION.into()
    }

    fn input_schema() -> std::borrow::Cow<'static, str> {
        EXECUTE_CMD_SCHEMA.into()
    }

    fn aliases() -> Option<&'static [&'static str]> {
        Some(&["execute_bash", "execute_cmd", "shell"])
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecuteCmd {
    pub command: String,
    pub working_dir: Option<String>,
}

impl ExecuteCmd {
    /// How often batched output lines are flushed to the broadcast channel.
    /// Matches the TUI-side batch interval (see app-store.ts pendingToolOutputFlush).
    const STREAM_FLUSH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(32);

    pub fn tool_schema() -> serde_json::Value {
        let schema = schema_for!(Self);
        serde_json::to_value(schema).expect("creating tool schema should not fail")
    }

    /// Returns the canonicalized working directory path, if set.
    fn canonical_working_dir<P: SystemProvider>(&self, provider: &P) -> Result<Option<String>, String> {
        self.working_dir
            .as_ref()
            .map(|dir| {
                canonicalize_path_sys(dir, provider).map_err(|e| format!("Invalid working directory '{}': {}", dir, e))
            })
            .transpose()
    }

    pub async fn validate<P: SystemProvider>(&self, provider: &P) -> Result<(), String> {
        if self.command.is_empty() {
            return Err("Command must not be empty".to_string());
        }
        if let Some(ref dir) = self.canonical_working_dir(provider)?
            && !Path::new(dir).is_dir()
        {
            return Err(format!("Working directory is not a directory: {}", dir));
        }
        Ok(())
    }

    /// Execute with optional streaming support. When `event_tx` is `Some`, emits
    /// batched `ToolCallUpdate` events for stdout/stderr. Lines are accumulated
    /// and flushed on a timer to avoid overwhelming the broadcast channel when a
    /// command produces tens of thousands of lines (e.g. `brazil-build`).
    /// When `None`, falls back to blocking `wait_with_output()`.
    pub async fn execute<P: SystemProvider>(
        &self,
        provider: &P,
        event_tx: Option<(String, broadcast::Sender<AgentEvent>)>,
    ) -> ToolExecutionResult {
        let Some((tool_use_id, tx)) = event_tx else {
            return self.execute_blocking(provider).await;
        };

        let mut child = self.spawn_child(provider)?;

        let stdout = tokio::io::BufReader::new(
            child
                .stdout
                .take()
                .expect("stdout should be piped since we set Stdio::piped()"),
        );
        let stderr = tokio::io::BufReader::new(
            child
                .stderr
                .take()
                .expect("stderr should be piped since we set Stdio::piped()"),
        );
        let mut stdout_lines = stdout.lines();
        let mut stderr_lines = stderr.lines();

        let mut accumulated_stdout = String::new();
        let mut accumulated_stderr = String::new();
        let mut stdout_done = false;
        let mut stderr_done = false;

        // Buffer for batching lines before sending to the broadcast channel.
        let mut pending_output = String::new();
        let mut flush_interval = tokio::time::interval(Self::STREAM_FLUSH_INTERVAL);
        // Consume the first immediate tick so the loop starts clean.
        flush_interval.tick().await;

        let status = loop {
            tokio::select! {
                biased;

                // Flush buffered output on timer tick.
                _ = flush_interval.tick() => {
                    if !pending_output.is_empty() {
                        let _ = tx.send(AgentEvent::Update(UpdateEvent::ToolCallUpdate {
                            id: tool_use_id.clone(),
                            content: ContentChunk::Text(std::mem::take(&mut pending_output)),
                        }));
                    }
                }

                line = stdout_lines.next_line(), if !stdout_done => {
                    match line {
                        Ok(Some(line)) => {
                            let clean = sanitize_unicode_tags(&line);
                            pending_output.push_str(&clean);
                            pending_output.push('\n');
                            accumulated_stdout.push_str(&clean);
                            accumulated_stdout.push('\n');
                        }
                        Ok(None) => stdout_done = true,
                        Err(e) => {
                            tracing::warn!("stdout read error: {e}");
                            stdout_done = true;
                        }
                    }
                }
                line = stderr_lines.next_line(), if !stderr_done => {
                    match line {
                        Ok(Some(line)) => {
                            let clean = sanitize_unicode_tags(&line);
                            pending_output.push_str(&clean);
                            pending_output.push('\n');
                            accumulated_stderr.push_str(&clean);
                            accumulated_stderr.push('\n');
                        }
                        Ok(None) => stderr_done = true,
                        Err(e) => {
                            tracing::warn!("stderr read error: {e}");
                            stderr_done = true;
                        }
                    }
                }
                status = child.wait() => {
                    // Child exited. Drain remaining pipe data with a timeout —
                    // a grandchild daemon may still hold the FDs open.
                    const DRAIN_TIMEOUT: Duration = Duration::from_millis(100);
                    loop {
                        let mut drained = false;
                        if !stdout_done {
                            match tokio::time::timeout(DRAIN_TIMEOUT, stdout_lines.next_line()).await {
                                Ok(Ok(Some(line))) => {
                                    let clean = sanitize_unicode_tags(&line);
                                    pending_output.push_str(&clean);
                                    pending_output.push('\n');
                                    accumulated_stdout.push_str(&clean);
                                    accumulated_stdout.push('\n');
                                    drained = true;
                                },
                                _ => stdout_done = true,
                            }
                        }
                        if !stderr_done {
                            match tokio::time::timeout(DRAIN_TIMEOUT, stderr_lines.next_line()).await {
                                Ok(Ok(Some(line))) => {
                                    let clean = sanitize_unicode_tags(&line);
                                    pending_output.push_str(&clean);
                                    pending_output.push('\n');
                                    accumulated_stderr.push_str(&clean);
                                    accumulated_stderr.push('\n');
                                    drained = true;
                                },
                                _ => stderr_done = true,
                            }
                        }
                        if !drained || (stdout_done && stderr_done) { break; }
                    }
                    break status.map_err(|e| ToolExecutionError::io(
                        format!("No exit status for '{}'", &self.command), e))?;
                }
            }
            if stdout_done && stderr_done {
                // Pipes closed naturally (no daemon). Still need to wait for exit.
                break child
                    .wait()
                    .await
                    .map_err(|e| ToolExecutionError::io(format!("No exit status for '{}'", &self.command), e))?;
            }
        };

        // Flush any remaining buffered output.
        if !pending_output.is_empty() {
            let _ = tx.send(AgentEvent::Update(UpdateEvent::ToolCallUpdate {
                id: tool_use_id.clone(),
                content: ContentChunk::Text(pending_output),
            }));
        }

        Ok(build_output_result(
            &accumulated_stdout,
            &accumulated_stderr,
            &status.to_string(),
        ))
    }

    /// Blocking fallback: collects all output at process completion.
    /// Used when no event channel is provided.
    ///
    /// Uses a concurrent read loop with timeout-bounded drain instead of
    /// `wait_with_output()`, which blocks until all pipe FDs are closed.
    /// A grandchild daemon that inherits the piped FDs would cause a hang.
    async fn execute_blocking<P: SystemProvider>(&self, provider: &P) -> ToolExecutionResult {
        let mut child = self.spawn_child(provider)?;

        let mut stdout = child.stdout.take().expect("stdout should be piped");
        let mut stderr = child.stderr.take().expect("stderr should be piped");

        let mut stdout_buf = Vec::new();
        let mut stderr_buf = Vec::new();
        let mut stdout_done = false;
        let mut stderr_done = false;
        let mut stdout_chunk = [0u8; 4096];
        let mut stderr_chunk = [0u8; 4096];

        let exit_status = loop {
            tokio::select! {
                n = stdout.read(&mut stdout_chunk), if !stdout_done => match n {
                    Ok(0) => stdout_done = true,
                    Ok(n) => stdout_buf.extend_from_slice(&stdout_chunk[..n]),
                    Err(_) => stdout_done = true,
                },
                n = stderr.read(&mut stderr_chunk), if !stderr_done => match n {
                    Ok(0) => stderr_done = true,
                    Ok(n) => stderr_buf.extend_from_slice(&stderr_chunk[..n]),
                    Err(_) => stderr_done = true,
                },
                status = child.wait() => {
                    // Drain remaining buffered output with a timeout. Child exit
                    // does not imply pipe EOF — a grandchild daemon may still
                    // hold the FDs open.
                    const DRAIN_TIMEOUT: Duration = Duration::from_millis(100);
                    loop {
                        let mut drained = false;
                        if !stdout_done {
                            match tokio::time::timeout(DRAIN_TIMEOUT, stdout.read(&mut stdout_chunk)).await {
                                Ok(Ok(0)) => stdout_done = true,
                                Ok(Ok(n)) => { stdout_buf.extend_from_slice(&stdout_chunk[..n]); drained = true; },
                                _ => stdout_done = true,
                            }
                        }
                        if !stderr_done {
                            match tokio::time::timeout(DRAIN_TIMEOUT, stderr.read(&mut stderr_chunk)).await {
                                Ok(Ok(0)) => stderr_done = true,
                                Ok(Ok(n)) => { stderr_buf.extend_from_slice(&stderr_chunk[..n]); drained = true; },
                                _ => stderr_done = true,
                            }
                        }
                        if !drained || (stdout_done && stderr_done) { break; }
                    }
                    break status.map_err(|e| ToolExecutionError::io(
                        format!("No exit status for '{}'", &self.command), e))?;
                },
            };
        };

        let clean_stdout = sanitize_unicode_tags(stdout_buf.to_str_lossy());
        let clean_stderr = sanitize_unicode_tags(stderr_buf.to_str_lossy());

        Ok(build_output_result(
            &clean_stdout,
            &clean_stderr,
            &exit_status.to_string(),
        ))
    }

    /// Spawn the shell child process with piped stdout/stderr.
    fn spawn_child<P: SystemProvider>(&self, provider: &P) -> Result<tokio::process::Child, ToolExecutionError> {
        let process_dir = self
            .canonical_working_dir(provider)
            .map_err(ToolExecutionError::Custom)?
            .unwrap_or_else(|| {
                provider
                    .cwd()
                    .map_or_else(|_| ".".to_string(), |p| p.to_string_lossy().to_string())
            });

        let shell = provider
            .var("KIRO_CHAT_SHELL")
            .or_else(|_| provider.var("AMAZON_Q_CHAT_SHELL"))
            .unwrap_or("bash".to_string());

        let env_vars = env_vars_with_user_agent();
        let wrapped_command = wrap_cmd_with_fd_limit(&self.command);

        Command::new(shell)
            .arg("-c")
            .arg(&wrapped_command)
            .current_dir(&process_dir)
            .envs(env_vars)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ToolExecutionError::io(format!("Failed to spawn command '{}'", &self.command), e))
    }
}

/// Build the final JSON tool output from accumulated stdout/stderr.
fn build_output_result(stdout: &str, stderr: &str, exit_status: &str) -> ToolExecutionOutput {
    let result = serde_json::json!({
        "exit_status": exit_status,
        "stdout": format_output(stdout, MAX_COMMAND_OUTPUT_SIZE),
        "stderr": format_output(stderr, MAX_COMMAND_OUTPUT_SIZE),
    });
    ToolExecutionOutput {
        items: vec![ToolExecutionOutputItem::Json(result)],
    }
}

/// Format command output with truncation.
fn format_output(output: &str, max_size: usize) -> String {
    format!(
        "{}{}",
        truncate_safe(output, max_size),
        if output.len() > max_size { " ... truncated" } else { "" }
    )
}

/// Returns `true` if the character is from an invisible or control Unicode range
/// that is considered unsafe for LLM input. These rarely appear in normal input,
/// so stripping them is generally safe.
/// The replacement character U+FFFD (�) is preserved to indicate invalid bytes.
fn is_hidden(c: char) -> bool {
    match c {
        '\u{E0000}'..='\u{E007F}' |     // TAG characters (used for hidden prompts)
        '\u{200B}'..='\u{200F}'  |      // zero-width space, ZWJ, ZWNJ, RTL/LTR marks
        '\u{2028}'..='\u{202F}'  |      // line / paragraph separators, narrow NB-SP
        '\u{205F}'..='\u{206F}'  |      // format control characters
        '\u{FFF0}'..='\u{FFFC}'  |
        '\u{FFFE}'..='\u{FFFF}'   // Specials block (non-characters)
        => true,
        _ => false,
    }
}

/// Remove hidden / control characters from `text`.
///
/// * `text`   –  raw user input or file content
///
/// The function keeps things **O(n)** with a single allocation and logs how many
/// characters were dropped. 400 KB worst-case size ⇒ sub-millisecond runtime.
fn sanitize_unicode_tags(text: impl AsRef<str>) -> String {
    let mut removed = 0;
    let out: String = text
        .as_ref()
        .chars()
        .filter(|&c| {
            let bad = is_hidden(c);
            if bad {
                removed += 1;
            }
            !bad
        })
        .collect();

    if removed > 0 {
        tracing::debug!("Detected and removed {} hidden chars", removed);
    }
    out
}

/// Helper function to set up environment variables with user agent metadata.
fn env_vars_with_user_agent() -> HashMap<String, String> {
    let mut env_vars: HashMap<String, String> = std::env::vars().collect();

    // Prevent git from spawning an interactive editor (e.g. for `git commit -e`
    // during `git rebase --continue`). Since child processes run with stdin as
    // /dev/null, editors like vim and nvim hang indefinitely instead of exiting.
    env_vars.insert("GIT_EDITOR".to_string(), "true".to_string());

    // Set up additional metadata for the AWS CLI user agent
    let user_agent_metadata_value =
        format!("{USER_AGENT_APP_NAME} {USER_AGENT_VERSION_KEY}/{USER_AGENT_VERSION_VALUE}");

    // Check if the user agent metadata env var already exists
    let existing_value = std::env::var(USER_AGENT_ENV_VAR).ok();

    // If the user agent metadata env var already exists, append to it, otherwise set it
    if let Some(existing_value) = existing_value {
        if !existing_value.is_empty() {
            env_vars.insert(
                USER_AGENT_ENV_VAR.to_string(),
                format!("{existing_value} {user_agent_metadata_value}"),
            );
        } else {
            env_vars.insert(USER_AGENT_ENV_VAR.to_string(), user_agent_metadata_value);
        }
    } else {
        env_vars.insert(USER_AGENT_ENV_VAR.to_string(), user_agent_metadata_value);
    }

    env_vars
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::test::TestBase;

    // ── Streamed chunks are sanitized ────────────────────────
    // Feature: shell-output-streaming
    // Streamed chunks are sanitized and correspond to source lines
    //
    // For any input containing hidden Unicode characters, `sanitize_unicode_tags`
    // MUST remove every `is_hidden` char and preserve all others in order.
    //
    // Validates: Requirements 1.2, 2.1, 4.1
    // ─────────────────────────────────────────────────────────────────────

    /// Representative hidden chars — one from each `is_hidden` range.
    const HIDDEN_SAMPLES: &[char] = &[
        '\u{E0000}',
        '\u{E0041}',
        '\u{E007F}', // TAG range
        '\u{200B}',
        '\u{200D}',
        '\u{200F}', // zero-width / directional marks
        '\u{2028}',
        '\u{202A}',
        '\u{202F}', // separators / narrow NB-SP
        '\u{205F}',
        '\u{2066}',
        '\u{206F}', // format controls
        '\u{FFF0}',
        '\u{FFFC}', // specials
        '\u{FFFE}',
        '\u{FFFF}', // specials (non-characters)
    ];

    #[test]
    fn sanitize_removes_all_hidden_ranges() {
        // Build a string with every hidden sample interleaved with visible text.
        let visible = "hello";
        let mut input = String::new();
        for &h in HIDDEN_SAMPLES {
            input.push_str(visible);
            input.push(h);
        }
        input.push_str(visible);

        let result = sanitize_unicode_tags(&input);

        // No hidden chars survive.
        for c in result.chars() {
            assert!(!is_hidden(c), "hidden char U+{:04X} survived sanitization", c as u32);
        }
        // Only the visible segments remain, concatenated.
        let expected = visible.repeat(HIDDEN_SAMPLES.len() + 1);
        assert_eq!(result, expected);
    }

    #[test]
    fn sanitize_preserves_visible_chars_in_order() {
        let visible_chars = ['a', 'Z', '0', '🦀', '你', 'Ω', '\n', '\t', ' '];
        let mut input = String::new();
        let mut expected = String::new();
        for &v in &visible_chars {
            input.push(v);
            expected.push(v);
            // Inject a hidden char after each visible char.
            input.push('\u{200B}');
        }

        let result = sanitize_unicode_tags(&input);
        assert_eq!(result, expected, "visible chars not preserved in order");
    }

    #[test]
    fn sanitize_is_idempotent() {
        let inputs = [
            "",
            "plain ascii",
            "emoji 🎉 and CJK 你好",
            "mixed\u{200B}hidden\u{E0041}chars",
            "\u{200B}\u{200D}\u{FFFE}", // all hidden
        ];
        for input in inputs {
            let once = sanitize_unicode_tags(input);
            let twice = sanitize_unicode_tags(&once);
            assert_eq!(once, twice, "not idempotent for input: {input:?}");
        }
    }

    #[test]
    fn sanitize_returns_empty_for_all_hidden_input() {
        let input: String = HIDDEN_SAMPLES.iter().collect();
        assert_eq!(sanitize_unicode_tags(&input), "");
    }

    #[test]
    fn sanitize_noop_on_pure_visible_input() {
        let inputs = [
            "hello world",
            "Rust 🦀 > C",
            "line1\nline2\nline3",
            "tabs\there\tand\tthere",
            "",
        ];
        for input in inputs {
            assert_eq!(
                sanitize_unicode_tags(input),
                input,
                "modified pure visible input: {input:?}"
            );
        }
    }

    #[test]
    fn sanitize_handles_hidden_at_boundaries() {
        // Hidden at start.
        assert_eq!(sanitize_unicode_tags("\u{200B}abc"), "abc");
        // Hidden at end.
        assert_eq!(sanitize_unicode_tags("abc\u{200B}"), "abc");
        // Hidden only.
        assert_eq!(sanitize_unicode_tags("\u{E0000}"), "");
        // Single visible char surrounded by hidden.
        assert_eq!(sanitize_unicode_tags("\u{200B}x\u{FFFE}"), "x");
    }

    #[test]
    fn is_hidden_recognises_all_ranges() {
        let samples = ['\u{E0000}', '\u{200B}', '\u{2028}', '\u{205F}', '\u{FFF0}'];

        for ch in samples {
            assert!(is_hidden(ch), "char U+{:X} should be hidden", ch as u32);
        }

        for ch in ['a', '你', '\u{03A9}'] {
            assert!(!is_hidden(ch), "char {:?} should NOT be hidden", ch);
        }
    }

    #[test]
    fn sanitize_keeps_visible_text_intact() {
        let visible = "Rust 🦀 > C";
        assert_eq!(sanitize_unicode_tags(visible), visible);
    }

    #[test]
    fn sanitize_handles_large_mixture() {
        let visible_block = "abcXYZ";
        let hidden_block = "\u{200B}\u{E0000}";
        let mut big_input = String::new();
        for _ in 0..50_000 {
            big_input.push_str(visible_block);
            big_input.push_str(hidden_block);
        }

        let result = sanitize_unicode_tags(&big_input);

        assert_eq!(result.len(), 50_000 * visible_block.len());

        assert!(result.chars().all(|c| !is_hidden(c)));
    }

    // ── Blocking fallback when no event channel is provided ──
    // Feature: shell-output-streaming
    //
    // For any command, calling `execute(provider, None)` SHALL produce a
    // `ToolExecutionOutput` with a JSON object containing exactly
    // `exit_status`, `stdout`, and `stderr` fields.
    // ─────────────────────────────────────────────────────────────────────

    /// Helper: assert the output has the correct JSON schema with exactly
    /// `exit_status`, `stdout`, `stderr` fields, all strings.
    fn assert_blocking_output_schema(result: &ToolExecutionOutput) {
        assert_eq!(result.items.len(), 1, "expected exactly one output item");
        let json = match &result.items[0] {
            ToolExecutionOutputItem::Json(v) => v,
            other => panic!("expected Json output item, got: {other:?}"),
        };
        let obj = json.as_object().expect("output should be a JSON object");

        // Exactly three fields.
        assert_eq!(
            obj.len(),
            3,
            "expected exactly 3 fields (exit_status, stdout, stderr), got: {:?}",
            obj.keys().collect::<Vec<_>>()
        );

        for field in ["exit_status", "stdout", "stderr"] {
            assert!(obj.contains_key(field), "missing field: {field}");
            assert!(
                obj[field].is_string(),
                "field '{field}' should be a string, got: {:?}",
                obj[field]
            );
        }
    }

    #[tokio::test]
    async fn blocking_fallback_simple_echo() {
        let test_base = TestBase::new().await;
        let cmd = ExecuteCmd {
            command: "echo hello".to_string(),
            working_dir: None,
        };

        let result = cmd.execute(&test_base, None).await.unwrap();
        assert_blocking_output_schema(&result);

        let json = match &result.items[0] {
            ToolExecutionOutputItem::Json(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(json["exit_status"], "exit status: 0");
        assert!(json["stdout"].as_str().unwrap().contains("hello"));
    }

    #[tokio::test]
    async fn blocking_fallback_stderr_output() {
        let test_base = TestBase::new().await;
        let cmd = ExecuteCmd {
            command: "echo err >&2".to_string(),
            working_dir: None,
        };

        let result = cmd.execute(&test_base, None).await.unwrap();
        assert_blocking_output_schema(&result);

        let json = match &result.items[0] {
            ToolExecutionOutputItem::Json(v) => v,
            _ => unreachable!(),
        };
        assert!(json["stderr"].as_str().unwrap().contains("err"));
    }

    #[tokio::test]
    async fn blocking_fallback_mixed_stdout_stderr() {
        let test_base = TestBase::new().await;
        let cmd = ExecuteCmd {
            command: "echo out && echo err >&2".to_string(),
            working_dir: None,
        };

        let result = cmd.execute(&test_base, None).await.unwrap();
        assert_blocking_output_schema(&result);

        let json = match &result.items[0] {
            ToolExecutionOutputItem::Json(v) => v,
            _ => unreachable!(),
        };
        assert!(json["stdout"].as_str().unwrap().contains("out"));
        assert!(json["stderr"].as_str().unwrap().contains("err"));
    }

    #[tokio::test]
    async fn blocking_fallback_empty_output() {
        let test_base = TestBase::new().await;
        let cmd = ExecuteCmd {
            command: "true".to_string(),
            working_dir: None,
        };

        let result = cmd.execute(&test_base, None).await.unwrap();
        assert_blocking_output_schema(&result);

        let json = match &result.items[0] {
            ToolExecutionOutputItem::Json(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(json["exit_status"], "exit status: 0");
    }

    #[tokio::test]
    async fn blocking_fallback_nonzero_exit() {
        let test_base = TestBase::new().await;
        let cmd = ExecuteCmd {
            command: "exit 42".to_string(),
            working_dir: None,
        };

        let result = cmd.execute(&test_base, None).await.unwrap();
        assert_blocking_output_schema(&result);

        let json = match &result.items[0] {
            ToolExecutionOutputItem::Json(v) => v,
            _ => unreachable!(),
        };
        assert!(json["exit_status"].as_str().unwrap().contains("42"));
    }

    #[tokio::test]
    async fn blocking_fallback_multiline_output() {
        let test_base = TestBase::new().await;
        let cmd = ExecuteCmd {
            command: "printf 'line1\\nline2\\nline3\\n'".to_string(),
            working_dir: None,
        };

        let result = cmd.execute(&test_base, None).await.unwrap();
        assert_blocking_output_schema(&result);

        let json = match &result.items[0] {
            ToolExecutionOutputItem::Json(v) => v,
            _ => unreachable!(),
        };
        let stdout = json["stdout"].as_str().unwrap();
        assert!(stdout.contains("line1"));
        assert!(stdout.contains("line2"));
        assert!(stdout.contains("line3"));
    }

    #[tokio::test]
    async fn blocking_fallback_special_chars() {
        let test_base = TestBase::new().await;
        // Test with unicode, emoji, and special shell characters
        let cmd = ExecuteCmd {
            command: r#"printf 'hello 🦀 world\n'"#.to_string(),
            working_dir: None,
        };

        let result = cmd.execute(&test_base, None).await.unwrap();
        assert_blocking_output_schema(&result);

        let json = match &result.items[0] {
            ToolExecutionOutputItem::Json(v) => v,
            _ => unreachable!(),
        };
        assert!(json["stdout"].as_str().unwrap().contains("🦀"));
    }

    #[tokio::test]
    async fn blocking_fallback_with_working_dir() {
        let test_base = TestBase::new().await.with_directory("mydir").await;
        let dir_path = test_base
            .join("mydir")
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();

        let cmd = ExecuteCmd {
            command: "pwd".to_string(),
            working_dir: Some(dir_path.clone()),
        };

        let result = cmd.execute(&test_base, None).await.unwrap();
        assert_blocking_output_schema(&result);

        let json = match &result.items[0] {
            ToolExecutionOutputItem::Json(v) => v,
            _ => unreachable!(),
        };
        assert!(json["stdout"].as_str().unwrap().trim().ends_with(&dir_path));
    }

    #[tokio::test]
    /// Verify that execute doesn't hang when the command spawns a background
    /// daemon that inherits the piped stdout/stderr FDs. wait_with_output()
    /// blocks until all pipe FDs are closed — child exit does not imply EOF.
    async fn test_execute_does_not_hang_when_daemon_holds_pipe_fds() {
        use crate::util::test::TestBase;

        let test_base = TestBase::new().await;
        let cmd = ExecuteCmd {
            command: r#"echo "before"; perl -e 'exit 0 if fork; sleep 300'; echo "after""#.to_string(),
            working_dir: None,
        };

        let result = tokio::time::timeout(std::time::Duration::from_secs(5), cmd.execute(&test_base, None))
            .await
            .expect("execute hung — wait_with_output blocked on daemon-held pipe FDs")
            .unwrap();

        let json = match &result.items[0] {
            ToolExecutionOutputItem::Json(v) => v,
            _ => panic!("Expected JSON output"),
        };

        assert_eq!(json["exit_status"], "exit status: 0");
        assert!(json["stdout"].as_str().unwrap().contains("before"));
        assert!(json["stdout"].as_str().unwrap().contains("after"));
    }

    #[tokio::test]
    /// Same as above but exercises the streaming path (event_tx = Some),
    /// which is the only production codepath.
    async fn test_streaming_does_not_hang_when_daemon_holds_pipe_fds() {
        let test_base = TestBase::new().await;
        let (tx, _rx) = broadcast::channel::<AgentEvent>(64);

        let cmd = ExecuteCmd {
            command: r#"echo "before"; perl -e 'exit 0 if fork; sleep 300'; echo "after""#.to_string(),
            working_dir: None,
        };

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            cmd.execute(&test_base, Some(("test-id".to_string(), tx))),
        )
        .await
        .expect("streaming execute hung — daemon holding pipe FDs blocked next_line()")
        .unwrap();

        let json = extract_json(&result);
        assert_eq!(json["exit_status"], "exit status: 0");
        assert!(json["stdout"].as_str().unwrap().contains("before"));
        assert!(json["stdout"].as_str().unwrap().contains("after"));
    }

    #[tokio::test]
    async fn test_execute_with_working_dir() {
        let test_base = TestBase::new().await.with_directory("subdir").await;
        let canonical_str = test_base
            .join("subdir")
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();

        let cmd = ExecuteCmd {
            command: "pwd".to_string(),
            working_dir: Some(canonical_str.clone()),
        };

        let result = cmd.execute(&test_base, None).await.unwrap();
        let json = match &result.items[0] {
            ToolExecutionOutputItem::Json(v) => v,
            _ => panic!("Expected JSON output"),
        };

        assert_eq!(json["exit_status"], "exit status: 0");
        assert!(json["stdout"].as_str().unwrap().trim().ends_with(&canonical_str));
    }

    // ── Stream error resilience preserves the surviving stream ──
    // Feature: shell-output-streaming
    // Stream error resilience preserves the surviving stream
    //
    // For any process where one of stdout or stderr encounters a read error
    // or closes early, the other stream SHALL continue to produce
    // `ToolCallUpdate` events until EOF, and the final `ToolExecutionOutput`
    // SHALL include the accumulated content from the surviving stream.
    // ────────────────────────────────────────────────────────────────────────

    /// Helper: extract the JSON output from a `ToolExecutionResult`.
    fn extract_json(result: &ToolExecutionOutput) -> &serde_json::Value {
        assert_eq!(result.items.len(), 1, "expected exactly one output item");
        match &result.items[0] {
            ToolExecutionOutputItem::Json(v) => v,
            other => panic!("expected Json output item, got: {other:?}"),
        }
    }

    /// Helper: collect all `ToolCallUpdate` text events from a broadcast receiver.
    fn drain_updates(mut rx: broadcast::Receiver<AgentEvent>) -> Vec<String> {
        let mut texts = Vec::new();
        while let Ok(event) = rx.try_recv() {
            if let AgentEvent::Update(UpdateEvent::ToolCallUpdate {
                content: ContentChunk::Text(text),
                ..
            }) = event
            {
                texts.push(text);
            }
        }
        texts
    }

    #[tokio::test]
    async fn stream_resilience_stderr_closed_stdout_survives() {
        // Close stderr immediately, then write multiple lines to stdout.
        // The streaming loop should handle the closed stderr gracefully
        // and fully accumulate stdout.
        let test_base = TestBase::new().await;
        let (tx, rx) = broadcast::channel::<AgentEvent>(64);

        let cmd = ExecuteCmd {
            command: "exec 2>&-; echo line_a; echo line_b; echo line_c".to_string(),
            working_dir: None,
        };

        let result = cmd
            .execute(&test_base, Some(("test-id".to_string(), tx)))
            .await
            .unwrap();

        let json = extract_json(&result);
        let stdout = json["stdout"].as_str().unwrap();
        assert!(stdout.contains("line_a"), "stdout missing line_a: {stdout}");
        assert!(stdout.contains("line_b"), "stdout missing line_b: {stdout}");
        assert!(stdout.contains("line_c"), "stdout missing line_c: {stdout}");

        // Verify ToolCallUpdate events were emitted for stdout lines.
        let updates = drain_updates(rx);
        let combined: String = updates.concat();
        assert!(combined.contains("line_a"), "updates missing line_a");
        assert!(combined.contains("line_b"), "updates missing line_b");
        assert!(combined.contains("line_c"), "updates missing line_c");
    }

    #[tokio::test]
    async fn stream_resilience_stdout_closed_stderr_survives() {
        // Close stdout immediately, then write multiple lines to stderr.
        // The streaming loop should handle the closed stdout gracefully
        // and fully accumulate stderr.
        let test_base = TestBase::new().await;
        let (tx, rx) = broadcast::channel::<AgentEvent>(64);

        let cmd = ExecuteCmd {
            command: "exec 1>&-; echo err_x >&2; echo err_y >&2; echo err_z >&2".to_string(),
            working_dir: None,
        };

        let result = cmd
            .execute(&test_base, Some(("test-id".to_string(), tx)))
            .await
            .unwrap();

        let json = extract_json(&result);
        let stderr = json["stderr"].as_str().unwrap();
        assert!(stderr.contains("err_x"), "stderr missing err_x: {stderr}");
        assert!(stderr.contains("err_y"), "stderr missing err_y: {stderr}");
        assert!(stderr.contains("err_z"), "stderr missing err_z: {stderr}");

        // Verify ToolCallUpdate events were emitted for stderr lines.
        let updates = drain_updates(rx);
        let combined: String = updates.concat();
        assert!(combined.contains("err_x"), "updates missing err_x");
        assert!(combined.contains("err_y"), "updates missing err_y");
        assert!(combined.contains("err_z"), "updates missing err_z");
    }

    #[tokio::test]
    async fn stream_resilience_stdout_closes_mid_stream_stderr_continues() {
        // Stdout produces one line then closes; stderr continues with more lines.
        // Both streams' content should appear in the final output.
        let test_base = TestBase::new().await;
        let (tx, _rx) = broadcast::channel::<AgentEvent>(64);

        let cmd = ExecuteCmd {
            // Use a subshell: stdout writes one line then closes its fd,
            // while stderr continues writing.
            command: "echo early_out; exec 1>&-; echo late_err_1 >&2; echo late_err_2 >&2".to_string(),
            working_dir: None,
        };

        let result = cmd
            .execute(&test_base, Some(("test-id".to_string(), tx)))
            .await
            .unwrap();

        let json = extract_json(&result);
        let stdout = json["stdout"].as_str().unwrap();
        let stderr = json["stderr"].as_str().unwrap();

        assert!(stdout.contains("early_out"), "stdout missing early_out: {stdout}");
        assert!(stderr.contains("late_err_1"), "stderr missing late_err_1: {stderr}");
        assert!(stderr.contains("late_err_2"), "stderr missing late_err_2: {stderr}");
    }

    #[tokio::test]
    async fn stream_resilience_stderr_closes_mid_stream_stdout_continues() {
        // Stderr produces one line then closes; stdout continues with more lines.
        let test_base = TestBase::new().await;
        let (tx, _rx) = broadcast::channel::<AgentEvent>(64);

        let cmd = ExecuteCmd {
            command: "echo early_err >&2; exec 2>&-; echo late_out_1; echo late_out_2".to_string(),
            working_dir: None,
        };

        let result = cmd
            .execute(&test_base, Some(("test-id".to_string(), tx)))
            .await
            .unwrap();

        let json = extract_json(&result);
        let stdout = json["stdout"].as_str().unwrap();
        let stderr = json["stderr"].as_str().unwrap();

        assert!(stderr.contains("early_err"), "stderr missing early_err: {stderr}");
        assert!(stdout.contains("late_out_1"), "stdout missing late_out_1: {stdout}");
        assert!(stdout.contains("late_out_2"), "stdout missing late_out_2: {stdout}");
    }

    #[tokio::test]
    async fn stream_resilience_output_schema_preserved_on_stream_close() {
        // Even when one stream is closed, the final output must have the
        // correct JSON schema with exit_status, stdout, stderr fields.
        let test_base = TestBase::new().await;
        let (tx, _rx) = broadcast::channel::<AgentEvent>(64);

        // Close stderr, write to stdout.
        let cmd = ExecuteCmd {
            command: "exec 2>&-; echo ok".to_string(),
            working_dir: None,
        };

        let result = cmd
            .execute(&test_base, Some(("test-id".to_string(), tx)))
            .await
            .unwrap();

        assert_blocking_output_schema(&result);
    }

    #[tokio::test]
    async fn stream_resilience_both_streams_closed_immediately() {
        // Both streams closed immediately — the loop should terminate
        // gracefully and produce a valid output with empty stdout/stderr.
        let test_base = TestBase::new().await;
        let (tx, _rx) = broadcast::channel::<AgentEvent>(64);

        let cmd = ExecuteCmd {
            command: "exec 1>&- 2>&-".to_string(),
            working_dir: None,
        };

        let result = cmd
            .execute(&test_base, Some(("test-id".to_string(), tx)))
            .await
            .unwrap();

        assert_blocking_output_schema(&result);
        let json = extract_json(&result);
        // Both streams should be empty (or contain only whitespace).
        assert!(
            json["stdout"].as_str().unwrap().trim().is_empty(),
            "stdout should be empty when fd closed immediately"
        );
        assert!(
            json["stderr"].as_str().unwrap().trim().is_empty(),
            "stderr should be empty when fd closed immediately"
        );
    }

    #[tokio::test]
    async fn stream_resilience_no_event_channel_fallback_with_closed_stream() {
        // Even in blocking fallback mode (no event_tx), a closed stream
        // should not prevent the other stream's content from appearing.
        let test_base = TestBase::new().await;

        let cmd = ExecuteCmd {
            command: "exec 2>&-; echo fallback_ok".to_string(),
            working_dir: None,
        };

        let result = cmd.execute(&test_base, None).await.unwrap();
        assert_blocking_output_schema(&result);

        let json = extract_json(&result);
        assert!(
            json["stdout"].as_str().unwrap().contains("fallback_ok"),
            "blocking fallback should preserve stdout when stderr is closed"
        );
    }

    /// Red-to-green: 50 000 lines of output must NOT cause the broadcast
    /// receiver to lag.  Before the batching fix the per-line send pattern
    /// would overflow the 1 024-slot broadcast channel, returning
    /// `RecvError::Lagged` and dropping events.  With batching the lines
    /// are coalesced on a timer so the channel stays well within capacity.
    #[tokio::test]
    async fn stream_50k_lines_no_broadcast_lag() {
        let test_base = TestBase::new().await;
        // Use a realistic channel size matching production (agent/mod.rs).
        let (tx, mut rx) = broadcast::channel::<AgentEvent>(1024);

        let cmd = ExecuteCmd {
            command: "seq 1 50000".to_string(),
            working_dir: None,
        };

        // Spawn a consumer that drains events and detects Lagged errors.
        let consumer = tokio::spawn(async move {
            let mut event_count: u64 = 0;
            let mut total_bytes: usize = 0;
            loop {
                match rx.recv().await {
                    Ok(AgentEvent::Update(UpdateEvent::ToolCallUpdate {
                        content: ContentChunk::Text(text),
                        ..
                    })) => {
                        event_count += 1;
                        total_bytes += text.len();
                    },
                    Ok(_) => {},
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        panic!("Broadcast receiver lagged by {n} events — batching is broken");
                    },
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            (event_count, total_bytes)
        });

        let result = cmd
            .execute(&test_base, Some(("test-id".to_string(), tx)))
            .await
            .unwrap();

        // Drop the sender so the consumer sees Closed.
        drop(result);

        let (event_count, total_bytes) = consumer.await.unwrap();

        // With batching, 50k lines should be coalesced into far fewer events
        // than the 1024-slot channel capacity.  Without batching this would
        // be 50 000 events and the receiver would lag.
        assert!(
            event_count < 1024,
            "Expected fewer than 1024 batched events, got {event_count} — batching may not be working"
        );

        // All 50 000 lines must still arrive (each line is "N\n").
        // "seq 1 50000" produces 288 894 bytes (sum of digit lengths + newlines).
        assert!(
            total_bytes > 200_000,
            "Expected >200KB of output, got {total_bytes} — lines may have been dropped"
        );
    }
}
