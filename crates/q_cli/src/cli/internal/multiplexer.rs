use std::sync::Arc;
use std::time::Duration;

use base64::prelude::*;
use bytes::Bytes;
use eyre::{
    Context,
    ContextCompat,
    Result,
    anyhow,
    bail,
};
use fig_ipc::{
    BufferedReader,
    RecvMessage,
};
use fig_proto::FigProtobufEncodable;
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
use fig_proto::remote::clientbound::request::Request as ClientboundRequest;
use fig_proto::remote::clientbound::response::Response as ClientboundResponse;
use fig_proto::remote::clientbound::{
    PseudoterminalExecuteRequest,
    RunProcessRequest,
};
use fig_proto::remote::{
    Clientbound,
    Hostbound,
    clientbound,
    hostbound,
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
use tokio::io::AsyncWriteExt;
use tokio::net::UnixListener;
use tokio::select;
use tokio::sync::mpsc::{
    self,
    UnboundedSender,
};
use tokio::time::timeout;
use tracing::{
    error,
    info,
};

pub async fn execute() -> Result<()> {
    // DO NOT REMOVE, this is needed such that CloudShell does not time out!
    eprintln!("Starting multiplexer, this is required for AWS CloudShell.");
    info!("starting multiplexer");

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
        error!(%err, "Error removing socket")
    };

    // Create the socket
    info!("binding to socket");
    let listener = UnixListener::bind(&socket_path)?;

    // Get a handle to stdout and stdin
    let mut stdout = tokio::io::stdout();
    let stdin = tokio::io::stdin();
    let mut reader = BufferedReader::new(stdin);

    let tmpdir = directories::logs_dir().unwrap();
    let mut mux_output = tokio::fs::File::create(tmpdir.join("mux-output.bin")).await.unwrap();

    let figterm_state = Arc::new(FigtermState::new());

    let (host_sender, mut host_receiver) = mpsc::unbounded_channel::<Bytes>();

    loop {
        select! {
            stream = listener.accept() => match stream {
                Ok((stream, _)) => {
                    info!("accepting steam");
                    tokio::spawn(handle_remote_ipc(stream, figterm_state.clone(), SimpleHookHandler {
                        sender: host_sender.clone(),
                    }));
                },
                Err(err) => error!("{PTY_BINARY_NAME} connection failed to accept: {err:?}"),
            },
            message = reader.recv_message::<Clientbound>() => match message {
                Ok(Some(message)) => {
                    info!("reader.recv_message::<Clientbound>()");
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
                Ok(None) => {
                    info!("{PTY_BINARY_NAME} connection closed");
                    break;
                },
                Err(err) => {
                    error!("Error: {err:?}");
                    if !err.is_disconnect() {
                        error!(%err, "Failed receiving remote message");
                    }
                    // break;
                },
            },
            encoded = host_receiver.recv() => match encoded {
                Some(encoded) => {

                    info!("host_receiver  recv()");


                    let b64 = BASE64_STANDARD.encode(encoded);

                    stdout.write_all(b64.as_bytes()).await?;
                    stdout.write_all(b"\n").await?;
                    stdout.flush().await?;

                    mux_output.write_all(b64.as_bytes()).await?;
                    mux_output.write_all(b"\n").await?;
                    mux_output.flush().await?;
                },
                None => bail!("host recv none"),
            }
        }
    }

    Ok(())
}

async fn handle_client_bound_message(
    message: Clientbound,
    state: &Arc<FigtermState>,
    host_sender: &UnboundedSender<Bytes>,
) -> Result<Option<FigtermCommand>> {
    let Some(packet) = message.packet else {
        bail!("received malformed message");
    };

    info!("packet: {:?}", packet);

    Ok(Some(match packet {
        clientbound::Packet::Request(request) => match request.request.context("no request")? {
            ClientboundRequest::Intercept(InterceptRequest {
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
            ClientboundRequest::InsertText(InsertTextRequest {
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
            ClientboundRequest::SetBuffer(SetBufferRequest { text, cursor_position }) => {
                FigtermCommand::SetBuffer { text, cursor_position }
            },
            ClientboundRequest::RunProcess(RunProcessRequest {
                executable,
                arguments,
                working_directory,
                env,
            }) => {
                let session_sender = &state.most_recent().context("most recent 2")?.sender;
                let (message, rx) = FigtermCommand::run_process(executable, arguments, working_directory, env);
                session_sender
                    .send(message)
                    .context("Failed sending command to figterm")?;

                let timeout_duration = Duration::from_secs(10);

                let response = timeout(timeout_duration, rx)
                    .await
                    .context("Timed out waiting for figterm response")?
                    .context("Failed to receive figterm response")?;

                if let hostbound::response::Response::RunProcess(response) = response {
                    let msg = Hostbound {
                        packet: Some(hostbound::Packet::Response(hostbound::Response {
                            nonce: Some(0xbeef),
                            response: Some(hostbound::response::Response::RunProcess(response)),
                        })),
                    };

                    host_sender.send(match msg.encode_fig_protobuf() {
                        Ok(encoded_message) => encoded_message,
                        Err(err) => {
                            error!(%err, "Failed to encode message");
                            return Err(err.into());
                        },
                    })?;

                    return Ok(None);
                } else {
                    bail!("invalid response type");
                }
            },
            ClientboundRequest::PseudoterminalExecute(PseudoterminalExecuteRequest {
                command,
                working_directory,
                background_job,
                is_pipelined,
                env,
            }) => {
                let (message, rx) = FigtermCommand::pseudoterminal_execute(
                    command,
                    working_directory,
                    background_job,
                    is_pipelined,
                    env,
                );

                let session_sender = &state.most_recent().context("most recent 3")?.sender;
                session_sender.send(message)?;

                let response = timeout(Duration::from_secs(10), rx)
                    .await
                    .context("Qterm response timed out after 10 sec")?
                    .context("Qterm response failed to receive from sender")?;

                if let hostbound::response::Response::PseudoterminalExecute(response) = response {
                    let msg = Hostbound {
                        packet: Some(hostbound::Packet::Response(hostbound::Response {
                            nonce: Some(0xbeef),
                            response: Some(hostbound::response::Response::PseudoterminalExecute(response)),
                        })),
                    };

                    host_sender.send(match msg.encode_fig_protobuf() {
                        Ok(encoded_message) => encoded_message,
                        Err(err) => {
                            error!(%err, "Failed to encode message");
                            return Err(err.into());
                        },
                    })?;

                    return Ok(None);
                } else {
                    bail!("invalid response type");
                }
            },
            _ => bail!("INVALID REQUEST"),
        },
        _ => {
            error!("Invalid packet: {packet:?}");
            return Ok(None);
        },
    }))
}

struct SimpleHookHandler {
    sender: UnboundedSender<Bytes>,
}

impl SimpleHookHandler {
    fn resererialize_send(&mut self, message: hostbound::request::Request) -> eyre::Result<()> {
        info!("sending on sender");

        let hostbound = Hostbound {
            packet: Some(hostbound::Packet::Request(hostbound::Request {
                nonce: Some(0xbeef),
                request: Some(message),
            })),
        };

        self.sender.send(match hostbound.encode_fig_protobuf() {
            Ok(encoded_message) => encoded_message,
            Err(err) => {
                error!("Failed to encode message: {err:?}");
                return Err(err.into());
            },
        })?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl fig_remote_ipc::RemoteHookHandler for SimpleHookHandler {
    type Error = eyre::Error;

    async fn edit_buffer(
        &mut self,
        edit_buffer_hook: &EditBufferHook,
        _session_id: &FigtermSessionId,
        _figterm_state: &Arc<FigtermState>,
    ) -> Result<Option<ClientboundResponse>, Self::Error> {
        self.resererialize_send(hostbound::request::Request::EditBuffer(edit_buffer_hook.clone()))?;
        Ok(None)
    }

    async fn prompt(
        &mut self,
        prompt_hook: &PromptHook,
        _session_id: &FigtermSessionId,
        _figterm_state: &Arc<FigtermState>,
    ) -> Result<Option<ClientboundResponse>, Self::Error> {
        self.resererialize_send(hostbound::request::Request::Prompt(prompt_hook.clone()))?;
        Ok(None)
    }

    async fn pre_exec(
        &mut self,
        pre_exec_hook: &PreExecHook,
        _session_id: &FigtermSessionId,
        _figterm_state: &Arc<FigtermState>,
    ) -> Result<Option<ClientboundResponse>, Self::Error> {
        self.resererialize_send(hostbound::request::Request::PreExec(pre_exec_hook.clone()))?;
        Ok(None)
    }

    async fn post_exec(
        &mut self,
        post_exec_hook: &PostExecHook,
        _session_id: &FigtermSessionId,
        _figterm_state: &Arc<FigtermState>,
    ) -> Result<Option<ClientboundResponse>, Self::Error> {
        self.resererialize_send(hostbound::request::Request::PostExec(post_exec_hook.clone()))?;
        Ok(None)
    }

    async fn intercepted_key(
        &mut self,
        intercepted_key: InterceptedKeyHook,
    ) -> Result<Option<ClientboundResponse>, Self::Error> {
        self.resererialize_send(hostbound::request::Request::InterceptedKey(intercepted_key.clone()))?;
        Ok(None)
    }

    async fn account_info(&mut self) -> Result<Option<ClientboundResponse>, Self::Error> {
        Err(anyhow!("account info not implemented"))
    }

    async fn start_exchange_credentials(&mut self) -> Result<Option<ClientboundResponse>, Self::Error> {
        Err(anyhow!("start_exchange_credentials not implemented"))
    }

    async fn confirm_exchange_credentials(&mut self) -> Result<Option<ClientboundResponse>, Self::Error> {
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
            ClientboundRequest::Intercept(InterceptRequest {
                intercept_command: Some(InterceptCommand::SetFigjsIntercepts(SetFigjsIntercepts {
                    intercept_bound_keystrokes: false,
                    intercept_global_keystrokes: false,
                    actions: vec![],
                    override_actions: false,
                })),
            }),
            ClientboundRequest::Intercept(InterceptRequest {
                intercept_command: Some(InterceptCommand::SetFigjsVisible(SetFigjsVisible { visible: false })),
            }),
            ClientboundRequest::InsertText(InsertTextRequest {
                insertion: None,
                deletion: None,
                offset: None,
                immediate: None,
                insertion_buffer: None,
                insert_during_command: None,
            }),
            ClientboundRequest::SetBuffer(SetBufferRequest {
                text: "text".into(),
                cursor_position: None,
            }),
        ];

        for message in messages {
            let state = Arc::new(FigtermState::new());
            let (sender, _) = mpsc::unbounded_channel();
            let message = Clientbound {
                packet: Some(clientbound::Packet::Request(clientbound::Request {
                    request: Some(message),
                    nonce: None,
                })),
            };

            let result = handle_client_bound_message(message, &state, &sender).await;

            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_simple_hook_handler_resererialize_send() {
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let mut handler = SimpleHookHandler { sender };

        let message = hostbound::request::Request::EditBuffer(EditBufferHook {
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
        handler.resererialize_send(message).unwrap();

        let received = receiver.try_recv().unwrap();
        println!("{received:?}");
        assert!(!received.is_empty());

        // let a: Hostbound = FigMessage {
        //     inner: Bytes::from(received),
        //     message_type: FigMessageType::Protobuf,
        // }
        // .decode()
        // .unwrap();

        // println!("{a:?}")
    }
}
