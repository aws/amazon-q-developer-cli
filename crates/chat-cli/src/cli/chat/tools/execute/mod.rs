use std::io::Write;

use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};
use eyre::Result;
use regex::Regex;
use serde::Deserialize;
use tracing::error;

use super::env_vars_with_user_agent;
use crate::cli::agent::{
    Agent,
    PermissionEvalResult,
};
use crate::cli::chat::sanitize_unicode_tags;
use crate::cli::chat::tools::{
    InvokeOutput,
    MAX_TOOL_RESPONSE_SIZE,
    OutputKind,
};
use crate::cli::chat::util::truncate_safe;
use crate::os::Os;
use crate::util::pattern_matching::matches_any_pattern;

// Platform-specific modules
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

#[cfg(not(windows))]
mod unix;
#[cfg(not(windows))]
pub use unix::*;

// Common readonly commands that are safe to execute without user confirmation
pub const READONLY_COMMANDS: &[&str] = &[
    "ls", "cat", "echo", "pwd", "which", "head", "tail", "find", "grep", "dir", "type",
];

#[derive(Debug, Clone, Deserialize)]
pub struct ExecuteCommand {
    pub command: String,
    pub summary: Option<String>,
}

impl ExecuteCommand {
    pub fn requires_acceptance(&self, allowed_commands: Option<&Vec<String>>, allow_read_only: bool) -> bool {
        // Always require acceptance for multi-line commands.
        if self.command.contains("\n") || self.command.contains("\r") {
            return true;
        }

        let default_arr = vec![];
        let allowed_commands = allowed_commands.unwrap_or(&default_arr);

        let has_regex_match = allowed_commands
            .iter()
            .map(|cmd| Regex::new(&format!(r"\A{}\z", cmd)))
            .filter(Result::is_ok)
            .flatten()
            .any(|regex| regex.is_match(&self.command));
        if has_regex_match {
            return false;
        }

        let Some(args) = shlex::split(&self.command) else {
            return true;
        };
        const DANGEROUS_PATTERNS: &[&str] = &["<(", "$(", "`", ">", "&&", "||", "&", ";", "$", "\n", "\r", "IFS"];

        if args
            .iter()
            .any(|arg| DANGEROUS_PATTERNS.iter().any(|p| arg.contains(p)))
        {
            return true;
        }

        // Split commands by pipe and check each one
        let mut current_cmd = Vec::new();
        let mut all_commands = Vec::new();

        for arg in args {
            if arg == "|" {
                if !current_cmd.is_empty() {
                    all_commands.push(current_cmd);
                }
                current_cmd = Vec::new();
            } else if arg.contains("|") {
                // if pipe appears without spacing e.g. `echo myimportantfile|args rm` it won't get
                // parsed out, in this case - we want to verify before running
                return true;
            } else {
                current_cmd.push(arg);
            }
        }
        if !current_cmd.is_empty() {
            all_commands.push(current_cmd);
        }

        // Check if each command in the pipe chain starts with a safe command
        for cmd_args in all_commands {
            match cmd_args.first() {
                // Special casing for `find` so that we support most cases while safeguarding
                // against unwanted mutations
                Some(cmd)
                    if cmd == "find"
                        && cmd_args.iter().any(|arg| {
                            arg.contains("-exec") // includes -execdir
                                || arg.contains("-delete")
                                || arg.contains("-ok") // includes -okdir
                                || arg.contains("-fprint") // includes -fprint0 and -fprintf
                        }) =>
                {
                    return true;
                },
                Some(cmd) => {
                    // Special casing for `grep`. -P flag for perl regexp has RCE issues, apparently
                    // should not be supported within grep but is flagged as a possibility since this is perl
                    // regexp.
                    if cmd == "grep"
                        && cmd_args
                            .iter()
                            .any(|arg| arg.contains("-P") || arg.contains("--perl-regexp"))
                    {
                        return true;
                    }
                    let is_cmd_read_only = READONLY_COMMANDS.contains(&cmd.as_str());
                    if !allow_read_only || !is_cmd_read_only {
                        return true;
                    }
                },
                None => return true,
            }
        }

        false
    }

