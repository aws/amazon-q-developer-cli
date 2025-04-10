use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{
    AtomicU64,
    Ordering,
};

use anyhow::{
    Context,
    Result,
};
use fig_ipc::{
    BufferedReader,
    RecvMessage,
    SendMessage,
};
use fig_proto::figterm::{
    InsertTextRequest,
    InterceptRequest,
    SetBufferRequest,
    intercept_request,
};
use fig_proto::local::ShellContext;
use fig_proto::remote::clientbound::request::Request;
use fig_proto::remote::clientbound::{
    self,
    HandshakeResponse,
};
use fig_proto::remote::{
    Clientbound,
    Hostbound,
    RunProcessRequest,
    hostbound,
};
use fig_util::PTY_BINARY_NAME;
use time::OffsetDateTime;
use tokio::net::{
    UnixListener,
    UnixStream,
};
use tokio::select;
use tokio::sync::Notify;
use tokio::time::{
    Duration,
    Instant,
    MissedTickBehavior,
};
use tracing::{
    debug,
    error,
    info,
    trace,
    warn,
};
use uuid::Uuid;

use crate::RemoteHookHandler;
use crate::figterm::{
    EditBuffer,
    FigtermCommand,
    FigtermSession,
    FigtermState,
    InterceptMode,
};

pub async fn start_remote_ipc(
    socket_path: PathBuf,
    figterm_state: Arc<FigtermState>,
    hook: impl RemoteHookHandler + Send + Clone + 'static,
) -> Result<()> {
    if let Some(parent) = socket_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent).context("Failed creating socket path")?;
        }

        #[cfg(unix)]
        {
            use std::fs::Permissions;
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, Permissions::from_mode(0o700))?;
        }
    }

    tokio::fs::remove_file(&socket_path).await.ok();

    let listener = UnixListener::bind(socket_path)?;

    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(handle_remote_ipc(stream, figterm_state.clone(), hook.clone()));
    }

    Ok(())
}

