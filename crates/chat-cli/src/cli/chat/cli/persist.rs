use std::process::ExitCode;

use chat_cli_v2::agent::acp::schema::SessionInfoEntry;
use chat_cli_v2::agent::session::kas::{
    KasAcpSessionClient,
    KasSessionClient,
};
use clap::Subcommand;
use crossterm::execute;
use crossterm::style::{
    self,
    Stylize,
};
use dialoguer::Select;
use eyre::Result;

use crate::cli::ConversationState;
use crate::cli::chat::context::ContextFilePath;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::os::Os;
use crate::theme::StyledText;
use crate::util::consts::env_var::KIRO_TEST_MOCK_KAS_SESSIONS;
use crate::util::paths;

/// Display entry for chat session selection
pub struct ChatSessionDisplayEntry {
    pub session_id: String,
    pub timestamp: String,
    pub summary: String,
    pub msg_count: usize,
}

/// Format conversation summary from user prompts
fn format_conversation_summary(conv_state: &ConversationState) -> String {
    conv_state.get_user_prompts().last().map_or_else(
        || "(empty conversation)".to_string(),
        |s| {
            let single_line = s.replace(['\n', '\r'], " ");
            if single_line.chars().count() > 150 {
                let truncated: String = single_line.chars().take(150).collect();
                format!("{truncated}...")
            } else {
                single_line
            }
        },
    )
}

/// Build display entries from conversation list
pub fn build_session_entries(
    conversations: Vec<(String, ConversationState, i64, i64)>,
) -> Vec<ChatSessionDisplayEntry> {
    conversations
        .into_iter()
        .map(
            |(conv_id, conv_state, _created_at, updated_at)| ChatSessionDisplayEntry {
                session_id: conv_id,
                timestamp: format_timestamp(updated_at),
                summary: format_conversation_summary(&conv_state),
                msg_count: conv_state.history().len() * 2,
            },
        )
        .collect()
}

impl std::fmt::Display for ChatSessionDisplayEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} | {} | {} msgs", self.timestamp, self.summary, self.msg_count)
    }
}

/// Commands for managing chat sessions
#[deny(missing_docs)]
#[derive(Debug, PartialEq, Subcommand)]
pub enum ChatSubcommand {
    /// Start a new conversation, optionally with an initial prompt
    New {
        /// Optional initial prompt for the new conversation
        #[arg(trailing_var_arg = true)]
        prompt: Vec<String>,
    },
    /// Resume a saved chat session
    Resume,
    /// Save the current chat session to a file
    Save {
        /// Path where the chat session will be saved
        path: String,
        #[arg(short, long)]
        /// Force overwrite if file already exists
        force: bool,
    },
    /// Load a previous chat session from a file
    Load {
        /// Path to the chat session file to load
        path: String,
    },
    /// Save the current chat session using a custom script that receives conversation JSON via
    /// stdin
    #[command(name = "save-via-script")]
    ScriptSave {
        /// Path to script (should exit 0 on success)
        script: String,
    },
    /// Load a chat session using a custom script that outputs conversation JSON to stdout
    #[command(name = "load-via-script")]
    ScriptLoad {
        /// Path to script (should exit 0 on success)
        script: String,
    },
}

impl ChatSubcommand {
    pub async fn execute(self, os: &mut Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        macro_rules! tri {
            ($v:expr, $name:expr, $path:expr) => {
                match $v {
                    Ok(v) => v,
                    Err(err) => {
                        execute!(
                            session.stderr,
                            StyledText::error_fg(),
                            style::Print(format!("\nFailed to {} {}: {}\n\n", $name, $path, &err)),
                            StyledText::reset_attributes()
                        )?;

                        return Ok(ChatState::PromptUser {
                            skip_printing_tools: true,
                        });
                    },
                }
            };
        }

        match self {
            Self::New { prompt } => {
                // Exit tangent mode first so we save the original conversation, not the tangent
                if session.conversation.is_in_tangent_mode() {
                    session.conversation.exit_tangent_mode();
                }

                // Save current conversation before forking
                let save_failed = match std::env::current_dir() {
                    Ok(cwd) => {
                        if let Err(e) = os.database.set_conversation_by_path(&cwd, &session.conversation) {
                            tracing::warn!(?e, "Failed to save current conversation before starting new one");
                            true
                        } else {
                            false
                        }
                    },
                    Err(e) => {
                        tracing::warn!(
                            ?e,
                            "Failed to get current directory; current conversation may not be saved"
                        );
                        true
                    },
                };

                if save_failed {
                    execute!(
                        session.stderr,
                        StyledText::warning_fg(),
                        style::Print(
                            "\n⚠ Warning: Could not save current conversation. It may not be available for /chat resume.\n"
                        ),
                        StyledText::reset_attributes()
                    )?;
                }

                // Construct a fresh ConversationState via ::new, moving expensive resources
                // (ToolManager) and cloning cheap ones (Agents, Arc<CodeIntelligence>).
                session.new_conversation(os).await;

                execute!(
                    session.stderr,
                    StyledText::success_fg(),
                    style::Print("\n✔ Started new conversation. Use /chat resume to return to previous sessions.\n\n"),
                    StyledText::reset_attributes()
                )?;

                if prompt.is_empty() {
                    return Ok(ChatState::default());
                }

                let input = prompt.join(" ");
                session.conversation.append_user_transcript(&input);
                return Ok(ChatState::HandleInput { input });
            },
            Self::Resume => {
                return resume_chat_session(os, session).await;
            },
            Self::Save { path, force } => {
                let expanded_path = tri!(paths::expand_path(os, &path), "expand path", &path);
                let path_str = expanded_path.to_string_lossy().to_string();
                let contents = tri!(
                    serde_json::to_string_pretty(&session.conversation),
                    "export to",
                    &path_str
                );
                if os.fs.exists(&path_str) && !force {
                    execute!(
                        session.stderr,
                        StyledText::error_fg(),
                        style::Print(format!(
                            "\nFile at {} already exists. To overwrite, use -f or --force\n\n",
                            &path_str
                        )),
                        StyledText::reset_attributes()
                    )?;
                    return Ok(ChatState::PromptUser {
                        skip_printing_tools: true,
                    });
                }
                tri!(os.fs.write(&path_str, contents).await, "export to", &path_str);

                execute!(
                    session.stderr,
                    StyledText::success_fg(),
                    style::Print(format!("\n✔ Exported chat session state to {}\n", &path_str)),
                    StyledText::reset_attributes(),
                    style::Print(format!("To restore this session later, use: /chat load {}\n", &path))
                )?;
            },
            Self::ScriptSave { script } => {
                match script_save(&script, session) {
                    Ok(_) => {
                        execute!(
                            session.stderr,
                            StyledText::success_fg(),
                            style::Print(format!("\n✔ Saved chat session via script {}\n", &script)),
                            StyledText::reset_attributes(),
                            style::Print("To restore this session later, use: /chat load-via-script <load-script>\n"),
                        )?;
                    },
                    Err(_) => {
                        // Silent failure - script can print its own errors to stderr
                    },
                }
            },
            Self::Load { path } => {
                let expanded_path = tri!(paths::expand_path(os, &path), "expand path", &path);
                let path_str = expanded_path.to_string_lossy().to_string();

                // Try the original path first
                let original_result = os.fs.read_to_string(&path_str).await;

                // If the original path fails and doesn't end with .json, try with .json appended
                let contents = if original_result.is_err() && !path_str.ends_with(".json") {
                    let json_path = format!("{path_str}.json");
                    match os.fs.read_to_string(&json_path).await {
                        Ok(content) => content,
                        Err(_) => {
                            // If both paths fail, return the original error for better user experience
                            tri!(original_result, "import from", &path)
                        },
                    }
                } else {
                    tri!(original_result, "import from", &path)
                };

                let new_state: ConversationState = tri!(serde_json::from_str(&contents), "import from", &path);
                let chat_state = restore_conversation_state(session, new_state);
                session.conversation.update_state(true).await;

                execute!(
                    session.stderr,
                    StyledText::success_fg(),
                    style::Print(format!("\n✔ Imported chat session state from {}\n\n", &path)),
                    StyledText::reset_attributes()
                )?;

                return Ok(chat_state);
            },
            Self::ScriptLoad { script } => {
                match script_load(&script) {
                    Ok(new_state) => {
                        let chat_state = restore_conversation_state(session, new_state);
                        session.conversation.update_state(true).await;
                        execute!(
                            session.stderr,
                            StyledText::success_fg(),
                            style::Print(format!("\n✔ Loaded chat session via script {}\n\n", &script)),
                            StyledText::reset_attributes()
                        )?;

                        return Ok(chat_state);
                    },
                    Err(_) => {
                        // Silent failure - script can print its own errors to stderr
                    },
                }
            },
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::New { .. } => "new",
            Self::Resume => "resume",
            Self::Save { .. } => "save",
            Self::Load { .. } => "load",
            Self::ScriptSave { .. } => "save-via-script",
            Self::ScriptLoad { .. } => "load-via-script",
        }
    }
}