    pub async fn invoke(&self, os: &Os, output: &mut impl Write) -> Result<InvokeOutput> {
        let output = run_command(os, &self.command, MAX_TOOL_RESPONSE_SIZE / 3, Some(output)).await?;
        let clean_stdout = sanitize_unicode_tags(&output.stdout);
        let clean_stderr = sanitize_unicode_tags(&output.stderr);

        let result = serde_json::json!({
            "exit_status": output.exit_status.unwrap_or(0).to_string(),
            "stdout": clean_stdout,
            "stderr": clean_stderr,
        });

        Ok(InvokeOutput {
            output: OutputKind::Json(result),
        })
    }

    pub fn queue_description(&self, output: &mut impl Write) -> Result<()> {
        queue!(output, style::Print("I will run the following shell command: "),)?;

        // TODO: Could use graphemes for a better heuristic
        if self.command.len() > 20 {
            queue!(output, style::Print("\n"),)?;
        }

        queue!(
            output,
            style::SetForegroundColor(Color::Green),
            style::Print(&self.command),
            style::Print("\n"),
            style::ResetColor
        )?;

        // Add the summary if available
        if let Some(ref summary) = self.summary {
            super::display_purpose(Some(summary), output)?;
        }

        queue!(output, style::Print("\n"))?;

        Ok(())
    }

    pub async fn validate(&mut self, _os: &Os) -> Result<()> {
        // TODO: probably some small amount of PATH checking
        Ok(())
    }

    pub fn eval_perm(&self, _os: &Os, agent: &Agent) -> PermissionEvalResult {
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Settings {
            #[serde(default)]
            allowed_commands: Vec<String>,
            #[serde(default)]
            denied_commands: Vec<String>,
            #[serde(default = "default_allow_read_only")]
            auto_allow_readonly: bool,
        }

        fn default_allow_read_only() -> bool {
            false
        }

        let Self { command, .. } = self;
        let tool_name = if cfg!(windows) { "execute_cmd" } else { "execute_bash" };
        let is_in_allowlist = matches_any_pattern(&agent.allowed_tools, tool_name);
        match agent.tools_settings.get(tool_name) {
            Some(settings) => {
                let Settings {
                    allowed_commands,
                    denied_commands,
                    auto_allow_readonly,
                } = match serde_json::from_value::<Settings>(settings.clone()) {
                    Ok(settings) => settings,
                    Err(e) => {
                        error!("Failed to deserialize tool settings for execute_bash: {:?}", e);
                        return PermissionEvalResult::Ask;
                    },
                };

                let denied_match_set = denied_commands
                    .iter()
                    .filter_map(|dc| Regex::new(&format!(r"\A{dc}\z")).ok())
                    .filter(|r| r.is_match(command))
                    .map(|r| r.to_string())
                    .collect::<Vec<_>>();

                if !denied_match_set.is_empty() {
                    return PermissionEvalResult::Deny(denied_match_set);
                }

                if is_in_allowlist {
                    PermissionEvalResult::Allow
                } else if self.requires_acceptance(Some(&allowed_commands), auto_allow_readonly) {
                    PermissionEvalResult::Ask
                } else {
                    PermissionEvalResult::Allow
                }
            },
            None if is_in_allowlist => PermissionEvalResult::Allow,
            _ => {
                if self.requires_acceptance(None, default_allow_read_only()) {
                    PermissionEvalResult::Ask
                } else {
                    PermissionEvalResult::Allow
                }
            },
        }
    }
}

pub struct CommandResult {
    pub exit_status: Option<i32>,
    /// Truncated stdout
    pub stdout: String,
    /// Truncated stderr
    pub stderr: String,
}

