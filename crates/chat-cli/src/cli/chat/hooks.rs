use std::collections::HashMap;
use std::io::Write;
use std::process::Stdio;
use std::time::{
    Duration,
    Instant,
};

use bstr::ByteSlice;
use crossterm::style::{
    Color,
    Stylize,
};
use crossterm::{
    cursor,
    execute,
    queue,
    style,
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
use serde::{
    Deserialize,
    Serialize,
};
use spinners::{
    Spinner,
    Spinners,
};

use super::util::truncate_safe;

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_MAX_OUTPUT_SIZE: usize = 1024 * 10;
const DEFAULT_CACHE_TTL_SECONDS: u64 = 0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hook {
    pub trigger: HookTrigger,

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
    #[serde(default = "Hook::default_cache_ttl_seconds")]
    pub cache_ttl_seconds: u64,

    // Type-specific fields
    /// The bash command to execute
    pub command: Option<String>, // For inline hooks

    // Internal data
    #[serde(skip)]
    pub name: String,
    #[serde(skip)]
    pub is_global: bool,
}

impl Hook {
    pub fn new_inline_hook(trigger: HookTrigger, command: String) -> Self {
        Self {
            trigger,
            r#type: HookType::Inline,
            disabled: Self::default_disabled(),
            timeout_ms: Self::default_timeout_ms(),
            max_output_size: Self::default_max_output_size(),
            cache_ttl_seconds: Self::default_cache_ttl_seconds(),
            command: Some(command),
            is_global: false,
            name: "new hook".to_string(),
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

    fn default_cache_ttl_seconds() -> u64 {
        DEFAULT_CACHE_TTL_SECONDS
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum HookType {
    // Execute an inline shell command
    Inline,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HookTrigger {
    ConversationStart,
    PerPrompt,
}

#[derive(Debug, Clone)]
pub struct CachedHook {
    output: String,
    expiry: Option<Instant>,
}

/// Maps a hook name to a [`CachedHook`]
#[derive(Debug, Clone, Default)]
pub struct HookExecutor {
    pub global_cache: HashMap<String, CachedHook>,
    pub profile_cache: HashMap<String, CachedHook>,
}

impl HookExecutor {
    pub fn new() -> Self {
        Self {
            global_cache: HashMap::new(),
            profile_cache: HashMap::new(),
        }
    }

    /// Run and cache [`Hook`]s. Any hooks that are already cached will be returned without
    /// executing. Hooks that fail to execute will not be returned.
    ///
    /// If `updates` is `Some`, progress on hook execution will be written to it.
    /// Errors encountered with write operations to `updates` are ignored.
    ///
    /// Note: [`HookTrigger::ConversationStart`] hooks never leave the cache.
    pub async fn run_hooks(&mut self, hooks: Vec<&Hook>, mut updates: Option<&mut impl Write>) -> Vec<(Hook, String)> {
        let mut results = Vec::with_capacity(hooks.len());
        let mut futures = FuturesUnordered::new();

        // Start all hook future OR fetch from cache if available
        // Why enumerate? We want to return the hook results in the order of hooks that we received,
        // however, for output display we want to process hooks as they complete rather than the
        // order they were started in. The index will be used later to sort them back to output order.
        for (index, hook) in hooks.into_iter().enumerate() {
            if hook.disabled {
                continue;
            }

            if let Some(cached) = self.get_cache(hook) {
                results.push((index, (hook.clone(), cached.clone())));
                continue;
            }
            let future = self.execute_hook(hook);
            futures.push(async move { (index, future.await) });
        }

        // Start caching the results added after whats already their (they are from the cache already)
        let start_cache_index = results.len();

        let mut succeeded = 0;
        let total = futures.len();

        let mut spinner = None;
        let spinner_text = |complete: usize, total: usize| {
            format!(
                "{} of {} hooks finished",
                complete.to_string().blue(),
                total.to_string().blue(),
            )
        };
        if total != 0 && updates.is_some() {
            spinner = Some(Spinner::new(Spinners::Dots12, spinner_text(succeeded, total)));
        }

        // Process results as they complete
        let start_time = Instant::now();
        while let Some((index, (hook, result, duration))) = futures.next().await {
            // If output is enabled, handle that first
            if let Some(updates) = updates.as_deref_mut() {
                if let Some(spinner) = spinner.as_mut() {
                    spinner.stop();

                    // Erase the spinner
                    let _ = execute!(
                        updates,
                        cursor::MoveToColumn(0),
                        terminal::Clear(terminal::ClearType::CurrentLine),
                        cursor::Hide,
                    );
                }
                match &result {
                    Ok(_) => {
                        let _ = queue!(
                            updates,
                            style::SetForegroundColor(style::Color::Green),
                            style::Print("✓ "),
                            style::SetForegroundColor(style::Color::Blue),
                            style::Print(&hook.name),
                            style::ResetColor,
                            style::Print(" finished in "),
                            style::SetForegroundColor(style::Color::Yellow),
                            style::Print(format!("{:.2} s\n", duration.as_secs_f32())),
                            style::ResetColor,
                        );
                    },
                    Err(e) => {
                        let _ = queue!(
                            updates,
                            style::SetForegroundColor(style::Color::Red),
                            style::Print("✗ "),
                            style::SetForegroundColor(style::Color::Blue),
                            style::Print(&hook.name),
                            style::ResetColor,
                            style::Print(" failed after "),
                            style::SetForegroundColor(style::Color::Yellow),
                            style::Print(format!("{:.2} s", duration.as_secs_f32())),
                            style::ResetColor,
                            style::Print(format!(": {}\n", e)),
                        );
                    },
                }
            }

            // Process results regardless of output enabled
            if let Ok(output) = result {
                succeeded += 1;
                results.push((index, (hook.clone(), output)));
            }

            // Display ending summary or add a new spinner
            if let Some(updates) = updates.as_deref_mut() {
                // The futures set size decreases each time we process one
                if futures.is_empty() {
                    let symbol = if total == succeeded {
                        "✓".to_string().green()
                    } else {
                        "✗".to_string().red()
                    };

                    let _ = queue!(
                        updates,
                        style::SetForegroundColor(Color::Blue),
                        style::Print(format!("{symbol} {} in ", spinner_text(succeeded, total))),
                        style::SetForegroundColor(style::Color::Yellow),
                        style::Print(format!("{:.2} s\n", start_time.elapsed().as_secs_f32())),
                        style::ResetColor,
                    );
                } else {
                    spinner = Some(Spinner::new(Spinners::Dots, spinner_text(succeeded, total)));
                }
            }
        }
        drop(futures);

        // Fill cache with executed results, skipping what was already from cache
        results.iter().skip(start_cache_index).for_each(|(_, (hook, output))| {
            let expiry = match hook.trigger {
                HookTrigger::ConversationStart => None,
                HookTrigger::PerPrompt => Some(Instant::now() + Duration::from_secs(hook.cache_ttl_seconds)),
            };
            self.insert_cache(hook, CachedHook {
                output: output.clone(),
                expiry,
            });
        });

        // Return back to order at request start
        results.sort_by_key(|(idx, _)| *idx);
        results.into_iter().map(|(_, r)| r).collect()
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

        let command_future = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();
        let timeout = Duration::from_millis(hook.timeout_ms);

        // Run with timeout
        match tokio::time::timeout(timeout, command_future).await {
            Ok(result) => {
                let result = result?;
                if result.status.success() {
                    let stdout = result.stdout.to_str_lossy();
                    let stdout = format!(
                        "{}{}",
                        truncate_safe(&stdout, hook.max_output_size),
                        if stdout.len() > hook.max_output_size {
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
            Err(_) => Err(eyre!("command timed out after {} ms", timeout.as_millis())),
        }
    }

    /// Will return a cached hook's output if it exists and isn't expired.
    fn get_cache(&self, hook: &Hook) -> Option<String> {
        let cache = if hook.is_global {
            &self.global_cache
        } else {
            &self.profile_cache
        };

        cache.get(&hook.name).and_then(|o| {
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

    fn insert_cache(&mut self, hook: &Hook, hook_output: CachedHook) {
        let cache = if hook.is_global {
            &mut self.global_cache
        } else {
            &mut self.profile_cache
        };

        cache.insert(hook.name.clone(), hook_output);
    }
}

#[cfg(test)]
mod tests {
    use std::io::Stdout;
    use std::time::Duration;

    use tokio::time::sleep;

    use super::*;

    #[test]
    fn test_hook_creation() {
        let command = "echo 'hello'";
        let hook = Hook::new_inline_hook(HookTrigger::PerPrompt, command.to_string());

        assert_eq!(hook.r#type, HookType::Inline);
        assert!(!hook.disabled);
        assert_eq!(hook.timeout_ms, DEFAULT_TIMEOUT_MS);
        assert_eq!(hook.max_output_size, DEFAULT_MAX_OUTPUT_SIZE);
        assert_eq!(hook.cache_ttl_seconds, DEFAULT_CACHE_TTL_SECONDS);
        assert_eq!(hook.command, Some(command.to_string()));
        assert_eq!(hook.trigger, HookTrigger::PerPrompt);
        assert!(!hook.is_global);
    }

    #[tokio::test]
    async fn test_hook_executor_cached_conversation_start() {
        let mut executor = HookExecutor::new();
        let mut hook1 = Hook::new_inline_hook(HookTrigger::ConversationStart, "echo 'test1'".to_string());
        hook1.is_global = true;

        let mut hook2 = Hook::new_inline_hook(HookTrigger::ConversationStart, "echo 'test2'".to_string());
        hook2.is_global = false;

        // First execution should run the command
        let mut output = Vec::new();
        let results = executor.run_hooks(vec![&hook1, &hook2], Some(&mut output)).await;

        assert_eq!(results.len(), 2);
        assert!(results[0].1.contains("test1"));
        assert!(results[1].1.contains("test2"));
        assert!(!output.is_empty());

        // Second execution should use cache
        let mut output = Vec::new();
        let results = executor.run_hooks(vec![&hook1, &hook2], Some(&mut output)).await;

        assert_eq!(results.len(), 2);
        assert!(results[0].1.contains("test1"));
        assert!(results[1].1.contains("test2"));
        assert!(output.is_empty()); // Should not have run the hook, so no output.
    }

    #[tokio::test]
    async fn test_hook_executor_cached_per_prompt() {
        let mut executor = HookExecutor::new();
        let mut hook1 = Hook::new_inline_hook(HookTrigger::PerPrompt, "echo 'test1'".to_string());
        hook1.is_global = true;
        hook1.cache_ttl_seconds = 60;

        let mut hook2 = Hook::new_inline_hook(HookTrigger::PerPrompt, "echo 'test2'".to_string());
        hook2.is_global = false;
        hook2.cache_ttl_seconds = 60;

        // First execution should run the command
        let mut output = Vec::new();
        let results = executor.run_hooks(vec![&hook1, &hook2], Some(&mut output)).await;

        assert_eq!(results.len(), 2);
        assert!(results[0].1.contains("test1"));
        assert!(results[1].1.contains("test2"));
        assert!(!output.is_empty());

        // Second execution should use cache
        let mut output = Vec::new();
        let results = executor.run_hooks(vec![&hook1, &hook2], Some(&mut output)).await;

        assert_eq!(results.len(), 2);
        assert!(results[0].1.contains("test1"));
        assert!(results[1].1.contains("test2"));
        assert!(output.is_empty()); // Should not have run the hook, so no output.
    }

    #[tokio::test]
    async fn test_hook_executor_not_cached_per_prompt() {
        let mut executor = HookExecutor::new();
        let mut hook1 = Hook::new_inline_hook(HookTrigger::PerPrompt, "echo 'test1'".to_string());
        hook1.is_global = true;

        let mut hook2 = Hook::new_inline_hook(HookTrigger::PerPrompt, "echo 'test2'".to_string());
        hook2.is_global = false;

        // First execution should run the command
        let mut output = Vec::new();
        let results = executor.run_hooks(vec![&hook1, &hook2], Some(&mut output)).await;

        assert_eq!(results.len(), 2);
        assert!(results[0].1.contains("test1"));
        assert!(results[1].1.contains("test2"));
        assert!(!output.is_empty());

        // Second execution should use cache
        let mut output = Vec::new();
        let results = executor.run_hooks(vec![&hook1, &hook2], Some(&mut output)).await;

        assert_eq!(results.len(), 2);
        assert!(results[0].1.contains("test1"));
        assert!(results[1].1.contains("test2"));
        assert!(!output.is_empty());
    }

    #[tokio::test]
    async fn test_hook_timeout() {
        let mut executor = HookExecutor::new();
        let mut hook = Hook::new_inline_hook(HookTrigger::PerPrompt, "sleep 2".to_string());
        hook.timeout_ms = 100; // Set very short timeout

        let results = executor.run_hooks(vec![&hook], None::<&mut Stdout>).await;

        assert_eq!(results.len(), 0); // Should fail due to timeout
    }

    #[tokio::test]
    async fn test_disabled_hook() {
        let mut executor = HookExecutor::new();
        let mut hook = Hook::new_inline_hook(HookTrigger::PerPrompt, "echo 'test'".to_string());
        hook.disabled = true;

        let results = executor.run_hooks(vec![&hook], None::<&mut Stdout>).await;

        assert_eq!(results.len(), 0); // Disabled hook should not run
    }

    #[tokio::test]
    async fn test_cache_expiration() {
        let mut executor = HookExecutor::new();
        let mut hook = Hook::new_inline_hook(HookTrigger::PerPrompt, "echo 'test'".to_string());
        hook.cache_ttl_seconds = 1;

        // First execution
        let results1 = executor.run_hooks(vec![&hook], None::<&mut Stdout>).await;
        assert_eq!(results1.len(), 1);

        // Wait for cache to expire
        sleep(Duration::from_millis(1001)).await;

        // Second execution should run command again
        let results2 = executor.run_hooks(vec![&hook], None::<&mut Stdout>).await;
        assert_eq!(results2.len(), 1);
    }

    #[test]
    fn test_hook_cache_storage() {
        let mut executor: HookExecutor = HookExecutor::new();
        let hook = Hook::new_inline_hook(HookTrigger::PerPrompt, "".to_string());

        let cached_hook = CachedHook {
            output: "test output".to_string(),
            expiry: None,
        };

        executor.insert_cache(&hook, cached_hook.clone());

        assert_eq!(executor.get_cache(&hook), Some("test output".to_string()));
    }

    #[test]
    fn test_hook_cache_storage_expired() {
        let mut executor: HookExecutor = HookExecutor::new();
        let hook = Hook::new_inline_hook(HookTrigger::PerPrompt, "".to_string());

        let cached_hook = CachedHook {
            output: "test output".to_string(),
            expiry: Some(Instant::now()),
        };

        executor.insert_cache(&hook, cached_hook.clone());

        // Item should not return since it is expired
        assert_eq!(executor.get_cache(&hook), None);
    }

    #[tokio::test]
    async fn test_max_output_size() {
        let mut executor = HookExecutor::new();
        let mut hook = Hook::new_inline_hook(
            HookTrigger::PerPrompt,
            "for i in {1..1000}; do echo $i; done".to_string(),
        );
        hook.max_output_size = 100;

        let results = executor.run_hooks(vec![&hook], None::<&mut Stdout>).await;

        assert!(results[0].1.len() <= hook.max_output_size + " ... truncated".len());
    }
}
