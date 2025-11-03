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
    env_vars_with_user_agent,
    format_output,
};
use crate::os::Os;

/// Run a bash command on Unix systems.
/// # Arguments
/// * `command` - The command to run
/// * `max_result_size` - max size of output streams, truncating if required
/// * `updates` - output stream to push informational messages about the progress
/// # Returns
/// A [`CommandResult`]
pub async fn run_command<W: Write>(
    os: &Os,
    command: &str,
    max_result_size: usize,
    mut updates: Option<W>,
) -> Result<CommandResult> {
    let shell = std::env::var("AMAZON_Q_CHAT_SHELL").unwrap_or("bash".to_string());

    // Set up environment variables with user agent metadata for CloudTrail tracking
    let env_vars = env_vars_with_user_agent(os);

    // We need to maintain a handle on stderr and stdout, but pipe it to the terminal as well
    let mut child = tokio::process::Command::new(shell)
        .arg("-c")
        .arg(command)
        .envs(env_vars)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
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
        stdout: format_output(&stdout_final, max_result_size),
        stderr: format_output(&stderr_final, max_result_size),
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
}
