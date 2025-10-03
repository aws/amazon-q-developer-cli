use std::collections::HashMap;
use std::io::Write;
use std::process::Stdio;
use std::time::{
    Duration,
    Instant,
};

use bstr::ByteSlice;
use clap::Args;
use crossterm::style::{
    self,
    Attribute,
    Color,
    Stylize,
};
use crossterm::{
    cursor,
    execute,
    queue,
    terminal,
};
use eyre::{
    Result,
    eyre,
};
use futures::stream::{
    FuturesUnordered,
    StreamExt,
};
use spinners::{
    Spinner,
    Spinners,
};

use crate::cli::agent::hook::{
    Hook,
    HookTrigger,
};
use crate::cli::agent::is_mcp_tool_ref;
use crate::cli::chat::consts::AGENT_FORMAT_HOOKS_DOC_URL;
use crate::cli::chat::util::truncate_safe;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::util::MCP_SERVER_TOOL_DELIMITER;
use crate::util::pattern_matching::matches_any_pattern;

/// Hook execution result: (exit_code, output)
/// Output is stdout if exit_code is 0, stderr otherwise.
pub type HookOutput = (i32, String);

/// Check if a hook matches a tool name based on its matcher pattern
fn hook_matches_tool(hook: &Hook, tool_name: &str) -> bool {
    match &hook.matcher {
        None => true, // No matcher means the hook runs for all tools
        Some(pattern) => {
            match pattern.as_str() {
                "*" => true,                               // Wildcard matches all tools
                "@builtin" => !is_mcp_tool_ref(tool_name), // Built-in tools are not MCP tools
                _ => {
                    // If tool_name is MCP, check server pattern first
                    if is_mcp_tool_ref(tool_name) {
                        if let Some(server_name) = tool_name
                            .strip_prefix('@')
                            .and_then(|s| s.split(MCP_SERVER_TOOL_DELIMITER).next())
                        {
                            let server_pattern = format!("@{}", server_name);
                            if pattern == &server_pattern {
                                return true;
                            }
                        }
                    }

                    // Use matches_any_pattern for both MCP and built-in tools
                    let mut patterns = std::collections::HashSet::new();
                    patterns.insert(pattern.clone());
                    matches_any_pattern(&patterns, tool_name)
                },
            }
        },
    }
}

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub tool_response: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct CachedHook {
    output: String,
    expiry: Option<Instant>,
}

/// Maps a hook name to a [`CachedHook`]
#[derive(Debug, Clone, Default)]
pub struct HookExecutor {
    pub cache: HashMap<(HookTrigger, Hook), CachedHook>,
}

impl HookExecutor {
    pub fn new() -> Self {
        Self { cache: HashMap::new() }
    }

