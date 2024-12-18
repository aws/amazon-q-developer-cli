use std::net::{
    Ipv4Addr,
    SocketAddr,
};
use std::sync::Arc;
use std::time::Duration;

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
    // Clientbound,
    // Hostbound,
    // Packet,
    // clientbound,
    // hostbound,
    message_to_packet,
    packet_to_message,
};
use fig_proto::remote;
// use fig_proto::remote::clientbound::response::Response as ClientboundResponse;
use fig_proto::remote::{
    PseudoterminalExecuteRequest,
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
    TryStreamExt,
    future,
};
use tokio::io::{
    AsyncRead,
    AsyncWrite,
};
use tokio::net::{
    TcpListener,
    TcpStream,
    UnixListener,
};
use tokio::select;
use tokio::sync::mpsc::{
    self,
    UnboundedSender,
};
use tokio::time::timeout;
use tokio_util::codec::{
    FramedRead,
    FramedWrite,
};
use tracing::{
    error,
    info,
};

async fn accept_connection(tcp_stream: TcpStream) {
    let addr = tcp_stream
        .peer_addr()
        .expect("connected streams should have a peer address");
    info!("Peer address: {addr}");

    let ws_stream = tokio_tungstenite::accept_async(tcp_stream)
        .await
        .expect("Error during the websocket handshake occurred");

    info!("New WebSocket connection: {addr}");

    let (write, read) = ws_stream.split();
    // We should not forward messages other than text or binary.
    read.try_filter(|msg| future::ready(msg.is_text() || msg.is_binary()))
        .forward(write)
        .await
        .expect("Failed to forward messages")
}

async fn handle_stdio_stream<S: AsyncWrite + AsyncRead + Unpin>(mut stream: S) {
    let mut stdio_stream = tokio::io::join(tokio::io::stdin(), tokio::io::stdout());
    tokio::io::copy_bidirectional(&mut stream, &mut stdio_stream)
        .await
        .unwrap();
}

pub async fn execute() -> Result<()> {
    // DO NOT REMOVE, this is needed such that CloudShell does not time out!
    eprintln!("Starting multiplexer, this is required for AWS CloudShell.");
    info!("starting multiplexer");

    let (external_stream, internal_stream) = tokio::io::duplex(1024 * 4);

    let stdio = true;

    if stdio {
        tokio::spawn(handle_stdio_stream(external_stream));
    } else {
        let addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 8080);
        let try_socket = TcpListener::bind(&addr).await;
        let listener = try_socket.expect("Failed to bind");
        info!("Listening on: {}", addr);

        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(accept_connection(stream));
        }
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
    // let mut reader = BufferedReader::new(read_half);
    // let mut writer = BufferedReader::new(write_half);

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
                        Err(err) => error!("error: {err:?}")
                    };
                },
                Some(Err(err)) => {
                    error!("Error: {err:?}");
                },
                None => {
                    info!("{PTY_BINARY_NAME} connection closed");
                    break;
                },
            },
            encoded = host_receiver.recv() => match encoded {
                Some(hostbound) => {
                    info!("sending packet");
                    let packet = message_to_packet(hostbound, &PacketOptions { gzip: false });
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

    Ok(Some(match submessage {
        mux::clientbound::Submessage::Intercept(InterceptRequest {
            intercept_command: Some(command),
        }) => match command {
            InterceptCommand::SetFigjsIntercepts(SetFigjsIntercepts {
                intercept_bound_keystrokes,
                intercept_global_keystrokes,
                actions,
                override_actions,
            }) => FigtermCommand::InterceptFigJs {
                intercept_keystrokes: intercept_bound_keystrokes,
                intercept_global_keystrokes,
                actions,
                override_actions,
            },
            InterceptCommand::SetFigjsVisible(SetFigjsVisible { visible }) => {
                FigtermCommand::InterceptFigJSVisible { visible }
            },
        },
        mux::clientbound::Submessage::InsertText(InsertTextRequest {
            insertion,
            deletion,
            offset,
            immediate,
            insertion_buffer,
            insert_during_command,
        }) => FigtermCommand::InsertText {
            insertion,
            deletion: deletion.map(|d| d as i64),
            offset,
            immediate,
            insertion_buffer,
            insert_during_command,
        },
        mux::clientbound::Submessage::SetBuffer(SetBufferRequest { text, cursor_position }) => {
            FigtermCommand::SetBuffer { text, cursor_position }
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
                return Ok(None);
            } else {
                bail!("invalid response type");
            }
        },
        mux::clientbound::Submessage::PseudoterminalExecute(PseudoterminalExecuteRequest {
            command,
            working_directory,
            background_job,
            is_pipelined,
            env,
        }) => {
            let (message, rx) =
                FigtermCommand::pseudoterminal_execute(command, working_directory, background_job, is_pipelined, env);

            let session = state.most_recent().context("most recent 3")?;
            let sender = session.sender.clone();
            let session_id = session.id.to_string();
            drop(session);

            sender.send(message)?;

            let response = timeout(Duration::from_secs(10), rx)
                .await
                .context("Qterm response timed out after 10 sec")?
                .context("Qterm response failed to receive from sender")?;

            if let remote::hostbound::response::Response::PseudoterminalExecute(response) = response {
                let hostbound = mux::Hostbound {
                    session_id,
                    submessage: Some(mux::hostbound::Submessage::PseudoterminalExecuteResponse(response)),
                };
                host_sender.send(hostbound)?;
                return Ok(None);
            } else {
                bail!("invalid response type");
            }
        },
        _ => bail!("INVALID REQUEST"),
    }))
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

        // let a: Hostbound = FigMessage {
        //     inner: Bytes::from(received),
        //     message_type: FigMessageType::Protobuf,
        // }
        // .decode()
        // .unwrap();

        // println!("{a:?}")
    }
}