// Helper function to format command output with truncation
pub fn format_output(output: &str, max_size: usize) -> String {
    format!(
        "{}{}",
        truncate_safe(output, max_size),
        if output.len() > max_size { " ... truncated" } else { "" }
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::cli::agent::ToolSettingTarget;

    #[test]
    fn test_requires_acceptance_for_readonly_commands() {
        let cmds = &[
            // Safe commands
            ("ls ~", false),
            ("ls -al ~", false),
            ("pwd", false),
            ("echo 'Hello, world!'", false),
            ("which aws", false),
            // Potentially dangerous readonly commands
            ("echo hi > myimportantfile", true),
            ("ls -al >myimportantfile", true),
            ("echo hi 2> myimportantfile", true),
            ("echo hi >> myimportantfile", true),
            ("echo $(rm myimportantfile)", true),
            ("echo `rm myimportantfile`", true),
            ("echo hello && rm myimportantfile", true),
            ("echo hello&&rm myimportantfile", true),
            ("ls nonexistantpath || rm myimportantfile", true),
            ("echo myimportantfile | xargs rm", true),
            ("echo myimportantfile|args rm", true),
            ("echo <(rm myimportantfile)", true),
            ("cat <<< 'some string here' > myimportantfile", true),
            ("echo '\n#!/usr/bin/env bash\necho hello\n' > myscript.sh", true),
            ("cat <<EOF > myimportantfile\nhello world\nEOF", true),
            // newline checks
            ("which ls\ntouch asdf", true),
            ("which ls\rtouch asdf", true),
            // $IFS check
            (
                r#"IFS=';'; for cmd in "which ls;touch asdf"; do eval "$cmd"; done"#,
                true,
            ),
            // Safe piped commands
            ("find . -name '*.rs' | grep main", false),
            ("ls -la | grep .git", false),
            ("cat file.txt | grep pattern | head -n 5", false),
            // Unsafe piped commands
            ("find . -name '*.rs' | rm", true),
            ("ls -la | grep .git | rm -rf", true),
            ("echo hello | sudo rm -rf /", true),
            // `find` command arguments
            ("find important-dir/ -exec rm {} \\;", true),
            ("find . -name '*.c' -execdir gcc -o '{}.out' '{}' \\;", true),
            ("find important-dir/ -delete", true),
            (
                "echo y | find . -type f -maxdepth 1 -okdir open -a Calculator {} +",
                true,
            ),
            ("find important-dir/ -name '*.txt'", false),
            (r#"find / -fprintf "/path/to/file" <data-to-write> -quit"#, true),
            (r"find . -${t}exec touch asdf \{\} +", true),
            (r"find . -${t:=exec} touch asdf2 \{\} +", true),
            (r#"find /tmp -name "*"  -exe$9c touch /tmp/find_result {} +"#, true),
            // `grep` command arguments
            ("echo 'test data' | grep -P '(?{system(\"date\")})'", true),
            ("echo 'test data' | grep --perl-regexp '(?{system(\"date\")})'", true),
        ];
        for (cmd, expected) in cmds {
            let tool = serde_json::from_value::<ExecuteCommand>(serde_json::json!({
                "command": cmd,
            }))
            .unwrap();
            assert_eq!(
                tool.requires_acceptance(None, true),
                *expected,
                "expected command: `{}` to have requires_acceptance: `{}`",
                cmd,
                expected
            );
        }
    }

    #[test]
    fn test_requires_acceptance_for_windows_commands() {
        let cmds = &[
            // Safe Windows commands
            ("dir", false),
            ("type file.txt", false),
            ("echo Hello, world!", false),
            // Potentially dangerous Windows commands
            ("del file.txt", true),
            ("rmdir /s /q folder", true),
            ("rd /s /q folder", true),
            ("format c:", true),
            ("erase file.txt", true),
            ("copy file.txt > important.txt", true),
            ("move file.txt destination", true),
            // Command with pipes
            ("dir | findstr txt", true),
            ("type file.txt | findstr pattern", true),
            // Dangerous piped commands
            ("dir | del", true),
            ("type file.txt | del", true),
        ];

        for (cmd, expected) in cmds {
            let tool = serde_json::from_value::<ExecuteCommand>(serde_json::json!({
                "command": cmd,
            }))
            .unwrap();
            assert_eq!(
                tool.requires_acceptance(None, true),
                *expected,
                "expected command: `{}` to have requires_acceptance: `{}`",
                cmd,
                expected
            );
        }
    }

    #[test]
    fn test_requires_acceptance_allowed_commands() {
        let allowed_cmds: &[String] = &[
            String::from("git status"),
            String::from("root"),
            String::from("command subcommand a=[0-9]{10} b=[0-9]{10}"),
            String::from("command subcommand && command subcommand"),
        ];
        let cmds = &[
            // Command first argument 'root' allowed (allows all subcommands)
            ("root", false),
            ("root subcommand", true),
            // Valid allowed_command_regex matching
            ("git", true),
            ("git status", false),
            ("command subcommand a=0123456789 b=0123456789", false),
            ("command subcommand a=0123456789 b=012345678", true),
            ("command subcommand alternate a=0123456789 b=0123456789", true),
            // Control characters ignored due to direct allowed_command_regex match
            ("command subcommand && command subcommand", false),
        ];
        for (cmd, expected) in cmds {
            let tool = serde_json::from_value::<ExecuteCommand>(serde_json::json!({
                "command": cmd,
            }))
            .unwrap();
            assert_eq!(
                tool.requires_acceptance(Option::from(&allowed_cmds.to_vec()), true),
                *expected,
                "expected command: `{}` to have requires_acceptance: `{}`",
                cmd,
                expected
            );
        }
    }

    #[tokio::test]
    async fn test_eval_perm() {
        let tool_name = if cfg!(windows) { "execute_cmd" } else { "execute_bash" };
        let mut agent = Agent {
            name: "test_agent".to_string(),
            tools_settings: {
                let mut map = HashMap::<ToolSettingTarget, serde_json::Value>::new();
                map.insert(
                    ToolSettingTarget(tool_name.to_string()),
                    serde_json::json!({
                        "allowedCommands": ["allow_wild_card .*", "allow_exact"],
                        "deniedCommands": ["git .*"]
                    }),
                );
                map
            },
            ..Default::default()
        };
        let os = Os::new().await.unwrap();

        let tool_one = serde_json::from_value::<ExecuteCommand>(serde_json::json!({
            "command": "git status",
        }))
        .unwrap();

        let res = tool_one.eval_perm(&os, &agent);
        assert!(matches!(res, PermissionEvalResult::Deny(ref rules) if rules.contains(&"\\Agit .*\\z".to_string())));

        let tool_two = serde_json::from_value::<ExecuteCommand>(serde_json::json!({
            "command": "this_is_not_a_read_only_command",
        }))
        .unwrap();

        let res = tool_two.eval_perm(&os, &agent);
        assert!(matches!(res, PermissionEvalResult::Ask));

        let tool_allow_wild_card = serde_json::from_value::<ExecuteCommand>(serde_json::json!({
            "command": "allow_wild_card some_arg",
        }))
        .unwrap();
        let res = tool_allow_wild_card.eval_perm(&os, &agent);
        assert!(matches!(res, PermissionEvalResult::Allow));

        let tool_allow_exact_should_ask = serde_json::from_value::<ExecuteCommand>(serde_json::json!({
            "command": "allow_exact some_arg",
        }))
        .unwrap();
        let res = tool_allow_exact_should_ask.eval_perm(&os, &agent);
        assert!(matches!(res, PermissionEvalResult::Ask));

        let tool_allow_exact_should_allow = serde_json::from_value::<ExecuteCommand>(serde_json::json!({
            "command": "allow_exact",
        }))
        .unwrap();
        let res = tool_allow_exact_should_allow.eval_perm(&os, &agent);
        assert!(matches!(res, PermissionEvalResult::Allow));

        agent.allowed_tools.insert(tool_name.to_string());

        let res = tool_two.eval_perm(&os, &agent);
        assert!(matches!(res, PermissionEvalResult::Allow));

        // Denied list should remain denied
        let res = tool_one.eval_perm(&os, &agent);
        assert!(matches!(res, PermissionEvalResult::Deny(ref rules) if rules.contains(&"\\Agit .*\\z".to_string())));
    }

    #[tokio::test]
    async fn test_eval_perm_allow_read_only_default() {
        use crate::cli::agent::Agent;

        let os = Os::new().await.unwrap();

        // Test read-only command with default settings (allow_read_only = false)
        let readonly_cmd = serde_json::from_value::<ExecuteCommand>(serde_json::json!({
            "command": "ls -la",
        }))
        .unwrap();

        let agent = Agent::default();
        let res = readonly_cmd.eval_perm(&os, &agent);
        // Should ask for confirmation even for read-only commands by default
        assert!(matches!(res, PermissionEvalResult::Ask));

        // Test non-read-only command with default settings
        let write_cmd = serde_json::from_value::<ExecuteCommand>(serde_json::json!({
            "command": "rm file.txt",
        }))
        .unwrap();

        let res = write_cmd.eval_perm(&os, &agent);
        // Should ask for confirmation for write commands
        assert!(matches!(res, PermissionEvalResult::Ask));
    }

    #[tokio::test]
    async fn test_eval_perm_allow_read_only_enabled() {
        use std::collections::HashMap;

        use crate::cli::agent::{
            Agent,
            ToolSettingTarget,
        };

        let os = Os::new().await.unwrap();
        let tool_name = if cfg!(windows) { "execute_cmd" } else { "execute_bash" };

        let agent = Agent {
            name: "test_agent".to_string(),
            tools_settings: {
                let mut map = HashMap::<ToolSettingTarget, serde_json::Value>::new();
                map.insert(
                    ToolSettingTarget(tool_name.to_string()),
                    serde_json::json!({
                        "autoAllowReadonly": true
                    }),
                );
                map
            },
            ..Default::default()
        };

        // Test read-only command with allow_read_only = true
        let readonly_cmd = serde_json::from_value::<ExecuteCommand>(serde_json::json!({
            "command": "ls -la",
        }))
        .unwrap();

        let res = readonly_cmd.eval_perm(&os, &agent);
        // Should allow read-only commands without confirmation
        assert!(matches!(res, PermissionEvalResult::Allow));

        // Test write command with allow_read_only = true
        let write_cmd = serde_json::from_value::<ExecuteCommand>(serde_json::json!({
            "command": "rm file.txt",
        }))
        .unwrap();

        let res = write_cmd.eval_perm(&os, &agent);
        // Should still ask for confirmation for write commands
        assert!(matches!(res, PermissionEvalResult::Ask));
    }

    #[tokio::test]
    async fn test_eval_perm_allow_read_only_with_denied_commands() {
        use std::collections::HashMap;

        use crate::cli::agent::{
            Agent,
            ToolSettingTarget,
        };

        let os = Os::new().await.unwrap();
        let tool_name = if cfg!(windows) { "execute_cmd" } else { "execute_bash" };

        let agent = Agent {
            name: "test_agent".to_string(),
            tools_settings: {
                let mut map = HashMap::<ToolSettingTarget, serde_json::Value>::new();
                map.insert(
                    ToolSettingTarget(tool_name.to_string()),
                    serde_json::json!({
                        "autoAllowReadonly": true,
                        "deniedCommands": ["ls .*"]
                    }),
                );
                map
            },
            ..Default::default()
        };

        // Test read-only command that's in denied list
        let denied_readonly_cmd = serde_json::from_value::<ExecuteCommand>(serde_json::json!({
            "command": "ls -la",
        }))
        .unwrap();

        let res = denied_readonly_cmd.eval_perm(&os, &agent);
        // Should deny even read-only commands if they're in denied list
        assert!(
            matches!(res, PermissionEvalResult::Deny(ref commands) if commands.contains(&"\\Als .*\\z".to_string()))
        );

        // Test different read-only command not in denied list
        let allowed_readonly_cmd = serde_json::from_value::<ExecuteCommand>(serde_json::json!({
            "command": "cat file.txt",
        }))
        .unwrap();

        let res = allowed_readonly_cmd.eval_perm(&os, &agent);
        // Should allow read-only commands not in denied list
        assert!(matches!(res, PermissionEvalResult::Allow));
    }

    #[tokio::test]
    async fn test_cloudtrail_tracking() {
        use crate::cli::chat::consts::{
            USER_AGENT_APP_NAME,
            USER_AGENT_ENV_VAR,
            USER_AGENT_VERSION_KEY,
            USER_AGENT_VERSION_VALUE,
        };

        let os = Os::new().await.unwrap();

        // Test that env_vars_with_user_agent sets the AWS_EXECUTION_ENV variable correctly
        let env_vars = env_vars_with_user_agent(&os);

        // Check that AWS_EXECUTION_ENV is set
        assert!(env_vars.contains_key(USER_AGENT_ENV_VAR));

        let user_agent_value = env_vars.get(USER_AGENT_ENV_VAR).unwrap();

        // Check the format is correct
        let expected_metadata = format!(
            "{} {}/{}",
            USER_AGENT_APP_NAME, USER_AGENT_VERSION_KEY, USER_AGENT_VERSION_VALUE
        );
        assert!(user_agent_value.contains(&expected_metadata));
    }

    #[tokio::test]
    async fn test_cloudtrail_tracking_with_existing_env() {
        use crate::cli::chat::consts::{
            USER_AGENT_APP_NAME,
            USER_AGENT_ENV_VAR,
        };

        let os = Os::new().await.unwrap();

        // Set an existing AWS_EXECUTION_ENV value (safe because Os uses in-memory hashmap in tests)
        unsafe {
            os.env.set_var(USER_AGENT_ENV_VAR, "ExistingValue");
        }

        let env_vars = env_vars_with_user_agent(&os);
        let user_agent_value = env_vars.get(USER_AGENT_ENV_VAR).unwrap();

        // Should contain both the existing value and our metadata
        assert!(user_agent_value.contains("ExistingValue"));
        assert!(user_agent_value.contains(USER_AGENT_APP_NAME));
    }
}
