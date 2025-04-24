use std::collections::VecDeque;
use std::io::Write;
use std::str::from_utf8;
use std::sync::Arc;
use std::time::Duration;

use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};
use dialoguer::console::strip_ansi_codes;
use eyre::{
    Result,
    eyre,
};
use fig_os_shim::Context;
#[cfg(unix)]
use fig_util::pty::unix::open_pty;
#[cfg(windows)]
use fig_util::pty::win::open_pty;
use fig_util::pty::{
    AsyncMasterPtyExt,
    CommandBuilder,
};
use fig_util::shell::Shell;
use fig_util::terminal::{
    get_terminal_size,
    key_event_to_bytes,
};
use portable_pty::PtySize;
use regex::Regex;
use serde::Deserialize;

use super::super::util::truncate_safe;
use super::{
    InvokeOutput,
    MAX_TOOL_RESPONSE_SIZE,
    OutputKind,
};

const READONLY_COMMANDS: &[&str] = &["ls", "cat", "echo", "pwd", "which", "head", "tail", "find", "grep"];

#[derive(Debug, Clone, Deserialize)]
pub struct ExecuteBash {
    pub command: String,
}

impl ExecuteBash {
    pub fn requires_acceptance(&self) -> bool {
        let Some(args) = shlex::split(&self.command) else {
            return true;
        };

        const DANGEROUS_PATTERNS: &[&str] = &["<(", "$(", "`", ">", "&&", "||"];
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

    // Note: _updates is unused because `impl Write` cannot be shared across threads, so we write to
    // stdout directly. A type refactor is needed to support this.
    pub async fn invoke(&self, _updates: impl Write) -> Result<InvokeOutput> {
        let output = self.execute_pty_with_input(MAX_TOOL_RESPONSE_SIZE / 3, true).await?;
        let result = serde_json::json!({
            "exit_status": output.exit_status.to_string(),
            "stdout": output.stdout,
        });

        Ok(InvokeOutput {
            output: OutputKind::Json(result),
        })
    }

    /// Run a bash command using a PTY. The user's cwd, env vars, and shell configurations will be
    /// used. Records input from the user's terminal for text and key combinations.
    ///
    /// # Arguments
    /// * `max_result_size` - max size of output streams, truncating if required
    /// * `updates` - whether to push command output to stdout
    /// # Returns
    /// A [`CommandResult`]
    pub async fn execute_pty_with_input(&self, max_result_size: usize, updates: bool) -> Result<CommandResult> {
        crossterm::terminal::enable_raw_mode()?;
        let result = self._execute_pty(max_result_size, updates, true).await;
        crossterm::terminal::disable_raw_mode()?;

        // Clean out any remaining events.
        // Otherwise, the main terminal may behave strangely after returning.
        while crossterm::event::poll(Duration::from_millis(0))? {
            let _ = crossterm::event::read();
        }

        result
    }

    /// Run a bash command using a PTY. The user's cwd, env vars, and shell configurations will be
    /// used. Does not record any input.
    ///
    /// # Arguments
    /// * `max_result_size` - max size of output streams, truncating if required
    /// * `updates` - whether to push command output to stdout
    /// # Returns
    /// A [`CommandResult`]
    pub async fn execute_pty_without_input(&self, max_result_size: usize, updates: bool) -> Result<CommandResult> {
        self._execute_pty(max_result_size, updates, false).await
    }

    async fn _execute_pty(&self, max_result_size: usize, updates: bool, with_input: bool) -> Result<CommandResult> {
        const LINE_COUNT: usize = 1024;

        // Open a new pseudoterminal
        let pty_pair = open_pty(&get_terminal_size()).map_err(|e| eyre!("Failed to start PTY: {}", e))?;

        // Create a command builder for the shell command
        let shell = Shell::current_shell().map_or("bash", |s| s.as_str());
        let mut cmd_builder = CommandBuilder::new(shell);
        cmd_builder.args(["-cli", &self.command]);
        cmd_builder.cwd(std::env::current_dir()?);

        // Should work for most (all?) shells? Needs a bit more research.
        // This is all but required because otherwise the stdout from the PTY gets cluttered
        // with escape characters and shell integrations (e.g. current directory, current user, hostname).
        // We can clean the escape chars but the shell integrations are much harder. Is there a better way?
        // What happens if we don't use this: Q can get confused on what the output is actually saying.
        //
        // NOTE: This may disable certain interactive commands and display a warning for others
        cmd_builder.env("TERM", "dumb");

        let mut child = pty_pair
            .slave
            .spawn_command(cmd_builder)
            .map_err(|e| eyre!("Failed to get slave PTY: {}", e))?;
        let master = pty_pair
            .master
            .get_async_master_pty()
            .map_err(|e| eyre!("Failed to get master PTY: {}", e))?;
        let master = Arc::new(tokio::sync::Mutex::new(master));

        // Set up a channel to coordinate shutdown
        let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);

        // Handle output from the command
        let master_clone = Arc::clone(&master);
        let mut stdout_lines: VecDeque<String> = VecDeque::with_capacity(LINE_COUNT);

        let output_handle = tokio::spawn(async move {
            let mut buffer = [0u8; LINE_COUNT];
            let mut stdout = std::io::stdout();

            loop {
                let mut master_guard = master_clone.lock().await;

                // Timeout to give other asyncs time to run since read will block until it is able to read.
                // However, for most cases reading from PTY's stdout is more important than writing to its stdin.
                match tokio::time::timeout(Duration::from_millis(20), master_guard.read(&mut buffer)).await {
                    Ok(Ok(0)) => break Ok(stdout_lines), // End of stream
                    Ok(Ok(n)) => {
                        if updates {
                            stdout.write_all(&buffer[..n])?;
                            stdout.flush()?;
                        }

                        if let Ok(text) = from_utf8(&buffer) {
                            for subline in clean_pty_output(text).split_inclusive('\n') {
                                if stdout_lines.len() >= LINE_COUNT {
                                    stdout_lines.pop_front();
                                }
                                stdout_lines.push_back(strip_ansi_codes(subline).to_string().trim().to_string());
                            }
                        }
                    },
                    Ok(Err(e)) => {
                        break Err(eyre!("Failed reading from PTY: {}", e));
                    },
                    Err(_) => continue,
                }
            }
        });

        // Handle input from the user using crossterm
        let master_clone = Arc::clone(&master);
        let input_handle = if with_input {
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        // Check if the process is done
                        Some(_) = rx.recv() => break Ok(()),

                        // Use a separate task to poll for events to avoid blocking
                        // Note: this reads one character a time basically. Which is fine for
                        // everything unless the user pastes a large amount of text.
                        // Could use an upgrade to avoid this (maybe read from stdin and events at the same time)
                        event = tokio::task::spawn_blocking(crossterm::event::read) => {
                            match event {
                                Ok(Ok(crossterm::event::Event::Key(key))) => {
                                    // Convert the key event to bytes and send to the PTY
                                    let bytes = key_event_to_bytes(key);
                                    if !bytes.is_empty() {
                                        if let Err(e) = master_clone.lock().await.write_all(&bytes).await {
                                            break Err(eyre!("Failed writing to PTY: {}", e));
                                        }
                                    }
                                }
                                Ok(Ok(crossterm::event::Event::Resize(cols, rows))) => {
                                    // Handle terminal resize
                                    let size = PtySize {
                                        rows,
                                        cols,
                                        pixel_width: 0,
                                        pixel_height: 0,
                                    };
                                    let _ = master_clone.lock().await.resize(size);
                                }
                                Ok(Err(e)) => {
                                    break Err(eyre!("Failed reading terminal event: {}", e));
                                }
                                Err(e) => {
                                    break Err(eyre!("Read terminal events async task error: {}", e));
                                }
                                _ => {} // Ignore other events
                            }
                        }
                    }
                }
            })
        } else {
            // Create a completed JoinHandle that returns Ok(())

            tokio::spawn(async move { Ok(()) })
        };

        // Wait for the output handler to complete
        let stdout_lines = output_handle.await??;

        // Signal the input handler to stop
        tx.send(()).await?;

        // Wait for the input handler to complete
        let _ = input_handle.await?;

        // Wait for the child process to exit
        let exit_status = child.wait()?;

        let stdout = stdout_lines.into_iter().collect::<String>();
        Ok(CommandResult {
            exit_status: exit_status.exit_code(),
            stdout: format!(
                "{}{}",
                truncate_safe(&stdout, max_result_size),
                if stdout.len() > max_result_size {
                    " ... truncated"
                } else {
                    ""
                }
            ),
        })
    }

    pub fn queue_description(&self, updates: &mut impl Write) -> Result<()> {
        queue!(updates, style::Print("I will run the following shell command: "),)?;

        // TODO: Could use graphemes for a better heuristic
        if self.command.len() > 20 {
            queue!(updates, style::Print("\n"),)?;
        }

        Ok(queue!(
            updates,
            style::SetForegroundColor(Color::Green),
            style::Print(&self.command),
            style::Print("\n\n"),
            style::ResetColor
        )?)
    }

    pub async fn validate(&mut self, _ctx: &Context) -> Result<()> {
        // TODO: probably some small amount of PATH checking
        Ok(())
    }
}

fn clean_pty_output(input: &str) -> String {
    // Remove null characters
    let without_nulls = input.replace('\0', "");

    // Remove terminal control sequences
    let re = Regex::new(r"\x1B\][^\x07]*\x07").unwrap();
    let cleaned = re.replace_all(&without_nulls, "");

    cleaned.to_string()
}

pub struct CommandResult {
    pub exit_status: u32,
    /// Truncated stdout
    pub stdout: String,
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
