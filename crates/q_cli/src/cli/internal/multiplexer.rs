use std::net::{
    Ipv4Addr,
    SocketAddr,
};
use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

use bytes::{
    Bytes,
    BytesMut,
};
use clap::Args;
use eyre::{
    Context,
    ContextCompat,
    Result,
    bail,
};
use fig_ipc::Base64LineCodec;
use fig_proto::figterm::intercept_request::{
    InterceptCommand,
    SetFigjsIntercepts,
    SetFigjsVisible,
};
use fig_proto::figterm::{
    InsertTextRequest,
    InterceptRequest,
    SetBufferRequest,
};
use fig_proto::local::{
    EditBufferHook,
    InterceptedKeyHook,
    PostExecHook,
    PreExecHook,
    PromptHook,
};
use fig_proto::mux::{
    self,
    Packet,
    PacketOptions,
    Ping,
    message_to_packet,
    packet_to_message,
};
use fig_proto::remote;
use fig_proto::remote::RunProcessRequest;
use fig_remote_ipc::figterm::{
    FigtermCommand,
    FigtermState,
};
use fig_remote_ipc::remote::handle_remote_ipc;
use fig_util::{
    PTY_BINARY_NAME,
    directories,
};
use futures::{
    SinkExt,
    StreamExt,
};
use tokio::io::{
    AsyncRead,
    AsyncReadExt,
    AsyncWrite,
    AsyncWriteExt,
};
use tokio::net::{
    TcpListener,
    TcpStream,
    UnixListener,
};
use tokio::select;
use tokio::sync::broadcast;
use tokio::sync::mpsc::{
    self,
    UnboundedSender,
};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::codec::{
    FramedRead,
    FramedWrite,
};
use tracing::{
    debug,
    error,
    info,
    trace,
    warn,
};
use uuid::Uuid;

use crate::util::pid_file::PidLock;

#[derive(Debug, PartialEq, Eq, Args)]
pub struct MultiplexerArgs {
    #[arg(long, default_value_t = false)]
    websocket: bool,
    #[arg(long)]
    port: Option<u16>,
}

async fn accept_connection(
    tcp_stream: TcpStream,
    hostbound_tx: mpsc::Sender<Bytes>,
    mut clientbound_rx: broadcast::Receiver<Bytes>,
) {
    let addr = tcp_stream
        .peer_addr()
        .expect("connected streams should have a peer address");
    info!("Peer address: {addr}");

    let ws_stream = tokio_tungstenite::accept_async(tcp_stream)
        .await
        .expect("Error during the websocket handshake occurred");

    info!("New WebSocket connection: {addr}");

    let (mut write, mut read) = ws_stream.split();

    let clientbound_join: JoinHandle<Result<(), ()>> = tokio::spawn(async move {
        loop {
            match clientbound_rx.recv().await {
                Ok(bytes) => {
                    if let Err(err) = write.send(Message::Binary(bytes)).await {
                        error!(%err, "error sending to WebSocketStream");
                        return Err(());
                    }
                },
                Err(broadcast::error::RecvError::Lagged(lag)) => {
                    warn!(%lag, %addr, "clientbound_rx lagged");
                },
                Err(broadcast::error::RecvError::Closed) => {
                    info!("clientbound_rx closed");
                    return Err(());
                },
            }
        }
    });

    let hostbound_join: JoinHandle<Result<(), ()>> = tokio::spawn(async move {
        loop {
            match read.next().await {
                Some(Ok(message)) => {
                    let bytes = match message {
                        Message::Binary(bytes) => Some(bytes),
                        Message::Text(bytes) => Some(bytes.as_bytes().to_vec().into()),
                        _ => continue,
                    };
                    if let Some(bytes) = bytes {
                        hostbound_tx.send(bytes).await.unwrap();
                    }
                },
                Some(Err(err)) => {
                    error!(%err, "WebSocketStream error");
                    return Err(());
                },
                None => {
                    debug!("WebSocketStream ended");
                    return Err(());
                },
            }
        }
    });

    match tokio::try_join!(clientbound_join, hostbound_join) {
        Ok(_) => {},
        Err(err) => error!(%err, "error in websocket connection"),
    }

    info!("Websocket connection closed");
}

