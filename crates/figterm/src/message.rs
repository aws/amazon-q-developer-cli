use std::iter::repeat;
use std::path::{
    Path,
    PathBuf,
};
use std::time::{
    Duration,
    SystemTime,
};

use alacritty_terminal::Term;
use alacritty_terminal::term::ShellState;
use anyhow::Result;
use fig_proto::fig::{
    EnvironmentVariable,
    RunProcessResponse,
};
use fig_proto::figterm::figterm_request_message::Request as FigtermRequest;
use fig_proto::figterm::figterm_response_message::Response as FigtermResponse;
use fig_proto::figterm::intercept_request::{
    InterceptCommand,
    SetFigjsIntercepts,
    SetFigjsVisible,
};
use fig_proto::figterm::{
    self,
    FigtermRequestMessage,
    FigtermResponseMessage,
    TelemetryRequest,
};
use fig_proto::remote::{
    Clientbound,
    Hostbound,
    clientbound,
    hostbound,
};
use fig_util::env_var::PROCESS_LAUNCHED_BY_Q;
use flume::Sender;
use tokio::process::Command;
use tracing::{
    debug,
    error,
    trace,
    warn,
};

use crate::event_handler::EventHandler;
use crate::history::HistorySender;
use crate::interceptor::KeyInterceptor;
use crate::pty::AsyncMasterPty;
use crate::{
    EXPECTED_BUFFER,
    INSERT_ON_NEW_CMD,
    INSERTION_LOCKED_AT,
    MainLoopEvent,
    SHELL_ALIAS,
    SHELL_ENVIRONMENT_VARIABLES,
    inline,
    shell_state_to_context,
};

fn working_directory(path: Option<&str>, shell_state: &ShellState) -> PathBuf {
    let map_dir = |path: PathBuf| match path.canonicalize() {
        Ok(path) if path.is_dir() => Some(path),
        Ok(path) => {
            warn!(?path, "not a directory");
            None
        },
        Err(err) => {
            warn!(?path, %err, "failed to canonicalize path");
            None
        },
    };

    path.map(PathBuf::from)
        .and_then(map_dir)
        .or_else(|| {
            shell_state
                .get_context()
                .current_working_directory
                .clone()
                .and_then(map_dir)
        })
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| {
            cfg_if::cfg_if! {
                if #[cfg(windows)] {
                    PathBuf::from("C:\\")
                } else if #[cfg(unix)] {
                    PathBuf::from("/")
                }
            }
        })
}

fn create_command(executable: impl AsRef<Path>, working_directory: impl AsRef<Path>) -> Command {
    let env = (*SHELL_ENVIRONMENT_VARIABLES.lock().unwrap())
        .clone()
        .into_iter()
        .filter_map(|EnvironmentVariable { key, value }| value.map(|value| (key, value)))
        .collect::<Vec<_>>();

    let mut cmd = if executable.as_ref().is_absolute() {
        Command::new(executable.as_ref())
    } else {
        let path = env
            .iter()
            .find_map(|(key, value)| if key == "PATH" { Some(value.as_str()) } else { None });

        which::which_in(executable.as_ref(), path, working_directory.as_ref())
            .map_or_else(|_| Command::new(executable.as_ref()), Command::new)
    };

    #[cfg(target_os = "windows")]
    cmd.creation_flags(windows::Win32::System::Threading::DETACHED_PROCESS.0);

    cmd.current_dir(working_directory);

    #[cfg(unix)]
    {
        let pre_exec_fn = || {
            // Remove controlling terminal.
            nix::unistd::setsid()?;
            Ok(())
        };

        // SAFETY: this closure is run after forking the process and only affects the
        // child. setsid is async-signal-safe.
        unsafe { cmd.pre_exec(pre_exec_fn) };
    }

    if !env.is_empty() {
        cmd.env_clear();
        cmd.envs(env);
    }

    cmd.env_remove("LS_COLORS");
    cmd.env_remove("CLICOLOR_FORCE");
    cmd.env_remove("CLICOLOR");
    cmd.env_remove("COLORTERM");
    cmd.envs([
        (PROCESS_LAUNCHED_BY_Q, "1"),
        ("HISTFILE", ""),
        ("HISTCONTROL", "ignoreboth"),
        ("TERM", "xterm-256color"),
        ("NO_COLOR", "1"),
    ]);

    cmd.kill_on_drop(true);

    cmd
}

