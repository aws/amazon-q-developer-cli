use std::collections::HashMap;
use std::io::Write;
use std::process::Stdio;
use std::time::{
    Duration,
    Instant,
};

use bstr::ByteSlice;
use clap::{
    Args,
    Subcommand,
};
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
    ErrReport,
    Result,
    eyre,
};
use futures::stream::{
    FuturesUnordered,
    StreamExt,
};
use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};
use spinners::{
    Spinner,
    Spinners,
};

use crate::cli::chat::util::truncate_safe;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::os::Os;

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_MAX_OUTPUT_SIZE: usize = 1024 * 10;
const DEFAULT_CACHE_TTL_SECONDS: u64 = 0;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum HookType {
    // Execute an inline shell command
    Inline,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, JsonSchema)]
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
    pub async fn run_hooks(
        &mut self,
        hooks: Vec<&Hook>,
        output: &mut impl Write,
    ) -> Result<Vec<(Hook, String)>, ChatError> {
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

        if total != 0 {
            spinner = Some(Spinner::new(Spinners::Dots12, spinner_text(succeeded, total)));
        }

        // Process results as they complete
        let start_time = Instant::now();
        while let Some((index, (hook, result, duration))) = futures.next().await {
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

            match &result {
                Ok(_) => {
                    queue!(
                        output,
                        style::SetForegroundColor(style::Color::Green),
                        style::Print("✓ "),
                        style::SetForegroundColor(style::Color::Blue),
                        style::Print(&hook.name),
                        style::ResetColor,
                        style::Print(" finished in "),
                        style::SetForegroundColor(style::Color::Yellow),
                        style::Print(format!("{:.2} s\n", duration.as_secs_f32())),
                        style::ResetColor,
                    )?;
                },
                Err(e) => {
                    queue!(
                        output,
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
                    )?;
                },
            }

            // Process results regardless of output enabled
            if let Ok(output) = result {
                succeeded += 1;
                results.push((index, (hook.clone(), output)));
            }

            // Display ending summary or add a new spinner
            // The futures set size decreases each time we process one
            if futures.is_empty() {
                let symbol = if total == succeeded {
                    "✓".to_string().green()
                } else {
                    "✗".to_string().red()
                };

                queue!(
                    output,
                    style::SetForegroundColor(Color::Blue),
                    style::Print(format!("{symbol} {} in ", spinner_text(succeeded, total))),
                    style::SetForegroundColor(style::Color::Yellow),
                    style::Print(format!("{:.2} s\n", start_time.elapsed().as_secs_f32())),
                    style::ResetColor,
                )?;
            } else {
                spinner = Some(Spinner::new(Spinners::Dots, spinner_text(succeeded, total)));
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
        Ok(results.into_iter().map(|(_, r)| r).collect())
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

        #[cfg(unix)]
        let command_future = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();

        #[cfg(windows)]
        let command_future = tokio::process::Command::new("cmd")
            .arg("/C")
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

#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
#[command(
    before_long_help = "Use context hooks to specify shell commands to run. The output from these 
commands will be appended to the prompt to Amazon Q. Hooks can be defined 
in global or local profiles.

Notes:
• Hooks are executed in parallel
• 'conversation_start' hooks run on the first user prompt and are attached once to the conversation history sent to Amazon Q
• 'per_prompt' hooks run on each user prompt and are attached to the prompt, but are not stored in conversation history"
)]
pub struct HooksArgs {
    #[command(subcommand)]
    subcommand: Option<HooksSubcommand>,
}

impl HooksArgs {
    pub async fn execute(self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        if let Some(subcommand) = self.subcommand {
            return subcommand.execute(os, session).await;
        }

        let Some(context_manager) = &mut session.conversation.context_manager else {
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        };

        queue!(
            session.stderr,
            style::SetAttribute(Attribute::Bold),
            style::SetForegroundColor(Color::Magenta),
            style::Print(format!("\n👤 profile ({}):\n", &context_manager.current_profile)),
            style::SetAttribute(Attribute::Reset),
        )?;

        print_hook_section(
            &mut session.stderr,
            &context_manager.profile_config.hooks,
            HookTrigger::ConversationStart,
        )
        .map_err(map_chat_error)?;
        print_hook_section(
            &mut session.stderr,
            &context_manager.profile_config.hooks,
            HookTrigger::PerPrompt,
        )
        .map_err(map_chat_error)?;

        execute!(
            session.stderr,
            style::Print(format!(
                "\nUse {} to manage hooks.\n\n",
                "/hooks help".to_string().dark_green()
            )),
        )?;

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    pub fn subcommand_name(&self) -> Option<&'static str> {
        self.subcommand.as_ref().map(|s| s.name())
    }
}

#[deny(missing_docs)]
#[derive(Clone, Debug, PartialEq, Subcommand)]
pub enum HooksSubcommand {
    /// Add a new command context hook
    Add {
        /// The name of the hook
        name: String,
        /// When to trigger the hook, valid options: `per_prompt` or `conversation_start`
        #[arg(long, value_parser = ["per_prompt", "conversation_start"])]
        trigger: String,
        /// Shell command to execute
        #[arg(long, value_parser = clap::value_parser!(String))]
        command: String,
    },
    /// Remove an existing context hook
    #[command(name = "rm")]
    Remove {
        /// The name of the hook
        name: String,
    },
    /// Enable an existing context hook
    Enable {
        /// The name of the hook
        name: String,
    },
    /// Disable an existing context hook
    Disable {
        /// The name of the hook
        name: String,
    },
    /// Enable all existing context hooks
    EnableAll,
    /// Disable all existing context hooks
    DisableAll,
    /// Display the context rule configuration and matched files
    Show,
}

impl HooksSubcommand {
    pub async fn execute(self, _os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let Some(context_manager) = &mut session.conversation.context_manager else {
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        };

        match self {
            Self::Add { name, trigger, command } => {
                let trigger = if trigger == "conversation_start" {
                    HookTrigger::ConversationStart
                } else {
                    HookTrigger::PerPrompt
                };

                match context_manager.add_hook(name.clone(), Hook::new_inline_hook(trigger, command)) {
                    Ok(_) => {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Green),
                            style::Print(format!("\nAdded hook '{name}'.\n\n")),
                            style::SetForegroundColor(Color::Reset)
                        )?;
                    },
                    Err(e) => {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Red),
                            style::Print(format!("\nCannot add hook '{name}': {}\n\n", e)),
                            style::SetForegroundColor(Color::Reset)
                        )?;
                    },
                }
            },
            Self::Remove { name } => {
                let result = context_manager.remove_hook(&name);
                match result {
                    Ok(_) => {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Green),
                            style::Print(format!("\nRemoved hook '{name}'.\n\n")),
                            style::SetForegroundColor(Color::Reset)
                        )?;
                    },
                    Err(e) => {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Red),
                            style::Print(format!("\nCannot remove hook '{name}': {}\n\n", e)),
                            style::SetForegroundColor(Color::Reset)
                        )?;
                    },
                }
            },
            Self::Enable { name } => {
                let result = context_manager.set_hook_disabled(&name, false);
                match result {
                    Ok(_) => {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Green),
                            style::Print(format!("\nEnabled hook '{name}'.\n\n")),
                            style::SetForegroundColor(Color::Reset)
                        )?;
                    },
                    Err(e) => {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Red),
                            style::Print(format!("\nCannot enable hook '{name}': {}\n\n", e)),
                            style::SetForegroundColor(Color::Reset)
                        )?;
                    },
                }
            },
            Self::Disable { name } => {
                let result = context_manager.set_hook_disabled(&name, true);
                match result {
                    Ok(_) => {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Green),
                            style::Print(format!("\nDisabled hook '{name}'.\n\n")),
                            style::SetForegroundColor(Color::Reset)
                        )?;
                    },
                    Err(e) => {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Red),
                            style::Print(format!("\nCannot disable hook '{name}': {}\n\n", e)),
                            style::SetForegroundColor(Color::Reset)
                        )?;
                    },
                }
            },
            Self::EnableAll => {
                context_manager.set_all_hooks_disabled(false);
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Green),
                    style::Print("\nEnabled all hooks.\n\n"),
                    style::SetForegroundColor(Color::Reset)
                )?;
            },
            Self::DisableAll => {
                context_manager.set_all_hooks_disabled(true);
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Green),
                    style::Print("\nDisabled all hooks.\n\n"),
                    style::SetForegroundColor(Color::Reset)
                )?;
            },
            Self::Show => {
                execute!(
                    session.stderr,
                    style::SetAttribute(Attribute::Bold),
                    style::SetForegroundColor(Color::Magenta),
                    style::Print(format!("\n👤 profile ({}):\n", context_manager.current_profile)),
                    style::SetAttribute(Attribute::Reset),
                )?;

                print_hook_section(
                    &mut session.stderr,
                    &context_manager.profile_config.hooks,
                    HookTrigger::ConversationStart,
                )
                .map_err(map_chat_error)?;
                print_hook_section(
                    &mut session.stderr,
                    &context_manager.profile_config.hooks,
                    HookTrigger::PerPrompt,
                )
                .map_err(map_chat_error)?;
                execute!(session.stderr, style::Print("\n"))?;
            },
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            HooksSubcommand::Add { .. } => "add",
            HooksSubcommand::Remove { .. } => "remove",
            HooksSubcommand::Enable { .. } => "enable",
            HooksSubcommand::Disable { .. } => "disable",
            HooksSubcommand::EnableAll => "enable-all",
            HooksSubcommand::DisableAll => "disable-all",
            HooksSubcommand::Show => "show",
        }
    }
}

