//! Decision layer for shell permission evaluation.

use std::path::{
    Path,
    PathBuf,
};

use tracing::debug;

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
use crate::agent::util::path::canonicalize_path_sys;
use crate::util::providers::SystemProvider;

/// Context for evaluating redirect target paths against write permissions.
///
/// Construct via [`RedirectContext::new`] so allowed/denied paths and `effective_cwd`
/// all pass through the same canonicalizer as the redirect targets themselves -
/// otherwise the decider could either spuriously prompt on equivalent paths or, worse,
/// miss a redirect that physically escapes the allow-list via a symlink.
///
/// NOTE: This checks redirect targets against fs_write allowed/denied paths, treating
/// a redirect as a filesystem write operation. Trust v2 (kiro-agent trustv2.md) models
/// redirects differently - they're included in the shell command string and matched
/// against shell capability patterns, not filesystem capability rules. This check should
/// be revisited during the trust v2 migration.
#[derive(Debug, Clone, Default)]
pub struct RedirectContext {
    /// Allowed write paths (already canonicalized).
    pub allowed_write_paths: Vec<String>,
    /// Denied write paths (already canonicalized).
    pub denied_write_paths: Vec<String>,
    /// Effective cwd for resolving relative redirect targets (already canonicalized).
    /// Any `cd`/`pushd`/`popd` within the command itself is tracked internally by the
    /// decider.
    pub effective_cwd: PathBuf,
    /// Whether the fs_write tool is in the agent's allowed tools list.
    /// When true, all redirect targets are allowed (same as fs_write allowlist bypass).
    pub write_tool_allowed: bool,
}

impl RedirectContext {
    /// Build a [`RedirectContext`] from raw path strings, canonicalizing everything
    /// through [`canonicalize_path_sys`] so allowed/denied paths are compared on the
    /// same basis as redirect targets. Paths that fail to canonicalize are silently
    /// dropped - callers can't do anything useful with them anyway.
    ///
    /// `effective_cwd` should be the caller-provided working directory (e.g.
    /// `ExecuteCmd.working_dir`) if present; otherwise pass the process / ACP cwd.
    /// If its canonicalization fails, the returned context has an empty `effective_cwd`
    /// and the decider will fall back to asking for confirmation on any redirect.
    pub fn new<'a, P: SystemProvider>(
        allowed_write_paths: impl IntoIterator<Item = &'a str>,
        denied_write_paths: impl IntoIterator<Item = &'a str>,
        effective_cwd: impl AsRef<str>,
        write_tool_allowed: bool,
        provider: &P,
    ) -> Self {
        let canon = |p: &str| canonicalize_path_sys(p, provider).ok();
        Self {
            allowed_write_paths: allowed_write_paths.into_iter().filter_map(canon).collect(),
            denied_write_paths: denied_write_paths.into_iter().filter_map(canon).collect(),
            effective_cwd: canon(effective_cwd.as_ref()).map(PathBuf::from).unwrap_or_default(),
            write_tool_allowed,
        }
    }
}

