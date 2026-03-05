//! Shell permission system for evaluating command safety.
//!
//! The system uses a 3-layer evaluation approach:
//! 1. Parse - Parse with tree-sitter, split chained commands
//! 2. Detect - Dangerous patterns, readonly check
//! 3. Decide - Policy rules, user settings, aggregate results

mod decider;
mod detector;
mod parser;
mod trust_patterns;

use decider::decide;
use detector::detect;
use parser::parse_command;
use serde::Deserialize;
pub use trust_patterns::generate_trust_patterns;

use super::protocol::PermissionEvalResult;

/// Settings for shell permission evaluation.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ShellPermissionSettings {
    /// Commands that are explicitly allowed (regex patterns).
    pub allowed_commands: Vec<String>,
    /// Commands that are explicitly denied (regex patterns).
    pub denied_commands: Vec<String>,
    /// Whether to auto-allow readonly commands.
    pub auto_allow_readonly: bool,
    /// Whether to deny commands not in the allow list.
    pub deny_by_default: bool,
    /// Whether the tool is in the agent's allowed tools list.
    pub is_tool_allowed: bool,
}

/// Evaluate shell permission for a command.
pub fn evaluate_shell_permission(command: &str, settings: &ShellPermissionSettings) -> PermissionEvalResult {
    // Layer 1: Parse
    let parse_result = parse_command(command);
    if parse_result.parse_failed {
        return PermissionEvalResult::ask();
    }

    // Layer 2: Detect
    let detection = detect(&parse_result.commands);

    // Layer 3: Decide
    let decider_result = decide(&parse_result.commands, &detection, settings);

    // Guard against tree-sitter misparses — downgrade Allow to Ask
    if matches!(decider_result.result, PermissionEvalResult::Allow) && has_parser_blind_spots(command) {
        return PermissionEvalResult::ask();
    }

    match decider_result.result {
        PermissionEvalResult::Ask { .. } => {
            PermissionEvalResult::ask_with_options(generate_trust_patterns(&decider_result.trustable_commands))
        },
        other => other,
    }
}

/// Returns true if the command contains patterns that tree-sitter bash grammar mishandles.
fn has_parser_blind_spots(command: &str) -> bool {
    // TODO: Need to revisit and investigate why \r was considered a dangerous command in v1
    // implementation
    command.contains('\r')
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Deserialize)]
    struct TestGroup {
        name: String,
        #[serde(default)]
        settings: ShellPermissionSettings,
        cases: Vec<TestCase>,
    }

    #[derive(Debug, Deserialize)]
    struct TestCase {
        input: String,
        expected: String,
    }

    #[test]
    fn test_e2e_cases() {
        let json = include_str!("test_data/e2e_tests.json");
        let groups: Vec<TestGroup> = serde_json::from_str(json).expect("Failed to parse e2e_tests.json");

        let mut total = 0;
        for group in groups {
            for tc in group.cases {
                total += 1;
                let result = evaluate_shell_permission(&tc.input, &group.settings);
                let result_str = match &result {
                    PermissionEvalResult::Allow => "Allow",
                    PermissionEvalResult::Ask { .. } => "Ask",
                    PermissionEvalResult::Deny { .. } => "Deny",
                };

                assert_eq!(
                    result_str, tc.expected,
                    "[{}] input='{}' expected={}, got={:?}",
                    group.name, tc.input, tc.expected, result
                );
            }
        }
        println!("e2e_tests.json: {total} test cases passed");
    }

    #[test]
    fn test_readonly_command_allowed() {
        let settings = ShellPermissionSettings {
            auto_allow_readonly: true,
            ..Default::default()
        };
        let result = evaluate_shell_permission("ls -la", &settings);
        assert_eq!(result, PermissionEvalResult::Allow);
    }

    #[test]
    fn test_readonly_disabled_asks() {
        let settings = ShellPermissionSettings {
            auto_allow_readonly: false,
            ..Default::default()
        };
        let result = evaluate_shell_permission("ls -la", &settings);
        assert!(matches!(result, PermissionEvalResult::Ask { .. }));
    }

    #[test]
    fn test_tool_allowed_allows() {
        let settings = ShellPermissionSettings {
            is_tool_allowed: true,
            ..Default::default()
        };
        let result = evaluate_shell_permission("rm -rf /", &settings);
        assert_eq!(result, PermissionEvalResult::Allow);
    }

    #[test]
    fn test_dangerous_command_asks() {
        let settings = ShellPermissionSettings::default();
        let result = evaluate_shell_permission("find . -exec rm {} \\;", &settings);
        assert!(matches!(result, PermissionEvalResult::Ask { .. }));
    }

    #[test]
    fn test_denied_command() {
        let settings = ShellPermissionSettings {
            denied_commands: vec!["rm -rf .*".into()],
            ..Default::default()
        };
        let result = evaluate_shell_permission("rm -rf /", &settings);
        assert!(matches!(result, PermissionEvalResult::Deny { .. }));
    }

    #[test]
    fn test_allowed_command() {
        let settings = ShellPermissionSettings {
            allowed_commands: vec!["git .*".into()],
            ..Default::default()
        };
        let result = evaluate_shell_permission("git status", &settings);
        assert_eq!(result, PermissionEvalResult::Allow);
    }

    #[test]
    fn test_multiline_command_with_allowed_pattern() {
        let settings = ShellPermissionSettings {
            allowed_commands: vec!["ssh( .*)?".into()],
            auto_allow_readonly: true,
            ..Default::default()
        };
        // Single line - should match
        let single = r#"ssh -i ~/.ssh/key.pem ubuntu@host "sudo apt-get install -y tmux""#;
        let r = evaluate_shell_permission(single, &settings);
        println!("single line: {:?}", r);
        assert!(matches!(r, PermissionEvalResult::Allow), "single line should Allow");

        // Multi-line with backslash continuation - this is what the LLM sends
        let multi = "ssh -i ~/.ssh/key.pem ubuntu@host \\\n  \"sudo apt-get install -y tmux 2>&1 | tail -3\"";
        let r = evaluate_shell_permission(multi, &settings);
        println!("multi line: {:?}", r);
        assert!(
            matches!(r, PermissionEvalResult::Allow),
            "multi-line should Allow but got: {:?}",
            r
        );
    }
}
