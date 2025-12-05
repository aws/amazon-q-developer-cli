use std::collections::{
    HashMap,
    VecDeque,
};
use std::fmt;

use clap::Args;

use crate::cli::chat::consts::MAX_USER_MESSAGE_SIZE;
use crate::cli::chat::conversation::HistoryEntry;
use crate::cli::chat::message::{
    AssistantMessage,
    UserMessageContent,
};
use crate::cli::chat::tools::tool::ToolMetadata;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::os::Os;

#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
#[command(
    before_long_help = "/compact summarizes the conversation history to free up context space
while preserving essential information. This is useful for long-running conversations
that may eventually reach memory constraints.

When to use
• When you see the memory constraint warning message
• When a conversation has been running for a long time
• Before starting a new topic within the same session
• After completing complex tool operations

How it works
• Creates an AI-generated summary of your conversation
• Retains key information, code, and tool executions in the summary
• Clears the conversation history to free up space
• The assistant will reference the summary context in future responses

Compaction will be automatically performed whenever the context window overflows.
To disable this behavior, run: `kiro-cli settings chat.disableAutoCompaction true`"
)]
/// Arguments for the `/compact` command that summarizes conversation history to free up context
/// space.
///
/// This command creates an AI-generated summary of the conversation while preserving essential
/// information, code, and tool executions. It's useful for long-running conversations that
/// may reach memory constraints.
pub struct CompactArgs {
    /// The prompt to use when generating the summary
    prompt: Vec<String>,
    #[arg(long)]
    show_summary: bool,
    /// The number of user and assistant message pairs to exclude from the summarization.
    #[arg(long)]
    messages_to_exclude: Option<usize>,
    /// Whether or not large messages should be truncated.
    #[arg(long)]
    truncate_large_messages: Option<bool>,
    /// Maximum allowed size of messages in the conversation history. Requires
    /// truncate_large_messages to be set.
    #[arg(long, requires = "truncate_large_messages")]
    max_message_length: Option<usize>,
}

impl CompactArgs {
    pub async fn execute(self, os: &mut Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let default = CompactStrategy::default();
        let prompt = if self.prompt.is_empty() {
            None
        } else {
            Some(self.prompt.join(" "))
        };

        // Compact interrupts the current conversation so this will always result in a new user
        // turn.
        session.reset_user_turn();

        session
            .compact_history(os, prompt, self.show_summary, CompactStrategy {
                messages_to_exclude: self.messages_to_exclude.unwrap_or(default.messages_to_exclude),
                truncate_large_messages: self.truncate_large_messages.unwrap_or(default.truncate_large_messages),
                max_message_length: self.max_message_length.map_or(default.max_message_length, |v| {
                    v.clamp(UserMessageContent::TRUNCATED_SUFFIX.len(), MAX_USER_MESSAGE_SIZE)
                }),
            })
            .await
    }
}

/// Parameters for performing the history compaction request.
#[derive(Debug, Copy, Clone)]
pub struct CompactStrategy {
    /// Number of user/assistant pairs to exclude from the history as part of compaction.
    pub messages_to_exclude: usize,
    /// Whether or not to truncate large messages in the history.
    pub truncate_large_messages: bool,
    /// Maximum allowed size of messages in the conversation history.
    pub max_message_length: usize,
}

impl Default for CompactStrategy {
    fn default() -> Self {
        Self {
            messages_to_exclude: Default::default(),
            truncate_large_messages: Default::default(),
            max_message_length: MAX_USER_MESSAGE_SIZE,
        }
    }
}

/// Maximum number of files to include in the factual record.
const MAX_FILES_IN_FACTUAL_RECORD: usize = 30;
/// Maximum number of commands to include in the factual record.
const MAX_COMMANDS_IN_FACTUAL_RECORD: usize = 20;
/// Maximum number of reasonings to store per file or command.
const MAX_REASONINGS_PER_ITEM: usize = 5;
/// Maximum character length for file paths before truncation.
const MAX_FILE_PATH_LENGTH: usize = 100;
/// Character budget for files section (~3,500 tokens).
const FILES_CHAR_BUDGET: usize = 14_000;
/// Character budget for commands section (~1,500 tokens).
const COMMANDS_CHAR_BUDGET: usize = 6_000;
/// Weight applied to frequency when scoring items.
const FREQUENCY_WEIGHT: f64 = 2.0;
/// Weight applied to recency when scoring items.
const RECENCY_WEIGHT: f64 = 1.0;

