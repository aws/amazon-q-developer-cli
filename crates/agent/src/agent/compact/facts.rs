//! Factual record extraction from conversation history during compaction.
//!
//! Extracts files accessed and commands executed from tool uses to preserve
//! critical context that might be lost in LLM summarization.

use std::collections::HashMap;
use std::fmt;

use crate::agent::agent_config::parse::CanonicalToolName;
use crate::agent::agent_loop::types::Message;
use crate::agent::tools::{
    BuiltInTool,
    BuiltInToolName,
    Tool,
    ToolKind,
};
use crate::agent::util::truncate_safe;

/// Maximum number of files to include in the factual record.
const MAX_FILES: usize = 30;
/// Maximum number of commands to include in the factual record.
const MAX_COMMANDS: usize = 20;
/// Maximum number of reasoning summaries to store per item.
const MAX_REASONINGS: usize = 5;
/// Maximum character length for file paths before truncation.
const MAX_PATH_LEN: usize = 100;
/// Maximum character length for command display before truncation.
const MAX_CMD_LEN: usize = 150;

/// A file entry in the factual record.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: String,
    pub reasonings: Vec<String>,
    pub write_count: usize,
    pub read_count: usize,
}

/// A command entry in the factual record.
#[derive(Debug, Clone)]
pub struct CommandEntry {
    pub command: String,
    pub reasonings: Vec<String>,
    pub count: usize,
}

/// Factual information extracted from conversation history during compaction.
#[derive(Debug, Default)]
pub struct CompactionFacts {
    pub files_accessed: Vec<FileEntry>,
    pub commands_executed: Vec<CommandEntry>,
    pub total_files: usize,
    pub total_commands: usize,
}

impl CompactionFacts {
    /// Returns true if there are no facts to display.
    pub fn is_empty(&self) -> bool {
        self.files_accessed.is_empty() && self.commands_executed.is_empty()
    }
}

