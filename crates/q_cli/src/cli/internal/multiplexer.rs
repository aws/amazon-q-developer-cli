use std::net::{
    Ipv4Addr,
    SocketAddr,
};
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
    anyhow,
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
    PacketOptions,
    message_to_packet,
    packet_to_message,
};
use fig_proto::remote;
use fig_proto::remote::{
    RunProcessRequest,
};
use fig_remote_ipc::figterm::{
    FigtermCommand,
    FigtermSessionId,
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
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::frame::Payload;
use tokio_util::codec::{
    FramedRead,
    FramedWrite,
};
use tracing::{
    debug,
    error,
    info,
    warn,
};

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
                    if let Err(err) = write.send(Message::Binary(Payload::Shared(bytes))).await {
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
                        Message::Binary(Payload::Owned(bytes_mut)) => Some(bytes_mut.freeze()),
                        Message::Binary(Payload::Shared(bytes)) => Some(bytes),
                        Message::Binary(Payload::Vec(vec)) => Some(vec.into()),
                        Message::Text(payload) => Some(payload.as_slice().to_vec().into()),
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

async fn handle_stdio_stream<S: AsyncWrite + AsyncRead + Unpin>(mut stream: S) {
    let mut stdio_stream = tokio::io::join(tokio::io::stdin(), tokio::io::stdout());
    tokio::io::copy_bidirectional(&mut stream, &mut stdio_stream)
        .await
        .unwrap();
}

pub async fn execute(args: MultiplexerArgs) -> Result<()> {
    // DO NOT REMOVE, this is needed such that CloudShell does not time out!
    info!("starting multiplexer");
    eprintln!("Starting multiplexer, this is required for AWS CloudShell.");

    let (external_stream, internal_stream) = tokio::io::duplex(1024 * 4);

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

        tokio::spawn(async move {
            while let Some(msg) = clientbound_rx.recv().await {
                external_write.write_all(&msg).await.unwrap();
            }
        });

        tokio::spawn(async move {
            let addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), args.port.unwrap_or(8080));
            let try_socket = TcpListener::bind(&addr).await;
            let listener = try_socket.expect("Failed to bind");
            info!("Listening on: {addr}");

            while let Ok((tcp_stream, _)) = listener.accept().await {
                let clientbound_rx = clientbound_tx_clone.subscribe();
                tokio::spawn(accept_connection(tcp_stream, hostbound_tx.clone(), clientbound_rx));
            }
        });
    } else {
        tokio::spawn(handle_stdio_stream(external_stream));
    }

    // Ensure the socket path exists and has correct permissions
    let socket_path = directories::local_remote_socket_path()?;
    if let Some(parent) = socket_path.parent() {
        if !parent.exists() {
            info!("creating parent socket");
            std::fs::create_dir_all(parent).context("Failed creating socket path")?;
        }

        #[cfg(unix)]
        {
            use std::fs::Permissions;
            use std::os::unix::fs::PermissionsExt;
            info!("setting permissions");
            std::fs::set_permissions(parent, Permissions::from_mode(0o700))?;
        }
    }

    // Remove the socket file if it already exists
    info!("removing socket");
    if let Err(err) = tokio::fs::remove_file(&socket_path).await {
        error!(%err, "Error removing socket");
    };

    // Create the socket
    info!("binding to socket");
    let listener = UnixListener::bind(&socket_path)?;

    let (read_half, write_half) = tokio::io::split(internal_stream);

    let packet_codec = Base64LineCodec::<mux::Packet>::new();
    let mut writer = FramedWrite::new(write_half, packet_codec.clone());
    let mut reader = FramedRead::new(read_half, packet_codec);

    let figterm_state = Arc::new(FigtermState::new());

    let (host_sender, mut host_receiver) = mpsc::unbounded_channel::<mux::Hostbound>();

    loop {
        select! {
            stream = listener.accept() => match stream {
                Ok((stream, _)) => {
                    info!("accepting steam");
                    tokio::spawn(handle_remote_ipc(stream, figterm_state.clone(), SimpleHookHandler {
                        sender: host_sender.clone(),
                    }));
                },
                Err(err) => error!(?err, "{PTY_BINARY_NAME} connection failed to accept"),
            },
            packet = reader.next() => match packet {
                Some(Ok(packet)) => {
                    info!("received packet");
                    let message = packet_to_message(packet).unwrap();
                    match handle_client_bound_message(message, &figterm_state, &host_sender).await {
                        Ok(Some(msg)) => {
                            let session = figterm_state.most_recent().context("most recent 1")?;
                            info!("sending to session {}", session.id);
                            session.sender.send(msg)?;
                        }
                        Ok(None) => {}
                        Err(err) => error!(?err, "error")
                    };
                },
                Some(Err(err)) => {
                    error!(?err, "Error");
                },
                None => {
                    info!("{PTY_BINARY_NAME} connection closed");
                    break;
                },
            },
            encoded = host_receiver.recv() => match encoded {
                Some(hostbound) => {
                    info!("sending packet");
                    let packet = message_to_packet(hostbound, &PacketOptions { gzip: true });
                    writer.send(packet).await.unwrap();
                },
                None => bail!("host recv none"),
            }
        }
    }

    Ok(())
}

