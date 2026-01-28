//! IPC Server for test mode communication with external test processes.
//!
//! Spawned by `SessionManager` in test mode. Routes mock API responses to the
//! `MockResponseRegistry` which distributes them to the appropriate session.

use eyre::Result;
use serde::{
    Deserialize,
    Serialize,
};
use tokio::io::{
    AsyncBufReadExt,
    AsyncWriteExt,
    BufReader,
};
use tokio::net::UnixStream;
use tracing::{
    error,
    info,
};

use crate::api_client::MockResponseRegistryHandle;
use crate::api_client::model::ConversationState;
use crate::api_client::send_message_output::MockStreamItem;

/// Test command from external test process
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum TestCommand {
    /// Push mock send_message response items for a session.
    /// None signals end of response stream.
    #[serde(rename = "PUSH_SEND_MESSAGE_RESPONSE")]
    PushSendMessageResponse {
        session_id: String,
        events: Option<Vec<MockStreamItem>>,
    },
    /// Get captured LLM requests for a session.
    #[serde(rename = "GET_CAPTURED_REQUESTS")]
    GetCapturedRequests { session_id: String },
}

/// Test response to external test process
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum TestResponse {
    #[serde(rename = "PUSH_SEND_MESSAGE_RESPONSE")]
    PushSendMessageResponse,
    #[serde(rename = "GET_CAPTURED_REQUESTS")]
    GetCapturedRequests { requests: Vec<ConversationState> },
    #[serde(rename = "ERROR")]
    Error { error: String },
}

/// Message type discriminator
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageKind {
    Command,
    Response,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TestMessageCommand {
    pub id: String,
    #[serde(rename = "kind")]
    pub msg_kind: MessageKind,
    pub data: TestCommand,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TestMessageResponse {
    pub id: String,
    #[serde(rename = "kind")]
    pub msg_kind: MessageKind,
    pub data: TestResponse,
}

/// IPC Server that routes test commands to the MockResponseRegistry.
pub struct IpcServer;

impl IpcServer {
    /// Spawn the IPC server, routing mock responses to the registry.
    pub fn spawn(registry: MockResponseRegistryHandle) -> Result<()> {
        let socket_path = std::env::var("KIRO_TEST_CHAT_IPC_SOCKET_PATH")
            .map_err(|_e| eyre::eyre!("KIRO_TEST_CHAT_IPC_SOCKET_PATH not set"))?;

        tokio::spawn(async move {
            if let Err(e) = Self::run(socket_path, registry).await {
                error!("IPC server error: {}", e);
            }
        });

        Ok(())
    }

    async fn run(socket_path: String, registry: MockResponseRegistryHandle) -> Result<()> {
        let stream = UnixStream::connect(&socket_path).await?;
        info!("IPC server connected to {}", socket_path);

        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        while reader.read_line(&mut line).await? > 0 {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                line.clear();
                continue;
            }

            let response = match serde_json::from_str::<TestMessageCommand>(trimmed) {
                Ok(msg) => {
                    let response_data = match msg.data {
                        TestCommand::PushSendMessageResponse { session_id, events } => {
                            registry.push_events(session_id, events).await;
                            TestResponse::PushSendMessageResponse
                        },
                        TestCommand::GetCapturedRequests { session_id } => {
                            let requests = registry.get_captured_requests(&session_id).await;
                            TestResponse::GetCapturedRequests { requests }
                        },
                    };
                    TestMessageResponse {
                        id: msg.id,
                        msg_kind: MessageKind::Response,
                        data: response_data,
                    }
                },
                Err(e) => TestMessageResponse {
                    id: "unknown".into(),
                    msg_kind: MessageKind::Response,
                    data: TestResponse::Error {
                        error: format!("Failed to parse: {}", e),
                    },
                },
            };

            let response_json = serde_json::to_string(&response)?;
            writer.write_all(response_json.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;

            line.clear();
        }

        Ok(())
    }
}