    /// Run and cache [`Hook`]s. Any hooks that are already cached will be returned without
    /// executing. Hooks that fail to execute will not be returned. Returned hook order is
    /// undefined.
    ///
    /// If `updates` is `Some`, progress on hook execution will be written to it.
    /// Errors encountered with write operations to `updates` are ignored.
    ///
    /// Note: [`HookTrigger::AgentSpawn`] hooks never leave the cache.
    pub async fn run_hooks(
        &mut self,
        hooks: HashMap<HookTrigger, Vec<Hook>>,
        output: &mut impl Write,
        cwd: &str,
        prompt: Option<&str>,
        tool_context: Option<ToolContext>,
    ) -> Result<Vec<((HookTrigger, Hook), HookOutput)>, ChatError> {
        let mut cached = vec![];
        let mut futures = FuturesUnordered::new();
        for hook in hooks
            .into_iter()
            .flat_map(|(trigger, hooks)| hooks.into_iter().map(move |hook| (trigger, hook)))
        {
            // Filter hooks by tool matcher
            if let Some(tool_ctx) = &tool_context {
                if !hook_matches_tool(&hook.1, &tool_ctx.tool_name) {
                    continue; // Skip this hook - doesn't match tool
                }
            }

            if let Some(cache) = self.get_cache(&hook) {
                // Note: we only cache successful hook run. hence always using 0 as exit code for cached hook
                cached.push((hook.clone(), (0, cache)));
                continue;
            }
            futures.push(self.run_hook(hook, cwd, prompt, tool_context.clone()));
        }

        let mut complete = 0; // number of hooks that are run successfully with exit code 0
        let total = futures.len();
        let mut spinner = None;
        let spinner_text = |complete: usize, total: usize| {
            format!(
                "{} of {} hooks finished",
                complete.to_string().blue(),
                total.to_string().blue(),
            )
        };

        if total != 0 {
            spinner = Some(Spinner::new(Spinners::Dots12, spinner_text(complete, total)));
        }

        // Process results as they complete
        let mut results = vec![];
        let start_time = Instant::now();
        while let Some((hook, result, duration)) = futures.next().await {
            // If output is enabled, handle that first
            if let Some(spinner) = spinner.as_mut() {
                spinner.stop();

                // Erase the spinner
                execute!(
                    output,
                    cursor::MoveToColumn(0),
                    terminal::Clear(terminal::ClearType::CurrentLine),
                    cursor::Hide,
                )?;
            }

            if let Err(err) = &result {
                queue!(
                    output,
                    style::SetForegroundColor(style::Color::Red),
                    style::Print("✗ "),
                    style::SetForegroundColor(style::Color::Blue),
                    style::Print(&hook.1.command),
                    style::ResetColor,
                    style::Print(" failed after "),
                    style::SetForegroundColor(style::Color::Yellow),
                    style::Print(format!("{:.2} s", duration.as_secs_f32())),
                    style::ResetColor,
                    style::Print(format!(": {}\n", err)),
                )?;
            }

            // Process results regardless of output enabled
            if let Ok((exit_code, hook_output)) = &result {
                // Print warning if exit code is not 0
                if *exit_code != 0 {
                    queue!(
                        output,
                        style::SetForegroundColor(style::Color::Red),
                        style::Print("✗ "),
                        style::ResetColor,
                        style::Print(format!("{} \"", hook.0)),
                        style::Print(&hook.1.command),
                        style::Print("\""),
                        style::SetForegroundColor(style::Color::Red),
                        style::Print(format!(
                            " failed with exit code: {}, stderr: {})\n",
                            exit_code,
                            hook_output.trim_end()
                        )),
                        style::ResetColor,
                    )?;
                } else {
                    complete += 1;
                }
                results.push((hook, result.unwrap()));
            }

            // Display ending summary or add a new spinner
            // The futures set size decreases each time we process one
            if futures.is_empty() {
                let symbol = if total == complete {
                    "✓".to_string().green()
                } else {
                    "✗".to_string().red()
                };

                queue!(
                    output,
                    style::SetForegroundColor(Color::Blue),
                    style::Print(format!("{symbol} {} in ", spinner_text(complete, total))),
                    style::SetForegroundColor(style::Color::Yellow),
                    style::Print(format!("{:.2} s\n", start_time.elapsed().as_secs_f32())),
                    style::ResetColor,
                )?;
            } else {
                spinner = Some(Spinner::new(Spinners::Dots, spinner_text(complete, total)));
            }
        }
        drop(futures);

        // Fill cache with executed results, skipping what was already from cache
        for ((trigger, hook), (exit_code, output)) in &results {
            if *exit_code != 0 {
                continue; // Only cache successful hooks
            }
            self.cache.insert((*trigger, hook.clone()), CachedHook {
                output: output.clone(),
                expiry: match trigger {
                    HookTrigger::AgentSpawn => None,
                    HookTrigger::UserPromptSubmit => Some(Instant::now() + Duration::from_secs(hook.cache_ttl_seconds)),
                    HookTrigger::PreToolUse => Some(Instant::now() + Duration::from_secs(hook.cache_ttl_seconds)),
                    HookTrigger::PostToolUse => Some(Instant::now() + Duration::from_secs(hook.cache_ttl_seconds)),
                    HookTrigger::Stop => Some(Instant::now() + Duration::from_secs(hook.cache_ttl_seconds)),
                },
            });
        }

        results.append(&mut cached);

        Ok(results)
    }

    async fn run_hook(
        &self,
        hook: (HookTrigger, Hook),
        cwd: &str,
        prompt: Option<&str>,
        tool_context: Option<ToolContext>,
    ) -> ((HookTrigger, Hook), Result<HookOutput>, Duration) {
        let start_time = Instant::now();

        let command = &hook.1.command;

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

        let timeout = Duration::from_millis(hook.1.timeout_ms);

        // Generate hook command input in JSON format
        let mut hook_input = serde_json::json!({
            "hook_event_name": hook.0.to_string(),
            "cwd": cwd
        });

        // Set USER_PROMPT environment variable and add to JSON input if provided
        if let Some(prompt) = prompt {
            // Sanitize the prompt to avoid issues with special characters
            let sanitized_prompt = sanitize_user_prompt(prompt);
            cmd.env("USER_PROMPT", sanitized_prompt);
            hook_input["prompt"] = serde_json::Value::String(prompt.to_string());
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
                    truncate_safe(&raw_output, hook.1.max_output_size),
                    if raw_output.len() > hook.1.max_output_size {
                        " ... truncated"
                    } else {
                        ""
                    }
                );
                Ok((exit_code, formatted_output))
            },
            Ok(Err(err)) => Err(eyre!("failed to execute command: {}", err)),
            Err(_) => Err(eyre!("command timed out after {} ms", timeout.as_millis())),
        };

        (hook, result, start_time.elapsed())
    }

    /// Will return a cached hook's output if it exists and isn't expired.
    fn get_cache(&self, hook: &(HookTrigger, Hook)) -> Option<String> {
        self.cache.get(hook).and_then(|o| {
            if let Some(expiry) = o.expiry {
                if Instant::now() < expiry {
                    Some(o.output.clone())
                } else {
                    None
                }
            } else {
                Some(o.output.clone())
            }
        })
    }
}

