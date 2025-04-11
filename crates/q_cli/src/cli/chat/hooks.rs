use std::collections::HashMap;
use std::io::Write;
use std::time::{
    Duration,
    Instant,
};

use crossterm::style::Color;
use crossterm::{
    execute,
    queue,
    style,
};
use eyre::{
    Result,
    eyre,
};
use futures::future;
use futures::future::Either;
use serde::{
    Deserialize,
    Serialize,
};

use super::tools::execute_bash::run_command;

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_MAX_OUTPUT_SIZE: usize = 1024 * 10;
const DEFAULT_CACHE_TTL_SECONDS: u64 = 0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hook {
    /// Unique name of the hook
    pub name: String,

    pub r#type: HookType,

    #[serde(default = "Hook::default_disabled")]
    pub disabled: bool,

    /// Max time the hook can run before it throws a timeout error
    #[serde(default = "Hook::default_timeout_ms")]
    pub timeout_ms: u64,

    /// Max output size of the hook before it is truncated
    #[serde(default = "Hook::default_max_output_size")]
    pub max_output_size: usize,

    /// How long the hook output is cached before it will be executed again
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_ttl_seconds: Option<u64>,

    // Type-specific fields
    /// The bash command to execute
    pub command: Option<String>, // For inline hooks

    // Internal data
    #[serde(skip)]
    pub is_conversation_start: bool,
}

impl Hook {
    #[allow(dead_code)] // TODO: Remove
    pub fn new_inline_hook(name: &str, command: String, is_conversation_start: bool) -> Self {
        Self {
            name: name.to_string(),
            r#type: HookType::Inline,
            disabled: Self::default_disabled(),
            timeout_ms: Self::default_timeout_ms(),
            max_output_size: Self::default_max_output_size(),
            cache_ttl_seconds: None,
            command: Some(command),
            is_conversation_start,
        }
    }

    fn default_disabled() -> bool {
        false
    }

    fn default_timeout_ms() -> u64 {
        DEFAULT_TIMEOUT_MS
    }

    fn default_max_output_size() -> usize {
        DEFAULT_MAX_OUTPUT_SIZE
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HookType {
    // Execute an inline shell command
    Inline,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HookConfig {
    pub conversation_start: Vec<Hook>,
    pub per_prompt: Vec<Hook>,
}

#[derive(Debug, Clone)]
pub struct CachedHook {
    output: String,
    expiry: Option<Instant>,
}

#[derive(Debug, Clone)]
pub struct HookExecutor {
    pub execution_cache: HashMap<String, CachedHook>,
}

impl HookExecutor {
    pub fn new() -> Self {
        Self {
            execution_cache: HashMap::new(),
        }
    }

    /// Run all the currently enabled hooks from both the global and profile contexts
    /// # Arguments
    /// * `updates` - output stream to write hook run status to
    /// # Returns
    /// A vector containing pairs of a [`Hook`] definition and its execution output   
    pub async fn run_hooks(&mut self, hooks: Vec<&Hook>, updates: &mut impl Write) -> Vec<(Hook, String)> {
        let mut futures: Vec<future::Either<_, _>> = Vec::new();
        let mut num_cached = 0;

        for hook in hooks {
            if hook.disabled {
                continue;
            }

            // Check if the hook is cached. If so, push a completed future.
            if let Some(cached) = self.get_cache(&hook.name) {
                futures.push(Either::Left(future::ready((
                    hook,
                    Ok(cached.clone()),
                    Duration::from_secs(0),
                ))));
                num_cached += 1;
            } else {
                // Execute the hook asynchronously
                futures.push(Either::Right(self.execute_hook(hook)));
            }
        }

        // Skip output if we didn't run any hooks
        let skip_output = num_cached == futures.len();
        if !skip_output {
            let num_hooks = futures.len() - num_cached;
            let _ = execute!(
                updates,
                style::SetForegroundColor(Color::Green),
                style::Print(format!(
                    "\nRunning {} {} . . . ",
                    futures.len() - num_cached,
                    if num_hooks == 1 { "hook" } else { "hooks" }
                )),
            );
        }

        // Wait for results
        let results = future::join_all(futures).await;

        if !skip_output {
            // Output time data
            let total_duration = format!(
                "{:.3}s",
                results.iter().map(|(_, _, d)| d).sum::<Duration>().as_secs_f64()
            );
            let _ = queue!(
                updates,
                style::SetForegroundColor(Color::Green),
                style::Print(format!("completed in {}\n", total_duration)),
                style::SetForegroundColor(Color::Reset),
            );
        }

        let mut to_return = Vec::new();

        // If executions were successful, update cache.
        // Otherwise, report error.
        for (hook, result, _) in results {
            if result.is_ok() {
                // Conversation start hooks are always cached as they are expected to run once per session.
                let expiry = if hook.is_conversation_start {
                    None
                } else {
                    Some(
                        Instant::now()
                            + Duration::from_secs(hook.cache_ttl_seconds.unwrap_or(DEFAULT_CACHE_TTL_SECONDS)),
                    )
                };

                self.insert_cache(&hook.name, CachedHook {
                    output: result.as_ref().cloned().unwrap(),
                    expiry,
                });

                to_return.push((hook.clone(), result.unwrap()));
            } else if !skip_output {
                let _ = queue!(
                    updates,
                    style::SetForegroundColor(Color::Red),
                    style::Print(format!("hook '{}' failed: {}\n", hook.name, result.unwrap_err())),
                    style::SetForegroundColor(Color::Reset),
                );
            }
        }

        if !skip_output {
            let _ = execute!(updates, style::Print("\n"),);
        }

        to_return
    }

    async fn execute_hook<'a>(&self, hook: &'a Hook) -> (&'a Hook, Result<String>, Duration) {
        let start_time = Instant::now();
        let result = match hook.r#type {
            HookType::Inline => self.execute_inline_hook(hook).await,
        };

        (hook, result, start_time.elapsed())
    }

    async fn execute_inline_hook(&self, hook: &Hook) -> Result<String> {
        let command = hook.command.as_ref().ok_or_else(|| eyre!("no command specified"))?;

        let command_future = run_command(command, hook.max_output_size, None::<std::io::Stdout>);
        let timeout = Duration::from_millis(hook.timeout_ms);

        // Run with timeout
        match tokio::time::timeout(timeout, command_future).await? {
            Ok(result) => match result.exit_status.unwrap_or(0) {
                0 => Ok(result.stdout),
                code => Err(eyre!("command returned non-zero exit code: {}", code)),
            },
            Err(_) => Err(eyre!(
                "command failed to start or timed out after {} ms",
                timeout.as_millis()
            )),
        }
    }

    /// Will return a cached hook's output if it exists and isn't expired.
    fn get_cache(&self, name: &str) -> Option<String> {
        self.execution_cache.get(name).and_then(|o| {
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

    fn insert_cache(&mut self, name: &str, hook_output: CachedHook) {
        self.execution_cache.insert(name.to_string(), hook_output);
    }
}
