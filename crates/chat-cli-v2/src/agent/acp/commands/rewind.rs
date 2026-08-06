//! /rewind command execution
//!
//! The `/rewind` command lets the user fork a session at an earlier turn.
//! Without arguments it shows a picker listing every user prompt in the
//! current session (most recent first). When the user selects one, we clone
//! the session's log entries up through (and including) that turn into a new
//! session, and the TUI is signaled via `sessionId` in the response to load
//! the new session. The original session stays untouched.
//!
//! Compaction is handled transparently: a `Compaction` log entry is just an
//! additional entry in the JSONL file, and we copy entries verbatim up to
//! the selected turn. Rewinding past a compaction restores the full
//! pre-compaction history because the new session never sees the
//! `Compaction` entry.
//!
//! ## Future migration
//!
//! The duplication logic here (read source log → create new SessionDb → append
//! entries one-by-one) will be replaced once we have a "create session with
//! initial context" primitive (e.g. `session/new` with a `logEntries` or
//! `messages` param). When that lands, `rewind_to` collapses to:
//!
//! ```ignore
//! let log = read_entries_up_to(ctx.session_id, turn_end)?;
//! let new_id = ctx.kiro.new_session_with_context(&cwd, log).await?;
//! Ok(new_id)
//! ```
//!
//! Until then, we do the filesystem work here directly via `SessionDb`.

use agent::event_log::{
    LogEntry,
    LogEntryV1,
};
use agent::tui_commands::{
    CommandResult,
    RewindArgs,
};
use tracing::{
    debug,
    error,
};

use super::CommandContext;
use crate::agent::session::{
    SessionCreatedReason,
    SessionDb,
};
use crate::util::paths::sessions_dir;

/// Max length (chars) of the prompt preview shown in the picker row.
const PREVIEW_MAX_LEN: usize = 80;

/// Build the serialized turn list returned to the TUI when `/rewind` is
/// invoked with no args. Each entry is a plain JSON object with the fields
/// the TUI Explorer needs to render a row + preview. We use a raw JSON
/// shape here (via `serde_json::Value`) instead of a typed struct so the
/// schema lives entirely inside `/rewind` and doesn't pollute the shared
/// `CommandOption`.
///
/// Shape (per entry):
/// ```json
/// {
///   "logIndex": 12,                // the index into the session's event log
///   "label": "What is apple?",     // truncated single-line preview of the prompt
///   "group": "12%" | "1.2k" | "",  // context % or token count or empty
///   "responseSnippet": "..."       // first few lines of the assistant response
/// }
/// ```
///
/// Order: newest first.
fn build_turn_list(ctx: &CommandContext<'_>) -> serde_json::Value {
    let turns = match collect_turns(ctx) {
        Ok(t) => t,
        Err(e) => {
            debug!(error = %e, "rewind build_turn_list failed to load session");
            return serde_json::json!([]);
        },
    };

    let values: Vec<serde_json::Value> = turns
        .into_iter()
        .rev()
        .map(|turn| {
            let group = if let Some(pct) = turn.context_usage_percentage {
                format!("{:.0}%", pct)
            } else if let Some(tokens) = turn.token_count {
                format_tokens(tokens)
            } else {
                String::new()
            };
            serde_json::json!({
                "logIndex": turn.log_index,
                "label": turn.preview,
                "group": group,
                "responseSnippet": turn.response_snippet,
            })
        })
        .collect();

    serde_json::Value::Array(values)
}

/// Execute `/rewind`.
///
/// - `/rewind` (no args): builds the turn list and returns it under `CommandResult.data.turns` so
///   the TUI can open the Explorer without a second round-trip.
/// - `/rewind <log_index>`: clones entries `[0..=turn_end]` into a new session and returns `{
///   sessionId, switchSession: true }` so the TUI can load the forked session.
pub async fn execute(args: &RewindArgs, ctx: &CommandContext<'_>) -> CommandResult {
    let Some(turn_arg) = args.turn_index.as_deref() else {
        // No arg — open the Explorer. Return the picker data directly in
        // CommandResult.data so the TUI doesn't need a separate options RPC.
        let turns = build_turn_list(ctx);
        let mut result = CommandResult::success("");
        result.data = Some(serde_json::json!({ "turns": turns }));
        return result;
    };

    let log_index: usize = match turn_arg.trim().parse() {
        Ok(n) => n,
        Err(_) => return CommandResult::error(format!("Invalid turn index: {}", turn_arg)),
    };

    match rewind_to(ctx, log_index).await {
        Ok(new_id) => {
            let mut result = CommandResult::success(format!("Rewound to earlier turn (new session {new_id})"));
            result.data = Some(serde_json::json!({ "sessionId": new_id, "switchSession": true }));
            result
        },
        Err(e) => {
            error!(error = %e, log_index, "rewind failed");
            CommandResult::error(e)
        },
    }
}