/// Prints hook configuration grouped by trigger: conversation session start or per user message
pub fn print_hook_section(output: &mut impl Write, hooks: &HashMap<String, Hook>, trigger: HookTrigger) -> Result<()> {
    let section = match trigger {
        HookTrigger::ConversationStart => "On Session Start",
        HookTrigger::PerPrompt => "Per User Message",
    };
    let hooks: Vec<(&String, &Hook)> = hooks.iter().filter(|(_, h)| h.trigger == trigger).collect();

    queue!(
        output,
        style::SetForegroundColor(Color::Cyan),
        style::Print(format!("    {section}:\n")),
        style::SetForegroundColor(Color::Reset),
    )?;

    if hooks.is_empty() {
        queue!(
            output,
            style::SetForegroundColor(Color::DarkGrey),
            style::Print("      <none>\n"),
            style::SetForegroundColor(Color::Reset)
        )?;
    } else {
        for (name, hook) in hooks {
            if hook.disabled {
                queue!(
                    output,
                    style::SetForegroundColor(Color::DarkGrey),
                    style::Print(format!("      {} (disabled)\n", name)),
                    style::SetForegroundColor(Color::Reset)
                )?;
            } else {
                queue!(output, style::Print(format!("      {}\n", name)),)?;
            }
        }
    }
    Ok(())
}

