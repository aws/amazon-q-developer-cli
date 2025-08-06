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
use crate::cli::chat::consts::AGENT_FORMAT_HOOKS_DOC_URL;
use crate::cli::chat::util::truncate_safe;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};

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
        prompt: Option<&str>,
    ) -> Result<Vec<((HookTrigger, Hook), String)>, ChatError> {
        let mut cached = vec![];
        let mut futures = FuturesUnordered::new();
        for hook in hooks
            .into_iter()
            .flat_map(|(trigger, hooks)| hooks.into_iter().map(move |hook| (trigger, hook)))
        {
            if let Some(cache) = self.get_cache(&hook) {
                cached.push((hook.clone(), cache.clone()));
                continue;
            }
            futures.push(self.run_hook(hook, prompt));
        }

        let mut complete = 0;
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
            if let Ok(output) = result {
                complete += 1;
                results.push((hook, output));
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
        for ((trigger, hook), output) in &results {
            self.cache.insert((*trigger, hook.clone()), CachedHook {
                output: output.clone(),
                expiry: match trigger {
                    HookTrigger::AgentSpawn => None,
                    HookTrigger::UserPromptSubmit => Some(Instant::now() + Duration::from_secs(hook.cache_ttl_seconds)),
                },
            });
        }

        results.append(&mut cached);

        Ok(results)
    }

    async fn run_hook(
        &self,
        hook: (HookTrigger, Hook),
        prompt: Option<&str>,
    ) -> ((HookTrigger, Hook), Result<String>, Duration) {
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

        // Set USER_PROMPT environment variable if provided
        if let Some(prompt) = prompt {
            // Sanitize the prompt to avoid issues with special characters
            let sanitized_prompt = sanitize_user_prompt(prompt);
            cmd.env("USER_PROMPT", sanitized_prompt);
        }

        let command_future = cmd.output();

        // Run with timeout
        let result = match tokio::time::timeout(timeout, command_future).await {
            Ok(Ok(result)) => {
                if result.status.success() {
                    let stdout = result.stdout.to_str_lossy();
                    let stdout = format!(
                        "{}{}",
                        truncate_safe(&stdout, hook.1.max_output_size),
                        if stdout.len() > hook.1.max_output_size {
                            " ... truncated"
                        } else {
                            ""
                        }
                    );
                    Ok(stdout)
                } else {
                    Err(eyre!("command returned non-zero exit code: {}", result.status))
                }
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
    // Limit the size of input to first 4096 characters
    let truncated = if input.len() > 4096 { &input[0..4096] } else { input };

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
