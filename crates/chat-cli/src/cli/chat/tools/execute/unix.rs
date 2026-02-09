use std::io::Write;
use std::process::Stdio;

use eyre::{
    Context as EyreContext,
    Result,
};
use tokio::io::AsyncReadExt;
use tokio::select;
use tracing::error;

use super::{
    CommandResult,
    MAX_COMMAND_OUTPUT_SIZE,
    env_vars_with_user_agent,
    format_output,
};
use crate::os::Os;
use crate::util::env_var::get_chat_shell;

/// Run a bash command on Unix systems.
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
    let shell = get_chat_shell();

    // Set up environment variables with user agent metadata for CloudTrail tracking
    let env_vars = env_vars_with_user_agent(os);

    // We need to maintain a handle on stderr and stdout, but pipe it to the terminal as well
    let mut cmd = tokio::process::Command::new(shell);
    cmd.arg("-c")
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
        let mut stdout = child.stdout.take().unwrap();
        let mut stderr = child.stderr.take().unwrap();

        // Use a moderate buffer size for reading chunks
        // This allows us to display partial output (like prompts without newlines)
        // while still being efficient for larger outputs
        const CHUNK_SIZE: usize = 256;
        let mut stdout_chunk = [0u8; CHUNK_SIZE];
        let mut stderr_chunk = [0u8; CHUNK_SIZE];

        // Accumulated output for the final result.
        // Must be the same size or larger than max_result_size
        const MAX_ACCUMULATED: usize = MAX_COMMAND_OUTPUT_SIZE; // rolling buffer
        let mut stdout_accumulated = Vec::with_capacity(MAX_ACCUMULATED);
        let mut stderr_accumulated = Vec::with_capacity(MAX_ACCUMULATED);

        let mut stdout_done = false;
        let mut stderr_done = false;

        exit_status = loop {
            select! {
                biased;
                n = stdout.read(&mut stdout_chunk), if !stdout_done => match n {
                    Ok(0) => stdout_done = true,
                    Ok(n) => {
                        let chunk = &stdout_chunk[..n];
                        // Write to the terminal immediately (handles partial lines/prompts)
                        u.write_all(chunk)?;
                        u.flush()?;
                        // Accumulate for the result, with rolling buffer behavior
                        if stdout_accumulated.len() + n > MAX_ACCUMULATED {
                            let excess = stdout_accumulated.len() + n - MAX_ACCUMULATED;
                            stdout_accumulated.drain(..excess);
                        }
                        stdout_accumulated.extend_from_slice(chunk);
                    },
                    Err(err) => {
                        error!(%err, "Failed to read stdout of child process");
                        stdout_done = true;
                    },
                },
                n = stderr.read(&mut stderr_chunk), if !stderr_done => match n {
                    Ok(0) => stderr_done = true,
                    Ok(n) => {
                        let chunk = &stderr_chunk[..n];
                        // Write to the terminal immediately (handles partial lines/prompts)
                        u.write_all(chunk)?;
                        u.flush()?;
                        // Accumulate for the result, with rolling buffer behavior
                        if stderr_accumulated.len() + n > MAX_ACCUMULATED {
                            let excess = stderr_accumulated.len() + n - MAX_ACCUMULATED;
                            stderr_accumulated.drain(..excess);
                        }
                        stderr_accumulated.extend_from_slice(chunk);
                    },
                    Err(err) => {
                        error!(%err, "Failed to read stderr of child process");
                        stderr_done = true;
                    },
                },
                exit_status = child.wait() => {
                    // Process exited, but we need to drain any remaining output
                    // that might still be in the pipe buffers
                    loop {
                        let mut drained = false;
                        if !stdout_done {
                            match stdout.read(&mut stdout_chunk).await {
                                Ok(0) => stdout_done = true,
                                Ok(n) => {
                                    let chunk = &stdout_chunk[..n];
                                    u.write_all(chunk)?;
                                    if stdout_accumulated.len() + n > MAX_ACCUMULATED {
                                        let excess = stdout_accumulated.len() + n - MAX_ACCUMULATED;
                                        stdout_accumulated.drain(..excess);
                                    }
                                    stdout_accumulated.extend_from_slice(chunk);
                                    drained = true;
                                },
                                Err(_) => stdout_done = true,
                            }
                        }
                        if !stderr_done {
                            match stderr.read(&mut stderr_chunk).await {
                                Ok(0) => stderr_done = true,
                                Ok(n) => {
                                    let chunk = &stderr_chunk[..n];
                                    u.write_all(chunk)?;
                                    if stderr_accumulated.len() + n > MAX_ACCUMULATED {
                                        let excess = stderr_accumulated.len() + n - MAX_ACCUMULATED;
                                        stderr_accumulated.drain(..excess);
                                    }
                                    stderr_accumulated.extend_from_slice(chunk);
                                    drained = true;
                                },
                                Err(_) => stderr_done = true,
                            }
                        }
                        if !drained || (stdout_done && stderr_done) {
                            break;
                        }
                    }
                    u.flush()?;
                    break exit_status;
                },
            };
        }
        .wrap_err_with(|| format!("No exit status for '{command}'"))?;

        stdout_final = String::from_utf8_lossy(&stdout_accumulated).to_string();
        stderr_final = String::from_utf8_lossy(&stderr_accumulated).to_string();
    } else {
        // Take output all at once since we are not reporting anything in real time
        //
        // NOTE: If we don't split this logic, then any writes to stdout while calling
        // this function concurrently may cause the piped child output to be ignored

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

    #[ignore = "todo: fix failing on musl for some reason"]
    #[tokio::test]
    async fn test_execute_bash_tool() {
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
            assert_eq!(json.get("stdout").unwrap(), "Hello, world!");
            assert_eq!(json.get("stderr").unwrap(), "");
        } else {
            panic!("Expected JSON output");
        }

        // Verifying stderr
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
            assert_eq!(json.get("stderr").unwrap(), "Hello, world!");
        } else {
            panic!("Expected JSON output");
        }

        // Verifying exit code
        let v = serde_json::json!({
            "command": "exit 1",
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

    #[tokio::test]
    async fn test_run_command_with_working_dir() {
        use super::run_command;

        let os = Os::new().await.unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let canonical_path = temp_dir.path().canonicalize().unwrap();
        let canonical_str = canonical_path.to_string_lossy().to_string();

        let result = run_command(&os, "pwd", Some(&canonical_str), None::<std::io::Stdout>)
            .await
            .unwrap();

        assert_eq!(result.exit_status, Some(0));
        assert!(result.stdout.trim().ends_with(&canonical_str));
    }
}
