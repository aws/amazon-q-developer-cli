use std::collections::VecDeque;
use std::io::{
    self,
    Write,
};
use std::os::fd::{
    AsFd,
    AsRawFd,
    FromRawFd,
    RawFd,
};
use std::path::Path;

use console::strip_ansi_codes;
use fig_os_shim::Context;
use filedescriptor::FileDescriptor;
use nix::fcntl::{
    FcntlArg,
    FdFlag,
    OFlag,
    fcntl,
    open,
};
use nix::libc;
use nix::pty::{
    Winsize,
    grantpt,
    posix_openpt,
    ptsname,
    unlockpt,
};
use nix::sys::signal::{
    SigHandler,
    Signal,
    signal,
};
use nix::sys::stat::Mode;
use portable_pty::unix::close_random_fds;
use tokio::io::unix::AsyncFd;
use tokio::select;
use tokio::sync::mpsc::channel;
nix::ioctl_write_ptr_bad!(ioctl_tiocswinsz, libc::TIOCSWINSZ, Winsize);

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
use tracing::error;

use super::{
    InvokeOutput,
    MAX_TOOL_RESPONSE_SIZE,
    OutputKind,
};
use crate::cli::chat::truncate_safe;

const READONLY_COMMANDS: &[&str] = &["ls", "cat", "echo", "pwd", "which", "head", "tail", "find", "grep"];

#[derive(Debug, Clone, Deserialize)]
pub struct ExecuteShellCommands {
    pub command: String,
}

/// Helper function to set the close-on-exec flag for a raw descriptor
fn cloexec(fd: RawFd) -> Result<()> {
    let flags = fcntl(fd, FcntlArg::F_GETFD)?;
    fcntl(
        fd,
        FcntlArg::F_SETFD(FdFlag::from_bits_truncate(flags) | FdFlag::FD_CLOEXEC),
    )?;
    Ok(())
}

impl ExecuteShellCommands {
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