async fn handle_stdio_stream<S: AsyncWrite + AsyncRead + Unpin>(mut stream: S, error_tx: mpsc::Sender<std::io::Error>) {
    let mut stdio_stream = tokio::io::join(tokio::io::stdin(), tokio::io::stdout());
    if let Err(err) = tokio::io::copy_bidirectional(&mut stream, &mut stdio_stream).await {
        let _ = error_tx.send(err).await;
    }
}

pub async fn execute(args: MultiplexerArgs) -> Result<()> {
    #[cfg(unix)]
    let pid_lock = match fig_util::directories::runtime_dir() {
        Ok(dir) => Some(PidLock::new(dir.join("mux.lock")).await.ok()).flatten(),
        Err(err) => {
            error!(%err, "Failed to get runtime dir");
            None
        },
    };

    // DO NOT REMOVE, this is needed such that CloudShell does not time out!
    info!("starting multiplexer");
    eprintln!("Starting multiplexer, this is required for AWS CloudShell.");

    let (external_stream, internal_stream) = tokio::io::duplex(1024 * 4);

    let (error_tx, mut error_rx) = tokio::sync::mpsc::channel::<std::io::Error>(1);
    if args.websocket {
        let (clientbound_tx, _) = broadcast::channel::<Bytes>(10);
        let (hostbound_tx, mut clientbound_rx) = mpsc::channel::<Bytes>(10);
        let clientbound_tx_clone = clientbound_tx.clone();

        let (mut external_read, mut external_write) = tokio::io::split(external_stream);

        tokio::spawn(async move {
            let mut buf = BytesMut::new();
            while let Ok(n) = external_read.read_buf(&mut buf).await {
                if n == 0 {
                    break;
                }
                let _ = clientbound_tx.send(buf.split().freeze());
                buf.reserve(4096_usize.saturating_sub(buf.capacity()));
            }
        });

        tokio::spawn({
            let error_tx = error_tx.clone();
            async move {
                while let Some(msg) = clientbound_rx.recv().await {
                    if let Err(err) = external_write.write_all(&msg).await {
                        let _ = error_tx.send(err).await;
                    }
                }
            }
        });

        tokio::spawn({
            let error_tx = error_tx.clone();
            async move {
                let addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), args.port.unwrap_or(8080));
                let listener = match TcpListener::bind(&addr).await {
                    Ok(listener) => listener,
                    Err(err) => {
                        let _ = error_tx.send(err).await;
                        return;
                    },
                };
                info!("Listening on: {addr}");

                while let Ok((tcp_stream, stream_addr)) = listener.accept().await {
                    info!(%stream_addr, "Accepted stream");
                    let clientbound_rx = clientbound_tx_clone.subscribe();
                    tokio::spawn(accept_connection(tcp_stream, hostbound_tx.clone(), clientbound_rx));
                }
            }
        });
    } else {
        tokio::spawn(handle_stdio_stream(external_stream, error_tx));
    };

    // Ensure the socket path exists and has correct permissions
    let socket_path = directories::local_remote_socket_path()?;
    if let Some(parent) = socket_path.parent() {
        if !parent.exists() {
            info!(?parent, "creating socket parent dir");
            std::fs::create_dir_all(parent).context("Failed creating socket path")?;
        }

        #[cfg(unix)]
        {
            use std::fs::Permissions;
            use std::os::unix::fs::PermissionsExt;
            info!(?parent, "setting permissions");
            std::fs::set_permissions(parent, Permissions::from_mode(0o700))?;
        }
    }

    // Remove the socket file if it already exists
    info!(?socket_path, "removing socket");
    if let Err(err) = tokio::fs::remove_file(&socket_path).await {
        error!(%err, "Error removing socket");
    };

    // Create the socket
    info!(?socket_path, "binding to socket");
    let listener = UnixListener::bind(&socket_path)?;

    let (read_half, write_half) = tokio::io::split(internal_stream);

    let packet_codec = Base64LineCodec::<mux::Packet>::new();
    let mut writer = FramedWrite::new(write_half, packet_codec.clone());
    let mut reader = FramedRead::new(read_half, packet_codec);

    let figterm_state = Arc::new(FigtermState::new());

    let (host_sender, mut host_receiver) = mpsc::unbounded_channel::<mux::Hostbound>();

    loop {
        let control_flow = select! {
            stream = listener.accept() => {
                match stream {
                    Ok((stream, _)) => {
                        info!("accepting steam");
                        tokio::spawn(handle_remote_ipc(stream, figterm_state.clone(), SimpleHookHandler {
                            sender: host_sender.clone(),
                        }));
                    },
                    Err(err) => error!(?err, "{PTY_BINARY_NAME} connection failed to accept"),
                };
                ControlFlow::Continue(())
            },
            packet = reader.next() => forward_packet_to_figterm(packet, &figterm_state, &host_sender).await,
            encoded = host_receiver.recv() => {
                match encoded {
                    Some(hostbound) => {
                        info!("sending packet");
                        let packet = match message_to_packet(hostbound, &PacketOptions { gzip: true }) {
                            Ok(packet) => packet,
                            Err(err) => {
                                error!(?err, "error encoding packet");
                                continue;
                            },
                        };
                        if let Err(err) = writer.send(packet).await {
                            error!(?err, "error sending packet");
                        }
                    },
                    None => bail!("host recv none"),
                };
                ControlFlow::Continue(())
            },
            error_opt = error_rx.recv() => {
                match error_opt {
                    Some(err) => error!(?err, "error in multiplexer"),
                    None => error!("error_rx closed")
                };
                ControlFlow::Break(())
            },
            _ = tokio::signal::ctrl_c() => {
                eprintln!("\nExiting multiplexer: ctrl-c");
                ControlFlow::Break(())
            },
        };

        if control_flow.is_break() {
            break;
        }
    }

    #[cfg(unix)]
    let _ = pid_lock.map(|l| l.release());

    info!("quitting multiplexer");
    Ok(())
}