/// Stores factual information extracted from conversation history during compaction.
///
/// This struct contains the most important files and commands based on weighted scoring,
/// preserving critical context that might be lost in LLM summarization.
///
/// # Fields
/// - `files_modified`: Vector of (file_path, reasonings, total_count) tuples for files modified via
///   `fs_write`
/// - `commands_executed`: Vector of (command_text, reasonings, total_count) tuples for commands run
///   via `execute_bash`
/// - `total_files`: Total number of unique files in history (before truncation)
/// - `total_commands`: Total number of unique commands in history (before truncation)
#[derive(Debug, Default)]
pub(crate) struct CompactionFacts {
    files_modified: Vec<(String, Vec<String>, usize)>,
    commands_executed: Vec<(String, Vec<String>, usize)>,
    total_files: usize,
    total_commands: usize,
}

impl fmt::Display for CompactionFacts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.files_modified.is_empty() {
            writeln!(f, "### Files Modified")?;
            if self.total_files > self.files_modified.len() {
                writeln!(
                    f,
                    "_(Showing {} of {} files by importance. Indented items show summaries for the last 5 modifications.)_",
                    self.files_modified.len(),
                    self.total_files
                )?;
            } else {
                writeln!(f, "_(Indented items show summaries for the last 5 modifications.)_")?;
            }
            for (file, reasonings, total_count) in &self.files_modified {
                if reasonings.is_empty() {
                    writeln!(f, "* {file}")?;
                } else if reasonings.len() == 1 {
                    writeln!(f, "* {file} - {}", reasonings[0])?;
                } else {
                    if *total_count > reasonings.len() {
                        writeln!(
                            f,
                            "* {file} ({} modifications, showing summaries for last {})",
                            total_count,
                            reasonings.len()
                        )?;
                    } else {
                        writeln!(f, "* {file} ({total_count} modifications)")?;
                    }
                    for reasoning in reasonings {
                        writeln!(f, "  - {reasoning}")?;
                    }
                }
            }
            writeln!(f)?;
        }

        if !self.commands_executed.is_empty() {
            writeln!(f, "### Commands Executed")?;
            if self.total_commands > self.commands_executed.len() {
                writeln!(
                    f,
                    "_(Showing {} of {} commands by importance)_",
                    self.commands_executed.len(),
                    self.total_commands
                )?;
            }
            for (cmd, reasonings, _total_count) in &self.commands_executed {
                let display_cmd = if cmd.len() > 60 {
                    let truncated: String = cmd.chars().take(57).collect();
                    format!("{truncated}...")
                } else {
                    cmd.clone()
                };
                // Only show the most recent reasoning for commands
                if let Some(reasoning) = reasonings.last() {
                    writeln!(f, "* {display_cmd} - {reasoning}")?;
                } else {
                    writeln!(f, "* {display_cmd}")?;
                }
            }
        }

        Ok(())
    }
}

/// Internal struct for scoring items during selection.
struct ItemScore {
    item: String,
    reasonings: Vec<String>,
    total_count: usize,
    score: f64,
}

/// Truncates a file path to maximum length, preserving start and end.
fn truncate_path(path: &str) -> String {
    let chars: Vec<char> = path.chars().collect();
    if chars.len() <= MAX_FILE_PATH_LENGTH {
        path.to_string()
    } else {
        // 45 + "..." (3) + 45 = 93 chars
        let first: String = chars.iter().take(45).collect();
        let last: String = chars.iter().skip(chars.len() - 45).collect();
        format!("{first}...{last}")
    }
}

/// Calculates the character size of an item for budget tracking.
fn calculate_item_size(item: &str, reasonings: &[String]) -> usize {
    item.len() + reasonings.iter().map(|r| r.len()).sum::<usize>() + 50
}

