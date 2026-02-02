//! Layer 2: Command detection for dangerous patterns and readonly commands.

use std::collections::HashMap;

use serde::Deserialize;

use super::parser::{
    ChainOperator,
    ParsedCommand,
};

/// Danger level of a detected pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DangerLevel {
    /// No dangerous patterns detected.
    None,
    /// Risky but no direct code execution (redirections, variable expansion, chaining).
    Low,
    /// Can directly execute arbitrary code (command substitution, eval, find -exec, etc.).
    High,
}

/// Result of Layer 2 detection for a command chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectResult {
    /// Overall danger level (highest across all commands).
    pub danger_level: DangerLevel,
    /// Whether all commands are readonly.
    pub is_readonly: bool,
    /// Per-command danger levels (parallel to input commands).
    pub command_danger_levels: Vec<DangerLevel>,
    /// Per-command readonly flags (parallel to input commands).
    pub command_readonly: Vec<bool>,
}

#[derive(Deserialize)]
struct DetectorConfig {
    dangerous_commands: Vec<String>,
    dangerous_options: HashMap<String, Vec<String>>,
    dangerous_env_vars: Vec<String>,
    env_builtins: Vec<String>,
    shells: Vec<String>,
    safe_commands: Vec<String>,
    safe_options: HashMap<String, Vec<String>>,
}

fn load_config() -> DetectorConfig {
    let json = include_str!("detector_config.json");
    serde_json::from_str(json).expect("Failed to parse detector_config.json")
}

/// Run Layer 2 detection on a command chain.
pub fn detect(commands: &[ParsedCommand]) -> DetectResult {
    let config = load_config();

    // Per-command detection
    let command_danger_levels: Vec<_> = commands
        .iter()
        .map(|cmd| get_danger_level_with_config(cmd, &config))
        .collect();

    let command_readonly: Vec<_> = commands
        .iter()
        .map(|cmd| is_readonly_with_config(cmd, &config))
        .collect();

    // Multi-command detection (patterns that span commands)
    let chain_danger = detect_chain_patterns(commands, &config);

    // Aggregate
    let max_single = command_danger_levels.iter().max().copied().unwrap_or(DangerLevel::None);
    let danger_level = max_single.max(chain_danger);
    let is_readonly = command_readonly.iter().all(|&r| r);

    DetectResult {
        danger_level,
        is_readonly,
        command_danger_levels,
        command_readonly,
    }
}

/// Detect patterns that span multiple commands.
fn detect_chain_patterns(commands: &[ParsedCommand], config: &DetectorConfig) -> DangerLevel {
    for (i, cmd) in commands.iter().enumerate() {
        // Pipe to shell: `curl | bash`
        if let Some(ChainOperator::Pipe) = cmd.operator
            && let Some(next) = commands.get(i + 1)
            && config.shells.contains(&next.command_name)
        {
            return DangerLevel::High;
        }
    }
    DangerLevel::None
}

// ============================================================================
// Danger Detection
// ============================================================================

fn get_danger_level_with_config(cmd: &ParsedCommand, config: &DetectorConfig) -> DangerLevel {
    // High: direct code execution
    if cmd.has_command_substitution
        || cmd.has_process_substitution
        || has_dangerous_command_options(cmd, config)
        || is_dangerous_env_manipulation(cmd, config)
    {
        return DangerLevel::High;
    }

    // Low: risky but no direct execution
    if cmd.has_redirection
        || cmd.has_variable_expansion
        || cmd.has_variable_assignment
        || cmd.has_ansi_c_string
        || cmd.operator.is_some()
    {
        return DangerLevel::Low;
    }

    DangerLevel::None
}

fn has_dangerous_command_options(cmd: &ParsedCommand, config: &DetectorConfig) -> bool {
    if config.dangerous_commands.contains(&cmd.command_name) {
        return true;
    }

    if let Some(options) = config.dangerous_options.get(&cmd.command_name) {
        for opt in options {
            if cmd.args.iter().any(|a| a.contains(opt)) || cmd.command.contains(opt) {
                return true;
            }
        }
    }

    false
}

fn is_dangerous_env_manipulation(cmd: &ParsedCommand, config: &DetectorConfig) -> bool {
    if !cmd.has_variable_assignment || !config.env_builtins.contains(&cmd.command_name) {
        return false;
    }

    for arg in &cmd.args {
        if arg.starts_with('-') {
            continue;
        }
        let var_name = arg.split('=').next().unwrap_or(arg);
        if config
            .dangerous_env_vars
            .iter()
            .any(|v| v.eq_ignore_ascii_case(var_name))
        {
            return true;
        }
        break;
    }

    false
}