async fn forward_packet_to_figterm(
    packet: Option<Result<Packet, std::io::Error>>,
    figterm_state: &Arc<FigtermState>,
    host_sender: &UnboundedSender<mux::Hostbound>,
) -> ControlFlow<()> {
    match packet {
        Some(Ok(packet)) => {
            info!("received packet");
            let message: mux::Clientbound = match packet_to_message(packet) {
                Ok(message) => message,
                Err(err) => {
                    error!(?err, "error decoding packet");
                    return ControlFlow::Continue(());
                },
            };

            match handle_clientbound(message, figterm_state, host_sender).await {
                Ok(Some((msg, session_id))) => {
                    if let Some(session) = figterm_state.get(&session_id) {
                        info!("sending to session {}", session.id);
                        if let Err(err) = session.sender.send(msg) {
                            error!(?err, "error sending to session");
                        }
                    } else {
                        warn!("no session to send to");
                    }
                },
                Ok(None) => warn!("Nothing to send"),
                Err(err) => error!(?err, "error"),
            };
            ControlFlow::Continue(())
        },
        Some(Err(err)) => {
            error!(?err, "Error");
            ControlFlow::Continue(())
        },
        None => {
            info!("{PTY_BINARY_NAME} connection closed");
            ControlFlow::Break(())
        },
    }
}

async fn handle_clientbound(
    message: mux::Clientbound,
    state: &Arc<FigtermState>,
    host_sender: &UnboundedSender<mux::Hostbound>,
) -> Result<Option<(FigtermCommand, Uuid)>> {
    trace!(?message, "handle mux::Clientbound");

    let mux::Clientbound { submessage } = message;
    let Some(submessage) = submessage else {
        bail!("received malformed message, missing submessage");
    };

    match submessage {
        mux::clientbound::Submessage::Request(request) => match request.inner {
            Some(inner) => {
                let session_id = match Uuid::parse_str(&request.session_id) {
                    Ok(session_id) => session_id,
                    Err(err) => {
                        error!(?err, "error parsing session id");
                        bail!("received malformed mux::clientbound::Submessage::Request, invalid session id");
                    },
                };

                Ok(
                    handle_clienbound_request(inner, request.session_id, request.message_id, state, host_sender)
                        .await?
                        .map(|a| (a, session_id)),
                )
            },
            None => bail!("received malformed mux::clientbound::Submessage::Request, missing inner"),
        },
        mux::clientbound::Submessage::Ping(Ping { message_id }) => {
            trace!(%message_id, "received mux::clientbound::Submessage::Ping");
            host_sender
                .send(mux::Hostbound {
                    submessage: Some(mux::hostbound::Submessage::Pong(mux::Pong { message_id })),
                })
                .context("Failed sending pong")?;
            Ok(None)
        },
    }
}

