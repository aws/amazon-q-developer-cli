//! Decision layer for shell permission evaluation.

use super::ShellPermissionSettings;
use super::detector::{
    DangerLevel,
    DetectResult,
};
use super::parser::ParsedCommand;
use crate::agent::protocol::PermissionEvalResult;
use crate::agent::tool_permission::{
    PatternMode,
    Rule,
    RuleAction,
    match_rules,
};

/// Decide the final permission result for parsed commands.
pub fn decide(
    commands: &[ParsedCommand],
    detection: &DetectResult,
    settings: &ShellPermissionSettings,
) -> PermissionEvalResult {
    let deny_rules = build_rules(&settings.denied_commands, RuleAction::Deny);
    let allow_rules = build_rules(&settings.allowed_commands, RuleAction::Allow);

    // 1. Any deny matches → Deny
    let mut denied_patterns: Vec<String> = Vec::new();
    for cmd in commands {
        for rule in &deny_rules {
            if match_rules(&cmd.command, std::slice::from_ref(rule)).is_some() {
                denied_patterns.push(rule.pattern.clone());
            }
        }
    }
    if !denied_patterns.is_empty() {
        return PermissionEvalResult::Deny {
            reason: denied_patterns.join(", "),
        };
    }

    // 2. All allow matches → Allow
    let all_allowed_by_rule = commands
        .iter()
        .all(|cmd| match_rules(&cmd.command, &allow_rules).is_some());
    if all_allowed_by_rule {
        return PermissionEvalResult::Allow;
    }

    // 3. If tool is in allowed list, allow
    if settings.is_tool_allowed {
        return PermissionEvalResult::Allow;
    }

    // 4. Dangerous → Ask
    if detection.danger_level != DangerLevel::None {
        return PermissionEvalResult::Ask;
    }

    // 5. All readonly → Allow
    if settings.auto_allow_readonly && detection.is_readonly {
        return PermissionEvalResult::Allow;
    }

    // 6. Deny by default if enabled
    if settings.deny_by_default {
        return PermissionEvalResult::Deny {
            reason: "Command not in allowed list".to_string(),
        };
    }

    // 7. Default: ask
    PermissionEvalResult::Ask
}

/// Build rules from patterns with specified action.
fn build_rules(patterns: &[String], action: RuleAction) -> Vec<Rule> {
    patterns
        .iter()
        .map(|p| Rule::new(p, PatternMode::Regex, action))
        .collect()
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Deserialize)]
    struct TestCase {
        name: String,
        commands: Vec<String>,
        settings: TestSettings,
        detection: TestDetection,
        expected: String,
        #[serde(default)]
        expected_reason_contains: Vec<String>,
    }

    #[derive(Debug, Deserialize, Default)]
    struct TestSettings {
        #[serde(default)]
        denied_commands: Vec<String>,
        #[serde(default)]
        allowed_commands: Vec<String>,
        #[serde(default)]
        is_tool_allowed: bool,
        #[serde(default)]
        auto_allow_readonly: bool,
        #[serde(default)]
        deny_by_default: bool,
    }

    #[derive(Debug, Deserialize)]
    struct TestDetection {
        danger_level: String,
        is_readonly: bool,
    }

    fn load_test_cases() -> Vec<TestCase> {
        let json = include_str!("test_data/decider_tests.json");
        serde_json::from_str(json).expect("Failed to parse decider_tests.json")
    }

    fn parse_danger_level(s: &str) -> DangerLevel {
        match s {
            "None" => DangerLevel::None,
            "Low" => DangerLevel::Low,
            "High" => DangerLevel::High,
            _ => panic!("Unknown danger level: {}", s),
        }
    }

    #[test]
    fn test_decider_cases() {
        let cases = load_test_cases();
        let total = cases.len();

        for tc in cases {
            let commands: Vec<ParsedCommand> = tc
                .commands
                .iter()
                .map(|c| ParsedCommand {
                    command: c.clone(),
                    ..Default::default()
                })
                .collect();

            let settings = ShellPermissionSettings {
                denied_commands: tc.settings.denied_commands,
                allowed_commands: tc.settings.allowed_commands,
                is_tool_allowed: tc.settings.is_tool_allowed,
                auto_allow_readonly: tc.settings.auto_allow_readonly,
                deny_by_default: tc.settings.deny_by_default,
            };

            let danger_level = parse_danger_level(&tc.detection.danger_level);
            let detection = DetectResult {
                danger_level,
                is_readonly: tc.detection.is_readonly,
                command_danger_levels: vec![danger_level; commands.len()],
                command_readonly: vec![tc.detection.is_readonly; commands.len()],
            };

            let result = decide(&commands, &detection, &settings);

            let result_type = match &result {
                PermissionEvalResult::Allow => "Allow",
                PermissionEvalResult::Ask => "Ask",
                PermissionEvalResult::Deny { .. } => "Deny",
            };

            assert_eq!(
                result_type, tc.expected,
                "[{}] expected {}, got {:?}",
                tc.name, tc.expected, result
            );

            // Check reason contains expected patterns
            if !tc.expected_reason_contains.is_empty() {
                if let PermissionEvalResult::Deny { reason } = &result {
                    for pattern in &tc.expected_reason_contains {
                        assert!(
                            reason.contains(pattern),
                            "[{}] reason '{}' should contain '{}'",
                            tc.name,
                            reason,
                            pattern
                        );
                    }
                }
            }
        }
        println!("decider_tests.json: {total} test cases passed");
    }
}