/// Core: copy log entries [0..=turn_end_inclusive] from `ctx.session_id`
/// into a freshly created new session. Returns the new session id.
async fn rewind_to(ctx: &CommandContext<'_>, selected_prompt_index: usize) -> Result<String, String> {
    let sessions_dir = sessions_dir().map_err(|e| format!("Failed to find sessions directory: {e}"))?;

    // Load current session metadata (so we can clone SessionState) and log.
    let current = SessionDb::load_with_sessions_dir(&sessions_dir, ctx.session_id, None)
        .map_err(|e| format!("Failed to load current session: {e}"))?;
    let current_data = current.session();
    let entries = current
        .load_log_entries()
        .map_err(|e| format!("Failed to read session log: {e}"))?;
    // Drop the current session lock early so we don't hold two simultaneously.
    drop(current);

    if entries.is_empty() {
        return Err("Session has no turns to rewind to.".to_string());
    }

    if selected_prompt_index >= entries.len() {
        return Err(format!(
            "Turn index {} is out of range (session has {} log entries).",
            selected_prompt_index,
            entries.len()
        ));
    }

    // Verify the selected entry is a Prompt.
    match entries.get(selected_prompt_index) {
        Some(LogEntry::V1(LogEntryV1::Prompt { .. })) => {},
        _ => {
            return Err(format!(
                "Entry at index {} is not a user prompt.",
                selected_prompt_index
            ));
        },
    }

    // Find the end of the selected turn: the index BEFORE the next Prompt
    // (or end-of-log if this is the final turn). All entries up to and
    // including this end index belong to the selected turn.
    let turn_end_inclusive = entries
        .iter()
        .enumerate()
        .skip(selected_prompt_index + 1)
        .find_map(|(i, e)| matches!(e, LogEntry::V1(LogEntryV1::Prompt { .. })).then_some(i - 1))
        .unwrap_or(entries.len() - 1);

    // Generate new session id and create the new session with a cloned state.
    let new_id = uuid::Uuid::new_v4().to_string();
    let mut new_state = current_data.session_state.clone();

    // Reset per-turn metadatas — they'll be re-attached when the model next runs.
    // We keep the model/agent/permissions, but drop turn counters that won't match.
    if let crate::agent::session::SessionState::V1(ref mut v1) = new_state {
        // Update conversation_id on the cloned rts_model_state so the new session
        // isn't conflated with the original on the model's side.
        v1.rts_model_state.conversation_id = new_id.clone();

        // Trim user_turn_metadatas to match the number of kept prompts.
        //
        // Metadatas are appended positionally as turns complete — the Nth metadata
        // corresponds to the Nth completed user turn. The two lists (prompts in the
        // log vs metadatas in session state) are tail-aligned: the last K prompts
        // match the last K metadatas where K = min(prompts, metadatas).
        // After truncating the prompt list, drop the same count from the tail.
        let total_prompts = entries
            .iter()
            .filter(|e| matches!(e, LogEntry::V1(LogEntryV1::Prompt { .. })))
            .count();
        let kept_prompts = entries[..=turn_end_inclusive]
            .iter()
            .filter(|e| matches!(e, LogEntry::V1(LogEntryV1::Prompt { .. })))
            .count();
        let new_meta_len = compute_trimmed_meta_len(
            total_prompts,
            kept_prompts,
            v1.conversation_metadata.user_turn_metadatas.len(),
        );
        v1.conversation_metadata.user_turn_metadatas.truncate(new_meta_len);

        // History was rewritten, so the stored context usage reading no longer
        // describes it. Leaving it would make the first prompt in the rewound
        // session compact a conversation the user just deliberately restored.
        v1.conversation_metadata.last_context_usage = None;
    }

    let new_db = SessionDb::new(
        new_id.clone(),
        &current_data.cwd,
        new_state,
        Some(ctx.session_id.to_string()),
        SessionCreatedReason::Rewind,
    )
    .map_err(|e| format!("Failed to create new session: {e}"))?;

    // Copy kept log entries into the new session.
    for entry in &entries[..=turn_end_inclusive] {
        new_db
            .append_log_entry(entry)
            .map_err(|e| format!("Failed to write log entry to new session: {e}"))?;
    }

    // Drop the lock on the new session — the TUI will open it via session/load.
    drop(new_db);

    debug!(
        original = %ctx.session_id,
        new = %new_id,
        entries_copied = turn_end_inclusive + 1,
        total_entries = entries.len(),
        "rewind completed"
    );
    Ok(new_id)
}

