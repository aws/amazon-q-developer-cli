use std::sync::Arc;

use tokio::io::{
    AsyncBufReadExt as _,
    AsyncWriteExt as _,
    BufReader,
    Stdin,
    Stdout,
};
use tokio::process::{
    Child,
    ChildStdin,
};
use tokio::sync::{
    Mutex,
    broadcast,
};

use super::base_protocol::JsonRpcMessage;
use super::{
    Transport,
    TransportError,
};

#[derive(Debug)]
pub enum JsonRpcStdioTransport {
    Client {
        stdin: Arc<Mutex<ChildStdin>>,
        exclusive_receiver: Arc<Mutex<broadcast::Receiver<Result<JsonRpcMessage, TransportError>>>>,
        shared_receiver: broadcast::Receiver<Result<JsonRpcMessage, TransportError>>,
    },
    Server {
        stdout: Arc<Mutex<Stdout>>,
        exclusive_receiver: Arc<Mutex<broadcast::Receiver<Result<JsonRpcMessage, TransportError>>>>,
        shared_receiver: broadcast::Receiver<Result<JsonRpcMessage, TransportError>>,
    },
}

impl JsonRpcStdioTransport {
    pub fn client(child_process: Child) -> Result<Self, TransportError> {
        let (tx, receiver) = broadcast::channel::<Result<JsonRpcMessage, TransportError>>(100);
        let exclusive_receiver = Arc::new(Mutex::new(receiver));
        let shared_receiver = tx.subscribe();
        let Some(stdout) = child_process.stdout else {
            return Err(TransportError::Custom("No stdout found on child process".to_owned()));
        };
        let Some(stdin) = child_process.stdin else {
            return Err(TransportError::Custom("No stdin found on child process".to_owned()));
        };
        let stdin = Arc::new(Mutex::new(stdin));
        tokio::spawn(async move {
            let mut buffer = Vec::<u8>::new();
            let mut buf_reader = BufReader::new(stdout);
            loop {
                buffer.clear();
                // Messages are delimited by newlines and assumed to contain no embedded newlines
                // See https://spec.modelcontextprotocol.io/specification/2024-11-05/basic/transports/#stdio
                match buf_reader.read_until(b'\n', &mut buffer).await {
                    Ok(0) => continue,
                    Ok(_) => match serde_json::from_slice::<JsonRpcMessage>(buffer.as_slice()) {
                        Ok(msg) => {
                            let _ = tx.send(Ok(msg));
                        },
                        Err(e) => {
                            let _ = tx.send(Err(e.into()));
                        },
                    },
                    Err(e) => {
                        let _ = tx.send(Err(e.into()));
                    },
                }
            }
        });
        Ok(JsonRpcStdioTransport::Client {
            stdin,
            exclusive_receiver,
            shared_receiver,
        })
    }

    pub fn server(stdin: Stdin, stdout: Stdout) -> Result<Self, TransportError> {
        let (tx, receiver) = broadcast::channel::<Result<JsonRpcMessage, TransportError>>(100);
        let exclusive_receiver = Arc::new(Mutex::new(receiver));
        let shared_receiver = tx.subscribe();
        tokio::spawn(async move {
            let mut buffer = Vec::<u8>::new();
            let mut buf_reader = BufReader::new(stdin);
            loop {
                buffer.clear();
                // Messages are delimited by newlines and assumed to contain no embedded newlines
                // See https://spec.modelcontextprotocol.io/specification/2024-11-05/basic/transports/#stdio
                match buf_reader.read_until(b'\n', &mut buffer).await {
                    Ok(0) => continue,
                    Ok(_) => match serde_json::from_slice::<JsonRpcMessage>(buffer.as_slice()) {
                        Ok(msg) => {
                            let _ = tx.send(Ok(msg));
                        },
                        Err(e) => {
                            let _ = tx.send(Err(e.into()));
                        },
                    },
                    Err(e) => {
                        let _ = tx.send(Err(e.into()));
                    },
                }
            }
        });
        let stdout = Arc::new(Mutex::new(stdout));
        Ok(JsonRpcStdioTransport::Server {
            stdout,
            exclusive_receiver,
            shared_receiver,
        })
    }
}

#[async_trait::async_trait]
impl Transport for JsonRpcStdioTransport {
    async fn send(&self, msg: &JsonRpcMessage) -> Result<(), TransportError> {
        match self {
            JsonRpcStdioTransport::Client { stdin, .. } => {
                let mut serialized = serde_json::to_vec(msg)?;
                serialized.push(b'\n');
                let mut stdin = stdin.lock().await;
                stdin
                    .write_all(&serialized)
                    .await
                    .map_err(|e| TransportError::Custom(format!("Error writing to server: {:?}", e)))?;
                stdin
                    .flush()
                    .await
                    .map_err(|e| TransportError::Custom(format!("Error writing to server: {:?}", e)))?;
                Ok(())
            },
            JsonRpcStdioTransport::Server { stdout, .. } => {
                let mut serialized = serde_json::to_vec(msg)?;
                serialized.push(b'\n');
                let mut stdout = stdout.lock().await;
                stdout
                    .write_all(&serialized)
                    .await
                    .map_err(|e| TransportError::Custom(format!("Error writing to client: {:?}", e)))?;
                stdout
                    .flush()
                    .await
                    .map_err(|e| TransportError::Custom(format!("Error writing to client: {:?}", e)))?;
                Ok(())
            },
        }
    }

    async fn listen(&self) -> Result<JsonRpcMessage, TransportError> {
        match self {
            JsonRpcStdioTransport::Client { shared_receiver, .. }
            | JsonRpcStdioTransport::Server { shared_receiver, .. } => {
                let mut rx = shared_receiver.resubscribe();
                rx.recv().await?
            },
        }
    }

    async fn monitor(&self) -> Result<JsonRpcMessage, TransportError> {
        match self {
            JsonRpcStdioTransport::Client { exclusive_receiver, .. }
            | JsonRpcStdioTransport::Server { exclusive_receiver, .. } => {
                exclusive_receiver.lock().await.recv().await?
            },
        }
    }
}