/// Process the inner figterm request enum, shared between local and remote
pub async fn process_figterm_request(
    figterm_request: FigtermRequest,
    main_loop_tx: Sender<MainLoopEvent>,
    term: &Term<EventHandler>,
    pty_master: &mut Box<dyn AsyncMasterPty + Send + Sync>,
    key_interceptor: &mut KeyInterceptor,
) -> Result<Option<FigtermResponse>> {
    match figterm_request {
        FigtermRequest::InsertText(request) => {
            // If the shell is in prompt or a command is being executed, insert the text only
            // if the insert during command option is enabled.
            if term.shell_state().preexec && !request.insert_during_command() {
                return Ok(None);
            }

            let current_buffer = term.get_current_buffer().map(|buff| (buff.buffer, buff.cursor_idx));
            let mut insertion_string = String::new();
            if let Some((buffer, Some(position))) = current_buffer {
                if let Some(ref text_to_insert) = request.insertion {
                    trace!(?buffer, ?position);

                    // perform deletion
                    // if let Some(deletion) = command.deletion {
                    //     let deletion = deletion as usize;
                    //     buffer.drain(position - deletion..position);
                    // }
                    // // move cursor
                    // if let Some(offset) = command.offset {
                    //     position += offset as usize;
                    // }
                    // // split text by cursor
                    // let (left, right) = buffer.split_at(position);

                    INSERTION_LOCKED_AT.write().unwrap().replace(SystemTime::now());
                    let expected = format!("{buffer}{text_to_insert}");
                    trace!(?expected, "lock set, expected buffer");
                    *EXPECTED_BUFFER.lock().unwrap() = expected;
                }
                if let Some(ref insertion_buffer) = request.insertion_buffer {
                    if buffer.ne(insertion_buffer) {
                        if buffer.starts_with(insertion_buffer) {
                            if let Some(len_diff) = buffer.len().checked_sub(insertion_buffer.len()) {
                                insertion_string.extend(repeat('\x08').take(len_diff));
                            }
                        } else if insertion_buffer.starts_with(&buffer) {
                            insertion_string.push_str(&insertion_buffer[buffer.len()..]);
                        }
                    }
                }
            }
            insertion_string.push_str(&request.to_term_string());
            pty_master.write(insertion_string.as_bytes()).await?;
            Ok(None)
        },
        FigtermRequest::Intercept(request) => {
            match request.intercept_command {
                Some(InterceptCommand::SetFigjsIntercepts(SetFigjsIntercepts {
                    intercept_bound_keystrokes,
                    intercept_global_keystrokes,
                    actions,
                    override_actions,
                })) => {
                    key_interceptor.set_intercept_global(intercept_global_keystrokes);
                    key_interceptor.set_intercept(intercept_bound_keystrokes);
                    key_interceptor.set_actions(&actions, override_actions);
                },
                Some(InterceptCommand::SetFigjsVisible(SetFigjsVisible { visible })) => {
                    key_interceptor.set_window_visible(visible);
                },
                None => {},
            }

            Ok(None)
        },
        FigtermRequest::Diagnostics(_) => {
            let map_color = |color: &shell_color::VTermColor| -> figterm::TermColor {
                figterm::TermColor {
                    color: Some(match color {
                        shell_color::VTermColor::Rgb { red, green, blue } => {
                            figterm::term_color::Color::Rgb(figterm::term_color::Rgb {
                                r: *red as i32,
                                b: *blue as i32,
                                g: *green as i32,
                            })
                        },
                        shell_color::VTermColor::Indexed { idx } => figterm::term_color::Color::Indexed(*idx as u32),
                    }),
                }
            };

            let map_style = |style: &shell_color::SuggestionColor| -> figterm::TermStyle {
                figterm::TermStyle {
                    fg: style.fg().as_ref().map(map_color),
                    bg: style.bg().as_ref().map(map_color),
                }
            };

            let (edit_buffer, cursor_position) = term.get_current_buffer().map_or((None, None), |buf| {
                (Some(buf.buffer), buf.cursor_idx.and_then(|i| i.try_into().ok()))
            });

            let response = FigtermResponse::Diagnostics(figterm::DiagnosticsResponse {
                shell_context: Some(shell_state_to_context(term.shell_state())),
                fish_suggestion_style: term.shell_state().fish_suggestion_color.as_ref().map(map_style),
                zsh_autosuggestion_style: term.shell_state().zsh_autosuggestion_color.as_ref().map(map_style),
                edit_buffer,
                cursor_position,
            });

            Ok(Some(response))
        },
        FigtermRequest::InsertOnNewCmd(command) => {
            *INSERT_ON_NEW_CMD.lock().unwrap() = Some((command.text, command.bracketed, command.execute));
            Ok(None)
        },
        FigtermRequest::SetBuffer(_) => Err(anyhow::anyhow!("SetBuffer is not supported in figterm")),
        FigtermRequest::UpdateShellContext(request) => {
            if request.update_environment_variables {
                *SHELL_ENVIRONMENT_VARIABLES.lock().unwrap() = request.environment_variables;
            }
            if request.update_alias {
                *SHELL_ALIAS.lock().unwrap() = request.alias;
            }
            Ok(None)
        },
        FigtermRequest::NotifySshSessionStarted(notification) => {
            main_loop_tx
                .send(MainLoopEvent::PromptSSH {
                    uuid: notification.uuid,
                    remote_host: notification.remote_host,
                })
                .ok();
            Ok(None)
        },
        FigtermRequest::InlineShellCompletion(_) => anyhow::bail!("InlineShellCompletion is not supported over remote"),
        FigtermRequest::InlineShellCompletionAccept(_) => {
            anyhow::bail!("InlineShellCompletionAccept is not supported over remote")
        },
        FigtermRequest::InlineShellCompletionSetEnabled(_) => {
            anyhow::bail!("InlineShellCompletionSetEnabled is not supported over remote")
        },
        FigtermRequest::Telemtety(_) => anyhow::bail!("Telemetry is not supported over remote"),
    }
}