pub async fn handle_remote_ipc(
    stream: UnixStream,
    figterm_state: Arc<FigtermState>,
    mut hook: impl RemoteHookHandler + Send,
) {
    let (reader, writer) = tokio::io::split(stream);
    let (clientbound_tx, clientbound_rx) = flume::unbounded();

    let bad_connection = Arc::new(Notify::new());

    let (on_close_tx, mut on_close_rx) = tokio::sync::broadcast::channel(1);

    let outgoing_task = tokio::spawn(handle_outgoing(
        writer,
        clientbound_rx,
        bad_connection.clone(),
        on_close_tx.subscribe(),
    ));

    let ping_task = tokio::spawn(send_pings(clientbound_tx.clone(), on_close_tx.subscribe()));

    let mut initialized = false;
    let session_id = Uuid::new_v4();

    let mut reader = BufferedReader::new(reader);
    loop {
        tokio::select! {
            _ = on_close_rx.recv() => {
                debug!("Connection closed");
                break;
            }
            message = reader.recv_message::<Hostbound>() => match message {
                Ok(Some(message)) => {
                    trace!(?message, "Received remote message");
                    if let Some(response) = match message.packet {
                        Some(hostbound::Packet::Handshake(handshake)) => {
                            let result = if initialized {
                                // maybe they missed our response, but they should've been listening harder
                                Some(clientbound::Packet::HandshakeResponse(HandshakeResponse {
                                    success: false,
                                }))
                            } else if let Some(success) = figterm_state.with_update(session_id, |session| {
                                if session.secret == handshake.secret {
                                    initialized = true;
                                    session.writer = Some(clientbound_tx.clone());
                                    session.dead_since = None;
                                    session.on_close_tx = on_close_tx.clone();
                                    debug!(
                                        "Client auth for {} accepted because of secret match ({} = {})",
                                        handshake.id, session.secret, handshake.secret
                                    );
                                    true
                                } else {
                                    debug!(
                                        "Client auth for {} rejected because of secret mismatch ({} =/= {})",
                                        handshake.id, session.secret, handshake.secret
                                    );
                                    false
                                }
                            }) {
                                Some(clientbound::Packet::HandshakeResponse(HandshakeResponse { success }))
                            } else {
                                initialized = true;
                                let (command_tx, command_rx) = flume::unbounded();
                                tokio::spawn(handle_commands(command_rx, figterm_state.clone(), session_id));
                                debug!(
                                    "Client auth for {} accepted because of new id with secret {}",
                                    handshake.id, handshake.secret
                                );
                                figterm_state.insert(FigtermSession {
                                    id: session_id,
                                    secret: handshake.secret.clone(),
                                    sender: command_tx,
                                    writer: Some(clientbound_tx.clone()),
                                    dead_since: None,
                                    last_receive: Instant::now(),
                                    edit_buffer: EditBuffer {
                                        text: "".to_string(),
                                        cursor: 0,
                                    },
                                    context: None,
                                    terminal_cursor_coordinates: None,
                                    current_session_metrics: None,
                                    response_map: HashMap::new(),
                                    nonce_counter: Arc::new(AtomicU64::new(0)),
                                    on_close_tx: on_close_tx.clone(),
                                    intercept: InterceptMode::Unlocked,
                                    intercept_global: InterceptMode::Unlocked
                                });
                                Some(clientbound::Packet::HandshakeResponse(HandshakeResponse {
                                    success: true,
                                }))
                            };

                            if matches!(result, Some(clientbound::Packet::HandshakeResponse(HandshakeResponse { success: true }))) {
                                if let Some(parent_id) = handshake.parent_id {
                                    let inner = figterm_state.inner.lock();
                                    let sessions = inner.linked_sessions.values();
                                    for session in sessions {
                                        if let Some(ref writer) = session.writer {
                                            let notification = clientbound::Packet::NotifyChildSessionStarted(
                                                clientbound::NotifyChildSessionStarted { parent_id: parent_id.clone() }
                                            );
                                            writer.send(
                                                Clientbound {
                                                    packet: Some(notification)
                                                }
                                            ).ok();
                                        }
                                    }
                                }
                            }

                            result
                        },
                        Some(hostbound::Packet::Request(hostbound::Request { request: Some(request), nonce })) => {
                            if matches!(
                                request,
                                hostbound::request::Request::EditBuffer(_)
                                | hostbound::request::Request::Prompt(_)
                                | hostbound::request::Request::PreExec(_)
                                | hostbound::request::Request::InterceptedKey(_)
                            ) && !initialized {
                                debug!("Client tried to send remote hook without auth");
                                Some(clientbound::Packet::HandshakeResponse(HandshakeResponse {
                                    success: false,
                                }))
                            } else {
                                /*
                                    WARNING, when adding new remote requests you must sanitize the context,
                                    otherwise the client can forge a message from another session
                                */
                                let res = match request {
                                    hostbound::request::Request::EditBuffer(mut edit_buffer) => {
                                        sanitize_fn(&mut edit_buffer.context, session_id);
                                        if let Some(shell_context) = &edit_buffer.context {
                                            hook.shell_context(shell_context, session_id).await;
                                        }
                                        hook.edit_buffer(
                                            &edit_buffer,
                                            session_id,
                                            &figterm_state,
                                        )
                                        .await
                                    },
                                    hostbound::request::Request::Prompt(mut prompt) => {
                                        sanitize_fn(&mut prompt.context, session_id);
                                        if let Some(shell_context) = &prompt.context {
                                            hook.shell_context(shell_context, session_id).await;
                                        }
                                        hook.prompt(&prompt, session_id, &figterm_state).await
                                    },
                                    hostbound::request::Request::PreExec(mut pre_exec) => {
                                        sanitize_fn(&mut pre_exec.context, session_id);
                                        if let Some(shell_context) = &pre_exec.context {
                                            hook.shell_context(shell_context, session_id).await;
                                        }
                                        hook.pre_exec(&pre_exec, session_id, &figterm_state).await
                                    },
                                    hostbound::request::Request::PostExec(mut post_exec) => {
                                        sanitize_fn(&mut post_exec.context, session_id);
                                        if let Some(shell_context) = &post_exec.context {
                                            hook.shell_context(shell_context, session_id).await;
                                        }
                                        hook.post_exec(&post_exec, session_id, &figterm_state).await
                                    },
                                    hostbound::request::Request::InterceptedKey(mut intercepted_key) => {
                                        sanitize_fn(&mut intercepted_key.context, session_id);
                                        if let Some(shell_context) = &intercepted_key.context {
                                            hook.shell_context(shell_context, session_id).await;
                                        }
                                        hook.intercepted_key(intercepted_key, session_id).await
                                    },
                                } ;

                                match res {
                                    Ok(inner) => inner.map(|inner| clientbound::Packet::Response(clientbound::Response { nonce, response: Some(inner) })),
                                    Err(err) => {
                                        error!(%err, "Failed processing hook");
                                        None
                                    }
                                }
                            }
                        },
                        Some(hostbound::Packet::Response(hostbound::Response {
                            nonce,
                            response: Some(response),
                        })) => {
                            if initialized {
                                if let Some(nonce) = nonce {
                                    figterm_state
                                        .with(&session_id, |session| session.response_map.remove(&nonce))
                                        .flatten()
                                        .map(|channel| channel.send(response));
                                }
                            }
                            None
                        },
                        Some(hostbound::Packet::Pong(())) => {
                            trace!(?session_id, "Received pong");
                            figterm_state.with(&session_id, |session| {
                                session.last_receive = Instant::now();
                            });
                            None
                        },
                        Some(hostbound::Packet::Request(hostbound::Request { request: None, .. })
                            | hostbound::Packet::Response(hostbound::Response { response: None, .. }))
                            | None => {
                            warn!(?message.packet, "Received unknown remote packet");
                            None
                        }
                    } {
                        let _ = clientbound_tx.send(Clientbound { packet: Some(response) });
                    }
                }
                Ok(None) => {
                    debug!("{PTY_BINARY_NAME} connection closed");
                    break;
                }
                Err(err) => {
                    if !err.is_disconnect() {
                        warn!(%err, "Failed receiving remote message");
                    }
                    break;
                }
            }
        }
    }

    let _ = on_close_tx.send(());
    drop(clientbound_tx);

    // figterm_state.with_update(session_id.clone(), |session| {
    //     session.writer = None;
    //     session.dead_since = Some(Instant::now());
    // });
    figterm_state.remove_id(&session_id);

    if let Err(err) = ping_task.await {
        error!(%err, "remote ping task join error");
    }

    if let Err(err) = outgoing_task.await {
        error!(%err, "remote outgoing task join error");
    }

    info!("Disconnect from {session_id:?}");
}

