use std::io::Write;

use crossterm::{
    execute,
    queue,
    style,
};
use dialoguer::Select;

use super::ChatError;
use super::conversation::ConversationState;
use super::tools::{
    QueuedTool,
    Tool,
};
use crate::cli::agent::wrapper_types::ToolSettingTarget;
use crate::theme::StyledText;
use crate::util::{
    MCP_SERVER_TOOL_DELIMITER,
    dialoguer_theme,
};

/// Result of trust scope selection
pub enum TrustScopeSelection {
    /// User selected a granular trust pattern
    Pattern(agent::protocol::TrustOption),
    /// User selected to trust the entire tool
    TrustTool,
    /// User selected to allow just this one execution
    AllowOnce,
    /// User cancelled the selection (Escape)
    Cancelled,
}

/// Prompt user to select trust scope from available options.
/// For ExecuteCommand tools, shows granular options. For all others, returns TrustTool directly.
pub fn prompt_trust_scope(
    tool_use: &QueuedTool,
    stderr: &mut impl Write,
    stdout: &mut impl Write,
) -> TrustScopeSelection {
    if !matches!(tool_use.tool, Tool::ExecuteCommand(_)) {
        // Currently, granular options are supported only for ExecuteCommand
        return TrustScopeSelection::TrustTool;
    }

    let options = &tool_use.trust_options;
    let mut items: Vec<(String, TrustScopeSelection)> = Vec::new();

    // Tools may return empty trust options if they don't deem these are safe to trust this or just
    // don't support allowing settings for this particular action.
    // Eg: In shell, dangerousCommands supersedes allowCommands hence adding that to allowed commands
    // doesn't help
    if options.is_empty() {
        queue!(
            stderr,
            StyledText::brand_fg(),
            style::Print(" Note: Granular trust options are not available for this action\n"),
            StyledText::reset(),
        )
        .ok();
        items.push(("Allow action".into(), TrustScopeSelection::AllowOnce));
        items.push(("Trust Tool".into(), TrustScopeSelection::TrustTool));
    } else {
        for opt in options {
            items.push((
                format!("{:<20} → {}", opt.label, &opt.display),
                TrustScopeSelection::Pattern(opt.clone()),
            ));
        }
        items.push((format!("{:<20} → *", "Entire Tool"), TrustScopeSelection::TrustTool));
    }

    let display: Vec<&str> = items.iter().map(|(s, _)| s.as_str()).collect();

    stderr.flush().ok();
    stdout.flush().ok();

    let selected = match Select::with_theme(&dialoguer_theme())
        .with_prompt(format!(
            "{}({}) {} · ({}) {}",
            StyledText::secondary("Press "),
            StyledText::current_item("↑↓"),
            StyledText::secondary("to navigate"),
            StyledText::current_item("⏎"),
            StyledText::secondary("to select scope")
        ))
        .items(&display)
        .default(0)
        .report(false)
        .interact_on_opt(&dialoguer::console::Term::stderr())
    {
        Ok(Some(idx)) => idx,
        Ok(None) => {
            queue!(
                stderr,
                crossterm::cursor::MoveToColumn(0),
                crossterm::cursor::MoveUp(items.len() as u16 + 1),
                crossterm::terminal::Clear(crossterm::terminal::ClearType::FromCursorDown),
            )
            .ok();
            return TrustScopeSelection::Cancelled;
        },
        Err(_) => return TrustScopeSelection::Cancelled,
    };

    queue!(
        stderr,
        crossterm::cursor::MoveUp(1),
        crossterm::terminal::Clear(crossterm::terminal::ClearType::FromCursorDown),
    )
    .ok();

    items
        .into_iter()
        .nth(selected)
        .map_or(TrustScopeSelection::Cancelled, |(_, s)| s)
}

/// Apply the user's trust scope selection.
/// Returns `true` if the tool should be executed, `false` if cancelled.
pub fn apply_trust_selection(
    selection: TrustScopeSelection,
    tool_use: &QueuedTool,
    conversation: &mut ConversationState,
    stderr: &mut impl Write,
) -> Result<bool, ChatError> {
    match selection {
        TrustScopeSelection::Pattern(selected) => {
            if let Some(agent) = conversation.agents.get_active_mut() {
                let settings = agent
                    .tools_settings
                    .entry(ToolSettingTarget(tool_use.name.clone()))
                    .or_insert_with(|| serde_json::json!({ &selected.setting_key: [] }));

                if let Some(obj) = settings.as_object_mut() {
                    let arr: &mut serde_json::Value = obj
                        .entry(&selected.setting_key)
                        .or_insert_with(|| serde_json::json!([]));
                    if let Some(arr) = arr.as_array_mut() {
                        for pattern in &selected.patterns {
                            arr.push(serde_json::Value::String(pattern.clone()));
                        }
                    }
                }

                execute!(
                    stderr,
                    StyledText::success_fg(),
                    style::Print("\n✓ Trusted: "),
                    StyledText::brand_fg(),
                    style::Print(&selected.display),
                    StyledText::reset(),
                    style::Print("\n\n"),
                )
                .ok();
            }
            Ok(true)
        },
        TrustScopeSelection::TrustTool => {
            let formatted_tool_name = conversation
                .tool_manager
                .tn_map
                .get(&tool_use.name)
                .map(|info| {
                    format!(
                        "@{}{MCP_SERVER_TOOL_DELIMITER}{}",
                        info.server_name, info.host_tool_name
                    )
                })
                .unwrap_or(tool_use.name.clone());
            conversation.agents.trust_tools(vec![formatted_tool_name]);

            if let Some(agent) = conversation.agents.get_active() {
                agent
                    .print_overridden_permissions(stderr)
                    .map_err(|_e| ChatError::Custom("Failed to validate agent tool settings".into()))?;
            }
            Ok(true)
        },
        TrustScopeSelection::AllowOnce => Ok(true),
        TrustScopeSelection::Cancelled => Ok(false),
    }
}