/// Process a figterm request message
#[allow(clippy::too_many_arguments)]
pub async fn process_figterm_message(
    figterm_request_message: FigtermRequestMessage,
    main_loop_tx: Sender<MainLoopEvent>,
    response_tx: Sender<FigtermResponseMessage>,
    term: &Term<EventHandler>,
    history_sender: &HistorySender,
    pty_master: &mut Box<dyn AsyncMasterPty + Send + Sync>,
    key_interceptor: &mut KeyInterceptor,
    session_id: &str,
) -> Result<()> {
    match figterm_request_message.request {
        Some(FigtermRequest::InlineShellCompletion(request)) => {
            let history_sender = history_sender.clone();
            let session_id = session_id.to_owned();

            tokio::spawn(inline::handle_request(request, session_id, response_tx, history_sender));
        },
        Some(FigtermRequest::InlineShellCompletionAccept(request)) => {
            tokio::spawn(inline::handle_accept(request, session_id.to_owned()));
        },
        Some(FigtermRequest::InlineShellCompletionSetEnabled(request)) => {
            tokio::spawn(inline::handle_set_enabled(request, session_id.to_owned()));
        },
        Some(FigtermRequest::Telemtety(TelemetryRequest { event_blob })) => {
            match fig_telemetry::AppTelemetryEvent::from_json(&event_blob) {
                Ok(event) => {
                    tokio::spawn(fig_telemetry::send_event(event));
                },
                Err(err) => error!(%err, "Failed to parse telemetry event"),
            }
        },
        Some(request) => {
            match process_figterm_request(request, main_loop_tx, term, pty_master, key_interceptor).await {
                Ok(Some(response)) => {
                    let response_message = FigtermResponseMessage {
                        response: Some(response),
                    };
                    if let Err(err) = response_tx.send_async(response_message).await {
                        error!(%err, "Failed sending request response");
                    }
                },
                Ok(None) => {},
                Err(err) => error!(%err, "Failed to process figterm message"),
            }
        },
        None => warn!("Qterm message with no request"),
    }
    Ok(())
}

async fn send_figterm_response_hostbound(
    response: Option<FigtermResponse>,
    nonce: Option<u64>,
    response_tx: &Sender<Hostbound>,
) {
    use hostbound::response::Response;

    if let Some(response) = response {
        let hostbound = Hostbound {
            packet: Some(hostbound::Packet::Response(hostbound::Response {
                nonce,
                response: Some(match response {
                    FigtermResponse::Diagnostics(diagnostics) => Response::Diagnostics(diagnostics),
                    FigtermResponse::InlineShellCompletion(_) => unreachable!(),
                }),
            })),
        };

        if let Err(err) = response_tx.send_async(hostbound).await {
            error!(%err, "Failed sending request response");
        }
    }
}