/// Summary of one user turn used to build a picker option.
struct TurnSummary {
    log_index: usize,
    /// Short single-line preview for the row label.
    preview: String,
    /// First few lines of the assistant response that followed this prompt.
    response_snippet: String,
    context_usage_percentage: Option<f32>,
    token_count: Option<u32>,
}

/// Walk the current session log, collecting one [`TurnSummary`] per user prompt.
fn collect_turns(ctx: &CommandContext<'_>) -> Result<Vec<TurnSummary>, String> {
    let sessions_dir = sessions_dir().map_err(|e| format!("Failed to find sessions directory: {e}"))?;
    let db = SessionDb::load_with_sessions_dir(&sessions_dir, ctx.session_id, None)
        .map_err(|e| format!("Failed to load session: {e}"))?;
    let session = db.session();
    let entries = db
        .load_log_entries()
        .map_err(|e| format!("Failed to read session log: {e}"))?;
    drop(db);

    // Prompt entries by (log_index, content).
    let prompts: Vec<(usize, &Vec<agent::agent_loop::types::ContentBlock>)> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match e {
            LogEntry::V1(LogEntryV1::Prompt { content, .. }) => Some((i, content)),
            LogEntry::V1(_) => None,
        })
        .collect();

    // Per-turn metadata list from session state (for context % + token counts).
    let user_turn_metadatas = match &session.session_state {
        crate::agent::session::SessionState::V1(v1) => &v1.conversation_metadata.user_turn_metadatas,
        crate::agent::session::SessionState::Unknown => return Ok(Vec::new()),
    };

    let mut summaries = Vec::with_capacity(prompts.len());

    // Metadatas are appended positionally as turns complete. The two lists
    // (prompts in the log vs metadatas in session state) are tail-aligned:
    // the last K prompts match the last K metadatas where
    // K = min(prompts.len(), user_turn_metadatas.len()).
    // Older prompts that fall off the front (e.g. pre-history lost to
    // compaction or cancelled turns without metadata) just don't get
    // metadata — they render without a context %.
    let prompt_count = prompts.len();
    let meta_count = user_turn_metadatas.len();
    let meta_offset = prompt_count.saturating_sub(meta_count);

    for (ordinal, (log_index, content)) in prompts.iter().enumerate() {
        let preview = prompt_preview(content).unwrap_or_else(|| "(empty)".to_string());

        // Tail-aligned lookup: prompt at ordinal N maps to metadata at N - meta_offset.
        let turn_meta = ordinal
            .checked_sub(meta_offset)
            .and_then(|meta_idx| user_turn_metadatas.get(meta_idx));
        let context_usage_percentage = turn_meta.and_then(|m| m.context_usage_percentage);
        let token_count = turn_meta.map(|m| m.input_token_count.saturating_add(m.output_token_count));

        // Collect the first few lines of the assistant response that followed this
        // prompt, up to the next Prompt entry (or end-of-log).
        let next_prompt_log_index = prompts.get(ordinal + 1).map_or(entries.len(), |(i, _)| *i);
        let response_snippet = assistant_response_snippet(&entries[*log_index + 1..next_prompt_log_index]);

        summaries.push(TurnSummary {
            log_index: *log_index,
            preview,
            response_snippet,
            context_usage_percentage,
            token_count,
        });
    }

    Ok(summaries)
}

/// Truncate prompt content to a single-line preview suitable for the picker.
fn prompt_preview(content: &[agent::agent_loop::types::ContentBlock]) -> Option<String> {
    let text = content.iter().find_map(|b| b.text())?;
    let single_line = text.lines().next().unwrap_or(text).trim();
    if single_line.is_empty() {
        return None;
    }
    if single_line.chars().count() <= PREVIEW_MAX_LEN {
        return Some(single_line.to_string());
    }
    let truncated: String = single_line.chars().take(PREVIEW_MAX_LEN - 1).collect();
    Some(format!("{}…", truncated))
}

/// Format a token count like `12.4k` / `450`.
fn format_tokens(n: u32) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f32 / 1000.0)
    } else {
        n.to_string()
    }
}

/// Number of assistant-response lines shown in the hover preview.
const PREVIEW_RESPONSE_LINES: usize = 8;
/// Maximum character width per preview line.
const PREVIEW_LINE_WIDTH: usize = 160;