impl fmt::Display for CompactionFacts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.files_accessed.is_empty() {
            writeln!(f, "### Files Modified and Read")?;
            if self.total_files > self.files_accessed.len() {
                writeln!(
                    f,
                    "_(Showing {} of {} files, most frequently and recently accessed first. Indented items show summaries for the last {} modifications.)_",
                    self.files_accessed.len(),
                    self.total_files,
                    MAX_REASONINGS
                )?;
            } else {
                writeln!(
                    f,
                    "_(Most frequently and recently accessed first. Indented items show summaries for the last {} modifications.)_",
                    MAX_REASONINGS
                )?;
            }
            for entry in &self.files_accessed {
                if entry.reasonings.is_empty() {
                    writeln!(f, "* {}", entry.path)?;
                } else if entry.reasonings.len() == 1 {
                    writeln!(f, "* {} - {}", entry.path, entry.reasonings[0])?;
                } else {
                    let count_display = format_counts(entry.write_count, entry.read_count);
                    if entry.write_count > entry.reasonings.len() {
                        writeln!(
                            f,
                            "* {} ({count_display}, showing summaries for last {} modifications)",
                            entry.path,
                            entry.reasonings.len()
                        )?;
                    } else {
                        writeln!(f, "* {} ({count_display})", entry.path)?;
                    }
                    for reasoning in &entry.reasonings {
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
            for entry in &self.commands_executed {
                let display_cmd = truncate_cmd(&entry.command);
                if let Some(reasoning) = entry.reasonings.last() {
                    writeln!(f, "* {display_cmd} - {reasoning}")?;
                } else {
                    writeln!(f, "* {display_cmd}")?;
                }
            }
        }

        Ok(())
    }
}

fn format_counts(writes: usize, reads: usize) -> String {
    match (writes, reads) {
        (0, 1) => "1 read".into(),
        (0, r) => format!("{r} reads"),
        (1, 0) => "1 modification".into(),
        (w, 0) => format!("{w} modifications"),
        (1, 1) => "1 modification, 1 read".into(),
        (1, r) => format!("1 modification, {r} reads"),
        (w, 1) => format!("{w} modifications, 1 read"),
        (w, r) => format!("{w} modifications, {r} reads"),
    }
}

fn truncate_cmd(s: &str) -> String {
    if s.len() <= MAX_CMD_LEN {
        s.to_string()
    } else {
        format!("{}...", truncate_safe(s, MAX_CMD_LEN))
    }
}

fn truncate_path(path: &str) -> String {
    if path.chars().count() <= MAX_PATH_LEN {
        path.to_string()
    } else {
        let chars: Vec<char> = path.chars().collect();
        let first: String = chars.iter().take(45).collect();
        let last: String = chars.iter().skip(chars.len() - 45).collect();
        format!("{first}...{last}")
    }
}

fn add_reasoning(reasonings: &mut Vec<String>, reasoning: Option<&str>) {
    if let Some(r) = reasoning {
        let trimmed: String = r.trim().chars().take(100).collect();
        if !trimmed.is_empty() {
            reasonings.push(trimmed);
            if reasonings.len() > MAX_REASONINGS {
                reasonings.remove(0);
            }
        }
    }
}

/// Selects the most important files from conversation history using weighted scoring.
///
/// # Algorithm
/// 1. **Scan history**: Iterate through messages and collect all file accesses into HashMap
/// 2. **Score and sort**: Transform HashMap to scored Vec and sort by score descending
/// 3. **Select**: Pick top files up to MAX_FILES
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
/// # Path Handling
/// - Original paths used for uniqueness checking (prevents merging different files)
/// - Paths truncated to 100 chars only for display
/// - Truncation format: first_45_chars + "..." + last_45_chars
///
/// # Returns
/// Tuple of (selected_files, total_unique_files)
fn extract_files(messages: &[Message]) -> (Vec<FileEntry>, usize) {
    // file_path -> (write_count, read_count, last_position, reasonings)
    let mut file_data: HashMap<String, (usize, usize, usize, Vec<String>)> = HashMap::new();

    for (pos, msg) in messages.iter().enumerate() {
        for tool_use in msg.tool_uses_iter() {
            let Ok(builtin_name) = tool_use.name.parse::<BuiltInToolName>() else {
                continue;
            };
            let canonical = CanonicalToolName::BuiltIn(builtin_name);
            let Ok(tool) = Tool::parse(&canonical, tool_use.input.clone()) else {
                continue;
            };

            match tool.kind() {
                ToolKind::BuiltIn(BuiltInTool::FileWrite(fw)) => {
                    let entry = file_data.entry(fw.path().to_string()).or_insert((0, 0, 0, Vec::new()));
                    entry.0 += 1;
                    entry.2 = pos;
                    add_reasoning(&mut entry.3, tool.tool_use_purpose.as_deref());
                },
                ToolKind::BuiltIn(BuiltInTool::FileRead(fr)) => {
                    if let Some(op) = fr.ops.first() {
                        let entry = file_data.entry(op.path.clone()).or_insert((0, 0, 0, Vec::new()));
                        entry.1 += 1;
                        entry.2 = pos;
                    }
                },
                _ => {},
            }
        }
    }

    let total = file_data.len();
    let end_idx = messages.len().max(1);

    let mut scored: Vec<_> = file_data
        .into_iter()
        .map(|(path, (w, r, pos, reasonings))| {
            let weighted_freq = (w as f64 * 2.0) + (r as f64);
            let recency = pos as f64 / end_idx as f64;
            let score = (weighted_freq * 2.0) + (recency * 1.0);
            (path, reasonings, w, r, score)
        })
        .collect();
    scored.sort_by(|a, b| b.4.partial_cmp(&a.4).unwrap_or(std::cmp::Ordering::Equal));

    let entries = scored
        .into_iter()
        .take(MAX_FILES)
        .map(|(path, reasonings, write_count, read_count, _)| FileEntry {
            path: truncate_path(&path),
            reasonings,
            write_count,
            read_count,
        })
        .collect();

    (entries, total)
}

/// Selects the most important commands from conversation history using weighted scoring.
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
/// # Reasoning Storage
/// - Stores up to 5 most recent reasoning summaries per command
/// - Reasonings extracted from tool's purpose field
/// - Truncated to 100 characters each
///
/// # Returns
/// Tuple of (selected_commands, total_unique_commands)
fn extract_commands(messages: &[Message]) -> (Vec<CommandEntry>, usize) {
    // command -> (count, last_position, reasonings)
    let mut cmd_data: HashMap<String, (usize, usize, Vec<String>)> = HashMap::new();

    for (pos, msg) in messages.iter().enumerate() {
        for tool_use in msg.tool_uses_iter() {
            let Ok(builtin_name) = tool_use.name.parse::<BuiltInToolName>() else {
                continue;
            };
            let canonical = CanonicalToolName::BuiltIn(builtin_name);
            let Ok(tool) = Tool::parse(&canonical, tool_use.input.clone()) else {
                continue;
            };

            if let ToolKind::BuiltIn(BuiltInTool::ExecuteCmd(ec)) = tool.kind() {
                let entry = cmd_data.entry(ec.command.clone()).or_insert((0, 0, Vec::new()));
                entry.0 += 1;
                entry.1 = pos;
                add_reasoning(&mut entry.2, tool.tool_use_purpose.as_deref());
            }
        }
    }

    let total = cmd_data.len();
    let end_idx = messages.len().max(1);

    let mut scored: Vec<_> = cmd_data
        .into_iter()
        .map(|(cmd, (count, pos, reasonings))| {
            let recency = pos as f64 / end_idx as f64;
            let score = (count as f64 * 2.0) + (recency * 1.0);
            (cmd, reasonings, count, score)
        })
        .collect();
    scored.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));

    let entries = scored
        .into_iter()
        .take(MAX_COMMANDS)
        .map(|(command, reasonings, count, _)| CommandEntry {
            command,
            reasonings,
            count,
        })
        .collect();

    (entries, total)
}