/// Restores a conversation state into the current session, preserving session-specific fields
/// Returns a ChatState that will trigger a conversation summary
fn restore_conversation_state(session: &mut ChatSession, mut new_state: ConversationState) -> ChatState {
    std::mem::swap(&mut new_state.tool_manager, &mut session.conversation.tool_manager);
    std::mem::swap(&mut new_state.mcp_enabled, &mut session.conversation.mcp_enabled);
    std::mem::swap(&mut new_state.model_info, &mut session.conversation.model_info);

    // For context, we would only take paths that are not in the current agent
    // And we'll place them as temporary context
    // Note that we are NOT doing the same with hooks because hooks are more
    // instrinsically linked to agent and it affects the behavior of an agent
    if let Some(cm) = &new_state.context_manager
        && let Some(existing_cm) = &mut session.conversation.context_manager
    {
        let existing_paths = &mut existing_cm.paths;
        for incoming_path in &cm.paths {
            if !existing_paths.contains(incoming_path) {
                existing_paths.push(ContextFilePath::Session(incoming_path.get_path_as_str().to_string()));
            }
        }
    }

    std::mem::swap(
        &mut new_state.context_manager,
        &mut session.conversation.context_manager,
    );
    std::mem::swap(&mut new_state.agents, &mut session.conversation.agents);
    session.conversation = new_state;

    ChatState::HandleInput {
        input: "In a few words, summarize our conversation so far.".to_owned(),
    }
}

fn format_timestamp(timestamp_ms: i64) -> String {
    use chrono::{
        TimeZone,
        Utc,
    };

    let datetime = Utc.timestamp_millis_opt(timestamp_ms).single();
    match datetime {
        Some(dt) => {
            let now = Utc::now();
            let diff = now.signed_duration_since(dt);

            let secs = diff.num_seconds();
            if secs < 60 {
                format!("{secs} seconds ago")
            } else if secs < 3600 {
                let mins = secs / 60;
                format!("{mins} minutes ago")
            } else if secs < 86400 {
                let hours = secs / 3600;
                format!("{hours} hours ago")
            } else {
                let days = secs / 86400;
                format!("{days} days ago")
            }
        },
        None => "unknown".to_string(),
    }
}

/// A unified session entry from V1 (SQLite), V2 (filesystem), or KAS (ACP).
#[derive(Debug)]
pub(crate) struct SessionEntry {
    session_id: String,
    summary: String,
    /// `None` for sources that don't track it (e.g. KAS's `session/list`).
    msg_count: Option<usize>,
    updated_at_ms: i64,
    source: SessionSource,
    /// WHERE the session runs (`"local"`|`"cloud-sandbox"`), from a V3 row's
    /// `_meta.kiro.executionTarget.kind`. `None` for V1/V2 (always local) and for
    /// V3 rows without the field. Surfaced by the merged picker as a cloud tag.
    execution_target: Option<String>,
}

impl From<chat_cli_v2::agent::acp::schema::SessionInfoEntry> for SessionEntry {
    fn from(k: chat_cli_v2::agent::acp::schema::SessionInfoEntry) -> Self {
        let updated_at_ms = k
            .updated_at
            .as_deref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map_or(0, |dt| dt.timestamp_millis());
        Self {
            session_id: k.session_id,
            summary: k.title.unwrap_or_else(|| "(no title)".to_string()),
            msg_count: k.message_count,
            updated_at_ms,
            source: SessionSource::Kas,
            execution_target: k.execution_target,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionSource {
    V1,
    V2,
    Kas,
}

impl std::fmt::Display for SessionSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::V1 => f.write_str("classic"),
            Self::V2 => f.write_str("v2"),
            Self::Kas => f.write_str("v3"),
        }
    }
}

impl serde::Serialize for SessionSource {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// JSON envelope for `--list-sessions --format json`. The outer wrapper
/// is an array carrying one entry per cwd. Today the listing is always
/// scoped to the current cwd, so the array is single-element; the
/// shape leaves room for a future `--all-cwds` flag without a breaking
/// rename.
#[derive(Debug, serde::Serialize)]
pub(crate) struct SessionListingJson<'a> {
    pub cwd: String,
    pub sessions: Vec<SessionEntryJson<'a>>,
}

/// Per-session JSON shape, matching the camelCase fields used on
/// KAS's own `session/list` response so a TUI consumer reads both
/// surfaces with one parser.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionEntryJson<'a> {
    pub session_id: &'a str,
    pub source: SessionSource,
    pub title: &'a str,
    /// RFC3339 timestamp built from the entry's stored epoch millis.
    /// An entry with a zero or unparseable timestamp serializes as
    /// the Unix epoch; this matches the ordering behavior (oldest
    /// possible) and is unambiguous on round-trip.
    pub updated_at: String,
    /// Omitted when the source did not report a count (KAS).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_count: Option<usize>,
    /// WHERE the session runs (`"local"`|`"cloud-sandbox"`). Omitted for V1/V2 and
    /// for V3 rows lacking it, so a TUI consumer reads it as local.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_target: Option<&'a str>,
}