/// Sanitizes a string value to be used as an environment variable
fn sanitize_user_prompt(input: &str) -> String {
    let truncated = truncate_safe(input, 4096);

    // Remove any potentially problematic characters
    truncated.replace(|c: char| c.is_control() && c != '\n' && c != '\r' && c != '\t', "")
}

#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
#[command(
    before_long_help = "Use context hooks to specify shell commands to run. The output from these 
commands will be appended to the prompt to Amazon Q.

Refer to the documentation for how to configure hooks with your agent: https://github.com/aws/amazon-q-developer-cli/blob/main/docs/agent-format.md#hooks-field

Notes:
• Hooks are executed in parallel
• 'conversation_start' hooks run on the first user prompt and are attached once to the conversation history sent to Amazon Q
• 'per_prompt' hooks run on each user prompt and are attached to the prompt, but are not stored in conversation history"
)]
/// Arguments for the hooks command that displays configured context hooks
pub struct HooksArgs;

impl HooksArgs {
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let Some(context_manager) = &mut session.conversation.context_manager else {
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        };

        let mut out = Vec::new();
        for (trigger, hooks) in &context_manager.hooks {
            writeln!(&mut out, "{trigger}:")?;
            match hooks.is_empty() {
                true => writeln!(&mut out, "<none>")?,
                false => {
                    for hook in hooks {
                        writeln!(&mut out, "  - {}", hook.command)?;
                    }
                },
            }
        }