async fn handle_clienbound_request(
    inner: mux::clientbound::request::Inner,
    session_id: String,
    message_id: String,
    state: &Arc<FigtermState>,
    host_sender: &UnboundedSender<mux::Hostbound>,
) -> Result<Option<FigtermCommand>> {
    Ok(match inner {
        mux::clientbound::request::Inner::Intercept(InterceptRequest { intercept_command }) => {
            match intercept_command {
                Some(InterceptCommand::SetFigjsIntercepts(SetFigjsIntercepts {
                    intercept_bound_keystrokes,
                    intercept_global_keystrokes,
                    actions,
                    override_actions,
                })) => Some(FigtermCommand::InterceptFigJs {
                    intercept_keystrokes: intercept_bound_keystrokes,
                    intercept_global_keystrokes,
                    actions,
                    override_actions,
                }),
                Some(InterceptCommand::SetFigjsVisible(SetFigjsVisible { visible })) => {
                    Some(FigtermCommand::InterceptFigJSVisible { visible })
                },
                None => None,
            }
        },
        mux::clientbound::request::Inner::InsertText(InsertTextRequest {
            insertion,
            deletion,
            offset,
            immediate,
            insertion_buffer,
            insert_during_command,
        }) => Some(FigtermCommand::InsertText {
            insertion,
            deletion: deletion.map(|d| d as i64),
            offset,
            immediate,
            insertion_buffer,
            insert_during_command,
        }),
        mux::clientbound::request::Inner::SetBuffer(SetBufferRequest { text, cursor_position }) => {
            Some(FigtermCommand::SetBuffer { text, cursor_position })
        },
        mux::clientbound::request::Inner::RunProcess(RunProcessRequest {
            executable,
            arguments,
            working_directory,
            env,
            timeout,
        }) => {
            let timeout = timeout.map(Into::into);
            let (command, rx) = FigtermCommand::run_process(executable, arguments, working_directory, env, timeout);

            let session_id = Uuid::parse_str(&session_id).context("failed to parse session id")?;
            let sender = state.get(&session_id).context("failed to get sender")?.sender.clone();
            sender.send(command).context("Failed sending command to figterm")?;
            drop(sender);

            let timeout_duration = timeout.unwrap_or(Duration::from_secs(60));
            let response = tokio::time::timeout(timeout_duration, rx)
                .await
                .context("Timed out waiting for figterm response")?
                .context("Failed to receive figterm response")?;

            if let remote::hostbound::response::Response::RunProcess(response) = response {
                let hostbound = mux::Hostbound {
                    submessage: Some(mux::hostbound::Submessage::Response(mux::hostbound::Response {
                        session_id: session_id.to_string(),
                        message_id,
                        inner: Some(mux::hostbound::response::Inner::RunProcess(response)),
                    })),
                };
                host_sender.send(hostbound)?;
                None
            } else {
                bail!("invalid response type");
            }
        },
    })
}

struct SimpleHookHandler {
    sender: UnboundedSender<mux::Hostbound>,
}

impl SimpleHookHandler {
    fn send(&mut self, submessage: mux::hostbound::Submessage) -> eyre::Result<()> {
        info!("sending on sender");
        self.sender.send(mux::Hostbound {
            submessage: Some(submessage),
        })?;
        Ok(())
    }

    fn send_request(&mut self, session_id: Uuid, request: mux::hostbound::request::Inner) -> eyre::Result<()> {
        self.send(mux::hostbound::Submessage::Request(mux::hostbound::Request {
            session_id: session_id.to_string(),
            message_id: Uuid::new_v4().to_string(),
            inner: Some(request),
        }))
    }
}

#[async_trait::async_trait]
impl fig_remote_ipc::RemoteHookHandler for SimpleHookHandler {
    type Error = eyre::Error;