impl<'a> SessionListingJson<'a> {
    /// Build the envelope from raw entries, canonicalizing `cwd` so
    /// the JSON output is symlink-stable. Falls back to the
    /// unresolved path if canonicalization fails.
    pub fn from_entries(cwd: &std::path::Path, entries: &'a [SessionEntry]) -> Self {
        let canonical = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
        let sessions = entries.iter().map(SessionEntryJson::from_entry).collect();
        Self {
            cwd: canonical.display().to_string(),
            sessions,
        }
    }
}

impl<'a> SessionEntryJson<'a> {
    fn from_entry(e: &'a SessionEntry) -> Self {
        let updated_at = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(e.updated_at_ms)
            .unwrap_or_default()
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        Self {
            session_id: &e.session_id,
            source: e.source,
            title: &e.summary,
            updated_at,
            message_count: e.msg_count,
            execution_target: e.execution_target.as_deref(),
        }
    }
}

/// Collect all sessions (V1 + V2) for a given cwd, sorted most-recent first.
fn collect_all_sessions(db: &crate::database::Database, cwd: &std::path::Path) -> Vec<SessionEntry> {
    let v2_dir = chat_cli_v2::util::paths::sessions_dir().ok();
    collect_all_sessions_impl(db, cwd, v2_dir.as_deref())
}

fn collect_all_sessions_impl(
    db: &crate::database::Database,
    cwd: &std::path::Path,
    v2_sessions_dir: Option<&std::path::Path>,
) -> Vec<SessionEntry> {
    let mut entries = Vec::new();

    // V1 sessions from SQLite
    if let Ok(conversations) = db.list_conversations_by_path(cwd) {
        for (conv_id, conv_state, _created_at, updated_at) in conversations {
            entries.push(SessionEntry {
                session_id: conv_id,
                summary: format_conversation_summary(&conv_state),
                msg_count: Some(conv_state.history().len() * 2),
                updated_at_ms: updated_at,
                source: SessionSource::V1,
                execution_target: None, // V1 sessions are always local
            });
        }
    }

    // V2 sessions from filesystem
    if let Some(sessions_dir) = v2_sessions_dir
        && let Ok(v2_sessions) = chat_cli_v2::agent::session::list_sessions(sessions_dir, Some(cwd))
    {
        for s in v2_sessions {
            entries.push(SessionEntry {
                session_id: s.session_id,
                summary: s.title.unwrap_or_else(|| "(no title)".to_string()),
                msg_count: Some(s.message_count),
                updated_at_ms: s.updated_at.timestamp_millis(),
                source: SessionSource::V2,
                execution_target: None, // V2 sessions are always local
            });
        }
    }

    entries.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
    entries
}

/// Delete a session from V1 (SQLite) and/or V2 (filesystem).
///
/// Returns which stores the session was deleted from, or `Err` if V2 lock fails.
fn delete_any_session(
    db: &crate::database::Database,
    session_id: &str,
    source: Option<SessionSource>,
) -> std::result::Result<(bool, bool), String> {
    let v2_dir = chat_cli_v2::util::paths::sessions_dir().ok();
    delete_any_session_impl(db, session_id, v2_dir.as_deref(), source)
}

/// Returns `(v1_deleted, v2_deleted)`.
fn delete_any_session_impl(
    db: &crate::database::Database,
    session_id: &str,
    v2_sessions_dir: Option<&std::path::Path>,
    source: Option<SessionSource>,
) -> std::result::Result<(bool, bool), String> {
    let delete_v1 = source.is_none() || source == Some(SessionSource::V1);
    let delete_v2 = source.is_none() || source == Some(SessionSource::V2);

    // Try V2 first — it requires a lock and can fail, so do it before the
    // irreversible V1 delete.
    let v2_ok = if delete_v2 {
        match v2_sessions_dir.map(|d| chat_cli_v2::agent::session::delete_session(d, session_id)) {
            Some(Ok(v)) => v,
            Some(Err(e)) => return Err(e.to_string()),
            None => false,
        }
    } else {
        false
    };

    let v1_ok = delete_v1 && db.delete_conversation_by_id(session_id).unwrap_or(false);
    Ok((v1_ok, v2_ok))
}

/// Handle `--list-sessions` and `--delete-session` flags before TUI launch.
///
/// Returns `Some(ExitCode)` if a flag was handled, `None` to continue normal dispatch.
///
/// V1 (SQLite) and V2 (~/.kiro/sessions/cli/) sessions are always included.
/// KAS sessions are included when [`is_kas_enabled`] returns true.
///
/// `format` selects the output renderer for `--list-sessions`:
/// - [`OutputFormat::Plain`] writes a human-readable table to stderr (the historical default).
/// - [`OutputFormat::Json`] / [`OutputFormat::JsonPretty`] write a single JSON value to stdout in
///   the [`SessionListingJson`] envelope, intended for the TUI to consume via `KIRO_CHAT_CLI_BIN`.
pub async fn handle_list_delete_session_flags(
    list_sessions: bool,
    delete_session: Option<&str>,
    delete_source: Option<SessionSource>,
    cloud_delete_enabled: bool,
    format: crate::cli::OutputFormat,
    os: &Os,
) -> Option<ExitCode> {
    if list_sessions {
        let cwd = match std::env::current_dir() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Error: cannot determine current working directory: {e}");
                return Some(ExitCode::FAILURE);
            },
        };
        let entries = collect_sessions(os, &cwd).await;
        match format {
            crate::cli::OutputFormat::Plain => {
                if let Err(e) = render_session_entries(&mut std::io::stderr(), &cwd.display().to_string(), &entries) {
                    eprintln!("Error: {e:#}");
                    return Some(ExitCode::FAILURE);
                }
            },
            crate::cli::OutputFormat::Json | crate::cli::OutputFormat::JsonPretty => {
                let envelope = SessionListingJson::from_entries(&cwd, &entries);
                // Top-level shape is an array of `{cwd, sessions}` so a
                // future `--all-cwds` flag can extend the listing without
                // a breaking rename. Today the array is single-element.
                let listing = vec![envelope];
                let out = match format {
                    crate::cli::OutputFormat::JsonPretty => serde_json::to_string_pretty(&listing),
                    _ => serde_json::to_string(&listing),
                };
                match out {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("Error: {e:#}");
                        return Some(ExitCode::FAILURE);
                    },
                }
            },
        }
        return Some(ExitCode::SUCCESS);
    }

    if let Some(session_id) = delete_session {
        return Some(handle_delete_session(os, session_id, delete_source, cloud_delete_enabled).await);
    }

    None
}

/// Collect V1+V2 sessions, plus KAS sessions when KAS launch succeeds. KAS
/// launch failures are logged via `tracing::warn` and do not abort the listing.
async fn collect_sessions(os: &Os, cwd: &std::path::Path) -> Vec<SessionEntry> {
    let mut entries = collect_all_sessions(&os.database, cwd);
    match collect_kas_via_mock_or_spawn(os, cwd).await {
        Ok(mut kas) => entries.append(&mut kas),
        Err(e) => tracing::warn!("KAS sessions unavailable: {e:#}"),
    }
    entries.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
    entries
}

