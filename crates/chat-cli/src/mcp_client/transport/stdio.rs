use std::sync::Arc;

use tokio::io::{
    AsyncBufReadExt,
    AsyncRead,
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
    Listener,
    LogListener,
    Transport,
    TransportError,
};

#[derive(Debug)]
pub enum JsonRpcStdioTransport {
    Client {
        stdin: Arc<Mutex<ChildStdin>>,
        receiver: broadcast::Receiver<Result<JsonRpcMessage, TransportError>>,
        log_receiver: broadcast::Receiver<String>,
    },
    Server {
        stdout: Arc<Mutex<Stdout>>,
        receiver: broadcast::Receiver<Result<JsonRpcMessage, TransportError>>,
    },
}

impl JsonRpcStdioTransport {
    fn spawn_reader<R: AsyncRead + Unpin + Send + 'static>(
        reader: R,
        tx: broadcast::Sender<Result<JsonRpcMessage, TransportError>>,
    ) {
        tokio::spawn(async move {
            let mut buffer = Vec::<u8>::new();
            let mut buf_reader = BufReader::new(reader);
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
    }

    pub fn client(child_process: Child) -> Result<Self, TransportError> {
        let (tx, receiver) = broadcast::channel::<Result<JsonRpcMessage, TransportError>>(100);
        let Some(stdout) = child_process.stdout else {
            return Err(TransportError::Custom("No stdout found on child process".to_owned()));
        };
        let Some(stdin) = child_process.stdin else {
            return Err(TransportError::Custom("No stdin found on child process".to_owned()));
        };
        let Some(stderr) = child_process.stderr else {
            return Err(TransportError::Custom("No stderr found on child process".to_owned()));
        };
        let (log_tx, log_receiver) = broadcast::channel::<String>(100);
        tokio::task::spawn(async move {
            let stderr = tokio::io::BufReader::new(stderr);
            let mut lines = stderr.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = log_tx.send(line);
            }
        });
        let stdin = Arc::new(Mutex::new(stdin));
        Self::spawn_reader(stdout, tx);
        Ok(JsonRpcStdioTransport::Client {
            stdin,
            receiver,
            log_receiver,
        })
    }

    pub fn server(stdin: Stdin, stdout: Stdout) -> Result<Self, TransportError> {
        let (tx, receiver) = broadcast::channel::<Result<JsonRpcMessage, TransportError>>(100);
        Self::spawn_reader(stdin, tx);
        let stdout = Arc::new(Mutex::new(stdout));
        Ok(JsonRpcStdioTransport::Server { stdout, receiver })
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

    fn get_listener(&self) -> impl Listener {
        match self {
            JsonRpcStdioTransport::Client { receiver, .. } | JsonRpcStdioTransport::Server { receiver, .. } => {
                StdioListener {
                    receiver: receiver.resubscribe(),
                }
            },
        }
    }

    async fn shutdown(&self) -> Result<(), TransportError> {
        match self {
            JsonRpcStdioTransport::Client { stdin, .. } => {
                let mut stdin = stdin.lock().await;
                Ok(stdin.shutdown().await?)
            },
            JsonRpcStdioTransport::Server { stdout, .. } => {
                let mut stdout = stdout.lock().await;
                Ok(stdout.shutdown().await?)
            },
        }
    }

    fn get_log_listener(&self) -> impl LogListener {
        match self {
            JsonRpcStdioTransport::Client { log_receiver, .. } => StdioLogListener {
                receiver: log_receiver.resubscribe(),
            },
            JsonRpcStdioTransport::Server { .. } => unreachable!("server does not need a log listener"),
        }
    }
}

pub struct StdioListener {
    pub receiver: broadcast::Receiver<Result<JsonRpcMessage, TransportError>>,
}

#[async_trait::async_trait]
impl Listener for StdioListener {
    async fn recv(&mut self) -> Result<JsonRpcMessage, TransportError> {
        self.receiver.recv().await?
    }
}

pub struct StdioLogListener {
    pub receiver: broadcast::Receiver<String>,
}

#[async_trait::async_trait]
impl LogListener for StdioLogListener {
    async fn recv(&mut self) -> Result<String, TransportError> {
        Ok(self.receiver.recv().await?)
    }
}

#[cfg(test)]
mod tests {
    use std::process::Stdio;

    use serde_json::{
        Value,
        json,
    };
    use tokio::process::Command;

    use super::*;

    // Helpers for testing
    fn create_test_message() -> JsonRpcMessage {
        serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "test_method",
            "params": {
                "test_param": "test_value"
            }
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn test_client_transport() {
        let mut cmd = Command::new("cat");
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());

        // Inject our mock transport instead
        let child = cmd.spawn().expect("Failed to spawn command");
        let transport = JsonRpcStdioTransport::client(child).expect("Failed to create client transport");

        let message = create_test_message();
        let result = transport.send(&message).await;
        assert!(result.is_ok(), "Failed to send message: {:?}", result);

        let echo = transport
            .get_listener()
            .recv()
            .await
            .expect("Failed to receive message");
        let echo_value = serde_json::to_value(&echo).expect("Failed to convert echo to value");
        let message_value = serde_json::to_value(&message).expect("Failed to convert message to value");
        assert!(are_json_values_equal(&echo_value, &message_value));
    }

    fn are_json_values_equal(a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(a_val), Value::Bool(b_val)) => a_val == b_val,
            (Value::Number(a_val), Value::Number(b_val)) => a_val == b_val,
            (Value::String(a_val), Value::String(b_val)) => a_val == b_val,
            (Value::Array(a_arr), Value::Array(b_arr)) => {
                if a_arr.len() != b_arr.len() {
                    return false;
                }
                a_arr
                    .iter()
                    .zip(b_arr.iter())
                    .all(|(a_item, b_item)| are_json_values_equal(a_item, b_item))
            },
            (Value::Object(a_obj), Value::Object(b_obj)) => {
                if a_obj.len() != b_obj.len() {
                    return false;
                }
                a_obj.iter().all(|(key, a_value)| match b_obj.get(key) {
                    Some(b_value) => are_json_values_equal(a_value, b_value),
                    None => false,
                })
            },
            _ => false,
        }
    }
}
