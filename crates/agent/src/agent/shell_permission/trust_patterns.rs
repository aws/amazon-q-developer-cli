//! Trust pattern generation for shell commands.
//!
//! Generates tiered trust options (full, partial, base) from parsed commands,
//! allowing users to trust commands at different granularity levels.

use super::parser::ParsedCommand;
use crate::agent::protocol::TrustOption;

/// Generate trust patterns from parsed trustable commands.
///
/// Produces 1–3 tiers depending on the command structure:
///   - Exact command:   "git pull --rebase"       (always)
///   - Command prefix:  "git pull *"              (when first arg exists)
///   - Base command:    "git *"                   (always)
pub fn generate_trust_patterns(trustable_commands: &[ParsedCommand]) -> Vec<TrustOption> {
    if trustable_commands.is_empty() {
        return vec![];
    }

    // Collect raw strings for each tier
    let exact_raw: Vec<String> = trustable_commands.iter().map(|c| c.command.clone()).collect();
    let base_raw: Vec<String> = trustable_commands.iter().map(|c| c.command_name.clone()).collect();
    let prefix_raw: Vec<String> = trustable_commands
        .iter()
        .map(|parsed| {
            parsed
                .args
                .first()
                .map(|a| a.trim())
                .filter(|a| !a.starts_with('/') && !a.starts_with('-'))
                .map_or_else(
                    || parsed.command_name.clone(),
                    |arg| format!("{} {}", parsed.command_name, arg),
                )
        })
        .collect();

    // Build options
    let exact = build_exact_option("Full command", exact_raw);
    let prefix = build_wildcard_option("Partial command", prefix_raw);
    let base = build_wildcard_option("Base command", base_raw);

    // Assemble — skip prefix if identical to base
    let mut options = vec![exact];
    if prefix.patterns != base.patterns {
        options.push(prefix);
    }
    options.push(base);

    options
}

/// Deduplicate a vec while preserving order.
fn dedup_stable(items: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    items.into_iter().filter(|s| seen.insert(s.clone())).collect()
}

/// Build a TrustOption from raw command strings. Appends ` *` to display and `( .*)?` to patterns.
fn build_wildcard_option(label: &str, raw_commands: Vec<String>) -> TrustOption {
    let deduped = dedup_stable(raw_commands);
    TrustOption {
        label: label.into(),
        display: deduped
            .iter()
            .map(|c| format!("{} *", c))
            .collect::<Vec<_>>()
            .join(" , "),
        setting_key: "allowedCommands".into(),
        patterns: deduped.iter().map(|c| format!("{}( .*)?", regex::escape(c))).collect(),
    }
}

/// Build a TrustOption for exact command matching.
fn build_exact_option(label: &str, raw_commands: Vec<String>) -> TrustOption {
    let deduped = dedup_stable(raw_commands);
    TrustOption {
        label: label.into(),
        display: deduped.join(" , "),
        setting_key: "allowedCommands".into(),
        patterns: deduped.iter().map(|c| regex::escape(c)).collect(),
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;
    use crate::agent::shell_permission::parser::parse_command;

    #[derive(Debug, Deserialize)]
    struct ExpectedOption {
        label: String,
        #[serde(default)]
        display: Option<String>,
        #[serde(default)]
        patterns: Option<Vec<String>>,
    }

    #[derive(Debug, Deserialize)]
    struct TestCase {
        name: String,
        input: String,
        expected_option_count: usize,
        #[serde(default)]
        expected_options: Vec<ExpectedOption>,
    }

    #[test]
    fn test_trust_pattern_cases() {
        let json = include_str!("test_data/trust_pattern_tests.json");
        let cases: Vec<TestCase> = serde_json::from_str(json).expect("Failed to parse trust_pattern_tests.json");

        let mut total = 0;
        for tc in &cases {
            total += 1;
            let parsed = parse_command(&tc.input);
            let result = generate_trust_patterns(&parsed.commands);

            assert_eq!(
                result.len(),
                tc.expected_option_count,
                "[{}] input='{}' expected {} options, got {}",
                tc.name,
                tc.input,
                tc.expected_option_count,
                result.len()
            );

            for (i, expected) in tc.expected_options.iter().enumerate() {
                let actual = &result[i];
                assert_eq!(
                    actual.label, expected.label,
                    "[{}] option[{}] label mismatch: expected='{}', got='{}'",
                    tc.name, i, expected.label, actual.label
                );
                if let Some(display) = &expected.display {
                    assert_eq!(
                        &actual.display, display,
                        "[{}] option[{}] display mismatch: expected='{}', got='{}'",
                        tc.name, i, display, actual.display
                    );
                }
                if let Some(patterns) = &expected.patterns {
                    assert_eq!(
                        &actual.patterns, patterns,
                        "[{}] option[{}] patterns mismatch",
                        tc.name, i
                    );
                }
            }
        }
        println!("trust_pattern_tests.json: {total} test cases passed");
    }
}
