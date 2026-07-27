use std::collections::VecDeque;
use std::io::Write;
use std::process::Stdio;

use eyre::{
    Context as EyreContext,
    Result,
};
use tokio::io::AsyncBufReadExt;
use tokio::select;
use tracing::error;

use super::{
    CommandResult,
    MAX_COMMAND_OUTPUT_SIZE,
    env_vars_with_user_agent,
    format_output,
};
use crate::os::Os;

/// Environment variable that removes the current working directory from Windows'
/// executable search path. Without it, `cmd.exe`/`CreateProcess` resolve a bare
/// command name (e.g. `git`) from the working directory before PATH, so a binary
/// planted in the working directory runs instead of the intended program (CWE-427).
const NO_CWD_IN_EXE_PATH_ENV: &str = "NoDefaultCurrentDirectoryInExePath";

/// Harden the spawn environment so bare command names resolve only from PATH,
/// never from the working directory the command runs in.
fn harden_windows_search_path(env_vars: &mut std::collections::HashMap<String, String>) {
    env_vars.insert(NO_CWD_IN_EXE_PATH_ENV.to_string(), "1".to_string());
}

/// Run a command on Windows using the detected shell (PowerShell or cmd.exe).
/// # Arguments
/// * `command` - The command to run
/// * `working_dir` - Optional working directory for command execution
/// * `updates` - output stream to push informational messages about the progress
/// # Returns
/// A [`CommandResult`]
pub async fn run_command<W: Write>(
    os: &Os,
    command: &str,
    working_dir: Option<&str>,
    mut updates: Option<W>,
) -> Result<CommandResult> {
    // Set up environment variables with user agent metadata for CloudTrail tracking
    let mut env_vars = env_vars_with_user_agent(os);
    harden_windows_search_path(&mut env_vars);

    // We need to maintain a handle on stderr and stdout, but pipe it to the terminal as well
    let (shell, flag) = agent::util::shell::shell_command();
    let mut cmd = tokio::process::Command::new(shell);
    cmd.arg(flag)
        .arg(command)
        .envs(env_vars)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    let mut child = cmd
        .spawn()
        .wrap_err_with(|| format!("Unable to spawn command '{command}'"))?;

    let stdout_final: String;
    let stderr_final: String;
    let exit_status;

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
        .wrap_err_with(|| format!("No exit status for '{command}'"))?;

        u.flush()?;

        stdout_final = stdout_buf.into_iter().collect::<Vec<_>>().join("\n");
        stderr_final = stderr_buf.into_iter().collect::<Vec<_>>().join("\n");
    } else {
        // Take output all at once since we are not reporting anything in real time
        let output = child
            .wait_with_output()
            .await
            .wrap_err_with(|| format!("No exit status for '{command}'"))?;

        exit_status = output.status;
        stdout_final = String::from_utf8_lossy(&output.stdout).to_string();
        stderr_final = String::from_utf8_lossy(&output.stderr).to_string();
    }

    Ok(CommandResult {
        exit_status: exit_status.code(),
        stdout: format_output(&stdout_final, MAX_COMMAND_OUTPUT_SIZE),
        stderr: format_output(&stderr_final, MAX_COMMAND_OUTPUT_SIZE),
    })
}

#[cfg(test)]
mod tests {
    use crate::cli::chat::tools::OutputKind;
    use crate::cli::chat::tools::execute::ExecuteCommand;
    use crate::os::Os;

    #[test]
    fn harden_windows_search_path_removes_cwd_from_exe_search() {
        let mut env = std::collections::HashMap::new();
        super::harden_windows_search_path(&mut env);
        assert_eq!(
            env.get(super::NO_CWD_IN_EXE_PATH_ENV).map(String::as_str),
            Some("1"),
            "spawned commands must resolve executables from PATH only, not the working directory"
        );
    }

    #[tokio::test]
    async fn test_execute_cmd_tool() {
        let os = Os::new().await.unwrap();
        let mut stdout = std::io::stdout();

        // Verifying stdout
        let v = serde_json::json!({
            "command": "echo Hello, world!",
        });
        let out = serde_json::from_value::<ExecuteCommand>(v)
            .unwrap()
            .invoke(&os, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Json(json) = out.output {
            assert_eq!(json.get("exit_status").unwrap(), &0.to_string());
            assert!(json.get("stdout").unwrap().to_string().contains("Hello, world!"));
            assert_eq!(json.get("stderr").unwrap(), "");
        } else {
            panic!("Expected JSON output");
        }

        // Verifying stderr (using 2>&1 redirection for Windows)
        let v = serde_json::json!({
            "command": "echo Hello, world! 1>&2",
        });
        let out = serde_json::from_value::<ExecuteCommand>(v)
            .unwrap()
            .invoke(&os, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Json(json) = out.output {
            assert_eq!(json.get("exit_status").unwrap(), &0.to_string());
            assert_eq!(json.get("stdout").unwrap(), "");
            assert!(json.get("stderr").unwrap().to_string().contains("Hello, world!"));
        } else {
            panic!("Expected JSON output");
        }

        // Verifying exit code
        let v = serde_json::json!({
            "command": "exit /b 1",
        });
        let out = serde_json::from_value::<ExecuteCommand>(v)
            .unwrap()
            .invoke(&os, &mut stdout)
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
}
