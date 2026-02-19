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
    validate_regex,
};

/// Result of the decision layer.
pub struct DeciderResult {
    /// Aggregated permission result for the whole command chain.
    pub result: PermissionEvalResult,
    /// Commands where adding to allowedCommands would change the outcome.
    /// Only populated when result is Ask due to unresolved (non-dangerous) commands.
    #[allow(dead_code)]
    pub trustable_commands: Vec<ParsedCommand>,
}

impl DeciderResult {
    fn allow() -> Self {
        Self {
            result: PermissionEvalResult::Allow,
            trustable_commands: vec![],
        }
    }

    fn deny(reason: String) -> Self {
        Self {
            result: PermissionEvalResult::Deny { reason },
            trustable_commands: vec![],
        }
    }

    fn ask(trustable_commands: Vec<ParsedCommand>) -> Self {
        Self {
            result: PermissionEvalResult::ask(),
            trustable_commands,
        }
    }
}

/// Decide the final permission result for parsed commands.
pub fn decide(
    commands: &[ParsedCommand],
    detection: &DetectResult,
    settings: &ShellPermissionSettings,
) -> DeciderResult {
    // 0. Invalid denied regex patterns should deny all (security-first)
    for pattern in &settings.denied_commands {
        if let Err(p) = validate_regex(pattern) {
            return DeciderResult::deny(format!("Invalid regex pattern in deniedCommands: {}", p));
        }
    }

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
        return DeciderResult::deny(denied_patterns.join(", "));
    }

    // 2. If tool is in allowed list, allow
    if settings.is_tool_allowed {
        return DeciderResult::allow();
    }

    // 3. Dangerous → Ask (no trustable commands — allowedCommands doesn't override danger)
    if detection.danger_level != DangerLevel::None {
        return DeciderResult::ask(vec![]);
    }

    // 4. Check each command: must be (allowed OR readonly)
    let mut trustable_commands: Vec<ParsedCommand> = Vec::new();
    let mut all_commands_permitted = true;
    for (i, cmd) in commands.iter().enumerate() {
        let is_allowed = match_rules(&cmd.command, &allow_rules).is_some();
        let is_readonly = detection.command_readonly.get(i).copied().unwrap_or(false);

        if !(is_allowed || is_readonly && settings.auto_allow_readonly) {
            // Command not permitted by allow patterns or readonly auto-allow
            all_commands_permitted = false;
            trustable_commands.push(cmd.clone());
        }
    }
    if all_commands_permitted {
        return DeciderResult::allow();
    }

    // 5. Deny by default if enabled
    if settings.deny_by_default {
        return DeciderResult::deny("Command not in allowed list".to_string());
    }

    // 7. Default: ask
    DeciderResult::ask(trustable_commands)
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
        #[serde(default)]
        expected_trustable_commands: Vec<String>,
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
        #[serde(default)]
        is_readonly: bool,
        #[serde(default)]
        command_readonly: Vec<bool>,
    }

    fn load_test_cases() -> Vec<TestCase> {
        let json = include_str!("test_data/decider_tests.json");
        serde_json::from_str(json).expect("Failed to parse decider_tests.json")
    }

    fn parse_danger_level(s: &str) -> DangerLevel {
        match s {
            "None" => DangerLevel::None,
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
                .map(|c| {
                    let command_name = c.split_whitespace().next().unwrap_or("").to_string();
                    ParsedCommand {
                        command: c.clone(),
                        command_name,
                        ..Default::default()
                    }
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
            let command_readonly = if tc.detection.command_readonly.is_empty() {
                vec![tc.detection.is_readonly; commands.len()]
            } else {
                tc.detection.command_readonly.clone()
            };

            let detection = DetectResult {
                danger_level,
                is_readonly: tc.detection.is_readonly,
                command_danger_levels: vec![danger_level; commands.len()],
                command_readonly,
            };

            let result = decide(&commands, &detection, &settings);

            let result_type = match &result.result {
                PermissionEvalResult::Allow => "Allow",
                PermissionEvalResult::Ask { .. } => "Ask",
                PermissionEvalResult::Deny { .. } => "Deny",
            };

            assert_eq!(
                result_type, tc.expected,
                "[{}] expected {}, got {:?}",
                tc.name, tc.expected, result.result
            );

            // Check reason contains expected patterns
            if !tc.expected_reason_contains.is_empty() {
                if let PermissionEvalResult::Deny { reason } = &result.result {
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

            // Check trustable commands
            let actual_trustable: Vec<&str> = result
                .trustable_commands
                .iter()
                .map(|c| c.command_name.as_str())
                .collect();
            assert_eq!(
                actual_trustable, tc.expected_trustable_commands,
                "[{}] trustable_commands mismatch",
                tc.name
            );
        }
        println!("decider_tests.json: {total} test cases passed");
    }
}
