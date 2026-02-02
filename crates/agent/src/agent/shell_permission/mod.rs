//! Shell permission system for evaluating command safety.
//!
//! The system uses a 3-layer evaluation approach:
//! 1. Parse - Parse with tree-sitter, split chained commands
//! 2. Detect - Dangerous patterns, environment manipulation, readonly check
//! 3. Decide - Policy rules, user settings, aggregate results

mod detector;
mod parser;

use detector::{
    DangerLevel,
    detect,
};
use parser::parse_command;
use serde::{
    Deserialize,
    Serialize,
};

/// Result of shell permission evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShellPermissionResult {
    /// Command is allowed to execute without user confirmation.
    Allow,
    /// Command requires user confirmation before execution.
    Ask {
        /// Optional reason explaining why confirmation is needed.
        reason: Option<String>,
    },
    /// Command is denied with specific reasons.
    Deny {
        /// Explanation of why the command was denied.
        reason: String,
        /// Patterns that matched and caused the denial.
        matched_patterns: Vec<String>,
    },
}

/// Settings for shell permission evaluation.
#[derive(Debug, Clone, Default)]
pub struct ShellPermissionSettings {
    /// Commands that are explicitly allowed (glob patterns).
    pub allowed_commands: Vec<String>,
    /// Commands that are explicitly denied (glob patterns).
    pub denied_commands: Vec<String>,
    /// Whether to auto-allow readonly commands.
    pub auto_allow_readonly: bool,
    /// Whether to deny commands not in the allow list.
    pub deny_by_default: bool,
    /// Whether the tool is in the agent's allowed tools list.
    pub is_tool_allowed: bool,
}

/// Evaluate shell permission for a command.
pub fn evaluate_shell_permission(command: &str, settings: &ShellPermissionSettings) -> ShellPermissionResult {
    // Layer 1: Parse command
    let parse_result = parse_command(command);

    // If parsing failed, be conservative
    if parse_result.parse_failed {
        return ShellPermissionResult::Ask { reason: None };
    }

    // Layer 2: Detect
    let detection = detect(&parse_result.commands);

    // Layer 3: Decide (stub - to be implemented in PR3)
    // TODO: Apply policy rules
    // TODO: Apply user settings
    // TODO: Aggregate results
    if detection.danger_level != DangerLevel::None {
        return ShellPermissionResult::Ask { reason: None };
    }
    if settings.auto_allow_readonly && detection.is_readonly {
        return ShellPermissionResult::Allow;
    }

    if settings.is_tool_allowed {
        ShellPermissionResult::Allow
    } else {
        ShellPermissionResult::Ask { reason: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_readonly_command_allowed() {
        let settings = ShellPermissionSettings {
            auto_allow_readonly: true,
            ..Default::default()
        };
        let result = evaluate_shell_permission("ls -la", &settings);
        assert_eq!(result, ShellPermissionResult::Allow);
    }

    #[test]
    fn test_readonly_disabled_asks() {
        let settings = ShellPermissionSettings {
            auto_allow_readonly: false,
            ..Default::default()
        };
        let result = evaluate_shell_permission("ls -la", &settings);
        assert!(matches!(result, ShellPermissionResult::Ask { .. }));
    }

    #[test]
    fn test_tool_allowed_allows() {
        let settings = ShellPermissionSettings {
            is_tool_allowed: true,
            ..Default::default()
        };
        let result = evaluate_shell_permission("rm -rf /", &settings);
        assert_eq!(result, ShellPermissionResult::Allow);
    }

    #[test]
    fn test_dangerous_command_asks() {
        let settings = ShellPermissionSettings::default();
        let result = evaluate_shell_permission("find . -exec rm {} \\;", &settings);
        assert!(matches!(result, ShellPermissionResult::Ask { .. }));
    }
}