/// Commands that change the working directory.
const CWD_CHANGING_COMMANDS: &[&str] = &["cd", "pushd", "popd"];

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
pub fn decide<P: SystemProvider>(
    commands: &[ParsedCommand],
    detection: &DetectResult,
    settings: &ShellPermissionSettings,
    redirect_ctx: &RedirectContext,
    provider: &P,
) -> DeciderResult {
    // 0. Invalid denied regex patterns should deny all (security-first)
    for pattern in &settings.denied_commands {
        if let Err(p) = validate_regex(pattern) {
            return DeciderResult::deny(format!("Invalid regex pattern in deniedCommands: {}", p));
        }
    }

    let deny_rules = build_rules(&settings.denied_commands, RuleAction::Deny);
    let allow_rules = build_rules(&settings.allowed_commands, RuleAction::Allow);

    // 1. Any deny matches -> Deny
    let mut denied_patterns: Vec<String> = Vec::new();
    for cmd in commands {
        for rule in &deny_rules {
            if match_rules(&cmd.command, std::slice::from_ref(rule)).is_some() {
                denied_patterns.push(rule.pattern.clone());
            }
        }
    }
    if !denied_patterns.is_empty() {
        debug!(patterns = ?denied_patterns, "shell_permission::decide -> Deny (denied pattern match)");
        return DeciderResult::deny(denied_patterns.join(", "));
    }

    // 2. If tool is in allowed list, allow
    if settings.is_tool_allowed {
        debug!("shell_permission::decide -> Allow (tool in allowed list)");
        return DeciderResult::allow();
    }

    // 3. Dangerous -> check if redirect-only danger can be resolved via write permissions
    if detection.danger_level != DangerLevel::None {
        let is_redirect_only = detection.danger_level == DangerLevel::RedirectOutput;

        // /dev/null is always safe regardless of write path configuration
        let all_targets_dev_null = is_redirect_only
            && !detection.redirect_targets.is_empty()
            && detection.redirect_targets.iter().all(|t| t == "/dev/null");

        let redirect_allowed = all_targets_dev_null
            || (is_redirect_only
                && !detection.redirect_targets.is_empty()
                && (redirect_ctx.write_tool_allowed || all_redirect_targets_allowed(commands, redirect_ctx, provider)));

        if !redirect_allowed {
            if settings.deny_by_default {
                debug!(?detection.danger_level, "shell_permission::decide -> Deny (dangerous + deny_by_default)");
                return DeciderResult::deny("Command not in allowed list".to_string());
            }
            debug!(?detection.danger_level, "shell_permission::decide -> Ask (dangerous)");
            // For redirect-only danger, forward parsed commands so the UI can offer the
            // standard 3-tier trust options (Full / Partial / Base command). Without this
            // the UI falls back to AllowAlwaysTool which trusts the entire shell tool.
            // For other High danger (command substitution, dangerous flags, ...), keep
            // trustable_commands empty: command-pattern trust would not change the outcome
            // because High danger is rechecked at step 3 on every subsequent invocation.
            let trustable = if is_redirect_only { commands.to_vec() } else { vec![] };
            return DeciderResult::ask(trustable);
        }
        debug!("shell_permission::decide: redirect targets all in allowed write paths, continuing");
    }

    // 4. Check each command: must be (allowed OR readonly)
    let mut trustable_commands: Vec<ParsedCommand> = Vec::new();
    let mut all_commands_permitted = true;
    for (i, cmd) in commands.iter().enumerate() {
        let is_allowed = match_rules(&cmd.command, &allow_rules).is_some();
        let is_readonly = detection.command_readonly.get(i).copied().unwrap_or(false);

        if !(is_allowed || is_readonly && settings.auto_allow_readonly) {
            all_commands_permitted = false;
            trustable_commands.push(cmd.clone());
        }
    }
    if all_commands_permitted {
        debug!("shell_permission::decide -> Allow (all commands permitted by rules/readonly)");
        return DeciderResult::allow();
    }

    // 5. Deny by default if enabled
    if settings.deny_by_default {
        debug!("shell_permission::decide -> Deny (deny_by_default)");
        return DeciderResult::deny("Command not in allowed list".to_string());
    }

    // 6. Default: ask
    debug!(
        trustable = trustable_commands.len(),
        "shell_permission::decide -> Ask (default)"
    );
    DeciderResult::ask(trustable_commands)
}

/// Build rules from patterns with specified action.
fn build_rules(patterns: &[String], action: RuleAction) -> Vec<Rule> {
    patterns
        .iter()
        .map(|p| Rule::new(p, PatternMode::Regex, action))
        .collect()
}

