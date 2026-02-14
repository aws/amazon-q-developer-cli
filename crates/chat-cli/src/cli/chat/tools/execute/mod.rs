use std::io::Write;
use std::path::Path;

use agent::shell_permission::{
    ShellPermissionSettings,
    evaluate_shell_permission,
};
use crossterm::queue;
use crossterm::style::{
    self,
};
use eyre::Result;
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
    ToolInfo,
    display_tool_use,
};

/// Maximum size for command output (stdout/stderr) in bytes.
/// This is a third of MAX_TOOL_RESPONSE_SIZE to allow room for both stdout and stderr
/// plus the JSON structure in the response.
pub const MAX_COMMAND_OUTPUT_SIZE: usize = MAX_TOOL_RESPONSE_SIZE / 3;
use crate::cli::chat::util::truncate_safe;
use crate::os::Os;
use crate::theme::StyledText;
use crate::util::tool_permission_checker::is_tool_in_allowlist;

// Platform-specific modules
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

#[cfg(not(windows))]
mod unix;
#[cfg(not(windows))]
pub use unix::*;

#[derive(Debug, Clone, Deserialize)]
pub struct ExecuteCommand {
    pub command: String,
    pub summary: Option<String>,
    pub working_dir: Option<String>,
}

impl ExecuteCommand {
    pub const INFO: ToolInfo = ToolInfo {
        spec_name: "execute_bash",
        preferred_alias: "shell",
        aliases: &["execute_bash", "execute_cmd", "shell"],
    };

    /// Returns the canonicalized working directory path, if set.
    fn canonical_working_dir(&self, os: &Os) -> Result<Option<String>> {
        self.working_dir
            .as_ref()
            .map(|dir| {
                crate::util::paths::canonicalizes_path(os, dir)
                    .map_err(|e| eyre::eyre!("Invalid working directory '{}': {}", dir, e))
            })
            .transpose()
    }

