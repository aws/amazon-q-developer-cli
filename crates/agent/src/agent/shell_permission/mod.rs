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

pub use decider::RedirectContext;
use decider::decide;
use detector::detect;
use parser::parse_command;
use serde::Deserialize;
use tracing::debug;
pub use trust_patterns::generate_trust_patterns;

use super::protocol::PermissionEvalResult;
use crate::util::providers::SystemProvider;

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
pub fn evaluate_shell_permission<P: SystemProvider>(
    command: &str,
    settings: &ShellPermissionSettings,
    redirect_ctx: &RedirectContext,
    provider: &P,
) -> PermissionEvalResult {
    // Layer 1: Parse
    let parse_result = parse_command(command);
    if parse_result.parse_failed {
        debug!(command, "shell_permission: parse failed, asking");
        return PermissionEvalResult::ask();
    }

    for cmd in &parse_result.commands {
        debug!(
            command = %cmd.command,
            command_name = %cmd.command_name,
            args = ?cmd.args,
            has_redirection_to_file = cmd.has_redirection_to_file,
            has_command_substitution = cmd.has_command_substitution,
            has_process_substitution = cmd.has_process_substitution,
            has_variable_expansion = cmd.has_variable_expansion,
            operator = ?cmd.operator,
            "shell_permission: parsed command"
        );
    }

    // Layer 2: Detect
    let detection = detect(&parse_result.commands);

    // Layer 3: Decide
    let decider_result = decide(&parse_result.commands, &detection, settings, redirect_ctx, provider);

    // Guard against tree-sitter misparses — downgrade Allow to Ask
    if matches!(decider_result.result, PermissionEvalResult::Allow) && has_parser_blind_spots(command) {
        debug!(
            command,
            "shell_permission: parser blind spot detected, downgrading Allow to Ask"
        );
        return PermissionEvalResult::ask();
    }

    let result = match decider_result.result {
        PermissionEvalResult::Ask { .. } => {
            PermissionEvalResult::ask_with_options(generate_trust_patterns(&decider_result.trustable_commands))
        },
        other => other,
    };

    debug!(?result, command, "shell_permission: final result");
    result
}

