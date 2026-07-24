use std::collections::HashMap;
use std::io::Write;

use agent::permissions::PathAccessType;
use crossterm::{
    execute,
    queue,
    style,
};
use dialoguer::Select;

use super::ChatError;
use super::conversation::ConversationState;
use super::tools::tool::ToolMetadata;
use super::tools::{
    QueuedTool,
    Tool,
};
use crate::cli::agent::wrapper_types::ToolSettingTarget;
use crate::database::settings::Setting;
use crate::os::Os;
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

/// Check if a tool supports granular trust options.
fn supports_granular_trust(tool: &Tool) -> bool {
    matches!(tool, Tool::ExecuteCommand(_) | Tool::FsRead(_) | Tool::FsWrite(_))
}

/// Prompt user to select trust scope from available options.
/// For tools with granular options (ExecuteCommand, FsRead, FsWrite), shows options.
/// For all others, returns TrustTool directly.
pub fn prompt_trust_scope(
    tool_use: &QueuedTool,
    stderr: &mut impl Write,
    stdout: &mut impl Write,
    os: &Os,
) -> TrustScopeSelection {
    let disable_granular_trust = os
        .database
        .settings
        .get_bool(Setting::ChatDisableGranularTrust)
        .unwrap_or(false);
    if disable_granular_trust || !supports_granular_trust(&tool_use.tool) {
        // No granular options for this tool type
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
                // Check if this is a filesystem path trust (runtime_read_paths or runtime_write_paths)
                if let Some(access_type) = PathAccessType::from_setting_key(&selected.setting_key) {
                    // Store in runtime_permissions (session-scoped, not persisted)
                    for path in &selected.patterns {
                        agent
                            .runtime_permissions
                            .grant_path_canonicalized(path.clone(), access_type);
                    }
                } else {
                    // For other tools (e.g., execute_bash with command patterns),
                    // store in tools_settings (persisted to agent config).
                    let key = resolve_tools_settings_key(&tool_use.name, &agent.tools_settings);
                    let settings = agent
                        .tools_settings
                        .entry(ToolSettingTarget(key))
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
            conversation.agents.trust_tools(vec![formatted_tool_name.clone()]);

            if let Some(agent) = conversation.agents.get_active() {
                agent
                    .print_overridden_permission_for_tool(&formatted_tool_name, stderr)
                    .map_err(|_e| ChatError::Custom("Failed to validate agent tool settings".into()))?;
            }
            Ok(true)
        },
        TrustScopeSelection::AllowOnce => Ok(true),
        TrustScopeSelection::Cancelled => Ok(false),
    }
}

/// Resolve the `tools_settings` key for a tool by checking its aliases against existing entries.
///
/// When the agent config uses one alias (e.g. `"shell"`) but the LLM sends the spec name
/// (e.g. `"execute_bash"`), we need to find the existing key so we merge into it rather than
/// creating a duplicate entry that shadows the original config.
pub(crate) fn resolve_tools_settings_key(
    tool_name: &str,
    tools_settings: &HashMap<ToolSettingTarget, serde_json::Value>,
) -> String {
    ToolMetadata::get_by_spec_name(tool_name)
        .and_then(|info| {
            info.aliases
                .iter()
                .find(|alias| tools_settings.contains_key(**alias))
                .map(|alias| (*alias).to_string())
        })
        .unwrap_or_else(|| tool_name.to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::cli::agent::{
        Agent,
        Agents,
    };
    use crate::cli::chat::tool_manager::ToolManager;
    use crate::os::Os;

    #[tokio::test]
    async fn test_trust_tool_warning_scoped_to_newly_trusted_tool() {
        let mut os = Os::new().await.unwrap();

        let mut agent = Agent::default();
        agent.allowed_tools.insert("execute_bash".to_string());
        agent.tools_settings.insert(
            ToolSettingTarget("execute_bash".to_string()),
            json!({ "allowedCommands": ["git status"] }),
        );
        let mut agents = Agents::default();
        agents.agents.insert("test-agent".to_string(), agent);
        agents.active_idx = "test-agent".to_string();

        let mut output = vec![];
        let mut tool_manager = ToolManager::default();
        let tool_config = tool_manager.load_tools(&mut os, &mut output).await.unwrap();
        let mut conversation = ConversationState::new(
            "fake_conv_id",
            agents,
            tool_config,
            tool_manager,
            None,
            &os,
            false,
            None,
        )
        .await;

        let tool_use = QueuedTool {
            id: "tool_1".to_string(),
            name: "jira_get_issue".to_string(),
            preferred_alias: "jira_get_issue".to_string(),
            accepted: false,
            tool: Tool::Introspect(serde_json::from_value(json!({})).unwrap()),
            tool_input: json!({}),
            trust_options: vec![],
        };

        let mut stderr: Vec<u8> = vec![];
        let executed = apply_trust_selection(
            TrustScopeSelection::TrustTool,
            &tool_use,
            &mut conversation,
            &mut stderr,
        )
        .unwrap();
        assert!(executed);

        let agent = conversation.agents.get_active().unwrap();
        assert!(agent.allowed_tools.contains("jira_get_issue"));
        assert!(agent.allowed_tools.contains("execute_bash"));

        let warning_output = String::from_utf8_lossy(&stderr);
        assert!(
            !warning_output.contains("execute_bash"),
            "trusting one tool must not print warnings about other trusted tools, got: {warning_output}"
        );
    }

    #[tokio::test]
    async fn test_trust_tool_warns_when_trusted_tool_has_overridden_settings() {
        let mut os = Os::new().await.unwrap();

        let mut agent = Agent::default();
        agent.tools_settings.insert(
            ToolSettingTarget("execute_bash".to_string()),
            json!({ "allowedCommands": ["git status"] }),
        );
        let mut agents = Agents::default();
        agents.agents.insert("test-agent".to_string(), agent);
        agents.active_idx = "test-agent".to_string();

        let mut output = vec![];
        let mut tool_manager = ToolManager::default();
        let tool_config = tool_manager.load_tools(&mut os, &mut output).await.unwrap();
        let mut conversation = ConversationState::new(
            "fake_conv_id",
            agents,
            tool_config,
            tool_manager,
            None,
            &os,
            false,
            None,
        )
        .await;

        let tool_use = QueuedTool {
            id: "tool_1".to_string(),
            name: "execute_bash".to_string(),
            preferred_alias: "execute_bash".to_string(),
            accepted: false,
            tool: Tool::Introspect(serde_json::from_value(json!({})).unwrap()),
            tool_input: json!({}),
            trust_options: vec![],
        };

        let mut stderr: Vec<u8> = vec![];
        apply_trust_selection(
            TrustScopeSelection::TrustTool,
            &tool_use,
            &mut conversation,
            &mut stderr,
        )
        .unwrap();

        let warning_output = String::from_utf8_lossy(&stderr);
        assert!(
            warning_output.contains("execute_bash"),
            "trusting a tool with overridden settings must warn about it, got: {warning_output}"
        );
    }
}