    pub async fn invoke(&self, os: &Os, output: &mut impl Write) -> Result<InvokeOutput> {
        let working_dir = self.canonical_working_dir(os)?;
        let output = run_command(os, &self.command, working_dir.as_deref(), Some(output)).await?;
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

    pub fn queue_description(&self, tool: &super::tool::Tool, os: &Os, output: &mut impl Write) -> Result<()> {
        queue!(output, style::Print("I will run the following command: "),)?;
        queue!(
            output,
            StyledText::brand_fg(),
            style::Print(&self.command),
            StyledText::reset(),
        )?;
        if let Some(ref dir) = self.working_dir {
            let cwd = os.env.current_dir().unwrap_or_default();
            let formatted = super::format_path(cwd, dir);
            if !formatted.is_empty() {
                queue!(
                    output,
                    style::Print(" (in "),
                    StyledText::brand_fg(),
                    style::Print(&formatted),
                    StyledText::reset(),
                    style::Print(")"),
                )?;
            }
        }
        display_tool_use(tool, output)?;
        queue!(output, style::Print("\n"))?;

        // Add the summary as purpose if available on a separate line
        if let Some(ref summary) = self.summary {
            queue!(
                output,
                style::Print("Purpose: "),
                style::Print(summary),
                style::Print("\n"),
            )?;
        }

        queue!(output, style::Print("\n"))?;

        Ok(())
    }

    pub async fn validate(&mut self, os: &Os) -> Result<()> {
        if let Some(ref dir) = self.canonical_working_dir(os)?
            && !Path::new(dir).is_dir()
        {
            eyre::bail!("Working directory is not a directory: {}", dir);
        }
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
            #[serde(default)]
            deny_by_default: bool,
            #[serde(default = "default_allow_read_only")]
            auto_allow_readonly: bool,
        }

        fn default_allow_read_only() -> bool {
            false
        }

        let Self { command, .. } = self;
        let is_in_allowlist = Self::INFO
            .aliases
            .iter()
            .any(|alias| is_tool_in_allowlist(&agent.allowed_tools, alias, None));

        // Parse settings from agent config
        let settings: Settings = match Self::INFO
            .aliases
            .iter()
            .find_map(|alias| agent.tools_settings.get(*alias))
        {
            Some(v) => match serde_json::from_value::<Settings>(v.clone()) {
                Ok(settings) => settings,
                Err(e) => {
                    error!("Failed to deserialize tool settings for execute_bash: {:?}", e);
                    return PermissionEvalResult::ask();
                },
            },
            None => Settings {
                allowed_commands: vec![],
                denied_commands: vec![],
                deny_by_default: false,
                auto_allow_readonly: default_allow_read_only(),
            },
        };

        let shell_settings = ShellPermissionSettings {
            allowed_commands: settings.allowed_commands,
            denied_commands: settings.denied_commands,
            auto_allow_readonly: settings.auto_allow_readonly,
            deny_by_default: settings.deny_by_default,
            is_tool_allowed: is_in_allowlist,
        };

        match evaluate_shell_permission(command, &shell_settings) {
            agent::protocol::PermissionEvalResult::Allow => PermissionEvalResult::Allow,
            agent::protocol::PermissionEvalResult::Ask { .. } => PermissionEvalResult::ask(),
            agent::protocol::PermissionEvalResult::Deny { reason } => PermissionEvalResult::Deny(vec![reason]),
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
    use crate::cli::agent::{
        Agent,
        ToolSettingTarget,
    };

    /// Helper: build an Agent with the given tool settings JSON.
    fn make_agent(settings_json: serde_json::Value) -> Agent {
        let tool_name = ExecuteCommand::INFO.preferred_alias;
        Agent {
            name: "test_agent".to_string(),
            tools_settings: {
                let mut map = HashMap::<ToolSettingTarget, serde_json::Value>::new();
                map.insert(ToolSettingTarget(tool_name.to_string()), settings_json);
                map
            },
            ..Default::default()
        }
    }

    /// Helper: run eval_perm() for each (command, should_require_acceptance) pair.
    /// Collects all mismatches and panics with a summary.
    async fn assert_eval_perm_cases(agent: &Agent, cases: &[(&str, bool)]) {
        let os = Os::new().await.unwrap();
        let mut failures = Vec::new();

        for (cmd, expected) in cases {
            let tool = serde_json::from_value::<ExecuteCommand>(serde_json::json!({
                "command": cmd,
            }))
            .unwrap();
            let res = tool.eval_perm(&os, agent);

            let requires_acceptance = matches!(res, PermissionEvalResult::Ask { .. } | PermissionEvalResult::Deny(_));
            if requires_acceptance != *expected {
                failures.push(format!(
                    "Command '{}': expected requires_acceptance={}, got {:?}",
                    cmd, expected, res
                ));
            }
        }

        assert!(
            failures.is_empty(),
            "Backward compatibility failures:\n{}",
            failures.join("\n")
        );
    }

    /// Migrated from requires_acceptance() to eval_perm() to verify backward compatibility
    /// with the new tree-sitter based shell permission system.
    #[tokio::test]
    async fn test_requires_acceptance_for_readonly_commands() {
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
            ("find important-dir/ -fls /etc/passwd", true),
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
        let agent = make_agent(serde_json::json!({
            "autoAllowReadonly": true
        }));

        assert_eval_perm_cases(&agent, cmds).await;
    }

    /// Migrated from requires_acceptance() to eval_perm() to verify backward compatibility
    /// with the new tree-sitter based shell permission system.
    #[tokio::test]
    async fn test_requires_acceptance_for_windows_commands() {
        let cmds: &[(&str, bool)] = &[
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

        let agent = make_agent(serde_json::json!({
            "autoAllowReadonly": true
        }));
        assert_eval_perm_cases(&agent, cmds).await;
    }

    /// Migrated from requires_acceptance() to eval_perm() to verify backward compatibility
    /// with the new tree-sitter based shell permission system.
    #[tokio::test]
    async fn test_requires_acceptance_allowed_commands() {
        let agent = make_agent(serde_json::json!({
            "autoAllowReadonly": true,
            "allowedCommands": [
                "git status",
                "root",
                "command subcommand a=[0-9]{10} b=[0-9]{10}",
                "command subcommand && command subcommand"
            ]
        }));

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
            // dangerous patterns
            // New system correctly handles dangerous chars inside single quotes (literal strings)
            ("echo 'test<(data'", false),
            ("echo 'test$(data)'", false),
            ("echo 'test`data`'", false),
            ("echo 'test' > output.txt", true),
            ("echo 'test data' && touch main.py", true),
            ("echo 'test' || rm file", true),
            ("echo 'test' & background", true),
            ("echo 'test data'; touch main.py", true),
            ("echo $HOME", true),
            // New system correctly handles \n inside single quotes (literal string)
            ("echo 'test\nrm file'", false),
            ("echo 'test\rrm file'", true),
            ("IFS=/ malicious", true),
            (r#"/c/"+"/m/"+"/d/.exe"#, true),
            ("$^(calc.exe)", true),
            ("curl http://trusted.com@evil.com", true),
        ];

        assert_eval_perm_cases(&agent, cmds).await;
    }

    #[tokio::test]
    async fn test_eval_perm() {
        let tool_name = ExecuteCommand::INFO.preferred_alias;
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
        assert!(matches!(res, PermissionEvalResult::Deny(ref rules) if rules.contains(&"git .*".to_string())));

        let tool_two = serde_json::from_value::<ExecuteCommand>(serde_json::json!({
            "command": "this_is_not_a_read_only_command",
        }))
        .unwrap();

        let res = tool_two.eval_perm(&os, &agent);
        assert!(matches!(res, PermissionEvalResult::Ask { .. }));

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
        assert!(matches!(res, PermissionEvalResult::Ask { .. }));

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
        assert!(matches!(res, PermissionEvalResult::Deny(ref rules) if rules.contains(&"git .*".to_string())));
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
        assert!(matches!(res, PermissionEvalResult::Ask { .. }));

        // Test non-read-only command with default settings
        let write_cmd = serde_json::from_value::<ExecuteCommand>(serde_json::json!({
            "command": "rm file.txt",
        }))
        .unwrap();

        let res = write_cmd.eval_perm(&os, &agent);
        // Should ask for confirmation for write commands
        assert!(matches!(res, PermissionEvalResult::Ask { .. }));
    }

    #[tokio::test]
    async fn test_eval_perm_allow_read_only_enabled() {
        let os = Os::new().await.unwrap();
        let tool_name = ExecuteCommand::INFO.preferred_alias;

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
        assert!(matches!(res, PermissionEvalResult::Ask { .. }));
    }

    #[tokio::test]
    async fn test_eval_perm_allow_read_only_with_denied_commands() {
        let os = Os::new().await.unwrap();
        let tool_name = ExecuteCommand::INFO.preferred_alias;

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
        assert!(matches!(res, PermissionEvalResult::Deny(ref commands) if commands.contains(&"ls .*".to_string())));

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
    async fn test_eval_perm_denied_commands_invalid_regex() {
        let os = Os::new().await.unwrap();
        let tool_name = ExecuteCommand::INFO.preferred_alias;
        let agent = Agent {
            name: "test_agent".to_string(),
            tools_settings: {
                let mut map = HashMap::<ToolSettingTarget, serde_json::Value>::new();
                map.insert(
                    ToolSettingTarget(tool_name.to_string()),
                    serde_json::json!({
                        "deniedCommands": ["^(?!ls$).*"]  // Invalid regex with unsupported lookahead
                    }),
                );
                map
            },
            ..Default::default()
        };

        // Test command that should be denied by the pattern
        let pwd_cmd = serde_json::from_value::<ExecuteCommand>(serde_json::json!({"command": "pwd",})).unwrap();
        let res = pwd_cmd.eval_perm(&os, &agent);
        assert!(
            matches!(res, PermissionEvalResult::Deny(_)),
            "Invalid regex should deny all commands, got {res:?}"
        );
    }

    #[tokio::test]
    async fn test_eval_perm_deny_by_default() {
        let os = Os::new().await.unwrap();
        let tool_name = ExecuteCommand::INFO.preferred_alias;

        let agent = Agent {
            name: "test_agent".to_string(),
            tools_settings: {
                let mut map = HashMap::<ToolSettingTarget, serde_json::Value>::new();
                map.insert(
                    ToolSettingTarget(tool_name.to_string()),
                    serde_json::json!({
                        "allowedCommands": ["ls"],
                        "denyByDefault": true
                    }),
                );
                map
            },
            ..Default::default()
        };

        // Test allowed command - should be allowed
        let ls_cmd = serde_json::from_value::<ExecuteCommand>(serde_json::json!({"command": "ls",})).unwrap();
        let res = ls_cmd.eval_perm(&os, &agent);
        assert!(matches!(res, PermissionEvalResult::Allow));

        // Test non-allowed command - should be denied (not asked)
        let pwd_cmd = serde_json::from_value::<ExecuteCommand>(serde_json::json!({"command": "pwd"})).unwrap();
        let res = pwd_cmd.eval_perm(&os, &agent);
        assert!(matches!(res, PermissionEvalResult::Deny(_)));
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
        let expected_metadata = format!("{USER_AGENT_APP_NAME} {USER_AGENT_VERSION_KEY}/{USER_AGENT_VERSION_VALUE}");
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

    #[tokio::test]
    async fn test_validate_working_dir() {
        let os = Os::new().await.unwrap();
        let temp_dir = tempfile::tempdir().unwrap();

        // Valid directory
        let mut cmd = ExecuteCommand {
            command: "pwd".to_string(),
            summary: None,
            working_dir: Some(temp_dir.path().to_string_lossy().to_string()),
        };
        assert!(cmd.validate(&os).await.is_ok(), "valid directory should pass");

        // Non-existent directory
        cmd.working_dir = Some("/nonexistent_dir_12345".to_string());
        assert!(cmd.validate(&os).await.is_err(), "non-existent directory should fail");
    }
}
