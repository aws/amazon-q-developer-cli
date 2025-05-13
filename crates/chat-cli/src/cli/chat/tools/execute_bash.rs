use std::collections::VecDeque;
use std::io::Write;
use std::process::{
    ExitStatus,
    Stdio,
};
use std::str::from_utf8;

use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};
use eyre::{
    Context as EyreContext,
    Result,
};
use serde::Deserialize;
use tokio::io::AsyncBufReadExt;
use tokio::select;
use tracing::error;

use super::super::util::truncate_safe;
use super::{
    InvokeOutput,
    MAX_TOOL_RESPONSE_SIZE,
    OutputKind,
};
use crate::cli::chat::{
    CONTINUATION_LINE,
    PURPOSE_ARROW,
};
use crate::platform::Context;
const READONLY_COMMANDS: &[&str] = &["ls", "cat", "echo", "pwd", "which", "head", "tail", "find", "grep"];

#[derive(Debug, Clone, Deserialize)]
pub struct ExecuteBash {
    pub command: String,
    pub summary: Option<String>,
}

impl ExecuteBash {
    pub fn requires_acceptance(&self) -> bool {
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
                Some(cmd) if !READONLY_COMMANDS.contains(&cmd.as_str()) => return true,
                None => return true,
                _ => (),
            }
        }

        false
    }

    pub async fn invoke(&self, updates: impl Write) -> Result<InvokeOutput> {
        let output = run_command(&self.command, MAX_TOOL_RESPONSE_SIZE / 3, Some(updates)).await?;
        let result = serde_json::json!({
            "exit_status": output.exit_status.unwrap_or(0).to_string(),
            "stdout": output.stdout,
            "stderr": output.stderr,
        });

        Ok(InvokeOutput {
            output: OutputKind::Json(result),
        })
    }

    pub fn queue_description(&self, updates: &mut impl Write) -> Result<()> {
        queue!(updates, style::Print("I will run the following shell command: "),)?;

        // TODO: Could use graphemes for a better heuristic
        if self.command.len() > 20 {
            queue!(updates, style::Print("\n"),)?;
        }

        queue!(
            updates,
            style::SetForegroundColor(Color::Green),
            style::Print(&self.command),
            style::Print("\n"),
            style::ResetColor
        )?;

        // Add the summary if available
        if let Some(summary) = &self.summary {
            queue!(
                updates,
                style::Print(CONTINUATION_LINE),
                style::Print("\n"),
                style::Print(PURPOSE_ARROW),
                style::SetForegroundColor(Color::Blue),
                style::Print("Purpose: "),
                style::ResetColor,
                style::Print(summary),
                style::Print("\n"),
            )?;
        }

        queue!(updates, style::Print("\n"))?;

        Ok(())
    }

    pub async fn validate(&mut self, _ctx: &Context) -> Result<()> {
        // TODO: probably some small amount of PATH checking
        Ok(())
    }
}

pub struct CommandResult {
    pub exit_status: Option<i32>,
    /// Truncated stdout
    pub stdout: String,
    /// Truncated stderr
    pub stderr: String,
}