    pub async fn invoke(&self, mut updates: impl Write) -> Result<InvokeOutput> {
        // The pseudoterminal must be initialized with O_NONBLOCK since on macOS, the
        // it can not be safely set with fcntl() later on.
        // https://github.com/pkgw/stund/blob/master/tokio-pty-process/src/lib.rs#L127-L133
        cfg_if::cfg_if! {
            if #[cfg(any(target_os = "macos", target_os = "linux"))] {
                let oflag = OFlag::O_RDWR | OFlag::O_NONBLOCK;
            } else if #[cfg(target_os = "freebsd")] {
                let oflag = OFlag::O_RDWR;
            }
        }
        let master_pty = std::sync::Arc::new(posix_openpt(oflag).context("Failed to openpt")?);

        // Allow pseudoterminal pair to be generated
        grantpt(&master_pty).context("Failed to grantpt")?;
        unlockpt(&master_pty).context("Failed to unlockpt")?;

        // Get the name of the pseudoterminal
        // SAFETY: This is done before any threads are spawned, thus it being
        // non thread safe is not an issue
        let pty_name = { unsafe { ptsname(&master_pty) }? };

        // This will be the reader
        let slave_pty = open(Path::new(&pty_name), OFlag::O_RDWR, Mode::empty())?;

        let winsize = Winsize {
            ws_row: 30,
            ws_col: 100,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        unsafe { ioctl_tiocswinsz(slave_pty, &winsize) }?;

        cloexec(master_pty.as_fd().as_raw_fd())?;
        cloexec(slave_pty.as_raw_fd())?;

        let shell: String = std::env::var("SHELL").unwrap_or_else(|_| "bash".to_string());

        let slave_fd = unsafe { FileDescriptor::from_raw_fd(slave_pty.as_raw_fd()) };

        let mut base_command = tokio::process::Command::new(&shell);
        let command = base_command
            .arg("-c")
            .arg("-l")
            .arg("-i")
            .arg(&self.command)
            .stdin(slave_fd.as_stdio()?)
            .stdout(slave_fd.as_stdio()?)
            .stderr(slave_fd.as_stdio()?);

        let pre_exec_fn = move || {
            // Clean up a few things before we exec the program
            // Clear out any potentially problematic signal
            // dispositions that we might have inherited
            for signo in [
                Signal::SIGCHLD,
                Signal::SIGHUP,
                Signal::SIGINT,
                Signal::SIGQUIT,
                Signal::SIGTERM,
                Signal::SIGALRM,
            ] {
                unsafe { signal(signo, SigHandler::SigDfl) }?;
            }

            // Establish ourselves as a session leader.
            nix::unistd::setsid()?;

            // Clippy wants us to explicitly cast TIOCSCTTY using
            // type::from(), but the size and potentially signedness
            // are system dependent, which is why we're using `as _`.
            // Suppress this lint for this section of code.
            {
                // Set the pty as the controlling terminal.
                // Failure to do this means that delivery of
                // SIGWINCH won't happen when we resize the
                // terminal, among other undesirable effects.
                if unsafe { libc::ioctl(0, libc::TIOCSCTTY as _, 0) == -1 } {
                    return Err(io::Error::last_os_error());
                }
            }

            close_random_fds();

            Ok(())
        };

        unsafe { command.pre_exec(pre_exec_fn) };

        let mut child = command.spawn()?;

        let async_master = AsyncFd::new(master_pty.as_fd().as_raw_fd())?;

        const LINE_COUNT: usize = 1024;

        let (tx, mut rx) = channel(LINE_COUNT);
        let mut buffer = [0u8; LINE_COUNT];

        tokio::spawn(async move {
            loop {
                match async_master.readable().await {
                    Ok(mut guard) => {
                        let n = match guard.try_io(|inner| {
                            nix::unistd::read(inner.get_ref().as_raw_fd(), &mut buffer)
                                .map_err(|e| std::io::Error::from_raw_os_error(e as i32))
                        }) {
                            Ok(Ok(n)) => n,
                            Ok(Err(e)) => {
                                print!("{} ", e);
                                error!(%e, "Read error");
                                break;
                            },
                            Err(_) => continue,
                        };

                        if n == 0 {
                            break;
                        }

                        let raw_output = &buffer[..n];
                        if tx.send(raw_output.to_vec()).await.is_err() {
                            error!("channel closed");
                            break;
                        }
                    },
                    Err(e) => {
                        error!(%e, "readable failed");
                        break;
                    },
                }
            }
        });

        let mut stdout_lines: VecDeque<String> = VecDeque::with_capacity(LINE_COUNT);

        let exit_status = loop {
            select! {
                biased;
                Some(line) = rx.recv() => {
                    updates.write_all(&line)?;
                    updates.flush()?;

                    if let Ok(text) = std::str::from_utf8(&line) {
                        for subline in text.split_inclusive('\n') {
                            if stdout_lines.len() >= LINE_COUNT {
                                stdout_lines.pop_front();
                            }
                            stdout_lines.push_back(strip_ansi_codes(subline).to_string().trim().to_string());
                        }
                    }
                }
                status = child.wait() => {
                    break status;
                }
            };
        }
        .wrap_err_with(|| format!("No exit status for '{}'", &self.command))?;

        let stdout = stdout_lines.into_iter().collect::<String>();

        let output = serde_json::json!({
            "exit_status": exit_status.code().unwrap_or(0).to_string(),
            "stdout": format!(
                "{}{}",
                truncate_safe(&stdout, MAX_TOOL_RESPONSE_SIZE / 3),
                if stdout.len() > MAX_TOOL_RESPONSE_SIZE / 3 {
                    " ... truncated"
                } else {
                    ""
                }
            ),
        });

        child.kill().await?;

        Ok(InvokeOutput {
            output: OutputKind::Json(output),
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
            style::ResetColor
        )?)
    }

    pub async fn validate(&mut self, _ctx: &Context) -> Result<()> {
        // TODO: probably some small amount of PATH checking
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore = "todo: fix failing on musl for some reason"]
    #[tokio::test]
    async fn test_execute_shell_commands_tool() {
        let mut stdout = std::io::stdout();

        // Verifying stdout
        let v = serde_json::json!({
            "command": "echo Hello, world!",
        });
        let out = serde_json::from_value::<ExecuteShellCommands>(v)
            .unwrap()
            .invoke(&mut stdout)
            .await
            .unwrap();

        if let OutputKind::Json(json) = out.output {
            assert_eq!(json.get("exit_status").unwrap(), &0.to_string());
            assert_eq!(json.get("stdout").unwrap(), "Hello, world!");
        } else {
            panic!("Expected JSON output");
        }

        // Verifying exit code
        let v = serde_json::json!({
            "command": "exit 1",
            "interactive": false
        });
        let out = serde_json::from_value::<ExecuteShellCommands>(v)
            .unwrap()
            .invoke(&mut stdout)
            .await
            .unwrap();
        if let OutputKind::Json(json) = out.output {
            assert_eq!(json.get("exit_status").unwrap(), &1.to_string());
            assert_eq!(json.get("stdout").unwrap(), "");
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
            let tool = serde_json::from_value::<ExecuteShellCommands>(serde_json::json!({
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