/// Returns mocked KAS entries when `KIRO_TEST_MOCK_KAS_SESSIONS` is
/// set, otherwise spawns a real KAS child and queries `session/list`.
/// A malformed mock value panics rather than silently degrading - the
/// only callers are tests, and silent fallback would hide setup bugs.
async fn collect_kas_via_mock_or_spawn(os: &Os, cwd: &std::path::Path) -> Result<Vec<SessionEntry>> {
    if let Ok(raw) = std::env::var(KIRO_TEST_MOCK_KAS_SESSIONS) {
        let mocked: Vec<SessionInfoEntry> = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("{KIRO_TEST_MOCK_KAS_SESSIONS} is not valid JSON: {e}"));
        return Ok(mocked.into_iter().map(SessionEntry::from).collect());
    }
    with_kas_session_client(os, |client| async move { collect_kas_sessions(&client, cwd).await }).await
}

/// Run a closure against a freshly-spawned KAS session client. Handles the
/// shared boilerplate: spawn a piped KAS child, initialize ACP in a
/// `LocalSet`, hand the connected client to `f`.
async fn with_kas_session_client<F, Fut, R>(os: &Os, f: F) -> Result<R>
where
    F: FnOnce(KasAcpSessionClient) -> Fut,
    Fut: std::future::Future<Output = Result<R>>,
{
    let child = crate::cli::spawn_kas_process(os, crate::cli::KasStdio::Piped).await?;
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let client = KasAcpSessionClient::connect(child).await?;
            f(client).await
        })
        .await
}

/// Fetch KAS sessions for `cwd` and convert to the unified [`SessionEntry`]
/// shape.
async fn collect_kas_sessions<C: KasSessionClient>(client: &C, cwd: &std::path::Path) -> Result<Vec<SessionEntry>> {
    let kas_sessions = client.list_sessions(cwd).await?;
    Ok(kas_sessions.into_iter().map(SessionEntry::from).collect())
}

/// Delete a session from V1, V2, and/or KAS based on the optional `source`
/// filter. With no filter: tries V1+V2 unconditionally and KAS best-effort
/// (KAS launch failure is logged via `tracing::warn` and does not surface).
/// Prints exactly one user-facing line and returns the exit code.
async fn handle_delete_session(
    os: &Os,
    session_id: &str,
    source: Option<SessionSource>,
    cloud_delete_enabled: bool,
) -> ExitCode {
    let target_rust = matches!(source, None | Some(SessionSource::V1 | SessionSource::V2));
    let target_kas = matches!(source, None | Some(SessionSource::Kas));
    let kas_required = matches!(source, Some(SessionSource::Kas));

    let mut deleted = false;
    let mut errors: Vec<String> = Vec::new();

    if target_rust {
        match delete_any_session(&os.database, session_id, source) {
            // Only a real removal counts; a local no-op must not mask a
            // cloud-store delete failure below.
            Ok((v1, v2)) => {
                if v1 || v2 {
                    deleted = true;
                }
            },
            Err(e) => errors.push(e),
        }
    }

    if target_kas {
        match with_kas_session_client(os, |client| async move {
            // Default (local) store, plus the cloud store when enabled, so a bare
            // delete removes a cloud session too. KAS has no combined-store delete
            // and rejects "all", so this is two calls.
            client.delete_session(session_id, None).await?;
            if cloud_delete_enabled {
                client.delete_session(session_id, Some("remote")).await?;
            }
            Ok(())
        })
        .await
        {
            Ok(()) => deleted = true,
            Err(e) if kas_required => errors.push(format!("{e:#}")),
            Err(e) => tracing::warn!("KAS session delete unavailable: {e:#}"),
        }
    }

    if deleted {
        eprintln!("✔ Deleted chat session {session_id}");
        ExitCode::SUCCESS
    } else if errors.is_empty() {
        eprintln!("Error: chat session {session_id} not found");
        ExitCode::FAILURE
    } else {
        for e in &errors {
            eprintln!("Error: Failed to delete chat session {session_id}: {e}");
        }
        ExitCode::FAILURE
    }
}

/// List V1+V2 chat sessions for the current directory to a writer.
pub fn list_conversations(os: &Os, writer: &mut impl std::io::Write) -> Result<(), ChatError> {
    let cwd = match std::env::current_dir() {
        Ok(path) => path,
        Err(_) => return Ok(()),
    };

    let entries = collect_all_sessions(&os.database, &cwd);
    render_session_entries(writer, &cwd.display().to_string(), &entries)?;
    Ok(())
}

/// Shared output formatter for session listing. Produces identical output
/// regardless of the originating source (V1, V2, or KAS); the per-row
/// `source` column is the engine discriminator.
fn render_session_entries(
    writer: &mut impl std::io::Write,
    cwd_display: &str,
    entries: &[SessionEntry],
) -> Result<(), ChatError> {
    if entries.is_empty() {
        execute!(
            writer,
            StyledText::info_fg(),
            style::Print(format!("No saved chat sessions for {cwd_display}\n")),
            StyledText::reset(),
        )?;
        return Ok(());
    }

    execute!(
        writer,
        StyledText::info_fg(),
        style::Print(format!("\nChat sessions for {cwd_display}:\n\n")),
        StyledText::reset(),
    )?;

    for entry in entries {
        let timestamp = format_timestamp(entry.updated_at_ms);
        let msg_count_segment = match entry.msg_count {
            Some(n) => format!("{} | ", format!("{n} msgs").dim()),
            None => String::new(),
        };
        // WHERE the session runs. Only a cloud-sandbox session gets a tag; local
        // rows (V1/V2, and V3 without the field) render exactly as before.
        let where_segment = match entry.execution_target.as_deref() {
            Some("cloud-sandbox") => format!(" | {}", "cloud".to_string().dim()),
            _ => String::new(),
        };
        execute!(
            writer,
            style::Print("Chat SessionId: "),
            StyledText::brand_fg(),
            style::Print(format!("{}\n", entry.session_id)),
            StyledText::reset_attributes(),
            style::Print(format!(
                "  {} | {} | {}{}{}\n\n",
                timestamp.dim(),
                entry.summary,
                msg_count_segment,
                format!("{}", entry.source).dim(),
                where_segment,
            )),
        )?;
    }

    execute!(
        writer,
        style::Print("To delete a session, use: kiro-cli chat --delete-session <SESSION_ID>\n\n")
    )?;
    Ok(())
}

fn script_save(script: &str, session: &ChatSession) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;

    let json = serde_json::to_string_pretty(&session.conversation)?;

    let mut child = std::process::Command::new(script)
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(json.as_bytes())?;
    }

    let status = child.wait()?;
    if !status.success() {
        return Err("Script exited with non-zero status".into());
    }

    Ok(())
}

fn script_load(script: &str) -> Result<ConversationState, Box<dyn std::error::Error>> {
    let output = std::process::Command::new(script)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .output()?;

    if !output.status.success() {
        return Err("Script exited with non-zero status".into());
    }

    let json = String::from_utf8(output.stdout)?;
    let state = serde_json::from_str(&json)?;
    Ok(state)
}