/// Run a bash command.
/// # Arguments
/// * `max_result_size` - max size of output streams, truncating if required
/// * `updates` - output stream to push informational messages about the progress
/// # Returns
/// A [`CommandResult`]
pub async fn run_command<W: Write>(
    command: &str,
    max_result_size: usize,
    mut updates: Option<W>,
) -> Result<CommandResult> {
    // We need to maintain a handle on stderr and stdout, but pipe it to the terminal as well
    let mut child = tokio::process::Command::new("bash")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .wrap_err_with(|| format!("Unable to spawn command '{}'", command))?;

    let stdout_final: String;
    let stderr_final: String;
    let exit_status: ExitStatus;

    // Buffered output vs all-at-once
    if let Some(u) = updates.as_mut() {
        let stdout = child.stdout.take().unwrap();
        let stdout = tokio::io::BufReader::new(stdout);
        let mut stdout = stdout.lines();

        let stderr = child.stderr.take().unwrap();
        let stderr = tokio::io::BufReader::new(stderr);
        let mut stderr = stderr.lines();

        const LINE_COUNT: usize = 1024;
        let mut stdout_buf = VecDeque::with_capacity(LINE_COUNT);
        let mut stderr_buf = VecDeque::with_capacity(LINE_COUNT);

        let mut stdout_done = false;
        let mut stderr_done = false;
        exit_status = loop {
            select! {
                biased;
                line = stdout.next_line(), if !stdout_done => match line {
                    Ok(Some(line)) => {
                        writeln!(u, "{line}")?;
                        if stdout_buf.len() >= LINE_COUNT {
                            stdout_buf.pop_front();
                        }
                        stdout_buf.push_back(line);
                    },
                    Ok(None) => stdout_done = true,
                    Err(err) => error!(%err, "Failed to read stdout of child process"),
                },
                line = stderr.next_line(), if !stderr_done => match line {
                    Ok(Some(line)) => {
                        writeln!(u, "{line}")?;
                        if stderr_buf.len() >= LINE_COUNT {
                            stderr_buf.pop_front();
                        }
                        stderr_buf.push_back(line);
                    },
                    Ok(None) => stderr_done = true,
                    Err(err) => error!(%err, "Failed to read stderr of child process"),
                },
                exit_status = child.wait() => {
                    break exit_status;
                },
            };
        }
        .wrap_err_with(|| format!("No exit status for '{}'", command))?;

        u.flush()?;

        stdout_final = stdout_buf.into_iter().collect::<Vec<_>>().join("\n");
        stderr_final = stderr_buf.into_iter().collect::<Vec<_>>().join("\n");
    } else {
        // Take output all at once since we are not reporting anything in real time
        //
        // NOTE: If we don't split this logic, then any writes to stdout while calling
        // this function concurrently may cause the piped child output to be ignored

        let output = child
            .wait_with_output()
            .await
            .wrap_err_with(|| format!("No exit status for '{}'", command))?;

        exit_status = output.status;
        stdout_final = from_utf8(&output.stdout).unwrap_or_default().to_string();
        stderr_final = from_utf8(&output.stderr).unwrap_or_default().to_string();
    }

    Ok(CommandResult {
        exit_status: exit_status.code(),
        stdout: format!(
            "{}{}",
            truncate_safe(&stdout_final, max_result_size),
            if stdout_final.len() > max_result_size {
                " ... truncated"
            } else {
                ""
            }
        ),
        stderr: format!(
            "{}{}",
            truncate_safe(&stderr_final, max_result_size),
            if stderr_final.len() > max_result_size {
                " ... truncated"
            } else {
                ""
            }
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore = "todo: fix failing on musl for some reason"]
    #[tokio::test]
    async fn test_execute_bash_tool() {
        let mut stdout = std::io::stdout();

        // Verifying stdout
        let v = serde_json::json!({
            "command": "echo Hello, world!",
        });
        let out = serde_json::from_value::<ExecuteBash>(v)
            .unwrap()
            .invoke(&mut stdout)
            .await
            .unwrap();

        if let OutputKind::Json(json) = out.output {
            assert_eq!(json.get("exit_status").unwrap(), &0.to_string());
            assert_eq!(json.get("stdout").unwrap(), "Hello, world!");
            assert_eq!(json.get("stderr").unwrap(), "");
        } else {
            panic!("Expected JSON output");
        }

        // Verifying stderr
        let v = serde_json::json!({
            "command": "echo Hello, world! 1>&2",
        });
        let out = serde_json::from_value::<ExecuteBash>(v)
            .unwrap()
            .invoke(&mut stdout)
            .await
            .unwrap();

        if let OutputKind::Json(json) = out.output {
            assert_eq!(json.get("exit_status").unwrap(), &0.to_string());
            assert_eq!(json.get("stdout").unwrap(), "");
            assert_eq!(json.get("stderr").unwrap(), "Hello, world!");
        } else {
            panic!("Expected JSON output");
        }

        // Verifying exit code
        let v = serde_json::json!({
            "command": "exit 1",
            "interactive": false
        });
        let out = serde_json::from_value::<ExecuteBash>(v)
            .unwrap()
            .invoke(&mut stdout)
            .await
            .unwrap();
        if let OutputKind::Json(json) = out.output {
            assert_eq!(json.get("exit_status").unwrap(), &1.to_string());
            assert_eq!(json.get("stdout").unwrap(), "");
            assert_eq!(json.get("stderr").unwrap(), "");
        } else {
            panic!("Expected JSON output");
        }
    }

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
            ("find important-dir/ -name '*.txt'", false),
        ];
        for (cmd, expected) in cmds {
            let tool = serde_json::from_value::<ExecuteBash>(serde_json::json!({
                "command": cmd,
            }))
            .unwrap();
            assert_eq!(
                tool.requires_acceptance(),
                *expected,
                "expected command: `{}` to have requires_acceptance: `{}`",
                cmd,
                expected
            );
        }
    }
}