/// Collect the first few lines of the assistant's response from the log entries
/// that follow a prompt (and precede the next prompt).
fn assistant_response_snippet(entries_after_prompt: &[LogEntry]) -> String {
    let mut lines: Vec<String> = Vec::new();
    for entry in entries_after_prompt {
        let LogEntry::V1(LogEntryV1::AssistantMessage { content, .. }) = entry else {
            continue;
        };
        for block in content {
            if let Some(text) = block.text() {
                for line in text.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let truncated: String = if trimmed.chars().count() > PREVIEW_LINE_WIDTH {
                        trimmed
                            .chars()
                            .take(PREVIEW_LINE_WIDTH.saturating_sub(1))
                            .chain(std::iter::once('…'))
                            .collect()
                    } else {
                        trimmed.to_string()
                    };
                    lines.push(truncated);
                    if lines.len() >= PREVIEW_RESPONSE_LINES {
                        return lines.join("\n");
                    }
                }
            }
        }
    }
    lines.join("\n")
}

/// Compute how many metadatas to keep after dropping `total_prompts - kept_prompts`
/// turns from the tail. Pure arithmetic extracted for testability.
fn compute_trimmed_meta_len(total_prompts: usize, kept_prompts: usize, meta_len: usize) -> usize {
    let dropped_prompts = total_prompts.saturating_sub(kept_prompts);
    meta_len.saturating_sub(dropped_prompts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(999), "999");
        assert_eq!(format_tokens(1000), "1.0k");
        assert_eq!(format_tokens(12400), "12.4k");
    }

    #[test]
    fn test_prompt_preview_basic() {
        use agent::agent_loop::types::ContentBlock;
        let blocks = vec![ContentBlock::Text("Hello, world!".to_string())];
        assert_eq!(prompt_preview(&blocks).as_deref(), Some("Hello, world!"));
    }

    #[test]
    fn test_prompt_preview_multiline_takes_first_line() {
        use agent::agent_loop::types::ContentBlock;
        let blocks = vec![ContentBlock::Text("first\nsecond\nthird".to_string())];
        assert_eq!(prompt_preview(&blocks).as_deref(), Some("first"));
    }

    #[test]
    fn test_prompt_preview_truncates_long() {
        use agent::agent_loop::types::ContentBlock;
        let long = "a".repeat(200);
        let blocks = vec![ContentBlock::Text(long)];
        let preview = prompt_preview(&blocks).unwrap();
        // PREVIEW_MAX_LEN chars (final char is ellipsis)
        assert_eq!(preview.chars().count(), PREVIEW_MAX_LEN);
        assert!(preview.ends_with('…'));
    }

    #[test]
    fn test_prompt_preview_empty_returns_none() {
        use agent::agent_loop::types::ContentBlock;
        let blocks = vec![ContentBlock::Text("   \n   ".to_string())];
        assert_eq!(prompt_preview(&blocks), None);
    }

    #[test]
    fn test_prompt_preview_no_text_block() {
        assert_eq!(prompt_preview(&[]), None);
    }

    // --- Metadata trimming arithmetic ---

    #[test]
    fn test_trim_meta_equal_prompts_and_metas() {
        // 5 prompts, 5 metas, keep first 3 → drop 2 → keep 3 metas
        assert_eq!(compute_trimmed_meta_len(5, 3, 5), 3);
    }

    #[test]
    fn test_trim_meta_more_prompts_than_metas() {
        // 5 prompts but only 3 metas (2 early prompts had no metadata).
        // Keep first 2 prompts → drop 3 → 3.saturating_sub(3) = 0 metas kept.
        assert_eq!(compute_trimmed_meta_len(5, 2, 3), 0);
        // Keep first 4 → drop 1 → 3 - 1 = 2 metas kept.
        assert_eq!(compute_trimmed_meta_len(5, 4, 3), 2);
    }

    #[test]
    fn test_trim_meta_rewind_to_first_prompt() {
        // 3 prompts, 3 metas, keep only the first → drop 2 → keep 1
        assert_eq!(compute_trimmed_meta_len(3, 1, 3), 1);
    }

    #[test]
    fn test_trim_meta_rewind_to_last_prompt() {
        // 3 prompts, 3 metas, keep all → drop 0 → keep 3
        assert_eq!(compute_trimmed_meta_len(3, 3, 3), 3);
    }

    #[test]
    fn test_trim_meta_no_metas_at_all() {
        // Edge case: session has prompts but no metadatas (e.g. loaded from v1 export)
        assert_eq!(compute_trimmed_meta_len(3, 2, 0), 0);
    }
}