/// Extracts the most important files and commands from conversation history.
///
/// Selects top 30 files (read/write tools) and top 20 commands (shell tools)
/// using weighted scoring that prioritizes frequency and recency.
///
/// See `extract_files()` and `extract_commands()` for detailed scoring formulas.
pub fn extract_compaction_facts(messages: &[Message], exclude_last_n: usize) -> CompactionFacts {
    let end_idx = messages.len().saturating_sub(exclude_last_n);
    let history = &messages[..end_idx];

    let (files_accessed, total_files) = extract_files(history);
    let (commands_executed, total_commands) = extract_commands(history);

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
    use crate::agent::agent_loop::types::{
        ContentBlock,
        Role,
        ToolUseBlock,
    };
    use crate::agent::consts::TOOL_USE_PURPOSE_FIELD_NAME;
    use crate::agent::tools::execute_cmd::ExecuteCmd;
    use crate::agent::tools::fs_read::{
        FsRead,
        FsReadOp,
    };
    use crate::agent::tools::fs_write::{
        FileCreate,
        FsWrite,
    };

    fn create_fs_write_msg(path: &str, summary: &str) -> Message {
        let tool = FsWrite::Create(FileCreate {
            path: path.into(),
            content: "test".into(),
            start_line: None,
        });
        let mut input = serde_json::to_value(&tool).unwrap();
        if !summary.is_empty() {
            input
                .as_object_mut()
                .unwrap()
                .insert(TOOL_USE_PURPOSE_FIELD_NAME.to_string(), serde_json::json!(summary));
        }
        Message::new(
            Role::Assistant,
            vec![ContentBlock::ToolUse(ToolUseBlock {
                tool_use_id: "test".into(),
                name: BuiltInToolName::FsWrite.to_string(),
                input,
            })],
            None,
        )
    }

    fn create_fs_read_msg(path: &str) -> Message {
        let tool = FsRead {
            ops: vec![FsReadOp {
                path: path.into(),
                limit: None,
                offset: None,
            }],
        };
        let input = serde_json::to_value(&tool).unwrap();
        Message::new(
            Role::Assistant,
            vec![ContentBlock::ToolUse(ToolUseBlock {
                tool_use_id: "test".into(),
                name: BuiltInToolName::FsRead.to_string(),
                input,
            })],
            None,
        )
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
        // Should not panic on UTF-8 boundaries
        assert!(result.chars().count() <= 100);
    }

    #[test]
    fn test_writes_prioritized_over_reads() {
        let messages = vec![
            create_fs_write_msg("/file_a.rs", "Writing A"),
            create_fs_read_msg("/file_b.rs"),
            create_fs_read_msg("/file_b.rs"),
            create_fs_read_msg("/file_b.rs"),
        ];

        let facts = extract_compaction_facts(&messages, 0);

        assert_eq!(facts.files_accessed.len(), 2);

        let file_a = facts.files_accessed.iter().find(|e| e.path == "/file_a.rs").unwrap();
        assert_eq!(file_a.write_count, 1);
        assert_eq!(file_a.read_count, 0);

        let file_b = facts.files_accessed.iter().find(|e| e.path == "/file_b.rs").unwrap();
        assert_eq!(file_b.write_count, 0);
        assert_eq!(file_b.read_count, 3);
    }

    #[test]
    fn test_read_and_write_same_file() {
        let messages = vec![
            create_fs_read_msg("/file.rs"),
            create_fs_write_msg("/file.rs", "Writing 1"),
            create_fs_read_msg("/file.rs"),
            create_fs_write_msg("/file.rs", "Writing 2"),
            create_fs_write_msg("/file.rs", "Writing 3"),
        ];

        let facts = extract_compaction_facts(&messages, 0);

        assert_eq!(facts.files_accessed.len(), 1);
        assert_eq!(facts.files_accessed[0].path, "/file.rs");
        assert_eq!(facts.files_accessed[0].write_count, 3);
        assert_eq!(facts.files_accessed[0].read_count, 2);
        assert_eq!(facts.files_accessed[0].reasonings.len(), 3);
    }

    #[test]
    fn test_read_only_file() {
        let messages = vec![
            create_fs_read_msg("/readonly.rs"),
            create_fs_read_msg("/readonly.rs"),
            create_fs_read_msg("/readonly.rs"),
        ];

        let facts = extract_compaction_facts(&messages, 0);

        assert_eq!(facts.files_accessed.len(), 1);
        assert_eq!(facts.files_accessed[0].path, "/readonly.rs");
        assert_eq!(facts.files_accessed[0].write_count, 0);
        assert_eq!(facts.files_accessed[0].read_count, 3);
        assert_eq!(facts.files_accessed[0].reasonings.len(), 0);
    }

    #[test]
    fn test_max_reasonings_limit() {
        let messages: Vec<_> = (0..10)
            .map(|i| create_fs_write_msg("/file.rs", &format!("Write {}", i)))
            .collect();

        let facts = extract_compaction_facts(&messages, 0);

        assert_eq!(facts.files_accessed.len(), 1);
        assert_eq!(facts.files_accessed[0].write_count, 10);
        assert_eq!(facts.files_accessed[0].reasonings.len(), 5);
    }

    #[test]
    fn test_empty_history() {
        let messages: Vec<Message> = vec![];
        let facts = extract_compaction_facts(&messages, 0);

        assert_eq!(facts.files_accessed.len(), 0);
        assert_eq!(facts.commands_executed.len(), 0);
        assert_eq!(facts.total_files, 0);
        assert_eq!(facts.total_commands, 0);
    }

    #[test]
    fn test_display_format() {
        let messages = vec![
            create_fs_write_msg("/file.rs", "Write 1"),
            create_fs_write_msg("/file.rs", "Write 2"),
            create_fs_read_msg("/file.rs"),
        ];

        let facts = extract_compaction_facts(&messages, 0);
        let display = facts.to_string();

        assert!(display.contains("Files Modified and Read"));
        assert!(display.contains("2 modifications, 1 read"));
    }

    #[test]
    fn test_ignores_unknown_tools() {
        let messages = vec![Message::new(
            Role::Assistant,
            vec![ContentBlock::ToolUse(ToolUseBlock {
                tool_use_id: "test".into(),
                name: "unknown_tool".into(),
                input: serde_json::json!({"path": "/test.rs"}),
            })],
            None,
        )];
        let facts = extract_compaction_facts(&messages, 0);
        assert!(facts.is_empty());
    }

    #[test]
    fn test_execute_cmd() {
        let tool = ExecuteCmd {
            command: "cargo test".into(),
        };
        let mut input = serde_json::to_value(&tool).unwrap();
        input
            .as_object_mut()
            .unwrap()
            .insert(TOOL_USE_PURPOSE_FIELD_NAME.to_string(), serde_json::json!("Run tests"));
        let messages = vec![Message::new(
            Role::Assistant,
            vec![ContentBlock::ToolUse(ToolUseBlock {
                tool_use_id: "test".into(),
                name: BuiltInToolName::ExecuteCmd.to_string(),
                input,
            })],
            None,
        )];

        let facts = extract_compaction_facts(&messages, 0);

        assert_eq!(facts.commands_executed.len(), 1);
        assert_eq!(facts.commands_executed[0].command, "cargo test");
        assert_eq!(facts.commands_executed[0].reasonings, vec!["Run tests"]);
    }
}