/// Check if all redirect targets are within allowed write paths.
fn all_redirect_targets_allowed<P: SystemProvider>(
    commands: &[ParsedCommand],
    ctx: &RedirectContext,
    provider: &P,
) -> bool {
    // `effective_cwd` is canonicalized at construction time by `RedirectContext::new`.
    // If it's empty here, canonicalization failed for the caller's cwd and we can't
    // safely resolve relative redirect targets - fall through to Ask.
    if ctx.effective_cwd.as_os_str().is_empty() {
        debug!("effective_cwd is empty, falling through to Ask");
        return false;
    }
    let canonical_cwd = ctx.effective_cwd.clone();

    // Track whether cwd has been changed by a preceding command
    let mut cwd_changed = false;

    for cmd in commands {
        // Check if this command changes cwd before processing its redirects
        if CWD_CHANGING_COMMANDS.contains(&cmd.command_name.as_str()) {
            cwd_changed = !is_noop_cwd_change(cmd, &canonical_cwd, provider);
        }

        // Check this command's redirect targets
        for target in &cmd.redirect_targets {
            if target == "/dev/null" {
                continue;
            }

            let path = Path::new(target);

            let resolved = if path.is_absolute() {
                PathBuf::from(target)
            } else if cwd_changed {
                debug!(target, "redirect target unresolvable: cwd changed by preceding command");
                return false;
            } else {
                canonical_cwd.join(target)
            };

            // canonicalize_path_sys resolves symlinks on any existing ancestor and
            // rejoins the non-existent tail, so targets like `/tmp/new.log` resolve
            // to the same canonical form as the allowed paths (which are canonicalized
            // by the caller).
            let canonical = match canonicalize_path_sys(resolved.to_string_lossy(), provider) {
                Ok(p) => p,
                Err(_) => {
                    debug!(?resolved, "redirect target: canonicalization failed");
                    return false;
                },
            };

            if path_matches_any(&canonical, &ctx.denied_write_paths) {
                debug!(canonical, "redirect target denied by write path");
                return false;
            }

            if !path_matches_any(&canonical, &ctx.allowed_write_paths) {
                debug!(canonical, "redirect target not in allowed write paths");
                return false;
            }
        }
    }

    true
}

/// Check if a cwd-changing command is a no-op (targets the given canonical effective cwd).
fn is_noop_cwd_change<P: SystemProvider>(cmd: &ParsedCommand, canonical_cwd: &Path, provider: &P) -> bool {
    let Some(target) = cmd.args.first() else {
        return false; // cd with no args goes to $HOME
    };
    if cmd.has_variable_expansion {
        return false; // can't resolve
    }
    let cd_path = if Path::new(target).is_absolute() {
        PathBuf::from(target)
    } else {
        canonical_cwd.join(target)
    };
    let Ok(canonical) = canonicalize_path_sys(cd_path.to_string_lossy(), provider) else {
        return false;
    };
    // canonical_cwd is already canonical; compare directly.
    canonical == canonical_cwd.to_string_lossy()
}

/// Check if a path matches any pattern in the list using glob matching.
/// Patterns match both the exact path and any children (e.g., `/home/user` matches
/// `/home/user/file.txt`).
fn path_matches_any(path: &str, patterns: &[String]) -> bool {
    use globset::{
        Glob,
        GlobSetBuilder,
    };

    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        if let Ok(g) = Glob::new(pattern) {
            builder.add(g);
        }
        // Also match children
        let dir_pattern = format!("{}/**", pattern.trim_end_matches('/'));
        if let Ok(g) = Glob::new(&dir_pattern) {
            builder.add(g);
        }
    }
    let Ok(set) = builder.build() else {
        return false;
    };
    !set.matches(path).is_empty()
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
            "RedirectOutput" => DangerLevel::RedirectOutput,
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
                ..Default::default()
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
                redirect_targets: vec![],
            };

            let result = decide(
                &commands,
                &detection,
                &settings,
                &RedirectContext::default(),
                &crate::util::test::TestProvider::new(),
            );

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