fn has_parser_blind_spots(command: &str) -> bool {
    // TODO: Need to revisit and investigate why \r was considered a dangerous command in v1
    // implementation
    command.contains('\r')
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;
    use crate::util::test::TestProvider;

    #[derive(Debug, Deserialize)]
    struct TestGroup {
        name: String,
        #[serde(default)]
        settings: ShellPermissionSettings,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        allowed_write_paths: Vec<String>,
        #[serde(default)]
        denied_write_paths: Vec<String>,
        cases: Vec<TestCase>,
    }

    #[derive(Debug, Deserialize)]
    struct TestCase {
        input: String,
        expected: String,
        #[serde(default)]
        working_dir: Option<String>,
        /// Skip this case on the named platform with a reason.
        /// `platform` matches `cfg(target_family = "...")` values: `"unix"` or `"windows"`.
        #[serde(default)]
        ignore: Option<Ignore>,
    }

    #[derive(Debug, Deserialize)]
    struct Ignore {
        platform: String,
        #[allow(dead_code)]
        reason: String,
    }

    impl Ignore {
        fn matches_current_platform(&self) -> bool {
            match self.platform.as_str() {
                "windows" => cfg!(windows),
                "unix" => cfg!(unix),
                _ => false,
            }
        }
    }

    #[test]
    fn test_e2e_cases() {
        let json = include_str!("test_data/e2e_tests.json");
        let groups: Vec<TestGroup> = serde_json::from_str(json).expect("Failed to parse e2e_tests.json");

        // JSON uses Unix-style absolute paths; on Windows, prefix with a drive letter
        // and use native backslash separators so the PathBufs we construct here match
        // what the real filesystem canonicalizer returns.
        #[cfg(windows)]
        fn fixup_path(p: &str) -> String {
            if let Some(rest) = p.strip_prefix('/') {
                format!("C:\\{}", rest.replace('/', "\\"))
            } else {
                p.to_string()
            }
        }
        #[cfg(not(windows))]
        fn fixup_path(p: &str) -> String {
            p.to_string()
        }

        let mut total = 0;
        for group in groups {
            let provider = if let Some(ref cwd) = group.cwd {
                TestProvider::new_with_base(fixup_path(cwd))
            } else {
                TestProvider::new()
            };

            // Apply fixup_path lazily; `RedirectContext::new` canonicalizes these
            // internally (matching real call sites).
            let allowed_write_paths: Vec<String> = group.allowed_write_paths.iter().map(|p| fixup_path(p)).collect();
            let denied_write_paths: Vec<String> = group.denied_write_paths.iter().map(|p| fixup_path(p)).collect();

            for tc in &group.cases {
                if let Some(ref ignore) = tc.ignore
                    && ignore.matches_current_platform()
                {
                    continue;
                }
                total += 1;

                let effective_cwd = tc
                    .working_dir
                    .as_deref()
                    .map(|p| fixup_path(p))
                    .or_else(|| group.cwd.as_deref().map(|p| fixup_path(p)))
                    .unwrap_or_else(|| TestProvider::default_home().to_string());

                let redirect_ctx = RedirectContext::new(
                    allowed_write_paths.iter().map(String::as_str),
                    denied_write_paths.iter().map(String::as_str),
                    &effective_cwd,
                    false,
                    &provider,
                );

                let result = evaluate_shell_permission(&tc.input, &group.settings, &redirect_ctx, &provider);
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

    fn eval(command: &str, settings: &ShellPermissionSettings) -> PermissionEvalResult {
        let ctx = RedirectContext::default();
        evaluate_shell_permission(command, settings, &ctx, &TestProvider::new())
    }

    #[test]
    fn test_readonly_command_allowed() {
        let settings = ShellPermissionSettings {
            auto_allow_readonly: true,
            ..Default::default()
        };
        assert_eq!(eval("ls -la", &settings), PermissionEvalResult::Allow);
    }

    #[test]
    fn test_readonly_disabled_asks() {
        let settings = ShellPermissionSettings {
            auto_allow_readonly: false,
            ..Default::default()
        };
        assert!(matches!(eval("ls -la", &settings), PermissionEvalResult::Ask { .. }));
    }

    #[test]
    fn test_tool_allowed_allows() {
        let settings = ShellPermissionSettings {
            is_tool_allowed: true,
            ..Default::default()
        };
        assert_eq!(eval("rm -rf /", &settings), PermissionEvalResult::Allow);
    }

    #[test]
    fn test_dangerous_command_asks() {
        let settings = ShellPermissionSettings::default();
        assert!(matches!(
            eval("find . -exec rm {} \\;", &settings),
            PermissionEvalResult::Ask { .. }
        ));
    }

    #[test]
    fn test_denied_command() {
        let settings = ShellPermissionSettings {
            denied_commands: vec!["rm -rf .*".into()],
            ..Default::default()
        };
        assert!(matches!(eval("rm -rf /", &settings), PermissionEvalResult::Deny { .. }));
    }

    #[test]
    fn test_allowed_command() {
        let settings = ShellPermissionSettings {
            allowed_commands: vec!["git .*".into()],
            ..Default::default()
        };
        assert_eq!(eval("git status", &settings), PermissionEvalResult::Allow);
    }

    #[test]
    fn test_multiline_command_with_allowed_pattern() {
        let settings = ShellPermissionSettings {
            allowed_commands: vec!["ssh( .*)?".into()],
            auto_allow_readonly: true,
            ..Default::default()
        };
        let single = r#"ssh -i ~/.ssh/key.pem ubuntu@host "sudo apt-get install -y tmux""#;
        assert!(
            matches!(eval(single, &settings), PermissionEvalResult::Allow),
            "single line should Allow"
        );

        let multi = "ssh -i ~/.ssh/key.pem ubuntu@host \\\n  \"sudo apt-get install -y tmux 2>&1 | tail -3\"";
        let r = eval(multi, &settings);
        assert!(
            matches!(r, PermissionEvalResult::Allow),
            "multi-line should Allow but got: {:?}",
            r
        );
    }

    /// Regression for a macOS-visible bug (generalizable to any OS with symlinks):
    /// if an allowed-write-path is canonicalized via a symlink, a redirect target to a
    /// non-existent file under that symlinked path must also canonicalize through the
    /// symlink. Otherwise the glob match spuriously fails and the command is asked.
    ///
    /// Cross-platform: we create the symlink ourselves rather than relying on OS-level
    /// symlinks like macOS `/tmp -> /private/tmp`, so this test runs on Linux too.
    #[cfg(unix)]
    #[test]
    fn test_redirect_target_resolves_symlinked_allowed_path() {
        use std::os::unix::fs::symlink;

        use crate::agent::util::path::canonicalize_path_sys;
        use crate::agent::util::test::TestProvider;

        let dir = tempfile::tempdir().unwrap();
        // Layout:
        //   <tmp>/real_project/       (dir, used as cwd)
        //   <tmp>/link_project -> real_project
        // The agent is handed the symlink path in both the cwd and the allowed-path list.
        let real = dir.path().join("real_project");
        std::fs::create_dir(&real).unwrap();
        let link = dir.path().join("link_project");
        symlink(&real, &link).unwrap();

        // Real call sites canonicalize allowed write paths before handing them to the
        // shell permission system - so the allowed path ends up as the real target.
        let provider = TestProvider::new_with_base(dir.path());
        let allowed_canonical = canonicalize_path_sys(link.to_string_lossy(), &provider).unwrap();

        let settings = ShellPermissionSettings {
            allowed_commands: vec!["echo .*".into()],
            auto_allow_readonly: true,
            ..Default::default()
        };
        let ctx = RedirectContext {
            allowed_write_paths: vec![allowed_canonical.clone()],
            // cwd is handed to the decider via the symlink path - it must be canonicalized
            // internally, not relied on the caller.
            effective_cwd: link.clone(),
            ..Default::default()
        };

        // Redirect to a non-existent file under the symlinked cwd. Before the fix this
        // would be `Ask` because the target resolved to `<tmp>/link_project/out.log`
        // while the allowed path was `<tmp>/real_project`.
        let result = evaluate_shell_permission("echo hi > ./out.log", &settings, &ctx, &provider);
        assert!(
            matches!(result, PermissionEvalResult::Allow),
            "redirect under symlinked allowed path should Allow, got {result:?}"
        );

        // Absolute form through the symlink should also Allow.
        let abs_via_link = link.join("out2.log");
        let cmd = format!("echo hi > {}", abs_via_link.to_string_lossy());
        let result = evaluate_shell_permission(&cmd, &settings, &ctx, &provider);
        assert!(
            matches!(result, PermissionEvalResult::Allow),
            "absolute redirect through symlinked ancestor should Allow, got {result:?}"
        );

        // Sanity check: a path outside the allowed symlinked dir still asks.
        let outside = dir.path().join("outside.log");
        let cmd = format!("echo hi > {}", outside.to_string_lossy());
        let result = evaluate_shell_permission(&cmd, &settings, &ctx, &provider);
        assert!(
            matches!(result, PermissionEvalResult::Ask { .. }),
            "redirect outside allowed path should Ask, got {result:?}"
        );
    }

    /// Regression: an agent must not be able to escape an allow-listed directory by
    /// redirecting through `allowed_symlink/../file` when the symlink points outside.
    /// The kernel resolves `..` after following the symlink, so this writes to
    /// `physical_parent_of(symlink_target)/file`, which should NOT be inside the
    /// allow-listed dir.
    #[cfg(unix)]
    #[test]
    fn test_redirect_cannot_escape_allowed_via_symlink_dotdot() {
        use std::os::unix::fs::symlink;

        use crate::agent::util::path::canonicalize_path_sys;
        use crate::agent::util::test::TestProvider;

        let root = tempfile::tempdir().unwrap();
        // Layout:
        //   <root>/allowed/             (allow-list entry)
        //   <root>/outside/             (NOT allow-listed)
        //   <root>/allowed/escape -> <root>/outside
        let allowed = root.path().join("allowed");
        let outside = root.path().join("outside");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&outside).unwrap();
        symlink(&outside, allowed.join("escape")).unwrap();

        let provider = TestProvider::new_with_base(root.path());
        let allowed_canonical = canonicalize_path_sys(allowed.to_string_lossy(), &provider).unwrap();

        let settings = ShellPermissionSettings {
            allowed_commands: vec!["echo .*".into()],
            auto_allow_readonly: true,
            ..Default::default()
        };
        let ctx = RedirectContext {
            allowed_write_paths: vec![allowed_canonical],
            effective_cwd: allowed.clone(),
            ..Default::default()
        };

        // `> allowed/escape/../escaped.txt` must ASK: physical resolution lands at
        // <root>/escaped.txt (parent of <root>/outside), which is not in the allow-list.
        let cmd = format!("echo hi > {}/escape/../escaped.txt", allowed.to_string_lossy());
        let result = evaluate_shell_permission(&cmd, &settings, &ctx, &provider);
        assert!(
            matches!(result, PermissionEvalResult::Ask { .. }),
            "redirect using `symlink/..` to escape the allow-list must Ask, got {result:?}"
        );
    }
}
