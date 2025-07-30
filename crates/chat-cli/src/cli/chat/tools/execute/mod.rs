use std::io::Write;

use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};
use eyre::Result;
use serde::Deserialize;
use tracing::error;

use crate::cli::agent::{
    Agent,
    PermissionEvalResult,
};
use crate::cli::chat::tools::{
    InvokeOutput,
    MAX_TOOL_RESPONSE_SIZE,
    OutputKind,
};
use crate::cli::chat::util::truncate_safe;
use crate::os::Os;

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
        let default_arr = vec![];
        let allowed_commands = allowed_commands.unwrap_or(&default_arr);
        let Some(args) = shlex::split(&self.command) else {
            return true;
        };
        const DANGEROUS_PATTERNS: &[&str] = &["<(", "$(", "`", ">", "&&", "||", "&", ";"];

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
                        && cmd_args
                            .iter()
                            .any(|arg| arg.contains("-exec") || arg.contains("-delete")) =>
                {
                    return true;
                },
                Some(cmd) => {
                    if allowed_commands.contains(cmd) {
                        continue;
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

    pub async fn invoke(&self, output: &mut impl Write) -> Result<InvokeOutput> {
        let output = run_command(&self.command, MAX_TOOL_RESPONSE_SIZE / 3, Some(output)).await?;
        let result = serde_json::json!({
            "exit_status": output.exit_status.unwrap_or(0).to_string(),
            "stdout": output.stdout,
            "stderr": output.stderr,
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

    pub fn eval_perm(&self, agent: &Agent) -> PermissionEvalResult {
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Settings {
            #[serde(default)]
            allowed_commands: Vec<String>,
            #[serde(default)]
            denied_commands: Vec<String>,
            #[serde(default = "default_allow_read_only")]
            allow_read_only: bool,
        }

        fn default_allow_read_only() -> bool {
            true
        }

        let Self { command, .. } = self;
        let tool_name = if cfg!(windows) { "execute_cmd" } else { "execute_bash" };
        let is_in_allowlist = agent.allowed_tools.contains("execute_bash");
        match agent.tools_settings.get(tool_name) {
            Some(settings) if is_in_allowlist => {
                let Settings {
                    allowed_commands,
                    denied_commands,
                    allow_read_only,
                } = match serde_json::from_value::<Settings>(settings.clone()) {
                    Ok(settings) => settings,
                    Err(e) => {
                        error!("Failed to deserialize tool settings for execute_bash: {:?}", e);
                        return PermissionEvalResult::Ask;
                    },
                };

                if denied_commands.iter().any(|dc| command.contains(dc)) {
                    return PermissionEvalResult::Deny;
                }

                if self.requires_acceptance(Some(&allowed_commands), allow_read_only) {
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
    use super::*;

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
}
