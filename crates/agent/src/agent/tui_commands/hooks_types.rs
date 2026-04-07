//! Hook info types for the /hooks command

use serde::{
    Deserialize,
    Serialize,
};
use typeshare::typeshare;

use crate::agent::agent_config::definitions::{
    HookConfig,
    HookTrigger,
};

/// Information about a configured hook
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookInfo {
    /// The trigger type (e.g. agentSpawn, preToolUse)
    pub trigger: HookTrigger,
    /// The shell command to run
    pub command: String,
    /// Optional glob matcher for tool-scoped hooks
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matcher: Option<String>,
}

impl HookInfo {
    /// Create a `HookInfo` from a trigger and its config.
    pub fn from_config(trigger: HookTrigger, config: &HookConfig) -> Self {
        let (command, matcher) = match config {
            HookConfig::ShellCommand(h) => (h.command.clone(), h.opts.matcher.clone()),
            HookConfig::Tool(h) => (format!("tool:{}", h.tool_name), h.opts.matcher.clone()),
        };
        Self {
            trigger,
            command,
            matcher,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::agent_config::definitions::{
        BaseHookConfig,
        CommandHook,
        ToolHook,
    };

    #[test]
    fn test_hook_info_serialize() {
        let hook = HookInfo {
            trigger: HookTrigger::PreToolUse,
            command: "echo hello".to_string(),
            matcher: Some("fs_write".to_string()),
        };
        let json = serde_json::to_string(&hook).unwrap();
        assert!(json.contains(r#""trigger":"preToolUse""#));
        assert!(json.contains(r#""command":"echo hello""#));
        assert!(json.contains(r#""matcher":"fs_write""#));
    }

    #[test]
    fn test_hook_info_serialize_no_matcher() {
        let hook = HookInfo {
            trigger: HookTrigger::AgentSpawn,
            command: "git status".to_string(),
            matcher: None,
        };
        let json = serde_json::to_string(&hook).unwrap();
        assert!(json.contains(r#""trigger":"agentSpawn""#));
        assert!(json.contains(r#""command":"git status""#));
        // matcher should be omitted when None
        assert!(!json.contains("matcher"));
    }

    #[test]
    fn test_hook_info_deserialize() {
        let json = r#"{"trigger":"postToolUse","command":"validate.sh","matcher":"@mcp-server"}"#;
        let hook: HookInfo = serde_json::from_str(json).unwrap();
        assert_eq!(hook.trigger, HookTrigger::PostToolUse);
        assert_eq!(hook.command, "validate.sh");
        assert_eq!(hook.matcher, Some("@mcp-server".to_string()));
    }

    #[test]
    fn test_hook_info_deserialize_no_matcher() {
        let json = r#"{"trigger":"stop","command":"cleanup.sh"}"#;
        let hook: HookInfo = serde_json::from_str(json).unwrap();
        assert_eq!(hook.trigger, HookTrigger::Stop);
        assert_eq!(hook.command, "cleanup.sh");
        assert_eq!(hook.matcher, None);
    }

    #[test]
    fn test_from_config_shell_command() {
        let config = HookConfig::ShellCommand(CommandHook {
            command: "echo hello".to_string(),
            opts: BaseHookConfig {
                matcher: Some("fs_write".to_string()),
                ..Default::default()
            },
        });
        let info = HookInfo::from_config(HookTrigger::PreToolUse, &config);
        assert_eq!(info.trigger, HookTrigger::PreToolUse);
        assert_eq!(info.command, "echo hello");
        assert_eq!(info.matcher, Some("fs_write".to_string()));
    }

    #[test]
    fn test_from_config_tool_hook() {
        let config = HookConfig::Tool(ToolHook {
            tool_name: "my_tool".to_string(),
            args: serde_json::json!({}),
            opts: BaseHookConfig::default(),
        });
        let info = HookInfo::from_config(HookTrigger::PostToolUse, &config);
        assert_eq!(info.trigger, HookTrigger::PostToolUse);
        assert_eq!(info.command, "tool:my_tool");
        assert_eq!(info.matcher, None);
    }

    #[test]
    fn test_from_config_no_matcher() {
        let config = HookConfig::ShellCommand(CommandHook {
            command: "git status".to_string(),
            opts: BaseHookConfig::default(),
        });
        let info = HookInfo::from_config(HookTrigger::AgentSpawn, &config);
        assert_eq!(info.trigger, HookTrigger::AgentSpawn);
        assert_eq!(info.command, "git status");
        assert_eq!(info.matcher, None);
    }
}