fn select_important_items(
    history: &VecDeque<HistoryEntry>,
    exclude_last_n: usize,
    max_items: usize,
    char_budget: usize,
    tool_filter: impl Fn(&str) -> bool,
    extract_item: impl Fn(&serde_json::Value) -> Option<String>,
) -> (Vec<(String, Vec<String>, usize)>, usize) {
    // Track: (frequency, position, reasonings)
    let mut item_data: HashMap<String, (usize, usize, Vec<String>)> = HashMap::new();
    let end_idx = history.len().saturating_sub(exclude_last_n);

    for (position, entry) in history.iter().take(end_idx).enumerate() {
        if let AssistantMessage::ToolUse { tool_uses, .. } = &entry.assistant {
            for tool in tool_uses {
                if tool_filter(&tool.name) {
                    if let Some(item) = extract_item(&tool.args) {
                        // Extract reasoning from tool's summary parameter
                        let reasoning = tool
                            .args
                            .get("summary")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .trim()
                            .chars()
                            .take(100)
                            .collect::<String>();

                        item_data
                            .entry(item)
                            .and_modify(|(freq, pos, reasonings)| {
                                *freq += 1;
                                *pos = position;
                                // Keep only last N reasonings
                                if !reasoning.is_empty() {
                                    reasonings.push(reasoning.clone());
                                    if reasonings.len() > MAX_REASONINGS_PER_ITEM {
                                        reasonings.remove(0);
                                    }
                                }
                            })
                            .or_insert_with(|| {
                                let reasonings = if reasoning.is_empty() {
                                    Vec::new()
                                } else {
                                    vec![reasoning]
                                };
                                (1, position, reasonings)
                            });
                    }
                }
            }
        }
    }

    let total_items = item_data.len();

    let mut scored: Vec<ItemScore> = item_data
        .into_iter()
        .map(|(item, (frequency, position, reasonings))| {
            let freq_score = frequency as f64;
            let recency_score = position as f64 / end_idx.max(1) as f64;
            ItemScore {
                item,
                reasonings,
                total_count: frequency,
                score: (freq_score * FREQUENCY_WEIGHT) + (recency_score * RECENCY_WEIGHT),
            }
        })
        .collect();

    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    let mut char_count = 0;
    let mut selected = Vec::new();

    for item in scored.into_iter().take(max_items) {
        let item_size = calculate_item_size(&item.item, &item.reasonings);

        if char_count + item_size <= char_budget {
            char_count += item_size;
            selected.push((item.item, item.reasonings, item.total_count));
        } else {
            break;
        }
    }

    (selected, total_items)
}

fn select_important_files(
    history: &VecDeque<HistoryEntry>,
    exclude_last_n: usize,
    max_files: usize,
) -> (Vec<(String, Vec<String>, usize)>, usize) {
    select_important_items(
        history,
        exclude_last_n,
        max_files,
        FILES_CHAR_BUDGET,
        |name| name == ToolMetadata::FS_WRITE.spec_name,
        |args| args.get("path").and_then(|v| v.as_str()).map(truncate_path),
    )
}

fn select_important_commands(
    history: &VecDeque<HistoryEntry>,
    exclude_last_n: usize,
    max_commands: usize,
) -> (Vec<(String, Vec<String>, usize)>, usize) {
    select_important_items(
        history,
        exclude_last_n,
        max_commands,
        COMMANDS_CHAR_BUDGET,
        |name| name == ToolMetadata::EXECUTE_COMMAND.spec_name,
        |args| args.get("command").and_then(|v| v.as_str()).map(String::from),
    )
}

/// Extracts the most important files and commands from conversation history.
///
/// Uses weighted scoring (frequency × 2.0 + recency × 1.0) to select:
/// - Top 30 files modified via `fs_write` tool
/// - Top 20 commands executed via `execute_bash` tool
///
/// For each item, stores up to 5 most recent reasoning summaries from the tool's
/// `summary` parameter, providing context about why the file was modified or
/// command was executed.
///
/// # Arguments
/// * `history` - The conversation history to extract from
/// * `exclude_last_n` - Number of recent messages to exclude from extraction
///
/// # Returns
/// A `CompactionFacts` struct containing the selected items with their reasonings
/// and total counts before truncation.
pub(crate) fn extract_compaction_facts(history: &VecDeque<HistoryEntry>, exclude_last_n: usize) -> CompactionFacts {
    let (files_modified, total_files) = select_important_files(history, exclude_last_n, MAX_FILES_IN_FACTUAL_RECORD);
    let (commands_executed, total_commands) =
        select_important_commands(history, exclude_last_n, MAX_COMMANDS_IN_FACTUAL_RECORD);

    CompactionFacts {
        files_modified,
        commands_executed,
        total_files,
        total_commands,
    }
}