    async fn edit_buffer(
        &mut self,
        edit_buffer_hook: &EditBufferHook,
        session_id: Uuid,
        _figterm_state: &Arc<FigtermState>,
    ) -> Result<Option<remote::clientbound::response::Response>, Self::Error> {
        self.send_request(
            session_id,
            mux::hostbound::request::Inner::EditBuffer(edit_buffer_hook.clone()),
        )?;
        Ok(None)
    }

    async fn prompt(
        &mut self,
        prompt_hook: &PromptHook,
        session_id: Uuid,
        _figterm_state: &Arc<FigtermState>,
    ) -> Result<Option<remote::clientbound::response::Response>, Self::Error> {
        self.send_request(session_id, mux::hostbound::request::Inner::Prompt(prompt_hook.clone()))?;
        Ok(None)
    }

    async fn pre_exec(
        &mut self,
        pre_exec_hook: &PreExecHook,
        session_id: Uuid,
        _figterm_state: &Arc<FigtermState>,
    ) -> Result<Option<remote::clientbound::response::Response>, Self::Error> {
        self.send_request(
            session_id,
            mux::hostbound::request::Inner::PreExec(pre_exec_hook.clone()),
        )?;
        Ok(None)
    }

    async fn post_exec(
        &mut self,
        post_exec_hook: &PostExecHook,
        session_id: Uuid,
        _figterm_state: &Arc<FigtermState>,
    ) -> Result<Option<remote::clientbound::response::Response>, Self::Error> {
        self.send_request(
            session_id,
            mux::hostbound::request::Inner::PostExec(post_exec_hook.clone()),
        )?;
        Ok(None)
    }

    async fn intercepted_key(
        &mut self,
        intercepted_key: InterceptedKeyHook,
        session_id: Uuid,
    ) -> Result<Option<remote::clientbound::response::Response>, Self::Error> {
        self.send_request(
            session_id,
            mux::hostbound::request::Inner::InterceptedKey(intercepted_key.clone()),
        )?;
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use fig_proto::fig::ShellContext;
    use uuid::Uuid;

    use super::*;

    #[tokio::test]
    async fn test_handle_client_bound_message() {
        let messages = [
            mux::clientbound::request::Inner::Intercept(InterceptRequest {
                intercept_command: Some(InterceptCommand::SetFigjsIntercepts(SetFigjsIntercepts {
                    intercept_bound_keystrokes: false,
                    intercept_global_keystrokes: false,
                    actions: vec![],
                    override_actions: false,
                })),
            }),
            mux::clientbound::request::Inner::Intercept(InterceptRequest {
                intercept_command: Some(InterceptCommand::SetFigjsVisible(SetFigjsVisible { visible: false })),
            }),
            mux::clientbound::request::Inner::InsertText(InsertTextRequest {
                insertion: None,
                deletion: None,
                offset: None,
                immediate: None,
                insertion_buffer: None,
                insert_during_command: None,
            }),
            mux::clientbound::request::Inner::SetBuffer(SetBufferRequest {
                text: "text".into(),
                cursor_position: None,
            }),
        ];

        for message in messages {
            let state = Arc::new(FigtermState::new());
            let (sender, _) = mpsc::unbounded_channel();
            let message = mux::Clientbound {
                submessage: Some(mux::clientbound::Submessage::Request(mux::clientbound::Request {
                    session_id: Uuid::new_v4().to_string(),
                    message_id: Uuid::new_v4().to_string(),
                    inner: Some(message),
                })),
            };

            let result = handle_clientbound(message, &state, &sender).await.unwrap();
            println!("{result:?}");
        }
    }

    #[tokio::test]
    async fn test_simple_hook_handler_resererialize_send() {
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let mut handler = SimpleHookHandler { sender };

        let message = mux::hostbound::request::Inner::EditBuffer(EditBufferHook {
            context: Some(ShellContext {
                pid: Some(123),
                shell_path: Some("/bin/bash".into()),
                ..Default::default()
            }),
            text: "abc".into(),
            cursor: 1,
            histno: 2,
            terminal_cursor_coordinates: None,
        });
        handler.send_request(Uuid::new_v4(), message).unwrap();

        let received = receiver.try_recv().unwrap();
        println!("{received:?}");
    }
}