        if out.is_empty() {
            queue!(
                session.stderr,
                style::Print(
                    "No hooks are configured.\n\nRefer to the documentation for how to add hooks to your agent: "
                ),
                style::SetForegroundColor(Color::Green),
                style::Print(AGENT_FORMAT_HOOKS_DOC_URL),
                style::SetAttribute(Attribute::Reset),
                style::Print("\n"),
            )?;
        } else {
            session.stdout.write_all(&out)?;
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use tempfile::TempDir;

    use super::*;
    use crate::cli::agent::hook::{
        Hook,
        HookTrigger,
    };

    #[test]
    fn test_hook_matches_tool() {
        let hook_no_matcher = Hook {
            command: "echo test".to_string(),
            timeout_ms: 5000,
            cache_ttl_seconds: 0,
            max_output_size: 1000,
            matcher: None,
            source: crate::cli::agent::hook::Source::Session,
        };

        let fs_write_hook = Hook {
            command: "echo test".to_string(),
            timeout_ms: 5000,
            cache_ttl_seconds: 0,
            max_output_size: 1000,
            matcher: Some("fs_write".to_string()),
            source: crate::cli::agent::hook::Source::Session,
        };

        let fs_wildcard_hook = Hook {
            command: "echo test".to_string(),
            timeout_ms: 5000,
            cache_ttl_seconds: 0,
            max_output_size: 1000,
            matcher: Some("fs_*".to_string()),
            source: crate::cli::agent::hook::Source::Session,
        };

        let all_tools_hook = Hook {
            command: "echo test".to_string(),
            timeout_ms: 5000,
            cache_ttl_seconds: 0,
            max_output_size: 1000,
            matcher: Some("*".to_string()),
            source: crate::cli::agent::hook::Source::Session,
        };

        let builtin_hook = Hook {
            command: "echo test".to_string(),
            timeout_ms: 5000,
            cache_ttl_seconds: 0,
            max_output_size: 1000,
            matcher: Some("@builtin".to_string()),
            source: crate::cli::agent::hook::Source::Session,
        };

        let git_server_hook = Hook {
            command: "echo test".to_string(),
            timeout_ms: 5000,
            cache_ttl_seconds: 0,
            max_output_size: 1000,
            matcher: Some("@git".to_string()),
            source: crate::cli::agent::hook::Source::Session,
        };

        let git_status_hook = Hook {
            command: "echo test".to_string(),
            timeout_ms: 5000,
            cache_ttl_seconds: 0,
            max_output_size: 1000,
            matcher: Some("@git/status".to_string()),
            source: crate::cli::agent::hook::Source::Session,
        };

        // No matcher should match all tools
        assert!(hook_matches_tool(&hook_no_matcher, "fs_write"));
        assert!(hook_matches_tool(&hook_no_matcher, "execute_bash"));
        assert!(hook_matches_tool(&hook_no_matcher, "@git/status"));

        // Exact matcher should only match exact tool
        assert!(hook_matches_tool(&fs_write_hook, "fs_write"));
        assert!(!hook_matches_tool(&fs_write_hook, "fs_read"));

        // Wildcard matcher should match pattern
        assert!(hook_matches_tool(&fs_wildcard_hook, "fs_write"));
        assert!(hook_matches_tool(&fs_wildcard_hook, "fs_read"));
        assert!(!hook_matches_tool(&fs_wildcard_hook, "execute_bash"));

        // * should match all tools
        assert!(hook_matches_tool(&all_tools_hook, "fs_write"));
        assert!(hook_matches_tool(&all_tools_hook, "execute_bash"));
        assert!(hook_matches_tool(&all_tools_hook, "@git/status"));

        // @builtin should match built-in tools only
        assert!(hook_matches_tool(&builtin_hook, "fs_write"));
        assert!(hook_matches_tool(&builtin_hook, "execute_bash"));
        assert!(!hook_matches_tool(&builtin_hook, "@git/status"));

        // @git should match all git server tools
        assert!(hook_matches_tool(&git_server_hook, "@git/status"));
        assert!(!hook_matches_tool(&git_server_hook, "@other/tool"));
        assert!(!hook_matches_tool(&git_server_hook, "fs_write"));

        // @git/status should match exact MCP tool
        assert!(hook_matches_tool(&git_status_hook, "@git/status"));
        assert!(!hook_matches_tool(&git_status_hook, "@git/commit"));
        assert!(!hook_matches_tool(&git_status_hook, "fs_write"));
    }

    #[tokio::test]
    async fn test_hook_executor_with_tool_context() {
        let mut executor = HookExecutor::new();
        let mut output = Vec::new();

        // Create temp directory and file
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("hook_output.json");
        let test_file_str = test_file.to_string_lossy();

        // Create a simple hook that writes JSON input to a file
        #[cfg(unix)]
        let command = format!("cat > {}", test_file_str);
        #[cfg(windows)]
        let command = format!(
            "powershell -Command \"$input | Out-File -FilePath '{}'\"",
            test_file_str
        );

        let hook = Hook {
            command,
            timeout_ms: 5000,
            cache_ttl_seconds: 0,
            max_output_size: 1000,
            matcher: Some("fs_write".to_string()),
            source: crate::cli::agent::hook::Source::Session,
        };

        let mut hooks = HashMap::new();
        hooks.insert(HookTrigger::PreToolUse, vec![hook]);

        let tool_context = ToolContext {
            tool_name: "fs_write".to_string(),
            tool_input: serde_json::json!({
                "command": "create",
                "path": "/test/file.py"
            }),
            tool_response: None,
        };

        // Run the hook
        let result = executor
            .run_hooks(hooks, &mut output, ".", None, Some(tool_context))
            .await;

        assert!(result.is_ok());

        // Verify the hook wrote the JSON input to the file
        if let Ok(content) = std::fs::read_to_string(&test_file) {
            let json: serde_json::Value = serde_json::from_str(&content).unwrap();
            assert_eq!(json["hook_event_name"], "preToolUse");
            assert_eq!(json["tool_name"], "fs_write");
            assert_eq!(json["tool_input"]["command"], "create");
            assert_eq!(json["cwd"], ".");
        }
        // TempDir automatically cleans up when dropped
    }

    #[tokio::test]
    async fn test_hook_filtering_no_match() {
        let mut executor = HookExecutor::new();
        let mut output = Vec::new();

        // Hook that matches execute_bash (should NOT run for fs_write tool call)
        let execute_bash_hook = Hook {
            command: "echo 'should not run'".to_string(),
            timeout_ms: 5000,
            cache_ttl_seconds: 0,
            max_output_size: 1000,
            matcher: Some("execute_bash".to_string()),
            source: crate::cli::agent::hook::Source::Session,
        };

        let mut hooks = HashMap::new();
        hooks.insert(HookTrigger::PostToolUse, vec![execute_bash_hook]);

        let tool_context = ToolContext {
            tool_name: "fs_write".to_string(),
            tool_input: serde_json::json!({"command": "create"}),
            tool_response: Some(serde_json::json!({"success": true})),
        };

        // Run the hooks
        let result = executor
            .run_hooks(
                hooks,
                &mut output,
                ".",  // cwd - using current directory for now
                None, // prompt - no user prompt for this test
                Some(tool_context),
            )
            .await;

        assert!(result.is_ok());
        let hook_results = result.unwrap();

        // Should run 0 hooks because matcher doesn't match tool_name
        assert_eq!(hook_results.len(), 0);

        // Output should be empty since no hooks ran
        assert!(output.is_empty());
    }

    #[tokio::test]
    async fn test_hook_exit_code_2() {
        let mut executor = HookExecutor::new();
        let mut output = Vec::new();

        // Create a hook that exits with code 2 and outputs to stderr
        #[cfg(unix)]
        let command = "echo 'Tool execution blocked by security policy' >&2; exit 2";
        #[cfg(windows)]
        let command = "echo Tool execution blocked by security policy 1>&2 & exit /b 2";

        let hook = Hook {
            command: command.to_string(),
            timeout_ms: 5000,
            cache_ttl_seconds: 0,
            max_output_size: 1000,
            matcher: Some("fs_write".to_string()),
            source: crate::cli::agent::hook::Source::Session,
        };

        let hooks = HashMap::from([(HookTrigger::PreToolUse, vec![hook])]);

        let tool_context = ToolContext {
            tool_name: "fs_write".to_string(),
            tool_input: serde_json::json!({
                "command": "create",
                "path": "/sensitive/file.py"
            }),
            tool_response: None,
        };

        let results = executor
            .run_hooks(
                hooks,
                &mut output,
                ".",  // cwd
                None, // prompt
                Some(tool_context),
            )
            .await
            .unwrap();

        // Should have one result
        assert_eq!(results.len(), 1);

        let ((trigger, _hook), (exit_code, hook_output)) = &results[0];
        assert_eq!(*trigger, HookTrigger::PreToolUse);
        assert_eq!(*exit_code, 2);
        assert!(hook_output.contains("Tool execution blocked by security policy"));
    }

    #[tokio::test]
    async fn test_stop_hook() {
        let mut executor = HookExecutor::new();
        let mut output = Vec::new();

        // Create a simple Stop hook that outputs a message
        #[cfg(unix)]
        let command = "echo 'Turn completed successfully'";
        #[cfg(windows)]
        let command = "echo Turn completed successfully";

        let hook = Hook {
            command: command.to_string(),
            timeout_ms: 5000,
            cache_ttl_seconds: 0,
            max_output_size: 1000,
            matcher: None, // Stop hooks don't use matchers
            source: crate::cli::agent::hook::Source::Session,
        };

        let hooks = HashMap::from([(HookTrigger::Stop, vec![hook])]);

        let results = executor
            .run_hooks(
                hooks,
                &mut output,
                ".",  // cwd
                None, // prompt
                None, // tool_context - Stop doesn't have tool context
            )
            .await
            .unwrap();

        // Should have one result
        assert_eq!(results.len(), 1);

        let ((trigger, _hook), (exit_code, hook_output)) = &results[0];
        assert_eq!(*trigger, HookTrigger::Stop);
        assert_eq!(*exit_code, 0);
        assert!(hook_output.contains("Turn completed successfully"));
    }

    #[test]
    fn test_sanitize_user_prompt_cjk_characters() {
        // Test with CJK characters that would cause panic with naive byte slicing
        let korean_text = "한".repeat(2000); // Each Korean character is 3 bytes in UTF-8
        let result = sanitize_user_prompt(&korean_text);

        // Should not panic and should be truncated safely
        assert!(result.len() <= 4096);
        assert!(!result.is_empty());

        // Test with mixed ASCII and CJK at boundary
        let mixed_text = "a".repeat(4094) + "한국어"; // 4094 + 9 bytes = 4103 bytes
        let result = sanitize_user_prompt(&mixed_text);
        assert!(result.len() <= 4096);
        assert!(result.ends_with("a")); // Should end with ASCII, not partial CJK

        // Test with text shorter than limit
        let short_text = "안녕하세요";
        let result = sanitize_user_prompt(short_text);
        assert_eq!(result, short_text);

        // Test with control characters
        let text_with_controls = "Hello\x00World\nTest\r\tEnd";
        let result = sanitize_user_prompt(text_with_controls);
        assert_eq!(result, "HelloWorld\nTest\r\tEnd");
    }
}