pub async fn process_remote_message(
    clientbound_message: Clientbound,
    main_loop_tx: Sender<MainLoopEvent>,
    response_tx: Sender<Hostbound>,
    term: &Term<EventHandler>,
    pty_master: &mut Box<dyn AsyncMasterPty + Send + Sync>,
    key_interceptor: &mut KeyInterceptor,
) -> Result<()> {
    use clientbound::request::Request;
    use hostbound::response::Response;

    match clientbound_message.packet {
        Some(clientbound::Packet::Request(request)) => {
            let nonce = request.nonce;
            let make_response = move |response: Response| -> Hostbound {
                Hostbound {
                    packet: Some(hostbound::Packet::Response(hostbound::Response {
                        response: Some(response),
                        nonce,
                    })),
                }
            };

            match request.request {
                Some(Request::InsertText(request)) => {
                    send_figterm_response_hostbound(
                        process_figterm_request(
                            FigtermRequest::InsertText(request),
                            main_loop_tx,
                            term,
                            pty_master,
                            key_interceptor,
                        )
                        .await?,
                        nonce,
                        &response_tx,
                    )
                    .await;
                },
                Some(Request::Intercept(request)) => {
                    send_figterm_response_hostbound(
                        process_figterm_request(
                            FigtermRequest::Intercept(request),
                            main_loop_tx,
                            term,
                            pty_master,
                            key_interceptor,
                        )
                        .await?,
                        nonce,
                        &response_tx,
                    )
                    .await;
                },
                Some(Request::Diagnostics(request)) => {
                    send_figterm_response_hostbound(
                        process_figterm_request(
                            FigtermRequest::Diagnostics(request),
                            main_loop_tx,
                            term,
                            pty_master,
                            key_interceptor,
                        )
                        .await?,
                        nonce,
                        &response_tx,
                    )
                    .await;
                },
                Some(Request::InsertOnNewCmd(request)) => {
                    send_figterm_response_hostbound(
                        process_figterm_request(
                            FigtermRequest::InsertOnNewCmd(request),
                            main_loop_tx,
                            term,
                            pty_master,
                            key_interceptor,
                        )
                        .await?,
                        nonce,
                        &response_tx,
                    )
                    .await;
                },
                Some(Request::RunProcess(request)) => {
                    // TODO: we can infer shell as above for execute if no executable is provided.
                    let mut cmd = create_command(
                        &request.executable,
                        working_directory(request.working_directory.as_deref(), term.shell_state()),
                    );

                    cmd.args(request.arguments);
                    for var in request.env {
                        match var.value {
                            Some(value) => cmd.env(var.key, value),
                            None => cmd.env_remove(var.key),
                        };
                    }

                    tokio::spawn(async move {
                        debug!("running command");

                        let timeout_duration = request.timeout.map_or(Duration::from_secs(60), Into::into);
                        let command_timeout = tokio::time::timeout(timeout_duration, cmd.output());

                        let response = match command_timeout.await {
                            Ok(Ok(output)) => {
                                debug!("command successfully ran");
                                make_response(Response::RunProcess(RunProcessResponse {
                                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                                    exit_code: output.status.code().unwrap_or(0),
                                }))
                            },
                            Ok(Err(err)) => {
                                warn!(%err, executable = request.executable, "failed running executable");
                                make_response(Response::Error(format!(
                                    "failed running executable ({}): {err}",
                                    request.executable
                                )))
                            },
                            Err(err) => {
                                warn!(
                                    %err,
                                    executable = request.executable,
                                    timeout_ms =% timeout_duration.as_millis(),
                                    "timed out running executable"
                                );
                                make_response(Response::Error(format!(
                                    "timed out after {}ms running executable ({}): {err}",
                                    timeout_duration.as_millis(),
                                    request.executable,
                                )))
                            },
                        };

                        if let Err(err) = response_tx.send_async(response).await {
                            error!(%err, "Failed sending request response");
                        }
                    });
                },
                _ => warn!("unhandled request {request:?}"),
            }
        },
        Some(clientbound::Packet::Ping(())) => {
            let response = Hostbound {
                packet: Some(hostbound::Packet::Pong(())),
            };

            if let Err(err) = response_tx.send_async(response).await {
                error!(%err, "Failed sending request response");
            }
        },
        packet => warn!("unhandled packet {packet:?}"),
    };

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_command() {
        let output = create_command("cargo", "/").output().await.unwrap();
        assert!(output.status.success());

        let output = create_command("echo", "/").arg("hello world").output().await.unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"hello world\n");
        assert_eq!(output.stderr, b"");

        let output = create_command("bash", "/")
            .args(["-c", "echo hello world 1 1>&2; echo hello world 2; exit 25"])
            .output()
            .await
            .unwrap();
        assert_eq!(output.status.code(), Some(25));
        assert_eq!(output.stdout, b"hello world 2\n");
        assert_eq!(output.stderr, b"hello world 1\n");
    }
}
