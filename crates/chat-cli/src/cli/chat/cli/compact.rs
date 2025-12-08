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
/// Weight applied to file writes when calculating frequency score.
const WRITE_FREQUENCY_WEIGHT: f64 = 2.0;
/// Weight applied to file reads when calculating frequency score.
const READ_FREQUENCY_WEIGHT: f64 = 1.0;
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
/// - `files_accessed`: Vector of (file_path, reasonings, write_count, read_count) tuples for files
///   accessed via `write` or `read` tools
/// - `commands_executed`: Vector of (command_text, reasonings, total_count) tuples for commands run
///   via `shell` or `aws` tools
/// - `total_files`: Total number of unique files in history (before truncation)
/// - `total_commands`: Total number of unique commands in history (before truncation)
#[derive(Debug, Default)]
pub(crate) struct CompactionFacts {
    files_accessed: Vec<FileEntry>,
    commands_executed: Vec<(String, Vec<String>, usize)>,
    total_files: usize,
    total_commands: usize,
}

impl fmt::Display for CompactionFacts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.files_accessed.is_empty() {
            writeln!(f, "### Files Modified and Read")?;
            if self.total_files > self.files_accessed.len() {
                writeln!(
                    f,
                    "_(Showing {} of {} files, most frequently and recently accessed first. Writes weighted 2x more than reads. Indented items show summaries for the last 5 modifications.)_",
                    self.files_accessed.len(),
                    self.total_files
                )?;
            } else {
                writeln!(
                    f,
                    "_(Most frequently and recently accessed first. Indented items show summaries for the last 5 modifications.)_"
                )?;
            }
            for (file, reasonings, write_count, read_count) in &self.files_accessed {
                if reasonings.is_empty() {
                    writeln!(f, "* {file}")?;
                } else if reasonings.len() == 1 {
                    writeln!(f, "* {file} - {}", reasonings[0])?;
                } else {
                    // Format count display with proper pluralization
                    let count_display = match (*write_count, *read_count) {
                        (0, 1) => "1 read".to_string(),
                        (0, r) => format!("{r} reads"),
                        (1, 0) => "1 modification".to_string(),
                        (w, 0) => format!("{w} modifications"),
                        (1, 1) => "1 modification, 1 read".to_string(),
                        (1, r) => format!("1 modification, {r} reads"),
                        (w, 1) => format!("{w} modifications, 1 read"),
                        (w, r) => format!("{w} modifications, {r} reads"),
                    };

                    // Only show truncation message if we've truncated modification summaries
                    if *write_count > reasonings.len() {
                        let summary_word = if reasonings.len() == 1 {
                            "modification"
                        } else {
                            "modifications"
                        };
                        writeln!(
                            f,
                            "* {file} ({count_display}, showing summaries for last {} {})",
                            reasonings.len(),
                            summary_word
                        )?;
                    } else {
                        writeln!(f, "* {file} ({count_display})")?;
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
                    "_(Showing {} of {} commands, most frequently and recently executed first)_",
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

/// Internal struct for scoring files during selection.
struct FileScore {
    path: String,
    reasonings: Vec<String>,
    write_count: usize,
    read_count: usize,
    score: f64,
}

/// Internal struct for scoring commands during selection.
struct CommandScore {
    command: String,
    reasonings: Vec<String>,
    frequency: usize,
    score: f64,
}

/// Type alias for file entry: (path, reasonings, write_count, read_count)
type FileEntry = (String, Vec<String>, usize, usize);

/// Truncates a file path to maximum length, preserving start and end.
/// This ensures paths don't consume excessive tokens in the factual record.
/// Format: first_45_chars + "..." + last_45_chars = 93 total chars
fn truncate_path(path: &str) -> String {
    let chars: Vec<char> = path.chars().collect();
    if chars.len() <= MAX_FILE_PATH_LENGTH {
        path.to_string()
    } else {
        // Take first 45 and last 45 characters, join with "..."
        let first: String = chars.iter().take(45).collect();
        let last: String = chars.iter().skip(chars.len() - 45).collect();
        format!("{first}...{last}")
    }
}

/// Calculates the character size of an item for budget tracking.
/// Includes item text + all reasoning texts + 50 char overhead for formatting.
fn calculate_item_size(item: &str, reasonings: &[String]) -> usize {
    item.len() + reasonings.iter().map(|r| r.len()).sum::<usize>() + 50
}

/// Scans history and collects item data using provided filter and extractor.
///
/// Generic scanning function that can extract any type of tool usage from history.
/// Used by select_important_items for commands.
///
/// # Returns
/// HashMap mapping items to (frequency, last_position, reasonings)
fn scan_items(
    history: &VecDeque<HistoryEntry>,
    end_idx: usize,
    tool_filter: impl Fn(&str) -> bool,
    extract_item: impl Fn(&serde_json::Value) -> Option<String>,
) -> HashMap<String, (usize, usize, Vec<String>)> {
    let mut item_data: HashMap<String, (usize, usize, Vec<String>)> = HashMap::new();

    for (position, entry) in history.iter().take(end_idx).enumerate() {
        if let AssistantMessage::ToolUse { tool_uses, .. } = &entry.assistant {
            for tool in tool_uses {
                if tool_filter(&tool.name) {
                    if let Some(item) = extract_item(&tool.args) {
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

    item_data
}

/// Generic function to select important items from history using weighted scoring.
///
/// This is a flexible helper used by `select_important_commands()` to extract
/// and score any type of tool usage from conversation history.
///
/// # Algorithm
/// 1. **Scan history**: Iterate through history entries using provided filter and extractor
/// 2. **Score and sort**: Transform HashMap to scored Vec and sort by score descending
/// 3. **Select**: Pick top items respecting max count and character budget
///
/// Note: Only step 1 iterates through full history (expensive). Steps 2-3 operate on
/// small collections of unique items (typically 20-50 items).
///
/// # Scoring Formula
/// ```text
/// frequency = number of times item appears
/// recency = last_position / total_positions  (range: 0.0 to 1.0)
/// score = (frequency × 2.0) + (recency × 1.0)
/// ```
///
/// **Example:** Command executed 5 times, last at position 90/100:
/// - score = (5 × 2.0) + (0.9 × 1.0) = 10.9
///
/// # Parameters
/// - `tool_filter`: Closure to check if a tool name matches (e.g., "execute_bash")
/// - `extract_item`: Closure to extract the item string from tool args (e.g., command text)
/// - `char_budget`: Maximum characters allowed for all selected items
///
/// # Reasoning Storage
/// - Stores up to 5 most recent reasoning summaries per item
/// - Reasonings extracted from tool's "summary" parameter
/// - Truncated to 100 characters each
///
/// # Returns
/// Tuple of (selected_items, total_unique_items) where:
/// - selected_items: Vec of (item_string, reasonings, frequency)
/// - total_unique_items: Count before selection (for "showing X of Y" display)
fn select_important_items(
    history: &VecDeque<HistoryEntry>,
    exclude_last_n: usize,
    max_items: usize,
    char_budget: usize,
    tool_filter: impl Fn(&str) -> bool,
    extract_item: impl Fn(&serde_json::Value) -> Option<String>,
) -> (Vec<(String, Vec<String>, usize)>, usize) {
    let end_idx = history.len().saturating_sub(exclude_last_n);

    // Step 1: Scan history and collect all matching items
    let item_data = scan_items(history, end_idx, tool_filter, extract_item);
    let total_items = item_data.len();

    // Step 2: Score and sort items by frequency + recency
    let mut scored: Vec<CommandScore> = item_data
        .into_iter()
        .map(|(command, (frequency, position, reasonings))| {
            let freq_score = frequency as f64;
            let recency_score = position as f64 / end_idx.max(1) as f64;
            CommandScore {
                command,
                reasonings,
                frequency,
                score: (freq_score * FREQUENCY_WEIGHT) + (recency_score * RECENCY_WEIGHT),
            }
        })
        .collect();

    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // Step 3: Select items up to max count and character budget
    let mut char_count = 0;
    let mut selected = Vec::new();

    for item in scored.into_iter().take(max_items) {
        let item_size = calculate_item_size(&item.command, &item.reasonings);

        if char_count + item_size <= char_budget {
            char_count += item_size;
            selected.push((item.command, item.reasonings, item.frequency));
        } else {
            break;
        }
    }

    (selected, total_items)
}

/// Extracts file path from fs_read tool arguments.
/// Handles the operations array structure with different modes (Line, Directory, Search).
/// Returns the first path found, or None if no path can be extracted.
fn extract_fs_read_path(args: &serde_json::Value) -> Option<String> {
    // fs_read has structure: { "operations": [{ "mode": "Line", "path": "..." }] }
    let operations = args.get("operations")?.as_array()?;
    let first_op = operations.first()?;

    // Try to get "path" field (works for Line, Directory, Search modes)
    // Skip Image mode which has "image_paths" array
    first_op.get("path")?.as_str().map(String::from)
}

/// Scans history and collects file access data into a HashMap.
///
/// Iterates through history entries and tracks all read and write tool operations,
/// accumulating counts, positions, and reasoning summaries for each unique file path.
///
/// # Returns
/// HashMap mapping file paths to (write_count, read_count, last_position, reasonings)
fn scan_file_accesses(
    history: &VecDeque<HistoryEntry>,
    end_idx: usize,
) -> HashMap<String, (usize, usize, usize, Vec<String>)> {
    let mut file_data: HashMap<String, (usize, usize, usize, Vec<String>)> = HashMap::new();

    for (position, entry) in history.iter().take(end_idx).enumerate() {
        if let AssistantMessage::ToolUse { tool_uses, .. } = &entry.assistant {
            for tool in tool_uses {
                let is_write = tool.name == ToolMetadata::FS_WRITE.spec_name;
                let is_read = tool.name == ToolMetadata::FS_READ.spec_name;

                if is_write || is_read {
                    // Extract path based on tool type
                    let path_opt = if is_write {
                        // fs_write: path is directly in args
                        tool.args.get("path").and_then(|v| v.as_str()).map(String::from)
                    } else {
                        // fs_read: path is in operations array
                        extract_fs_read_path(&tool.args)
                    };

                    if let Some(path) = path_opt {
                        let reasoning = tool
                            .args
                            .get("summary")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .trim()
                            .chars()
                            .take(100)
                            .collect::<String>();

                        file_data
                            .entry(path)
                            .and_modify(|(write_cnt, read_cnt, pos, reasonings)| {
                                if is_write {
                                    *write_cnt += 1;
                                    if !reasoning.is_empty() {
                                        reasonings.push(reasoning.clone());
                                        if reasonings.len() > MAX_REASONINGS_PER_ITEM {
                                            reasonings.remove(0);
                                        }
                                    }
                                } else {
                                    *read_cnt += 1;
                                }
                                *pos = position;
                            })
                            .or_insert_with(|| {
                                let (w, r) = if is_write { (1, 0) } else { (0, 1) };
                                let reasonings = if reasoning.is_empty() || is_read {
                                    Vec::new()
                                } else {
                                    vec![reasoning]
                                };
                                (w, r, position, reasonings)
                            });
                    }
                }
            }
        }
    }

    file_data
}

/// Selects the most important files from conversation history using weighted scoring.
///
/// # Algorithm
/// 1. **Scan history**: Iterate through history entries and collect all file accesses into HashMap
/// 2. **Score and sort**: Transform HashMap to scored Vec and sort by score descending
/// 3. **Select**: Pick top files respecting max count and character budget
///
/// Note: Only step 1 iterates through full history (expensive). Steps 2-3 operate on
/// small collections of unique files (typically 30-100 items).
///
/// # Scoring Formula
/// Files with both reads and writes are scored using:
/// ```text
/// weighted_freq = (write_count × 2.0) + (read_count × 1.0)
/// recency = last_position / total_positions  (range: 0.0 to 1.0)
/// score = (weighted_freq × 2.0) + (recency × 1.0)
/// ```
///
/// **Example:** File with 3 writes, 2 reads, last accessed at position 80/100:
/// - weighted_freq = (3 × 2.0) + (2 × 1.0) = 8.0
/// - recency = 80/100 = 0.8
/// - score = (8.0 × 2.0) + (0.8 × 1.0) = 16.8
///
/// # Reasoning Storage
/// - Only **write operations** store reasoning summaries (up to 5 most recent)
/// - Read operations contribute to scoring but not to reasoning display
/// - Reasonings are truncated to 100 characters each
///
/// # Selection Limits
/// - Maximum 30 files (MAX_FILES_IN_FACTUAL_RECORD)
/// - Character budget: 14,000 chars (FILES_CHAR_BUDGET)
/// - Selection stops when either limit is reached
///
/// # Path Handling
/// - Original paths used for uniqueness checking (prevents merging different files)
/// - Paths truncated to 100 chars only for display and budget calculation
/// - Truncation format: first_45_chars + "..." + last_45_chars
///
/// # Returns
/// Tuple of (selected_files, total_unique_files) where:
/// - selected_files: Vec of (path, reasonings, write_count, read_count)
/// - total_unique_files: Count before selection (for "showing X of Y" display)
fn select_important_files(
    history: &VecDeque<HistoryEntry>,
    exclude_last_n: usize,
    max_files: usize,
) -> (Vec<FileEntry>, usize) {
    let end_idx = history.len().saturating_sub(exclude_last_n);

    // Step 1: Scan history and collect all file accesses
    let file_data = scan_file_accesses(history, end_idx);
    let total_files = file_data.len();

    // Step 2: Score and sort files by weighted frequency + recency
    let mut scored: Vec<FileScore> = file_data
        .into_iter()
        .map(|(path, (write_count, read_count, position, reasonings))| {
            let weighted_freq =
                (write_count as f64 * WRITE_FREQUENCY_WEIGHT) + (read_count as f64 * READ_FREQUENCY_WEIGHT);
            let recency_score = position as f64 / end_idx.max(1) as f64;
            FileScore {
                path,
                reasonings,
                write_count,
                read_count,
                score: (weighted_freq * FREQUENCY_WEIGHT) + (recency_score * RECENCY_WEIGHT),
            }
        })
        .collect();

    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // Step 3: Select files up to max count and character budget
    let mut char_count = 0;
    let mut selected = Vec::new();

    for file in scored.into_iter().take(max_files) {
        let truncated_path = truncate_path(&file.path);
        let item_size = calculate_item_size(&truncated_path, &file.reasonings);

        if char_count + item_size <= FILES_CHAR_BUDGET {
            char_count += item_size;
            selected.push((truncated_path, file.reasonings, file.write_count, file.read_count));
        } else {
            break;
        }
    }

    (selected, total_files)
}

/// Selects the most important commands from conversation history using weighted scoring.
///
/// Wrapper around `select_important_items()` configured for shell and aws commands.
///
/// # Scoring Formula
/// ```text
/// frequency = number of times command was executed
/// recency = last_position / total_positions  (range: 0.0 to 1.0)
/// score = (frequency × 2.0) + (recency × 1.0)
/// ```
///
/// **Example:** Command executed 5 times, last at position 90/100:
/// - score = (5 × 2.0) + (0.9 × 1.0) = 10.9
///
/// # Selection Limits
/// - Maximum 20 commands (MAX_COMMANDS_IN_FACTUAL_RECORD)
/// - Character budget: 6,000 chars (COMMANDS_CHAR_BUDGET)
/// - Selection stops when either limit is reached
///
/// # Reasoning Storage
/// - Stores up to 5 most recent reasoning summaries per command
/// - Reasonings extracted from tool's "summary" parameter
/// - Truncated to 100 characters each
///
/// # Returns
/// Tuple of (selected_commands, total_unique_commands) where:
/// - selected_commands: Vec of (command_text, reasonings, frequency)
/// - total_unique_commands: Count before selection (for "showing X of Y" display)
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
        // Filter: include both execute_bash and use_aws tools
        |name| name == ToolMetadata::EXECUTE_COMMAND.spec_name || name == ToolMetadata::USE_AWS.spec_name,
        // Extractor: get command string based on tool type
        |args| {
            // For execute_bash: extract "command" field
            if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                return Some(cmd.to_string());
            }
            // For use_aws: construct "aws {service} {operation}"
            if let (Some(service), Some(operation)) = (
                args.get("service_name").and_then(|v| v.as_str()),
                args.get("operation_name").and_then(|v| v.as_str()),
            ) {
                return Some(format!("aws {service} {operation}"));
            }
            None
        },
    )
}

/// Extracts the most important files and commands from conversation history.
///
/// Selects top 30 files (read/write tools) and top 20 commands (shell/aws tools)
/// using weighted scoring that prioritizes frequency and recency.
///
/// See `select_important_files()` and `select_important_commands()` for detailed
/// scoring formulas and selection logic.
///
/// # Arguments
/// * `history` - The conversation history to extract from
/// * `exclude_last_n` - Number of recent messages to exclude from extraction
///
/// # Returns
/// A `CompactionFacts` struct containing selected files and commands with their
/// reasonings, access counts, and totals before selection.
pub(crate) fn extract_compaction_facts(history: &VecDeque<HistoryEntry>, exclude_last_n: usize) -> CompactionFacts {
    // Extract top files (with read/write tracking)
    let (files_accessed, total_files) = select_important_files(history, exclude_last_n, MAX_FILES_IN_FACTUAL_RECORD);
    // Extract top commands (with frequency tracking)
    let (commands_executed, total_commands) =
        select_important_commands(history, exclude_last_n, MAX_COMMANDS_IN_FACTUAL_RECORD);

    CompactionFacts {
        files_accessed,
        commands_executed,
        total_files,
        total_commands,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::chat::message::{
        AssistantToolUse,
        UserMessage,
    };

    /// Creates a test entry with fs_write using the EXACT structure from real tool output
    fn create_fs_write_entry(path: &str, summary: &str) -> HistoryEntry {
        let args = serde_json::json!({
            "path": path,
            "summary": summary,
            "command": "append",
            "new_str": "test"
        });

        HistoryEntry {
            user: UserMessage::new_prompt("test".to_string(), None),
            assistant: AssistantMessage::ToolUse {
                message_id: None,
                content: String::new(),
                tool_uses: vec![AssistantToolUse {
                    id: "test".to_string(),
                    name: "fs_write".to_string(),
                    orig_name: "fs_write".to_string(),
                    args: args.clone(),
                    orig_args: args,
                }],
            },
            request_metadata: None,
        }
    }

    /// Creates a test entry with fs_read using the EXACT structure from real tool output
    fn create_fs_read_entry(path: &str) -> HistoryEntry {
        let args = serde_json::json!({
            "operations": [{
                "mode": "Line",
                "path": path
            }]
        });

        HistoryEntry {
            user: UserMessage::new_prompt("test".to_string(), None),
            assistant: AssistantMessage::ToolUse {
                message_id: None,
                content: String::new(),
                tool_uses: vec![AssistantToolUse {
                    id: "test".to_string(),
                    name: "fs_read".to_string(),
                    orig_name: "fs_read".to_string(),
                    args: args.clone(),
                    orig_args: args,
                }],
            },
            request_metadata: None,
        }
    }

    #[test]
    fn test_truncate_path_short() {
        let path = "/short/path.rs";
        assert_eq!(truncate_path(path), path);
    }

    #[test]
    fn test_truncate_path_long() {
        let path = "/very/long/path/that/definitely/exceeds/one/hundred/characters/and/absolutely/needs/to/be/truncated/properly/file.rs";
        let result = truncate_path(path);
        let result_len = result.chars().count();
        assert!(result_len <= 100, "Expected ≤100 chars, got {}", result_len);
        assert!(result.contains("..."));
        assert!(result.starts_with("/very/long/path"));
        assert!(result.ends_with("file.rs"));
    }

    #[test]
    fn test_truncate_path_utf8() {
        let path = "/path/with/émojis/🎉/and/spëcial/chàracters/that/is/very/long/and/needs/truncation/file.rs";
        let result = truncate_path(path);
        assert!(result.len() <= 100);
        // Should not panic on UTF-8 boundaries
    }

    #[test]
    fn test_writes_prioritized_over_reads() {
        let mut history = VecDeque::new();

        // File A: 1 write (weighted freq = 1 * 2.0 = 2.0)
        history.push_back(create_fs_write_entry("/file_a.rs", "Writing A"));

        // File B: 3 reads (weighted freq = 3 * 1.0 = 3.0)
        // Even though File B has higher weighted freq, File A should still be included
        for _ in 0..3 {
            history.push_back(create_fs_read_entry("/file_b.rs"));
        }

        let facts = extract_compaction_facts(&history, 0);

        // Both files should be present
        assert_eq!(facts.files_accessed.len(), 2);

        // File B should score higher due to frequency (3.0 vs 2.0)
        // but File A should have write_count > 0
        let file_a = facts
            .files_accessed
            .iter()
            .find(|(p, _, _, _)| p == "/file_a.rs")
            .unwrap();
        assert_eq!(file_a.2, 1); // write_count
        assert_eq!(file_a.3, 0); // read_count

        let file_b = facts
            .files_accessed
            .iter()
            .find(|(p, _, _, _)| p == "/file_b.rs")
            .unwrap();
        assert_eq!(file_b.2, 0); // write_count
        assert_eq!(file_b.3, 3); // read_count
    }

    #[test]
    fn test_read_and_write_same_file() {
        let mut history = VecDeque::new();

        // Same file: 2 reads, 3 writes
        history.push_back(create_fs_read_entry("/file.rs"));
        history.push_back(create_fs_write_entry("/file.rs", "Writing 1"));
        history.push_back(create_fs_read_entry("/file.rs"));
        history.push_back(create_fs_write_entry("/file.rs", "Writing 2"));
        history.push_back(create_fs_write_entry("/file.rs", "Writing 3"));

        let facts = extract_compaction_facts(&history, 0);

        assert_eq!(facts.files_accessed.len(), 1);
        assert_eq!(facts.files_accessed[0].0, "/file.rs");
        assert_eq!(facts.files_accessed[0].2, 3); // write_count
        assert_eq!(facts.files_accessed[0].3, 2); // read_count
        assert_eq!(facts.files_accessed[0].1.len(), 3); // 3 reasonings from writes
    }

    #[test]
    fn test_read_only_file() {
        let mut history = VecDeque::new();

        for _ in 0..3 {
            history.push_back(create_fs_read_entry("/readonly.rs"));
        }

        let facts = extract_compaction_facts(&history, 0);

        assert_eq!(facts.files_accessed.len(), 1);
        assert_eq!(facts.files_accessed[0].0, "/readonly.rs");
        assert_eq!(facts.files_accessed[0].2, 0); // write_count
        assert_eq!(facts.files_accessed[0].3, 3); // read_count
        assert_eq!(facts.files_accessed[0].1.len(), 0); // no reasonings for reads
    }

    #[test]
    fn test_max_reasonings_limit() {
        let mut history = VecDeque::new();

        // Add 10 writes to same file
        for i in 0..10 {
            history.push_back(create_fs_write_entry("/file.rs", &format!("Write {}", i)));
        }

        let facts = extract_compaction_facts(&history, 0);

        assert_eq!(facts.files_accessed.len(), 1);
        assert_eq!(facts.files_accessed[0].2, 10); // write_count = 10
        assert_eq!(facts.files_accessed[0].1.len(), 5); // but only 5 reasonings stored
    }

    #[test]
    fn test_budget_limiting() {
        let mut history = VecDeque::new();

        // Add many files with very long paths and reasonings to exceed budget
        for i in 0..100 {
            let path = format!("/path/with/reasonings/file_{:03}.rs", i);
            // Add 5 writes per file with long reasonings
            for j in 0..5 {
                let reasoning = format!(
                    "This is a very detailed reasoning text that explains what we did in this operation number {}",
                    j
                );
                history.push_back(create_fs_write_entry(&path, &reasoning));
            }
        }

        let facts = extract_compaction_facts(&history, 0);

        // Should stop before 30 due to budget (each file has 5 long reasonings)
        assert!(
            facts.files_accessed.len() < 30,
            "Expected < 30 files due to budget, got {}",
            facts.files_accessed.len()
        );
        assert_eq!(facts.total_files, 100);
    }

    #[test]
    fn test_empty_history() {
        let history = VecDeque::new();
        let facts = extract_compaction_facts(&history, 0);

        assert_eq!(facts.files_accessed.len(), 0);
        assert_eq!(facts.commands_executed.len(), 0);
        assert_eq!(facts.total_files, 0);
        assert_eq!(facts.total_commands, 0);
    }

    #[test]
    fn test_path_truncation_preserves_uniqueness() {
        let mut history = VecDeque::new();

        // Two different files with long paths that truncate to the same display value
        // Both paths are >100 chars and differ only in the middle (which gets replaced by "...")
        // This tests that we use original paths for uniqueness, not truncated paths
        let path1 =
            "/very/long/path/with/many/nested/directories/AAAAA/that/definitely/exceeds/one/hundred/characters/file.rs";
        let path2 =
            "/very/long/path/with/many/nested/directories/BBBBB/that/definitely/exceeds/one/hundred/characters/file.rs";

        // Write to both files multiple times
        history.push_back(create_fs_write_entry(path1, "Writing to file 1"));
        history.push_back(create_fs_write_entry(path1, "Writing to file 1 again"));
        history.push_back(create_fs_write_entry(path2, "Writing to file 2"));

        let facts = extract_compaction_facts(&history, 0);

        // Should have 2 distinct files, not 1 merged file
        // This is the key test: even though truncated paths are identical,
        // we should track them as separate files because original paths differ
        assert_eq!(
            facts.files_accessed.len(),
            2,
            "Expected 2 unique files despite identical truncated paths"
        );
        assert_eq!(facts.total_files, 2);

        // One file should have 2 writes, the other should have 1 write
        let write_counts: Vec<usize> = facts.files_accessed.iter().map(|(_, _, w, _)| *w).collect();
        assert!(write_counts.contains(&2), "One file should have 2 writes");
        assert!(write_counts.contains(&1), "One file should have 1 write");

        // Note: The truncated display paths WILL be identical, but that's OK
        // because we tracked them separately during collection
    }

    #[test]
    fn test_display_format() {
        let mut history = VecDeque::new();

        // File with both reads and writes
        history.push_back(create_fs_write_entry("/file.rs", "Write 1"));
        history.push_back(create_fs_write_entry("/file.rs", "Write 2"));
        history.push_back(create_fs_read_entry("/file.rs"));

        let facts = extract_compaction_facts(&history, 0);
        let display = facts.to_string();

        assert!(display.contains("Files Modified and Read"));
        assert!(display.contains("2 modifications, 1 read"));
    }
}