// ============================================================================
// Readonly Detection
// ============================================================================

fn is_readonly_with_config(cmd: &ParsedCommand, config: &DetectorConfig) -> bool {
    let cmd_name = cmd.command_name.as_str();

    if let Some(safe_opts) = config.safe_options.get(cmd_name) {
        return cmd.args.first().is_some_and(|sub| safe_opts.iter().any(|s| s == sub));
    }

    config.safe_commands.iter().any(|s| s == cmd_name)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cmd(command: &str) -> ParsedCommand {
        let parts: Vec<&str> = command.split_whitespace().collect();
        let cmd_name = parts.first().unwrap_or(&"").to_string();
        ParsedCommand {
            command: command.to_string(),
            command_path: cmd_name.clone(),
            command_name: cmd_name,
            args: parts.get(1..).unwrap_or(&[]).iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    fn make_cmd_with_flags(command: &str, redir: bool, subst: bool, var_exp: bool, var_assign: bool) -> ParsedCommand {
        let mut cmd = make_cmd(command);
        cmd.has_redirection = redir;
        cmd.has_command_substitution = subst;
        cmd.has_variable_expansion = var_exp;
        cmd.has_variable_assignment = var_assign;
        cmd
    }

    fn get_danger_level(cmd: &ParsedCommand) -> DangerLevel {
        let config = load_config();
        get_danger_level_with_config(cmd, &config)
    }

    fn is_readonly_command(cmd: &ParsedCommand) -> bool {
        let config = load_config();
        is_readonly_with_config(cmd, &config)
    }

    #[test]
    fn test_safe_command() {
        assert_eq!(get_danger_level(&make_cmd("ls -la")), DangerLevel::None);
    }

    #[test]
    fn test_redirection_low() {
        assert_eq!(
            get_danger_level(&make_cmd_with_flags("echo hello", true, false, false, false)),
            DangerLevel::Low
        );
    }

    #[test]
    fn test_command_substitution_high() {
        assert_eq!(
            get_danger_level(&make_cmd_with_flags("echo result", false, true, false, false)),
            DangerLevel::High
        );
    }

    #[test]
    fn test_variable_expansion_low() {
        assert_eq!(
            get_danger_level(&make_cmd_with_flags("echo value", false, false, true, false)),
            DangerLevel::Low
        );
    }

    #[test]
    fn test_variable_assignment_low() {
        assert_eq!(
            get_danger_level(&make_cmd_with_flags("cmd", false, false, false, true)),
            DangerLevel::Low
        );
    }

    #[test]
    fn test_dangerous_commands_high() {
        assert_eq!(get_danger_level(&make_cmd("eval 'code'")), DangerLevel::High);
        assert_eq!(get_danger_level(&make_cmd("xargs rm")), DangerLevel::High);
        assert_eq!(get_danger_level(&make_cmd("find . -exec rm {} \\;")), DangerLevel::High);
    }

    #[test]
    fn test_shell_c_high() {
        assert_eq!(get_danger_level(&make_cmd("bash -c 'cmd'")), DangerLevel::High);
    }

    #[test]
    fn test_dangerous_env_export_pager_high() {
        assert_eq!(
            get_danger_level(&make_cmd_with_flags("export PAGER=evil", false, false, false, true)),
            DangerLevel::High
        );
    }

    #[test]
    fn test_safe_env_export_low() {
        assert_eq!(
            get_danger_level(&make_cmd_with_flags("export MY_VAR=value", false, false, false, true)),
            DangerLevel::Low
        );
    }

    #[test]
    fn test_readonly_always_safe() {
        assert!(is_readonly_command(&make_cmd("ls -la")));
        assert!(is_readonly_command(&make_cmd("cat file")));
        assert!(is_readonly_command(&make_cmd("pwd")));
    }

    #[test]
    fn test_readonly_not_safe() {
        assert!(!is_readonly_command(&make_cmd("rm file")));
        assert!(!is_readonly_command(&make_cmd("mv a b")));
    }

    #[test]
    fn test_readonly_safe_subcommands() {
        assert!(is_readonly_command(&make_cmd("git status")));
        assert!(is_readonly_command(&make_cmd("cargo metadata")));
    }

    #[test]
    fn test_readonly_unsafe_subcommands() {
        assert!(!is_readonly_command(&make_cmd("git push")));
        assert!(!is_readonly_command(&make_cmd("cargo build")));
    }
}
