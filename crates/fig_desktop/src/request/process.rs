use std::time::Duration;

use fig_proto::fig::server_originated_message::Submessage as ServerOriginatedSubMessage;
use fig_proto::fig::{
    RunProcessRequest,
    RunProcessResponse,
};
use fig_proto::remote::hostbound;
use fig_remote_ipc::figterm::{
    FigtermCommand,
    FigtermState,
};
use fig_util::env_var::{
    PROCESS_LAUNCHED_BY_Q,
    Q_TERM,
};
use tokio::process::Command;
use tokio::time::timeout;
use tracing::debug;
use uuid::Uuid;

use super::RequestResult;

fn set_fig_vars(cmd: &mut Command) {
    cmd.env(Q_TERM, env!("CARGO_PKG_VERSION"));
    cmd.env(PROCESS_LAUNCHED_BY_Q, "1");

    cmd.env("HISTFILE", "");
    cmd.env("HISTCONTROL", "ignoreboth");
    cmd.env("TERM", "xterm-256color");
}

pub async fn run(request: RunProcessRequest, state: &FigtermState) -> RequestResult {
    debug!({
        term_session =? request.terminal_session_id,
        exe = request.executable,
        args =? request.arguments,
        cwd = request.working_directory(),
        env =? request.env,
        timeout =? request.timeout,
    }, "Running command");

    let uuid = request
        .terminal_session_id
        .as_deref()
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|err| format!("terminal_session_id is not a valid UUID: {err}"))?;

    let session_sender = state.with_maybe_id(&uuid, |session| session.sender.clone());

    if let Some(session_sender) = session_sender {
        let (message, rx) = FigtermCommand::run_process(
            request.executable,
            request.arguments,
            request.working_directory,
            request.env,
            request.timeout.map(Into::into),
        );
        session_sender
            .send(message)
            .map_err(|err| format!("failed sending command to figterm: {err}"))?;
        drop(session_sender);

        let timeout_duration = request
            .timeout
            .map(Duration::from)
            .unwrap_or_default()
            .max(Duration::from_secs(10));

        let response = timeout(timeout_duration, rx)
            .await
            .map_err(|_err| "Timed out waiting for figterm response")?
            .map_err(|err| format!("Failed to receive figterm response: {err}"))?;

        if let hostbound::response::Response::RunProcess(response) = response {
            RequestResult::Ok(Box::new(ServerOriginatedSubMessage::RunProcessResponse(
                RunProcessResponse { ..response },
            )))
        } else {
            Err("invalid response type".into())
        }
    } else {
        debug!("running locally");

        // TODO: we can infer shell as above for execute if no executable is provided.
        let mut cmd = Command::new(&request.executable);
        #[cfg(target_os = "windows")]
        cmd.creation_flags(windows::Win32::System::Threading::DETACHED_PROCESS.0);

        if let Some(working_directory) = request.working_directory {
            cmd.current_dir(working_directory);
        } else if let Ok(working_directory) = std::env::current_dir() {
            cmd.current_dir(working_directory);
        }
        for arg in request.arguments {
            cmd.arg(arg);
        }

        set_fig_vars(&mut cmd);

        for var in request.env {
            match var.value {
                Some(val) => cmd.env(var.key, val),
                None => cmd.env_remove(var.key),
            };
        }

        let output = cmd
            .output()
            .await
            .map_err(|err| format!("Failed running command {:?}: {err}", request.executable))?;

        RequestResult::Ok(Box::new(ServerOriginatedSubMessage::RunProcessResponse(
            RunProcessResponse {
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                exit_code: output.status.code().unwrap_or(0),
            },
        )))
    }
}
