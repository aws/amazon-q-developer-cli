//! /hooks command execution — lists configured hooks

use agent::tui_commands::{
    CommandResult,
    HookInfo,
};
use serde_json::json;

use super::CommandContext;

pub async fn execute(ctx: &CommandContext<'_>) -> CommandResult {
    let config = ctx.agent_configs.iter().find(|c| c.name() == ctx.current_agent_name);

    let hooks: Vec<HookInfo> = match config {
        Some(config) => {
            let mut hooks: Vec<HookInfo> = config
                .hooks()
                .iter()
                .flat_map(|(trigger, configs)| configs.iter().map(|config| HookInfo::from_config(*trigger, config)))
                .collect();
            hooks.sort_by(|a, b| {
                a.trigger
                    .to_string()
                    .cmp(&b.trigger.to_string())
                    .then(a.command.cmp(&b.command))
            });
            hooks
        },
        None => Vec::new(),
    };

    if hooks.is_empty() {
        return CommandResult::success_with_data(
            "No hooks configured",
            json!({ "hooks": [], "message": "No hooks configured" }),
        );
    }

    let message = format!(
        "{} hook{} configured",
        hooks.len(),
        if hooks.len() == 1 { "" } else { "s" }
    );

    let hooks_json: Vec<serde_json::Value> = hooks
        .iter()
        .map(|h| {
            let mut obj = json!({
                "trigger": h.trigger,
                "command": h.command,
            });
            if let Some(matcher) = &h.matcher {
                obj["matcher"] = json!(matcher);
            }
            obj
        })
        .collect();

    CommandResult::success_with_data(&message, json!({ "hooks": hooks_json, "message": message }))
}

#[cfg(test)]
mod tests {
    use agent::agent_config::definitions::HookTrigger;
    use agent::tui_commands::HookInfo;
    use serde_json::json;

    /// Helper to simulate what execute() does with hook data.
    /// Tests the JSON formatting logic.
    fn format_hooks_response(hooks: Vec<HookInfo>) -> serde_json::Value {
        if hooks.is_empty() {
            return json!({ "hooks": [], "message": "No hooks configured" });
        }

        let message = format!(
            "{} hook{} configured",
            hooks.len(),
            if hooks.len() == 1 { "" } else { "s" }
        );

        let hooks_json: Vec<serde_json::Value> = hooks
            .iter()
            .map(|h| {
                let mut obj = json!({
                    "trigger": h.trigger,
                    "command": h.command,
                });
                if let Some(matcher) = &h.matcher {
                    obj["matcher"] = json!(matcher);
                }
                obj
            })
            .collect();

        json!({ "hooks": hooks_json, "message": message })
    }

    #[test]
    fn test_empty_hooks_response() {
        let result = format_hooks_response(vec![]);
        assert_eq!(result["hooks"].as_array().unwrap().len(), 0);
        assert_eq!(result["message"], "No hooks configured");
    }

    #[test]
    fn test_single_hook_response() {
        let hooks = vec![HookInfo {
            trigger: HookTrigger::AgentSpawn,
            command: "git status".to_string(),
            matcher: None,
        }];
        let result = format_hooks_response(hooks);
        assert_eq!(result["message"], "1 hook configured");
        let hooks_arr = result["hooks"].as_array().unwrap();
        assert_eq!(hooks_arr.len(), 1);
        assert_eq!(hooks_arr[0]["trigger"], "agentSpawn");
        assert_eq!(hooks_arr[0]["command"], "git status");
        assert!(hooks_arr[0].get("matcher").is_none());
    }

    #[test]
    fn test_multiple_hooks_response() {
        let hooks = vec![
            HookInfo {
                trigger: HookTrigger::PreToolUse,
                command: "validate.sh".to_string(),
                matcher: Some("fs_write".to_string()),
            },
            HookInfo {
                trigger: HookTrigger::PostToolUse,
                command: "audit.sh".to_string(),
                matcher: Some("*".to_string()),
            },
            HookInfo {
                trigger: HookTrigger::Stop,
                command: "cleanup.sh".to_string(),
                matcher: None,
            },
        ];
        let result = format_hooks_response(hooks);
        assert_eq!(result["message"], "3 hooks configured");
        let hooks_arr = result["hooks"].as_array().unwrap();
        assert_eq!(hooks_arr.len(), 3);
        assert_eq!(hooks_arr[0]["trigger"], "preToolUse");
        assert_eq!(hooks_arr[1]["trigger"], "postToolUse");
        assert_eq!(hooks_arr[0]["matcher"], "fs_write");
        assert_eq!(hooks_arr[1]["matcher"], "*");
        assert!(hooks_arr[2].get("matcher").is_none());
    }

    #[test]
    fn test_hook_with_matcher_included_in_json() {
        let hooks = vec![HookInfo {
            trigger: HookTrigger::PreToolUse,
            command: "check-perms.sh".to_string(),
            matcher: Some("@builtin".to_string()),
        }];
        let result = format_hooks_response(hooks);
        let hook = &result["hooks"][0];
        assert_eq!(hook["matcher"], "@builtin");
    }
}