async fn handle_client_bound_message(
    message: mux::Clientbound,
    state: &Arc<FigtermState>,
    host_sender: &UnboundedSender<mux::Hostbound>,
) -> Result<Option<FigtermCommand>> {
    let Some(submessage) = message.submessage else {
        bail!("received malformed message");
    };

    info!("submessage: {:?}", submessage);

    Ok(match submessage {
        mux::clientbound::Submessage::Intercept(InterceptRequest { intercept_command }) => match intercept_command {
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
        },
        mux::clientbound::Submessage::InsertText(InsertTextRequest {
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
        mux::clientbound::Submessage::SetBuffer(SetBufferRequest { text, cursor_position }) => {
            Some(FigtermCommand::SetBuffer { text, cursor_position })
        },
        mux::clientbound::Submessage::RunProcess(RunProcessRequest {
            executable,
            arguments,
            working_directory,
            env,
        }) => {
            let (message, rx) = FigtermCommand::run_process(executable, arguments, working_directory, env);

            let session = state.most_recent().context("most recent 3")?;
            let sender = session.sender.clone();
            let session_id = session.id.to_string();
            drop(session);

            sender.send(message).context("Failed sending command to figterm")?;

            let timeout_duration = Duration::from_secs(10);

            let response = timeout(timeout_duration, rx)
                .await
                .context("Timed out waiting for figterm response")?
                .context("Failed to receive figterm response")?;

            if let remote::hostbound::response::Response::RunProcess(response) = response {
                let hostbound = mux::Hostbound {
                    session_id,
                    submessage: Some(mux::hostbound::Submessage::RunProcessResponse(response)),
                };
                host_sender.send(hostbound)?;
                None
            } else {
                bail!("invalid response type");
            }
        },
        mux::clientbound::Submessage::Diagnostics(_)
        | mux::clientbound::Submessage::InsertOnNewCmd(_)
        | mux::clientbound::Submessage::ReadFile(_) => None,
    })
}

struct SimpleHookHandler {
    sender: UnboundedSender<mux::Hostbound>,
}

impl SimpleHookHandler {
    fn resererialize_send(
        &mut self,
        session_id: &FigtermSessionId,
        submessage: mux::hostbound::Submessage,
    ) -> eyre::Result<()> {
        info!("sending on sender");
        let hostbound = mux::Hostbound {
            session_id: session_id.to_string(),
            submessage: Some(submessage),
        };
        self.sender.send(hostbound)?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl fig_remote_ipc::RemoteHookHandler for SimpleHookHandler {
    type Error = eyre::Error;

    async fn edit_buffer(
        &mut self,
        edit_buffer_hook: &EditBufferHook,
        session_id: &FigtermSessionId,
        _figterm_state: &Arc<FigtermState>,
    ) -> Result<Option<remote::clientbound::response::Response>, Self::Error> {
        self.resererialize_send(
            session_id,
            mux::hostbound::Submessage::EditBuffer(edit_buffer_hook.clone()),
        )?;
        Ok(None)
    }

    async fn prompt(
        &mut self,
        prompt_hook: &PromptHook,
        session_id: &FigtermSessionId,
        _figterm_state: &Arc<FigtermState>,
    ) -> Result<Option<remote::clientbound::response::Response>, Self::Error> {
        self.resererialize_send(session_id, mux::hostbound::Submessage::Prompt(prompt_hook.clone()))?;
        Ok(None)
    }

    async fn pre_exec(
        &mut self,
        pre_exec_hook: &PreExecHook,
        session_id: &FigtermSessionId,
        _figterm_state: &Arc<FigtermState>,
    ) -> Result<Option<remote::clientbound::response::Response>, Self::Error> {
        self.resererialize_send(session_id, mux::hostbound::Submessage::PreExec(pre_exec_hook.clone()))?;
        Ok(None)
    }

    async fn post_exec(
        &mut self,
        post_exec_hook: &PostExecHook,
        session_id: &FigtermSessionId,
        _figterm_state: &Arc<FigtermState>,
    ) -> Result<Option<remote::clientbound::response::Response>, Self::Error> {
        self.resererialize_send(session_id, mux::hostbound::Submessage::PostExec(post_exec_hook.clone()))?;
        Ok(None)
    }

    async fn intercepted_key(
        &mut self,
        intercepted_key: InterceptedKeyHook,
        session_id: &FigtermSessionId,
    ) -> Result<Option<remote::clientbound::response::Response>, Self::Error> {
        self.resererialize_send(
            session_id,
            mux::hostbound::Submessage::InterceptedKey(intercepted_key.clone()),
        )?;
        Ok(None)
    }

    async fn account_info(&mut self) -> Result<Option<remote::clientbound::response::Response>, Self::Error> {
        Err(anyhow!("account info not implemented"))
    }

    async fn start_exchange_credentials(
        &mut self,
    ) -> Result<Option<remote::clientbound::response::Response>, Self::Error> {
        Err(anyhow!("start_exchange_credentials not implemented"))
    }

    async fn confirm_exchange_credentials(
        &mut self,
    ) -> Result<Option<remote::clientbound::response::Response>, Self::Error> {
        Err(anyhow!("confirm_exchange_credentials not implemented"))
    }
}

#[cfg(test)]
mod tests {
    use fig_proto::fig::ShellContext;

    use super::*;

    #[tokio::test]
    async fn test_handle_client_bound_message() {
        let messages = [
            mux::clientbound::Submessage::Intercept(InterceptRequest {
                intercept_command: Some(InterceptCommand::SetFigjsIntercepts(SetFigjsIntercepts {
                    intercept_bound_keystrokes: false,
                    intercept_global_keystrokes: false,
                    actions: vec![],
                    override_actions: false,
                })),
            }),
            mux::clientbound::Submessage::Intercept(InterceptRequest {
                intercept_command: Some(InterceptCommand::SetFigjsVisible(SetFigjsVisible { visible: false })),
            }),
            mux::clientbound::Submessage::InsertText(InsertTextRequest {
                insertion: None,
                deletion: None,
                offset: None,
                immediate: None,
                insertion_buffer: None,
                insert_during_command: None,
            }),
            mux::clientbound::Submessage::SetBuffer(SetBufferRequest {
                text: "text".into(),
                cursor_position: None,
            }),
        ];

        for message in messages {
            let state = Arc::new(FigtermState::new());
            let (sender, _) = mpsc::unbounded_channel();
            let message = mux::Clientbound {
                session_id: "abcdef".into(),
                submessage: Some(message),
            };

            let result = handle_client_bound_message(message, &state, &sender).await;

            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_simple_hook_handler_resererialize_send() {
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let mut handler = SimpleHookHandler { sender };

        let message = mux::hostbound::Submessage::EditBuffer(EditBufferHook {
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
        handler
            .resererialize_send(&FigtermSessionId::new("abcdef"), message)
            .unwrap();

        let received = receiver.try_recv().unwrap();
        println!("{received:?}");
    }
}
