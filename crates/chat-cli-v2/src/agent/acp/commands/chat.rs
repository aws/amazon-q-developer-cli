//! /chat command — session listing and loading

use agent::tui_commands::{
    ChatArgs,
    CommandOption,
    CommandResult,
};

use super::CommandContext;
use crate::agent::acp::schema::SessionInfoEntry;
use crate::agent::acp::session_manager::SessionManagerHandle;

const TITLE_NOT_AVAILABLE: &str = "<title not available>";

pub async fn execute(_args: &ChatArgs, _ctx: &CommandContext<'_>) -> CommandResult {
    CommandResult::success("")
}

/// List sessions with title backfill. Shared by both the `_kiro.dev/session/list`
/// extension handler and the `getCommandOptions` path.
pub async fn list_sessions(
    session_tx: &SessionManagerHandle,
    cwd: Option<std::path::PathBuf>,
) -> Result<Vec<SessionInfoEntry>, sacp::Error> {
    let sessions = session_tx.list_sessions(cwd).await?;
    let sessions_dir = crate::util::paths::sessions_dir().ok();
    Ok(sessions
        .into_iter()
        .map(|s| {
            let title = s.title.or_else(|| {
                sessions_dir
                    .as_ref()
                    .and_then(|d| crate::agent::session::title_from_first_log_entry(d, &s.session_id))
            });
            SessionInfoEntry {
                session_id: s.session_id,
                cwd: s.cwd,
                title,
                updated_at: Some(s.updated_at.to_rfc3339()),
            }
        })
        .collect())
}

impl From<SessionInfoEntry> for CommandOption {
    fn from(s: SessionInfoEntry) -> Self {
        let label = format!(
            "{} ({})",
            s.title.as_deref().unwrap_or(TITLE_NOT_AVAILABLE),
            agent::util::truncate_safe(&s.session_id, 8),
        );
        let description = s.updated_at.map(|t| {
            crate::util::format_relative_time(&t.parse::<chrono::DateTime<chrono::Utc>>().unwrap_or_default())
        });
        CommandOption {
            value: s.session_id,
            label,
            description,
            group: None,
            hint: None,
        }
    }
}