async fn handle_outgoing(
    mut writer: tokio::io::WriteHalf<UnixStream>,
    outgoing: flume::Receiver<Clientbound>,
    bad_connection: Arc<Notify>,
    mut on_close_rx: tokio::sync::broadcast::Receiver<()>,
) {
    loop {
        tokio::select! {
            _ = on_close_rx.recv() => {
                debug!("remote outgoing task exiting");
                break;
            },
            message = outgoing.recv_async() => {
                if let Ok(message) = message {
                    trace!(?message, "Sending remote message");
                    if let Err(err) = writer.send_message(message).await {
                        error!(%err, "remote outgoing task send error");
                        bad_connection.notify_one();
                        return;
                    }
                } else {
                    debug!("remote outgoing task exiting");
                    break;
                }
            }
        }
    }
}

async fn handle_commands(
    incoming: flume::Receiver<FigtermCommand>,
    figterm_state: Arc<FigtermState>,
    session_id: Uuid,
) -> Option<()> {
    while let Ok(command) = incoming.recv_async().await {
        let (request, nonce_channel) = match command {
            FigtermCommand::InterceptFigJs {
                intercept_keystrokes,
                intercept_global_keystrokes,
                actions,
                override_actions,
            } => (
                Request::Intercept(InterceptRequest {
                    intercept_command: Some(intercept_request::InterceptCommand::SetFigjsIntercepts(
                        intercept_request::SetFigjsIntercepts {
                            intercept_bound_keystrokes: intercept_keystrokes,
                            intercept_global_keystrokes,
                            actions,
                            override_actions,
                        },
                    )),
                }),
                None,
            ),
            FigtermCommand::InterceptFigJSVisible { visible } => (
                Request::Intercept(InterceptRequest {
                    intercept_command: Some(intercept_request::InterceptCommand::SetFigjsVisible(
                        intercept_request::SetFigjsVisible { visible },
                    )),
                }),
                None,
            ),
            FigtermCommand::InsertText {
                insertion,
                deletion,
                offset,
                immediate,
                insertion_buffer,
                insert_during_command,
            } => (
                Request::InsertText(InsertTextRequest {
                    insertion,
                    deletion: deletion.map(|x| x as u64),
                    offset,
                    immediate,
                    insertion_buffer,
                    insert_during_command,
                }),
                None,
            ),
            FigtermCommand::SetBuffer { text, cursor_position } => {
                (Request::SetBuffer(SetBufferRequest { text, cursor_position }), None)
            },
            FigtermCommand::RunProcess {
                channel,
                executable,
                arguments,
                working_directory,
                env,
                timeout,
            } => (
                Request::RunProcess(RunProcessRequest {
                    executable,
                    arguments,
                    working_directory,
                    env,
                    timeout: timeout.map(Into::into),
                }),
                Some(channel),
            ),
        };

        let nonce = if let Some(channel) = nonce_channel {
            Some(figterm_state.with(&session_id, |session| {
                let nonce = session.nonce_counter.fetch_add(1, Ordering::Relaxed);
                session.response_map.insert(nonce, channel);
                nonce
            })?)
        } else {
            None
        };

        let is_insert_request = matches!(request, Request::InsertText(_));
        figterm_state.with(&session_id, |session| {
            if let Some(writer) = &session.writer {
                if writer
                    .try_send(Clientbound {
                        packet: Some(clientbound::Packet::Request(clientbound::Request {
                            request: Some(request),
                            nonce,
                        })),
                    })
                    .is_ok()
                {
                    if is_insert_request {
                        if let Some(ref mut metrics) = session.current_session_metrics {
                            metrics.num_insertions += 1;
                            metrics.end_time =
                                OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
                        }
                    }
                    session.last_receive = Instant::now();
                };
            }
        })?;
    }

    None
}

async fn send_pings(outgoing: flume::Sender<Clientbound>, mut on_close_rx: tokio::sync::broadcast::Receiver<()>) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        select! {
            _ = interval.tick() => {
                let _ = outgoing.try_send(Clientbound {
                    packet: Some(clientbound::Packet::Ping(())),
                });
            }
            _ = on_close_rx.recv() => break
        }
    }
}

// This has to be used to sanitize as a hook can contain an invalid session_id and it must
// be sanitized before being sent to any consumers
fn sanitize_fn(context: &mut Option<ShellContext>, session_id: Uuid) {
    if let Some(context) = context {
        context.session_id = Some(session_id.to_string());
    }
}
