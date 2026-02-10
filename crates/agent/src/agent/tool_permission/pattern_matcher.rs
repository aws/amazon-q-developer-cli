//! Pattern matching with allow/deny rules.
//!
//! Supports multiple pattern types:
//! - Regex: Anchored regex patterns (v1 compatible)
//! - Glob: File path style patterns
//! - Prefix: Word-boundary prefix matching

use globset::Glob;
use regex::Regex;
use serde::Deserialize;

/// Pattern matching mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub enum PatternMode {
    #[default]
    Regex,
    Glob,
    Prefix,
}

/// Action to take when pattern matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum RuleAction {
    Allow,
    Deny,
}

/// A pattern rule with mode and action.
#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub pattern: String,
    #[serde(default)]
    pub mode: PatternMode,
    pub action: RuleAction,
}

impl Rule {
    pub fn new(pattern: impl Into<String>, mode: PatternMode, action: RuleAction) -> Self {
        Self {
            pattern: pattern.into(),
            mode,
            action,
        }
    }
}

/// Match value against rules. Returns first matching rule.
/// Order: deny rules first, then allow rules.
pub fn match_rules<'a>(value: &str, rules: &'a [Rule]) -> Option<&'a Rule> {
    // Check deny rules first
    if let Some(rule) = rules
        .iter()
        .filter(|r| r.action == RuleAction::Deny)
        .find(|rule| matches_pattern(value, &rule.pattern, rule.mode))
    {
        return Some(rule);
    }

    // Check allow rules
    rules
        .iter()
        .filter(|r| r.action == RuleAction::Allow)
        .find(|rule| matches_pattern(value, &rule.pattern, rule.mode))
}

fn matches_pattern(value: &str, pattern: &str, mode: PatternMode) -> bool {
    match mode {
        PatternMode::Regex => matches_regex(value, pattern),
        PatternMode::Glob => matches_glob(value, pattern),
        PatternMode::Prefix => matches_prefix(value, pattern),
    }
}

fn matches_regex(value: &str, pattern: &str) -> bool {
    let anchored = match (pattern.starts_with('^'), pattern.ends_with('$')) {
        (true, true) => pattern.to_string(),
        (true, false) => format!("{pattern}$"),
        (false, true) => format!("^{pattern}"),
        (false, false) => format!("^{pattern}$"),
    };
    Regex::new(&anchored).map(|r| r.is_match(value)).unwrap_or(false)
}

fn matches_glob(value: &str, pattern: &str) -> bool {
    Glob::new(pattern)
        .map(|g| g.compile_matcher().is_match(value))
        .unwrap_or(false)
}

fn matches_prefix(value: &str, prefix: &str) -> bool {
    value == prefix || (value.starts_with(prefix) && value[prefix.len()..].starts_with(char::is_whitespace))
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Deserialize)]
    struct TestGroup {
        name: String,
        rules: Vec<Rule>,
        cases: Vec<TestCase>,
    }

    #[derive(Deserialize)]
    struct TestCase {
        input: String,
        expected: String,
    }

    #[test]
    fn test_pattern_matcher() {
        let json = include_str!("pattern_tests.json");
        let groups: Vec<TestGroup> = serde_json::from_str(json).expect("Failed to parse pattern_tests.json");

        for group in groups {
            for case in &group.cases {
                let result = match_rules(&case.input, &group.rules);
                let actual = match result {
                    Some(r) if r.action == RuleAction::Allow => "allow",
                    Some(_) => "deny",
                    None => "no_match",
                };

                assert_eq!(
                    actual, case.expected,
                    "[{}] input '{}': expected {}, got {}",
                    group.name, case.input, case.expected, actual
                );
            }
        }
    }
}