pub fn select_chat_session(entries: &[ChatSessionDisplayEntry], prompt_str: &str) -> Option<usize> {
    Select::with_theme(&crate::util::dialoguer_theme())
        .with_prompt(prompt_str)
        .items(entries)
        .default(0)
        .report(false)
        .interact_opt()
        .unwrap_or(None)
}

async fn resume_chat_session(os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
    let result = (|| -> Option<ChatState> {
        let cwd = std::env::current_dir()
            .inspect_err(|_| {
                execute!(
                    session.stderr,
                    StyledText::error_fg(),
                    style::Print("\nFailed to get current directory\n\n"),
                    StyledText::reset_attributes()
                )
                .ok();
            })
            .ok()?;

        let conversations = os
            .database
            .list_conversations_by_path(&cwd)
            .inspect_err(|_| {
                execute!(
                    session.stderr,
                    StyledText::error_fg(),
                    style::Print("\nFailed to list chat sessions\n\n"),
                    StyledText::reset_attributes()
                )
                .ok();
            })
            .ok()?;

        if conversations.is_empty() {
            execute!(
                session.stderr,
                StyledText::info_fg(),
                style::Print(format!("\nNo saved chat sessions for {}\n\n", cwd.display())),
                StyledText::reset_attributes()
            )
            .ok();
            return None;
        }

        let entries = build_session_entries(conversations);
        let prompt = "Select a chat session to resume:";

        let index = select_chat_session(&entries, prompt)?;
        let selected_id = &entries[index].session_id;

        let new_state = os
            .database
            .get_conversation_by_id(selected_id)
            .inspect_err(|_| {
                execute!(
                    session.stderr,
                    StyledText::error_fg(),
                    style::Print(format!("\nFailed to retrieve chat session '{selected_id}'\n\n")),
                    StyledText::reset_attributes()
                )
                .ok();
            })
            .ok()?
            .or_else(|| {
                execute!(
                    session.stderr,
                    StyledText::error_fg(),
                    style::Print(format!("\nChat session '{selected_id}' not found\n\n")),
                    StyledText::reset_attributes()
                )
                .ok();
                None
            })?;

        let chat_state = restore_conversation_state(session, new_state);

        execute!(
            session.stderr,
            StyledText::success_fg(),
            style::Print(format!("\n✔ Resumed chat session {selected_id}\n\n")),
            StyledText::reset_attributes()
        )
        .ok()?;

        Some(chat_state)
    })();

    if result.is_some() {
        session.conversation.update_state(true).await;
    }

    Ok(result.unwrap_or(ChatState::PromptUser {
        skip_printing_tools: true,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::chat::test_utils::create_test_session;
    use crate::os::Os;

    /// Create a minimal V2 session on disk (metadata JSON + empty JSONL log).
    fn create_v2_session(sessions_dir: &std::path::Path, session_id: &str, cwd: &std::path::Path, title: &str) {
        use chrono::Utc;

        std::fs::create_dir_all(sessions_dir).unwrap();
        let meta = serde_json::json!({
            "session_id": session_id,
            "cwd": cwd,
            "created_at": Utc::now(),
            "updated_at": Utc::now(),
            "title": title,
        });
        std::fs::write(
            sessions_dir.join(format!("{session_id}.json")),
            serde_json::to_string(&meta).unwrap(),
        )
        .unwrap();
        // Write a non-empty log line so count_log_lines returns > 0
        std::fs::write(
            sessions_dir.join(format!("{session_id}.jsonl")),
            "{\"version\":\"v1\",\"kind\":\"Prompt\",\"data\":{}}\n",
        )
        .unwrap();
    }

    #[tokio::test]
    async fn test_collect_all_sessions_merges_and_sorts() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let sessions_dir = temp_dir.path().join("v2sessions");

        let mut os = Os::new().await.unwrap();
        let cwd = std::env::current_dir().unwrap();

        // Create V1 session first (older)
        let (_, _) = create_test_session(&mut os, vec!["v1 message", "exit"], vec!["response"], None).await;

        // Small delay so V2 session has a later timestamp
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        create_v2_session(&sessions_dir, "v2-test-id", &cwd, "v2 session title");

        let entries = collect_all_sessions_impl(&os.database, &cwd, Some(&sessions_dir));

        // Merges both sources
        assert!(
            entries.len() >= 2,
            "expected at least 2 sessions, got {}",
            entries.len()
        );
        assert!(
            entries.iter().any(|e| e.summary.contains("v1 message")),
            "missing V1 session"
        );
        assert!(
            entries.iter().any(|e| e.session_id == "v2-test-id"),
            "missing V2 session"
        );

        let v2 = entries.iter().find(|e| e.session_id == "v2-test-id").unwrap();
        assert_eq!(v2.summary, "v2 session title");
        assert_eq!(v2.msg_count, Some(1));

        // Sorted most-recent first (V2 was created after V1)
        assert_eq!(entries[0].session_id, "v2-test-id");
    }

    #[tokio::test]
    async fn test_delete_any_session_v1() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let sessions_dir = temp_dir.path().join("v2sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();

        let mut os = Os::new().await.unwrap();

        let (conv_id, _) = create_test_session(&mut os, vec!["to delete", "exit"], vec!["r"], None).await;

        assert_eq!(
            delete_any_session_impl(&os.database, &conv_id, Some(&sessions_dir), None).unwrap(),
            (true, false)
        );
    }

    #[tokio::test]
    async fn test_delete_any_session_v2() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let sessions_dir = temp_dir.path().join("v2sessions");

        let os = Os::new().await.unwrap();

        create_v2_session(&sessions_dir, "v2-del", temp_dir.path(), "to delete");
        assert!(sessions_dir.join("v2-del.json").exists());

        let (_v1, v2) = delete_any_session_impl(&os.database, "v2-del", Some(&sessions_dir), None).unwrap();
        assert!(v2, "v2 session should be deleted");
        // v1 delete is a no-op (SQL DELETE 0 rows = Ok), so v1 may be true
        assert!(!sessions_dir.join("v2-del.json").exists());
        assert!(!sessions_dir.join("v2-del.jsonl").exists());
    }

    #[tokio::test]
    async fn test_delete_any_session_not_found_v2_only() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(temp_dir.path()).unwrap();
        let _os = Os::new().await.unwrap();
        let result = chat_cli_v2::agent::session::delete_session(temp_dir.path(), "nonexistent");
        assert!(matches!(result, Ok(false)));
    }

    #[tokio::test]
    async fn test_auto_save_and_resume() {
        let mut os = Os::new().await.unwrap();
        let user_input = "who is the 16th president of US";

        let (conversation_id, _session) = create_test_session(
            &mut os,
            vec![user_input, "exit"],
            vec!["Abraham Lincoln was the 16th president of the United States."],
            None,
        )
        .await;

        let (_, resumed_session) = create_test_session(
            &mut os,
            vec!["confirm", "exit"],
            vec!["Yes, correct!"],
            Some(conversation_id),
        )
        .await;

        assert!(
            resumed_session
                .conversation
                .get_user_prompts()
                .iter()
                .any(|p| p.contains(user_input)),
            "Resumed session should contain original message"
        );

        // Create a second conversation for the same path
        let user_input2 = "what is the capital of France";
        let (conversation_id2, _session2) = create_test_session(
            &mut os,
            vec![user_input2, "exit"],
            vec!["Paris is the capital of France."],
            None,
        )
        .await;

        // Resume the second conversation
        let (_, resumed_session2) = create_test_session(&mut os, vec!["exit"], vec![], Some(conversation_id2)).await;

        assert!(
            resumed_session2
                .conversation
                .get_user_prompts()
                .iter()
                .any(|p| p.contains(user_input2)),
            "Second resumed session should contain its message"
        );
        assert!(
            !resumed_session2
                .conversation
                .get_user_prompts()
                .iter()
                .any(|p| p.contains(user_input)),
            "Second resumed session should not contain first session's message"
        );
    }

    #[tokio::test]
    async fn test_list_sessions() {
        use super::list_conversations;

        let mut os = Os::new().await.unwrap();

        let (_, _) = create_test_session(&mut os, vec!["first message", "exit"], vec!["response 1"], None).await;

        let (_, _) = create_test_session(&mut os, vec!["second message", "exit"], vec!["response 2"], None).await;

        let mut output = Vec::new();
        list_conversations(&os, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();

        assert!(output_str.contains("first message"));
        assert!(output_str.contains("second message"));
        assert!(output_str.contains("Chat SessionId:"));
    }

    #[tokio::test]
    async fn test_list_sessions_empty() {
        use super::list_conversations;

        let os = Os::new().await.unwrap();
        let mut output = Vec::new();
        list_conversations(&os, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();

        assert!(output_str.contains("No saved chat sessions"));
    }

    #[tokio::test]
    async fn test_build_session_entries() {
        use super::build_session_entries;

        let mut os = Os::new().await.unwrap();
        let cwd = std::env::current_dir().unwrap();

        let (_, _) = create_test_session(&mut os, vec!["test message", "exit"], vec!["response"], None).await;

        let conversations = os.database.list_conversations_by_path(&cwd).unwrap();
        let entries = build_session_entries(conversations);

        assert_eq!(entries.len(), 1);
        assert!(entries[0].summary.contains("test message"));
    }

    #[tokio::test]
    async fn test_save_and_load_file() {
        async fn test_with_path(path: &str) {
            use super::ChatSubcommand;

            let mut os = Os::new().await.unwrap();
            let user_input = "who is the 16th president of US";

            let (_, mut session) =
                create_test_session(&mut os, vec![user_input, "exit"], vec!["Abraham Lincoln"], None).await;

            ChatSubcommand::Save {
                path: path.to_string(),
                force: true,
            }
            .execute(&mut os, &mut session)
            .await
            .unwrap();

            let (_, mut session2) = create_test_session(&mut os, vec![], vec![], None).await;

            ChatSubcommand::Load { path: path.to_string() }
                .execute(&mut os, &mut session2)
                .await
                .unwrap();

            assert_eq!(
                session.conversation.history().len(),
                session2.conversation.history().len(),
                "Loaded session should have same history length for path: {}",
                path
            );
        }

        // Test relative path
        test_with_path("test_session.json").await;

        // Test tilde expansion
        test_with_path("~/test_tilde.json").await;

        // Test relative with ./
        test_with_path("./test_relative.json").await;

        // Test absolute path
        let os = Os::new().await.unwrap();
        let cwd = os.env.current_dir().unwrap();
        let abs_path = cwd.join("test_absolute.json");
        test_with_path(&abs_path.to_string_lossy()).await;
    }

    #[tokio::test]
    async fn test_chat_new_resets_state_and_preserves_old_session() {
        use super::ChatSubcommand;

        let mut os = Os::new().await.unwrap();
        let user_input = "hello world";

        let (_, mut session) = create_test_session(&mut os, vec![user_input, "exit"], vec!["Hi there!"], None).await;

        let old_conversation_id = session.conversation.conversation_id().to_string();
        assert!(
            !session.conversation.history().is_empty(),
            "Should have history before /chat new"
        );

        ChatSubcommand::New { prompt: vec![] }
            .execute(&mut os, &mut session)
            .await
            .unwrap();

        // Verify state is reset
        assert!(
            session.conversation.history().is_empty(),
            "History should be empty after /chat new"
        );
        assert_ne!(
            session.conversation.conversation_id(),
            old_conversation_id,
            "Should have a new conversation ID"
        );

        // Verify old conversation is preserved in database
        let cwd = std::env::current_dir().unwrap();
        let conversations = os.database.list_conversations_by_path(&cwd).unwrap();
        assert!(
            conversations.iter().any(|(id, _, _, _)| id == &old_conversation_id),
            "Old conversation should be retrievable from database"
        );
    }

    #[tokio::test]
    async fn test_chat_new_with_prompt_returns_handle_input() {
        use super::ChatSubcommand;
        use crate::cli::chat::ChatState;

        let mut os = Os::new().await.unwrap();
        let (_, mut session) = create_test_session(&mut os, vec!["hi", "exit"], vec!["Hey!"], None).await;

        let result = ChatSubcommand::New {
            prompt: vec!["hello".into(), "world".into()],
        }
        .execute(&mut os, &mut session)
        .await
        .unwrap();

        assert!(matches!(result, ChatState::HandleInput { input } if input == "hello world"));
    }

    #[tokio::test]
    async fn test_chat_new_clears_conversation_state_fields() {
        use super::ChatSubcommand;

        let mut os = Os::new().await.unwrap();
        let (_, mut session) = create_test_session(&mut os, vec!["hello", "exit"], vec!["Hi!"], None).await;

        // Pollute fields that should be cleared
        session.conversation.transcript.push_back("old transcript".into());
        session
            .conversation
            .mcp_server_versions
            .insert("test-server".into(), "1.0".into());
        session.conversation.file_line_tracker.insert(
            "foo.rs".into(),
            crate::cli::chat::line_tracker::FileLineTracker::default(),
        );

        ChatSubcommand::New { prompt: vec![] }
            .execute(&mut os, &mut session)
            .await
            .unwrap();

        assert!(
            session.conversation.transcript.is_empty(),
            "transcript should be cleared"
        );
        assert!(
            session.conversation.mcp_server_versions.is_empty(),
            "mcp_server_versions should be cleared"
        );
        assert!(
            session.conversation.file_line_tracker.is_empty(),
            "file_line_tracker should be cleared"
        );
        assert!(
            session.conversation.user_turn_metadata.usage_info.is_empty(),
            "user_turn_metadata should be fresh"
        );
    }

    #[tokio::test]
    async fn test_resume_refreshes_model_info_context_window() {
        let mut os = Os::new().await.unwrap();
        let cwd = std::env::current_dir().unwrap();

        // Create and save a session
        let (conversation_id, _session) =
            create_test_session(&mut os, vec!["hello", "exit"], vec!["Hi there!"], None).await;

        // Load the saved conversation, tamper with context_window_tokens to simulate
        // a stale value from an older client version, then re-save it.
        let mut saved = os
            .database
            .get_conversation_by_id(&conversation_id)
            .unwrap()
            .expect("conversation should exist");
        let stale_window = 42_000;
        if let Some(ref mut info) = saved.model_info {
            info.context_window_tokens = stale_window;
        }
        os.database.set_conversation_by_path(&cwd, &saved).unwrap();

        // Resume the session — the refresh logic should fetch fresh model info
        let (_, resumed) = create_test_session(&mut os, vec!["exit"], vec![], Some(conversation_id)).await;

        // The mock API returns "model-1" with no token_limits, so from_api_model
        // falls back to default_context_window_for_model("model-1") = 200_000.
        // The resumed session should have the fresh value, not the stale 42_000.
        let context_window = resumed
            .conversation
            .model_info
            .as_ref()
            .map(|m| m.context_window_tokens);
        assert_ne!(
            context_window,
            Some(stale_window),
            "context_window_tokens should have been refreshed from the API"
        );
    }

    /// Verifies that `restore_conversation_state` + `update_state(true)` refreshes
    /// the tools list so that MCP servers installed after a session was saved become
    /// available when the session is loaded back.
    #[tokio::test]
    async fn test_restore_conversation_state_refreshes_tools() {
        use std::collections::HashMap;

        use crate::cli::agent::Agents;
        use crate::cli::chat::tool_manager::ToolManager;
        use crate::cli::chat::tools::{
            InputSchema,
            ToolOrigin,
            ToolSpec,
        };

        let mut os = Os::new().await.unwrap();

        let (_, mut session) = create_test_session(&mut os, vec!["hello", "exit"], vec!["Hi!"], None).await;

        // Simulate a tool in the current session's tool_manager schema
        // (as if it was loaded at CLI startup but not yet in the conversation's tools HashMap)
        let new_tool = ToolSpec {
            name: "mcp_new_tool".to_string(),
            description: "A tool from a newly installed server".to_string(),
            input_schema: InputSchema(serde_json::json!({"type": "object", "properties": {}})),
            tool_origin: ToolOrigin::Native,
        };
        session
            .conversation
            .tool_manager
            .schema
            .insert("mcp_new_tool".to_string(), new_tool);

        // Create a "loaded" ConversationState with empty tools (simulating deserialized saved session)
        let loaded_state = ConversationState::new(
            "loaded-conv-id",
            Agents::default(),
            HashMap::new(),
            ToolManager::default(),
            None,
            &os,
            false,
            None,
        )
        .await;

        assert!(loaded_state.tools.is_empty(), "loaded state should start with no tools");

        // Restore then refresh — mirrors what the fixed /chat load does
        let _chat_state = restore_conversation_state(&mut session, loaded_state);
        session.conversation.update_state(true).await;

        let has_mcp_tool = session.conversation.tools.values().flatten().any(|t| match t {
            crate::api_client::model::Tool::ToolSpecification(spec) => spec.name == "mcp_new_tool",
        });

        assert!(
            has_mcp_tool,
            "After restore, tools should include MCP tools from the current tool_manager"
        );
    }
}

#[cfg(test)]
mod kas_tests {
    use std::path::Path;

    use chat_cli_v2::agent::acp::schema::SessionInfoEntry;
    use chat_cli_v2::agent::session::kas::test::KasMockSessionClient;

    use super::*;

    fn kas_entry(session_id: &str, title: Option<&str>, updated_at: Option<&str>) -> SessionInfoEntry {
        SessionInfoEntry {
            session_id: session_id.to_string(),
            cwd: std::path::PathBuf::from("/tmp/project"),
            title: title.map(str::to_string),
            updated_at: updated_at.map(str::to_string),
            // KAS does not emit messageCount today.
            message_count: None,
            execution_target: None,
        }
    }

    #[tokio::test]
    async fn render_empty_shows_no_sessions_message() {
        let mut buf = Vec::new();
        render_session_entries(&mut buf, "/tmp/project", &[]).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(
            output.contains("No saved chat sessions for /tmp/project"),
            "got: {output:?}"
        );
    }

    #[tokio::test]
    async fn render_multiple_sessions_shows_header_ids_titles_and_footer() {
        let entries = vec![
            SessionEntry {
                session_id: "sess_v1".into(),
                summary: "V1 session".into(),
                msg_count: Some(4),
                updated_at_ms: 1_735_689_600_000, // 2025-01-01
                source: SessionSource::V1,
                execution_target: None,
            },
            SessionEntry {
                session_id: "sess_v2".into(),
                summary: "V2 session".into(),
                msg_count: Some(12),
                updated_at_ms: 1_735_776_000_000, // 2025-01-02
                source: SessionSource::V2,
                execution_target: None,
            },
            SessionEntry {
                session_id: "sess_kas".into(),
                summary: "(no title)".into(),
                msg_count: None,
                updated_at_ms: 1_735_862_400_000, // 2025-01-03
                source: SessionSource::Kas,
                execution_target: Some("cloud-sandbox".into()),
            },
        ];
        let mut buf = Vec::new();
        render_session_entries(&mut buf, "/tmp/project", &entries).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(output.contains("Chat sessions for /tmp/project:"));
        assert!(output.contains("sess_v1") && output.contains("V1 session"));
        assert!(output.contains("sess_v2") && output.contains("V2 session"));
        assert!(output.contains("sess_kas") && output.contains("(no title)"));
        assert!(output.contains("4 msgs") && output.contains("12 msgs"));
        assert_eq!(output.matches("msgs").count(), 2);
        for src in &["classic", "v2", "v3"] {
            assert!(output.contains(src), "source column should show {src}");
        }
        // The cloud-sandbox V3 row shows a WHERE tag; local rows do not.
        assert!(
            output.contains("cloud"),
            "cloud-sandbox row should show a 'cloud' WHERE tag"
        );
        // Delete-hint footer.
        assert!(output.contains("To delete a session, use: kiro-cli chat --delete-session"));
    }

    #[tokio::test]
    async fn collect_kas_sessions_converts_entries() {
        let client = KasMockSessionClient::new().with_list(vec![
            kas_entry("sess_abc", Some("Fix auth bug"), Some("2026-01-02T00:00:00Z")),
            kas_entry("sess_def", None, None),
        ]);
        let entries = collect_kas_sessions(&client, Path::new("/tmp/project")).await.unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].session_id, "sess_abc");
        assert_eq!(entries[0].summary, "Fix auth bug");
        assert_eq!(entries[0].msg_count, None);
        assert_eq!(entries[0].source, SessionSource::Kas);

        // Null title falls back to the "(no title)" placeholder.
        assert_eq!(entries[1].summary, "(no title)");
        // Null updated_at -> 0 ms (sorted last, but `collect` does not sort KAS alone).
        assert_eq!(entries[1].updated_at_ms, 0);
    }

    #[test]
    fn kas_session_entry_carries_execution_target() {
        // A remote row's `_meta.kiro.executionTarget.kind` (already parsed into the
        // schema entry) must survive the KAS -> unified SessionEntry conversion so the
        // merged listing can tag WHERE it runs.
        let mut cloud = kas_entry("sess_cloud", Some("Remote task"), Some("2026-01-04T00:00:00Z"));
        cloud.execution_target = Some("cloud-sandbox".to_string());
        let entry: SessionEntry = cloud.into();
        assert_eq!(entry.execution_target.as_deref(), Some("cloud-sandbox"));
        assert_eq!(entry.source, SessionSource::Kas);

        // A row without the field maps to None (treated as local).
        let local: SessionEntry = kas_entry("sess_local", None, None).into();
        assert_eq!(local.execution_target, None);
    }

    #[tokio::test]
    async fn collect_kas_sessions_propagates_error() {
        let client = KasMockSessionClient::new().with_list_err("rpc timeout");
        let err = collect_kas_sessions(&client, Path::new("/tmp")).await.unwrap_err();
        assert!(format!("{err:#}").contains("rpc timeout"));
    }

    /// Pins the `--list-sessions --format json` envelope shape. The
    /// TUI consumes this output via `KIRO_CHAT_CLI_BIN` and any
    /// silent rename in the field set will break the picker without
    /// a clear error.
    #[test]
    fn session_listing_json_shape() {
        let cwd = std::env::current_dir().unwrap();
        let entries = vec![
            SessionEntry {
                session_id: "v2-session-1".to_string(),
                summary: "Fix auth bug".to_string(),
                msg_count: Some(7),
                updated_at_ms: 1_700_000_000_000,
                source: SessionSource::V2,
                execution_target: None,
            },
            SessionEntry {
                session_id: "kas-session-2".to_string(),
                summary: "(no title)".to_string(),
                msg_count: None,
                updated_at_ms: 1_700_000_001_000,
                source: SessionSource::Kas,
                execution_target: Some("cloud-sandbox".to_string()),
            },
        ];
        let json = SessionListingJson::from_entries(&cwd, &entries);
        let parsed: serde_json::Value = serde_json::from_str(&serde_json::to_string(&json).unwrap()).unwrap();
        assert!(parsed.get("cwd").and_then(|v| v.as_str()).is_some());
        let sessions = parsed.get("sessions").and_then(|v| v.as_array()).unwrap();
        assert_eq!(sessions.len(), 2);

        // V2 entry pins every field name and the RFC3339 millis-precision format.
        let v2 = &sessions[0];
        assert_eq!(v2.get("sessionId").and_then(|v| v.as_str()), Some("v2-session-1"));
        assert_eq!(v2.get("source").and_then(|v| v.as_str()), Some("v2"));
        assert_eq!(v2.get("title").and_then(|v| v.as_str()), Some("Fix auth bug"));
        assert_eq!(v2.get("messageCount").and_then(|v| v.as_u64()), Some(7));
        assert_eq!(
            v2.get("updatedAt").and_then(|v| v.as_str()),
            Some("2023-11-14T22:13:20.000Z")
        );
        // Local (V2) rows omit executionTarget entirely (not null) — a consumer
        // reads its absence as local. Pins the WHERE field's dark-safe shape.
        assert!(
            v2.get("executionTarget").is_none(),
            "executionTarget must be omitted for local rows"
        );

        // KAS entry: source serializes as "v3" (matching Display) and
        // messageCount is omitted entirely (not null) when absent.
        let kas = &sessions[1];
        assert_eq!(kas.get("source").and_then(|v| v.as_str()), Some("v3"));
        assert!(
            kas.get("messageCount").is_none(),
            "messageCount must be omitted when None"
        );
        // A cloud-sandbox V3 row carries the WHERE tag as camelCase `executionTarget`.
        assert_eq!(
            kas.get("executionTarget").and_then(|v| v.as_str()),
            Some("cloud-sandbox")
        );
    }

    #[test]
    fn session_listing_json_canonicalizes_cwd() {
        let entries: Vec<SessionEntry> = vec![];
        // `/tmp` is a symlink to `/private/tmp` on macOS - canonicalize
        // resolves it. On Linux the path is already canonical and the
        // assertion still holds (canonical of `/tmp` == `/tmp`).
        let canonical = std::fs::canonicalize("/tmp").unwrap();
        let json = SessionListingJson::from_entries(Path::new("/tmp"), &entries);
        assert_eq!(json.cwd, canonical.display().to_string());
    }

    #[test]
    fn session_source_serializes_via_display() {
        assert_eq!(serde_json::to_string(&SessionSource::V1).unwrap(), "\"classic\"");
        assert_eq!(serde_json::to_string(&SessionSource::V2).unwrap(), "\"v2\"");
        assert_eq!(serde_json::to_string(&SessionSource::Kas).unwrap(), "\"v3\"");
    }

    // The Rust `--list-sessions` caps parse (parity with the TS handshake).
    // `advertises_remote_session_source` decides whether the list
    // request asks for remote rows (`sessionSource: "all"`). Lives here because
    // `chat_cli_v2` is `#![cfg(not(test))]` (its own unit tests don't run).
    fn caps(v: serde_json::Value) -> Option<serde_json::Map<String, serde_json::Value>> {
        v.as_object().cloned()
    }

    #[test]
    fn advertises_remote_when_sessionsources_has_remote() {
        assert!(chat_cli_v2::agent::session::kas::advertises_remote_session_source(
            &caps(serde_json::json!({ "kiro": { "sessionSources": ["local", "remote"] } }))
        ));
    }

    #[test]
    fn does_not_advertise_remote_when_only_local_or_malformed() {
        use chat_cli_v2::agent::session::kas::advertises_remote_session_source as adv;
        assert!(!adv(&caps(
            serde_json::json!({ "kiro": { "sessionSources": ["local"] } })
        )));
        // Dark-safe defaults: absent/empty/malformed `_meta` -> local-only list.
        assert!(!adv(&None));
        assert!(!adv(&caps(serde_json::json!({}))));
        assert!(!adv(&caps(serde_json::json!({ "kiro": {} }))));
        assert!(!adv(&caps(
            serde_json::json!({ "kiro": { "sessionSources": "remote" } })
        )));
    }

    #[test]
    fn kiro_meta_string_reads_description_and_created_at() {
        use chat_cli_v2::agent::session::kas::kiro_meta_string as k;
        // Remote row: name = description, age = createdAt (under `_meta.kiro`).
        let meta = caps(serde_json::json!({
            "kiro": { "description": "Fix login retry", "createdAt": "2026-07-06T12:00:00Z" }
        }));
        assert_eq!(k(&meta, "description").as_deref(), Some("Fix login retry"));
        assert_eq!(k(&meta, "createdAt").as_deref(), Some("2026-07-06T12:00:00Z"));
    }

    #[test]
    fn kiro_meta_string_is_none_for_local_or_malformed_rows() {
        use chat_cli_v2::agent::session::kas::kiro_meta_string as k;
        // Local row (no `_meta.kiro`) / missing key / non-string -> None, so the merged
        // list keeps the standard ACP title/updatedAt (dark-safe, byte-identical local).
        assert_eq!(k(&None, "description"), None);
        assert_eq!(k(&caps(serde_json::json!({})), "description"), None);
        assert_eq!(k(&caps(serde_json::json!({ "kiro": {} })), "createdAt"), None);
        assert_eq!(
            k(
                &caps(serde_json::json!({ "kiro": { "description": 42 } })),
                "description"
            ),
            None
        );
    }
}