pub fn map_chat_error(e: ErrReport) -> ChatError {
    ChatError::Custom(e.to_string().into())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::sleep;

    use super::*;
    use crate::cli::chat::util::test::create_test_context_manager;

    #[tokio::test]
    async fn test_add_hook() -> Result<()> {
        let mut manager = create_test_context_manager(None).unwrap();
        let hook = Hook::new_inline_hook(HookTrigger::ConversationStart, "echo test".to_string());

        // Test adding hook to profile config
        manager.add_hook("test_hook".to_string(), hook.clone())?;
        assert!(manager.profile_config.hooks.contains_key("test_hook"));

        // Test adding duplicate hook name
        assert!(manager.add_hook("test_hook".to_string(), hook).is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_remove_hook() -> Result<()> {
        let mut manager = create_test_context_manager(None).unwrap();
        let hook = Hook::new_inline_hook(HookTrigger::ConversationStart, "echo test".to_string());

        manager
            .add_hook("test_hook".to_string(), hook)
            .expect("Hook addition failed");

        // Test removing existing hook
        manager.remove_hook("test_hook").expect("Hook removal failed");
        assert!(!manager.profile_config.hooks.contains_key("test_hook"));

        // Test removing non-existent hook
        assert!(manager.remove_hook("test_hook").is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_set_hook_disabled() -> Result<()> {
        let mut manager = create_test_context_manager(None).unwrap();
        let hook = Hook::new_inline_hook(HookTrigger::ConversationStart, "echo test".to_string());

        manager.add_hook("test_hook".to_string(), hook).unwrap();

        // Test disabling hook
        manager.set_hook_disabled("test_hook", true).unwrap();
        assert!(manager.profile_config.hooks.get("test_hook").unwrap().disabled);

        // Test enabling hook
        manager.set_hook_disabled("test_hook", false).unwrap();
        assert!(!manager.profile_config.hooks.get("test_hook").unwrap().disabled);

        // Test with non-existent hook
        assert!(manager.set_hook_disabled("nonexistent", true).is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_set_all_hooks_disabled() -> Result<()> {
        let mut manager = create_test_context_manager(None).unwrap();
        let hook1 = Hook::new_inline_hook(HookTrigger::ConversationStart, "echo test".to_string());
        let hook2 = Hook::new_inline_hook(HookTrigger::ConversationStart, "echo test".to_string());

        manager
            .add_hook("hook1".to_string(), hook1)
            .expect("Hook addition failed");
        manager
            .add_hook("hook2".to_string(), hook2)
            .expect("Hook addition failed");

        // Test disabling all hooks
        manager.set_all_hooks_disabled(true);
        assert!(manager.profile_config.hooks.values().all(|h| h.disabled));

        // Test enabling all hooks
        manager.set_all_hooks_disabled(false);
        assert!(manager.profile_config.hooks.values().all(|h| !h.disabled));

        Ok(())
    }

    #[tokio::test]
    async fn test_run_hooks() -> Result<()> {
        let mut manager = create_test_context_manager(None).unwrap();
        let hook1 = Hook::new_inline_hook(HookTrigger::ConversationStart, "echo test".to_string());
        let hook2 = Hook::new_inline_hook(HookTrigger::ConversationStart, "echo test".to_string());

        manager.add_hook("hook1".to_string(), hook1).unwrap();
        manager.add_hook("hook2".to_string(), hook2).unwrap();

        // Run the hooks
        let results = manager.run_hooks(&mut vec![]).await.unwrap();
        assert_eq!(results.len(), 2); // Should include both hooks

        Ok(())
    }

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
        let mut output = vec![];
        let results = executor.run_hooks(vec![&hook1, &hook2], &mut output).await.unwrap();

        assert_eq!(results.len(), 2);
        assert!(results[0].1.contains("test1"));
        assert!(results[1].1.contains("test2"));
        assert!(!output.is_empty());

        // Second execution should use cache
        let mut output = Vec::new();
        let results = executor.run_hooks(vec![&hook1, &hook2], &mut output).await.unwrap();

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
        let mut output = vec![];
        let results = executor.run_hooks(vec![&hook1, &hook2], &mut output).await.unwrap();

        assert_eq!(results.len(), 2);
        assert!(results[0].1.contains("test1"));
        assert!(results[1].1.contains("test2"));
        assert!(!output.is_empty());

        // Second execution should use cache
        let mut output = Vec::new();
        let results = executor.run_hooks(vec![&hook1, &hook2], &mut output).await.unwrap();

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
        let results = executor.run_hooks(vec![&hook1, &hook2], &mut output).await.unwrap();

        assert_eq!(results.len(), 2);
        assert!(results[0].1.contains("test1"));
        assert!(results[1].1.contains("test2"));
        assert!(!output.is_empty());

        // Second execution should use cache
        let mut output = Vec::new();
        let results = executor.run_hooks(vec![&hook1, &hook2], &mut output).await.unwrap();

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

        let results = executor.run_hooks(vec![&hook], &mut vec![]).await.unwrap();

        assert_eq!(results.len(), 0); // Should fail due to timeout
    }

    #[tokio::test]
    async fn test_disabled_hook() {
        let mut executor = HookExecutor::new();
        let mut hook = Hook::new_inline_hook(HookTrigger::PerPrompt, "echo 'test'".to_string());
        hook.disabled = true;

        let results = executor.run_hooks(vec![&hook], &mut vec![]).await.unwrap();

        assert_eq!(results.len(), 0); // Disabled hook should not run
    }

    #[tokio::test]
    async fn test_cache_expiration() {
        let mut executor = HookExecutor::new();
        let mut hook = Hook::new_inline_hook(HookTrigger::PerPrompt, "echo 'test'".to_string());
        hook.cache_ttl_seconds = 1;

        // First execution
        let results1 = executor.run_hooks(vec![&hook], &mut vec![]).await.unwrap();
        assert_eq!(results1.len(), 1);

        // Wait for cache to expire
        sleep(Duration::from_millis(1001)).await;

        // Second execution should run command again
        let results2 = executor.run_hooks(vec![&hook], &mut vec![]).await.unwrap();
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

        // Use different commands based on OS
        #[cfg(unix)]
        let command = "for i in {1..1000}; do echo $i; done";

        #[cfg(windows)]
        let command = "for /L %i in (1,1,1000) do @echo %i";

        let mut hook = Hook::new_inline_hook(HookTrigger::PerPrompt, command.to_string());
        hook.max_output_size = 100;

        let results = executor.run_hooks(vec![&hook], &mut vec![]).await.unwrap();

        assert!(results[0].1.len() <= hook.max_output_size + " ... truncated".len());
    }

    #[tokio::test]
    async fn test_os_specific_command_execution() {
        let mut executor = HookExecutor::new();

        // Create a simple command that outputs the shell name
        #[cfg(unix)]
        let command = "echo $SHELL";

        #[cfg(windows)]
        let command = "echo %ComSpec%";

        let hook = Hook::new_inline_hook(HookTrigger::PerPrompt, command.to_string());

        let results = executor.run_hooks(vec![&hook], &mut vec![]).await.unwrap();

        assert_eq!(results.len(), 1, "Command execution should succeed");

        // Verify output contains expected shell information
        #[cfg(unix)]
        assert!(results[0].1.contains("/"), "Unix shell path should contain '/'");

        #[cfg(windows)]
        assert!(
            results[0].1.to_lowercase().contains("cmd.exe") || results[0].1.to_lowercase().contains("command.com"),
            "Windows shell path should contain cmd.exe or command.com"
        );
    }
}
