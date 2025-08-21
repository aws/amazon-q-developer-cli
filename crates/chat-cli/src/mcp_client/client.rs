use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{
    AtomicBool,
    AtomicU64,
    Ordering,
};
use std::sync::{
    Arc,
    RwLock as SyncRwLock,
};
use std::time::Duration;

use serde::{
    Deserialize,
    Serialize,
};
use thiserror::Error;
use tokio::time;
use tokio::time::error::Elapsed;

use super::transport::base_protocol::{
    JsonRpcError,
    JsonRpcMessage,
    JsonRpcNotification,
    JsonRpcRequest,
    JsonRpcVersion,
};
use super::transport::stdio::JsonRpcStdioTransport;
use super::transport::{
    self,
    Transport,
    TransportError,
};
use super::{
    JsonRpcResponse,
    Listener as _,
    LogListener,
    Messenger,
    PaginationSupportedOps,
    PromptGet,
    PromptsListResult,
    ResourceTemplatesListResult,
    ResourcesListResult,
    ServerCapabilities,
    ToolsListResult,
};
use crate::api_client::model::{
    ChatMessage,
    ConversationState,
    UserInputMessage,
};
use crate::api_client::{
    ApiClient,
    ApiClientError,
};
use crate::util::process::{
    Pid,
    terminate_process,
};

pub type ClientInfo = serde_json::Value;
pub type StdioTransport = JsonRpcStdioTransport;

/// Represents the capabilities of a client in the Model Context Protocol.
/// This structure is sent to the server during initialization to communicate
/// what features the client supports and provide information about the client.
/// When features are added to the client, these should be declared in the [From] trait implemented
/// for the struct.
#[derive(Default, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ClientCapabilities {
    protocol_version: JsonRpcVersion,
    capabilities: HashMap<String, serde_json::Value>,
    client_info: serde_json::Value,
}

impl From<ClientInfo> for ClientCapabilities {
    fn from(client_info: ClientInfo) -> Self {
        let mut capabilities = HashMap::new();

        // Add sampling capability support
        capabilities.insert("sampling".to_string(), serde_json::json!({}));

        ClientCapabilities {
            client_info,
            capabilities,
            ..Default::default()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ClientConfig {
    pub server_name: String,
    pub bin_path: String,
    pub args: Vec<String>,
    pub timeout: u64,
    pub client_info: serde_json::Value,
    pub env: Option<HashMap<String, String>>,
    pub sampling_enabled: bool,
}

#[allow(dead_code)]
#[derive(Debug, Error)]
pub enum ClientError {
    #[error(transparent)]
    TransportError(#[from] TransportError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
    #[error(transparent)]
    ApiClient(#[from] Box<ApiClientError>),
    #[error("Operation timed out: {context}")]
    RuntimeError {
        #[source]
        source: tokio::time::error::Elapsed,
        context: String,
    },
    #[error("Unexpected msg type encountered")]
    UnexpectedMsgType,
    #[error("{0}")]
    NegotiationError(String),
    #[error("Failed to obtain process id")]
    MissingProcessId,
    #[error("Invalid path received")]
    InvalidPath,
    #[error("{0}")]
    ProcessKillError(String),
    #[error("{0}")]
    PoisonError(String),
}

impl From<(tokio::time::error::Elapsed, String)> for ClientError {
    fn from((error, context): (tokio::time::error::Elapsed, String)) -> Self {
        ClientError::RuntimeError { source: error, context }
    }
}

#[derive(Debug)]
pub struct Client<T: Transport> {
    server_name: String,
    transport: Arc<T>,
    timeout: u64,
    pub server_process_id: Option<Pid>,
    client_info: serde_json::Value,
    current_id: Arc<AtomicU64>,
    pub messenger: Option<Box<dyn Messenger>>,
    // TODO: move this to tool manager that way all the assets are treated equally
    pub prompt_gets: Arc<SyncRwLock<HashMap<String, PromptGet>>>,
    pub is_prompts_out_of_date: Arc<AtomicBool>,
    sampling_enabled: bool,
    api_client: Option<Arc<ApiClient>>,
}

impl<T: Transport> Clone for Client<T> {
    fn clone(&self) -> Self {
        Self {
            server_name: self.server_name.clone(),
            transport: self.transport.clone(),
            timeout: self.timeout,
            // Note that we cannot have an id for the clone because we would kill the original
            // process when we drop the clone
            server_process_id: None,
            client_info: self.client_info.clone(),
            current_id: self.current_id.clone(),
            messenger: None,
            prompt_gets: self.prompt_gets.clone(),
            is_prompts_out_of_date: self.is_prompts_out_of_date.clone(),
            sampling_enabled: self.sampling_enabled,
            api_client: self.api_client.clone(),
        }
    }
}

impl Client<StdioTransport> {
    pub fn from_config(config: ClientConfig, api_client: Option<Arc<ApiClient>>) -> Result<Self, ClientError> {
        let ClientConfig {
            server_name,
            bin_path,
            args,
            timeout,
            client_info,
            env,
            sampling_enabled,
        } = config;
        let child = {
            let expanded_bin_path = shellexpand::tilde(&bin_path);

            // On Windows, we need to use cmd.exe to run the binary with arguments because Tokio
            // always assumes that the program has an .exe extension, which is not the case for
            // helpers like `uvx` or `npx`.
            let mut command = if cfg!(windows) {
                let mut cmd = tokio::process::Command::new("cmd.exe");
                cmd.args(["/C", &Self::build_windows_command(&expanded_bin_path, args)]);
                cmd
            } else {
                let mut cmd = tokio::process::Command::new(expanded_bin_path.to_string());
                cmd.args(args);
                cmd
            };

            command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .envs(std::env::vars());

            #[cfg(not(windows))]
            command.process_group(0);

            if let Some(env) = env {
                for (env_name, env_value) in env {
                    command.env(env_name, env_value);
                }
            }

            command.spawn()?
        };

        let server_process_id = child.id().ok_or(ClientError::MissingProcessId)?;
        let server_process_id = Some(Pid::from_u32(server_process_id));

        let transport = Arc::new(transport::stdio::JsonRpcStdioTransport::client(child)?);
        
        
        Ok(Self {
            server_name,
            transport,
            timeout,
            server_process_id,
            client_info,
            current_id: Arc::new(AtomicU64::new(0)),
            messenger: None,
            prompt_gets: Arc::new(SyncRwLock::new(HashMap::new())),
            is_prompts_out_of_date: Arc::new(AtomicBool::new(false)),
            sampling_enabled,
            api_client,
        })
    }

    fn build_windows_command(bin_path: &str, args: Vec<String>) -> String {
        let mut parts = Vec::new();

        // Add the binary path, quoted if necessary
        parts.push(Self::quote_windows_arg(bin_path));

        // Add all arguments, quoted if necessary
        for arg in args {
            parts.push(Self::quote_windows_arg(&arg));
        }

        parts.join(" ")
    }

    fn quote_windows_arg(arg: &str) -> String {
        // If the argument doesn't need quoting, return as-is
        if !arg.chars().any(|c| " \t\n\r\"".contains(c)) {
            return arg.to_string();
        }

        let mut result = String::from("\"");
        let mut backslashes = 0;

        for c in arg.chars() {
            match c {
                '\\' => {
                    backslashes += 1;
                    result.push('\\');
                },
                '"' => {
                    // Escape all preceding backslashes and the quote
                    for _ in 0..backslashes {
                        result.push('\\');
                    }
                    result.push_str("\\\"");
                    backslashes = 0;
                },
                _ => {
                    backslashes = 0;
                    result.push(c);
                },
            }
        }

        // Escape trailing backslashes before the closing quote
        for _ in 0..backslashes {
            result.push('\\');
        }

        result.push('"');
        result
    }
}

impl<T> Drop for Client<T>
where
    T: Transport,
{
    // IF the servers are implemented well, they will shutdown once the pipe closes.
    // This drop trait is here as a fail safe to ensure we don't leave behind any orphans.
    fn drop(&mut self) {
        if let Some(process_id) = self.server_process_id {
            let _ = terminate_process(process_id);
        }
        if let Some(ref messenger) = self.messenger {
            messenger.send_deinit_msg();
        }
    }
}

impl<T> Client<T>
where
    T: Transport,
{
    /// Exchange of information specified as per https://spec.modelcontextprotocol.io/specification/2024-11-05/basic/lifecycle/#initialization
    ///
    /// Also done are the following:
    /// - Spawns task for listening to server driven workflows
    /// - Spawns tasks to ask for relevant info such as tools and prompts in accordance to server
    ///   capabilities received
    pub async fn init(&self) -> Result<ServerCapabilities, ClientError> {
        let transport_ref = self.transport.clone();
        let server_name = self.server_name.clone();

        // Spawning a task to listen and log stderr output
        tokio::spawn(async move {
            let mut log_listener = transport_ref.get_log_listener();
            loop {
                match log_listener.recv().await {
                    Ok(msg) => {
                        tracing::trace!(target: "mcp", "{server_name} logged {}", msg);
                    },
                    Err(e) => {
                        tracing::error!(
                            "Error encountered while reading from stderr for {server_name}: {:?}\nEnding stderr listening task.",
                            e
                        );
                        break;
                    },
                }
            }
        });

        let init_params = Some({
            let client_cap = ClientCapabilities::from(self.client_info.clone());
            serde_json::json!(client_cap)
        });
        let init_resp = self.request("initialize", init_params).await?;
        if let Err(e) = examine_server_capabilities(&init_resp) {
            return Err(ClientError::NegotiationError(format!(
                "Client {} has failed to negotiate server capabilities with server: {:?}",
                self.server_name, e
            )));
        }
        let cap = {
            let result = init_resp.result.ok_or(ClientError::NegotiationError(format!(
                "Server {} init resp is missing result",
                self.server_name
            )))?;
            let cap = result
                .get("capabilities")
                .ok_or(ClientError::NegotiationError(format!(
                    "Server {} init resp result is missing capabilities",
                    self.server_name
                )))?
                .clone();
            serde_json::from_value::<ServerCapabilities>(cap)?
        };
        self.notify("initialized", None).await?;

        // TODO: group this into examine_server_capabilities
        // Prefetch prompts in the background. We should only do this after the server has been
        // initialized
        if cap.prompts.is_some() {
            self.is_prompts_out_of_date.store(true, Ordering::Relaxed);
            let client_ref = (*self).clone();
            let messenger_ref = self.messenger.as_ref().map(|m| m.duplicate());
            tokio::spawn(async move {
                fetch_prompts_and_notify_with_messenger(&client_ref, messenger_ref.as_ref()).await;
            });
        }
        if cap.tools.is_some() {
            let client_ref = (*self).clone();
            let messenger_ref = self.messenger.as_ref().map(|m| m.duplicate());
            tokio::spawn(async move {
                fetch_tools_and_notify_with_messenger(&client_ref, messenger_ref.as_ref()).await;
            });
        }

        let transport_ref = self.transport.clone();
        let server_name = self.server_name.clone();
        let messenger_ref = self.messenger.as_ref().map(|m| m.duplicate());
        let client_ref = (*self).clone();

        let prompts_list_changed_supported = cap.prompts.as_ref().is_some_and(|p| p.get("listChanged").is_some());
        let tools_list_changed_supported = cap.tools.as_ref().is_some_and(|t| t.get("listChanged").is_some());
        tokio::spawn(async move {
            let mut listener = transport_ref.get_listener();
            loop {
                match listener.recv().await {
                    Ok(msg) => {
                        match msg {
                            JsonRpcMessage::Request(req) => {
                                // Handle sampling requests from the server
                                if req.method == "sampling/createMessage" {
                                    let client_ref_inner = client_ref.clone();
                                    let transport_ref_inner = transport_ref.clone();
                                    tokio::spawn(async move {
                                        match client_ref_inner.handle_sampling_request(&req).await {
                                            Ok(response) => {
                                                let msg = JsonRpcMessage::Response(response);
                                                if let Err(e) = transport_ref_inner.send(&msg).await {
                                                    tracing::error!("Failed to send sampling response: {:?}", e);
                                                }
                                            },
                                            Err(e) => {
                                                tracing::error!("Failed to handle sampling request: {:?}", e);
                                                // Send error response
                                                let error_response = JsonRpcResponse {
                                                    jsonrpc: req.jsonrpc,
                                                    id: req.id,
                                                    result: None,
                                                    error: Some(super::transport::base_protocol::JsonRpcError {
                                                        code: -1,
                                                        message: format!("Sampling request failed: {}", e),
                                                        data: None,
                                                    }),
                                                };
                                                let msg = JsonRpcMessage::Response(error_response);
                                                if let Err(e) = transport_ref_inner.send(&msg).await {
                                                    tracing::error!("Failed to send error response: {:?}", e);
                                                }
                                            },
                                        }
                                    });
                                }
                                // Ignore other request types for now
                            },
                            JsonRpcMessage::Notification(notif) => {
                                let JsonRpcNotification { method, params, .. } = notif;
                                match method.as_str() {
                                    "notifications/message" | "message" => {
                                        let level = params
                                            .as_ref()
                                            .and_then(|p| p.get("level"))
                                            .and_then(|v| serde_json::to_string(v).ok());
                                        let data = params
                                            .as_ref()
                                            .and_then(|p| p.get("data"))
                                            .and_then(|v| serde_json::to_string(v).ok());
                                        if let (Some(level), Some(data)) = (level, data) {
                                            match level.to_lowercase().as_str() {
                                                "error" => {
                                                    tracing::error!(target: "mcp", "{}: {}", server_name, data);
                                                },
                                                "warn" => {
                                                    tracing::warn!(target: "mcp", "{}: {}", server_name, data);
                                                },
                                                "info" => {
                                                    tracing::info!(target: "mcp", "{}: {}", server_name, data);
                                                },
                                                "debug" => {
                                                    tracing::debug!(target: "mcp", "{}: {}", server_name, data);
                                                },
                                                "trace" => {
                                                    tracing::trace!(target: "mcp", "{}: {}", server_name, data);
                                                },
                                                _ => {},
                                            }
                                        }
                                    },
                                    "notifications/prompts/list_changed" | "prompts/list_changed"
                                        if prompts_list_changed_supported =>
                                    {
                                        // TODO: after we have moved the prompts to the tool
                                        // manager we follow the same workflow as the list changed
                                        // for tools
                                        fetch_prompts_and_notify_with_messenger(&client_ref, messenger_ref.as_ref())
                                            .await;
                                        client_ref.is_prompts_out_of_date.store(true, Ordering::Release);
                                    },
                                    "notifications/tools/list_changed" | "tools/list_changed"
                                        if tools_list_changed_supported =>
                                    {
                                        // Add a small delay to prevent rapid-fire loops
                                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                                        fetch_tools_and_notify_with_messenger(&client_ref, messenger_ref.as_ref())
                                            .await;
                                    },
                                    _ => {},
                                }
                            },
                            JsonRpcMessage::Response(_resp) => { /* noop since direct response is handled inside the request api */
                            },
                        }
                    },
                    Err(e) => {
                        tracing::error!("Background listening thread for client {}: {:?}", server_name, e);
                        // If we don't have anything on the other end, we should just end the task
                        // now
                        if let TransportError::RecvError(tokio::sync::broadcast::error::RecvError::Closed) = e {
                            tracing::error!(
                                "All senders dropped for transport layer for server {}: {:?}. This likely means the mcp server process is no longer running.",
                                server_name,
                                e
                            );
                            break;
                        }
                    },
                }
            }
        });

        Ok(cap)
    }

    /// Sends a request to the server associated.
    /// This call will yield until a response is received.
    pub async fn request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<JsonRpcResponse, ClientError> {
        let send_map_err = |e: Elapsed| (e, method.to_string());
        let recv_map_err = |e: Elapsed| (e, format!("recv for {method}"));
        let mut id = self.get_id();
        let request = JsonRpcRequest {
            jsonrpc: JsonRpcVersion::default(),
            id,
            method: method.to_owned(),
            params,
        };
        tracing::trace!(target: "mcp", "To {}:\n{:#?}", self.server_name, request);
        let msg = JsonRpcMessage::Request(request);
        time::timeout(Duration::from_millis(self.timeout), self.transport.send(&msg))
            .await
            .map_err(send_map_err)??;
        let mut listener = self.transport.get_listener();
        let mut resp = time::timeout(Duration::from_millis(self.timeout), async {
            // we want to ignore all other messages sent by the server at this point and let the
            // background loop handle them
            // We also want to ignore all messages emitted by the server to its stdout that does
            // not deserialize into a valid JsonRpcMessage (they are not supposed to do this but
            // too many people complained about this so we are adding this safeguard in)
            loop {
                if let Ok(JsonRpcMessage::Response(resp)) = listener.recv().await {
                    if resp.id == id {
                        break Ok::<JsonRpcResponse, TransportError>(resp);
                    }
                }
            }
        })
        .await
        .map_err(recv_map_err)??;
        // Pagination support: https://spec.modelcontextprotocol.io/specification/2024-11-05/server/utilities/pagination/#pagination-model
        let mut next_cursor = resp.result.as_ref().and_then(|v| v.get("nextCursor"));
        if next_cursor.is_some() {
            let mut current_resp = resp.clone();
            let mut results = Vec::<serde_json::Value>::new();
            let pagination_supported_ops = {
                let maybe_pagination_supported_op: Result<PaginationSupportedOps, _> = method.try_into();
                maybe_pagination_supported_op.ok()
            };
            if let Some(ops) = pagination_supported_ops {
                loop {
                    let result = current_resp.result.as_ref().cloned()
                        .ok_or_else(|| ClientError::NegotiationError("Missing result in paginated response".to_string()))?;
                    let mut list: Vec<serde_json::Value> = match ops {
                        PaginationSupportedOps::ResourcesList => {
                            let ResourcesListResult { resources: list, .. } =
                                serde_json::from_value::<ResourcesListResult>(result)
                                    .map_err(ClientError::Serialization)?;
                            list
                        },
                        PaginationSupportedOps::ResourceTemplatesList => {
                            let ResourceTemplatesListResult {
                                resource_templates: list,
                                ..
                            } = serde_json::from_value::<ResourceTemplatesListResult>(result)
                                .map_err(ClientError::Serialization)?;
                            list
                        },
                        PaginationSupportedOps::PromptsList => {
                            let PromptsListResult { prompts: list, .. } =
                                serde_json::from_value::<PromptsListResult>(result)
                                    .map_err(ClientError::Serialization)?;
                            list
                        },
                        PaginationSupportedOps::ToolsList => {
                            let ToolsListResult { tools: list, .. } = serde_json::from_value::<ToolsListResult>(result)
                                .map_err(ClientError::Serialization)?;
                            list
                        },
                    };
                    results.append(&mut list);
                    if next_cursor.is_none() {
                        break;
                    }
                    id = self.get_id();
                    let next_request = JsonRpcRequest {
                        jsonrpc: JsonRpcVersion::default(),
                        id,
                        method: method.to_owned(),
                        params: Some(serde_json::json!({
                            "cursor": next_cursor,
                        })),
                    };
                    let msg = JsonRpcMessage::Request(next_request);
                    time::timeout(Duration::from_millis(self.timeout), self.transport.send(&msg))
                        .await
                        .map_err(send_map_err)??;
                    let resp = time::timeout(Duration::from_millis(self.timeout), async {
                        loop {
                            if let Ok(JsonRpcMessage::Response(resp)) = listener.recv().await {
                                if resp.id == id {
                                    break Ok::<JsonRpcResponse, TransportError>(resp);
                                }
                            }
                        }
                    })
                    .await
                    .map_err(recv_map_err)??;
                    current_resp = resp;
                    next_cursor = current_resp.result.as_ref().and_then(|v| v.get("nextCursor"));
                }
                resp.result = Some({
                    let mut map = serde_json::Map::new();
                    map.insert(ops.as_key().to_owned(), serde_json::to_value(results)?);
                    serde_json::to_value(map)?
                });
            }
        }
        tracing::trace!(target: "mcp", "From {}:\n{:#?}", self.server_name, resp);
        
        Ok(resp)
    }

    /// Sends a notification to the server associated.
    /// Notifications are requests that expect no responses.
    pub async fn notify(&self, method: &str, params: Option<serde_json::Value>) -> Result<(), ClientError> {
        let send_map_err = |e: Elapsed| (e, method.to_string());
        let notification = JsonRpcNotification {
            jsonrpc: JsonRpcVersion::default(),
            method: format!("notifications/{}", method),
            params,
        };
        let msg = JsonRpcMessage::Notification(notification);
        Ok(
            time::timeout(Duration::from_millis(self.timeout), self.transport.send(&msg))
                .await
                .map_err(send_map_err)??,
        )
    }

    /// Converts MCP sampling request to Amazon Q conversation format
    fn convert_sampling_to_conversation(
        sampling_request: &super::facilitator_types::SamplingCreateMessageRequest,
    ) -> ConversationState {
        use super::facilitator_types::{
            Role,
            SamplingContent,
        };

        // Convert messages to chat history
        let mut history = Vec::new();
        let mut user_message_content = String::new();

        for message in &sampling_request.messages {
            let content = match &message.content {
                SamplingContent::Text { text } => text.clone(),
                SamplingContent::Image { .. } => "[Image content not supported in sampling]".to_string(),
                SamplingContent::Audio { .. } => "[Audio content not supported in sampling]".to_string(),
            };

            match message.role {
                Role::User => {
                    if user_message_content.is_empty() {
                        user_message_content = content;
                    } else {
                        // If we have multiple user messages, combine them
                        user_message_content.push_str("\n\n");
                        user_message_content.push_str(&content);
                    }
                },
                Role::Assistant => {
                    // Add assistant message to history
                    history.push(ChatMessage::AssistantResponseMessage(
                        crate::api_client::model::AssistantResponseMessage {
                            message_id: None,
                            content,
                            tool_uses: None,
                        },
                    ));
                },
            }
        }

        // If we still don't have user content, use a default
        if user_message_content.is_empty() {
            user_message_content = "Please help me with this task.".to_string();
        }

        // For sampling requests, we need to preserve the exact format requested
        // The system prompt should be treated as instructions, not appended to user content
        let final_user_content = if let Some(system_prompt) = &sampling_request.system_prompt {
            // Combine system prompt and user message in a way that preserves the instruction format
            format!("{}\n\nUser request: {}", system_prompt, user_message_content)
        } else {
            user_message_content
        };

        let user_input_message = UserInputMessage {
            content: final_user_content,
            user_input_message_context: None,
            user_intent: None,
            images: None,
            model_id: sampling_request
                .model_preferences
                .as_ref()
                .and_then(|prefs| prefs.hints.as_ref())
                .and_then(|hints| hints.first())
                .map(|hint| hint.name.clone()),
        };

        ConversationState {
            conversation_id: None, // New conversation for sampling
            user_input_message,
            history: if history.is_empty() { None } else { Some(history) },
        }
    }

    /// Converts Amazon Q API response to MCP sampling response format
    async fn convert_api_response_to_sampling(
        &self,
        mut api_response: crate::api_client::send_message_output::SendMessageOutput,
    ) -> Result<super::facilitator_types::SamplingCreateMessageResponse, ClientError> {
        use super::facilitator_types::{
            Role,
            SamplingContent,
            SamplingCreateMessageResponse,
        };
        use crate::api_client::model::ChatResponseStream;

        let mut content_parts = Vec::new();
        

        // Collect all response events
        while let Some(event) = api_response
            .recv()
            .await
            .map_err(|e| ClientError::ApiClient(Box::new(e)))?
        {
            match event {
                ChatResponseStream::AssistantResponseEvent { content } => {
                    content_parts.push(content);
                },
                ChatResponseStream::CodeEvent { content } => {
                    content_parts.push(content);
                },
                ChatResponseStream::InvalidStateEvent { reason: _, message: _ } => {
                },
                ChatResponseStream::MessageMetadataEvent {
                    conversation_id: _,
                    utterance_id: _,
                } => {
                },
                _other => {
                },
            }
        }

        let response_text = if content_parts.is_empty() {
            "I apologize, but I couldn't generate a response for your request.".to_string()
        } else {
            content_parts.join("")
        };

        Ok(SamplingCreateMessageResponse {
            role: Role::Assistant,
            content: SamplingContent::Text { text: response_text },
            model: Some("amazon-q-cli".to_string()),
            stop_reason: Some("endTurn".to_string()),
        })
    }

    /// Handles sampling/createMessage requests from MCP servers
    /// This allows servers to request LLM completions through the client
    pub async fn handle_sampling_request(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse, ClientError> {
        // Validate sampling is enabled
        if let Some(error_response) = self.validate_sampling_enabled(request) {
            return Ok(error_response);
        }

        // Validate and parse the request
        let sampling_request = Self::parse_sampling_request(request)?;

        // Check API client availability and process request
        match &self.api_client {
            Some(api_client) => {
                self.process_sampling_with_api(request, &sampling_request, api_client).await
            },
            None => {
                Ok(Self::create_fallback_response(request))
            },
        }
    }

    /// Validates that sampling is enabled for this server
    fn validate_sampling_enabled(&self, request: &JsonRpcRequest) -> Option<JsonRpcResponse> {
        if !self.sampling_enabled {
            return Some(JsonRpcResponse {
                jsonrpc: JsonRpcVersion::default(),
                id: request.id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: "Sampling not enabled for this server. Add 'sampling: true' to server configuration.".to_string(),
                    data: None,
                }),
            });
        }
        None
    }

    /// Parses and validates the sampling request
    fn parse_sampling_request(request: &JsonRpcRequest) -> Result<super::facilitator_types::SamplingCreateMessageRequest, ClientError> {
        if request.method != "sampling/createMessage" {
            return Err(ClientError::NegotiationError(format!(
                "Unsupported sampling method: {}. Expected 'sampling/createMessage'",
                request.method
            )));
        }

        let params = request
            .params
            .as_ref()
            .ok_or_else(|| ClientError::NegotiationError("Missing parameters for sampling request".to_string()))?;

        serde_json::from_value(params.clone()).map_err(ClientError::Serialization)
    }

    /// Creates a fallback response when API client is unavailable
    fn create_fallback_response(request: &JsonRpcRequest) -> JsonRpcResponse {
        let response = super::facilitator_types::SamplingCreateMessageResponse {
            role: super::facilitator_types::Role::Assistant,
            content: super::facilitator_types::SamplingContent::Text {
                text: "API client not available for LLM sampling. Please ensure the MCP client is properly configured.".to_string(),
            },
            model: Some("amazon-q-cli".to_string()),
            stop_reason: Some("no_api_client".to_string()),
        };

        JsonRpcResponse {
            jsonrpc: request.jsonrpc.clone(),
            id: request.id,
            result: Some(Self::convert_sampling_response_to_json(&response)),
            error: None,
        }
    }

    /// Processes sampling request with API client
    async fn process_sampling_with_api(
        &self,
        request: &JsonRpcRequest,
        sampling_request: &super::facilitator_types::SamplingCreateMessageRequest,
        api_client: &Arc<ApiClient>,
    ) -> Result<JsonRpcResponse, ClientError> {
        // Convert sampling request to conversation format
        let conversation_state = Self::convert_sampling_to_conversation(sampling_request);

        // Make API call to Amazon Q
        match api_client.send_message(conversation_state).await {
            Ok(api_response) => {
                self.handle_successful_api_response(request, api_response).await
            },
            Err(api_error) => {
                Ok(Self::create_error_response(request, &format!("I encountered an error while processing your request: {}", api_error), "error"))
            },
        }
    }

    /// Handles successful API response and converts to MCP format
    async fn handle_successful_api_response(
        &self,
        request: &JsonRpcRequest,
        api_response: crate::api_client::send_message_output::SendMessageOutput,
    ) -> Result<JsonRpcResponse, ClientError> {
        match self.convert_api_response_to_sampling(api_response).await {
            Ok(sampling_response) => {
                Ok(JsonRpcResponse {
                    jsonrpc: request.jsonrpc.clone(),
                    id: request.id,
                    result: Some(Self::convert_sampling_response_to_json(&sampling_response)),
                    error: None,
                })
            },
            Err(conversion_error) => {
                Ok(Self::create_error_response(request, &format!("Error processing LLM response: {}", conversion_error), "conversion_error"))
            },
        }
    }

    /// Creates an error response in MCP sampling format
    fn create_error_response(request: &JsonRpcRequest, error_message: &str, stop_reason: &str) -> JsonRpcResponse {
        let error_response = super::facilitator_types::SamplingCreateMessageResponse {
            role: super::facilitator_types::Role::Assistant,
            content: super::facilitator_types::SamplingContent::Text {
                text: error_message.to_string(),
            },
            model: Some("amazon-q-cli".to_string()),
            stop_reason: Some(stop_reason.to_string()),
        };

        JsonRpcResponse {
            jsonrpc: request.jsonrpc.clone(),
            id: request.id,
            result: Some(Self::convert_sampling_response_to_json(&error_response)),
            error: None,
        }
    }

    /// Converts SamplingCreateMessageResponse to JSON format
    fn convert_sampling_response_to_json(response: &super::facilitator_types::SamplingCreateMessageResponse) -> serde_json::Value {
        let content_obj = match &response.content {
            super::facilitator_types::SamplingContent::Text { text } => {
                serde_json::json!({"type": "text", "text": text})
            },
            super::facilitator_types::SamplingContent::Image { data, mime_type } => {
                serde_json::json!({"type": "image", "data": data, "mimeType": mime_type})
            },
            super::facilitator_types::SamplingContent::Audio { data, mime_type } => {
                serde_json::json!({"type": "audio", "data": data, "mimeType": mime_type})
            },
        };
        
        serde_json::json!({
            "role": "assistant",
            "content": content_obj,
            "model": response.model.as_ref().unwrap_or(&"amazon-q-cli".to_string()),
            "stopReason": response.stop_reason.as_ref().unwrap_or(&"endTurn".to_string())
        })
    }

    fn get_id(&self) -> u64 {
        self.current_id.fetch_add(1, Ordering::SeqCst)
    }
}

fn examine_server_capabilities(ser_cap: &JsonRpcResponse) -> Result<(), ClientError> {
    // Check the jrpc version.
    // Currently we are only proceeding if the versions are EXACTLY the same.
    let jrpc_version = ser_cap.jsonrpc.as_u32_vec();
    let client_jrpc_version = JsonRpcVersion::default().as_u32_vec();
    for (sv, cv) in jrpc_version.iter().zip(client_jrpc_version.iter()) {
        if sv != cv {
            return Err(ClientError::NegotiationError(
                "Incompatible jrpc version between server and client".to_owned(),
            ));
        }
    }
    Ok(())
}

#[allow(clippy::borrowed_box)]
async fn fetch_prompts_and_notify_with_messenger<T>(client: &Client<T>, messenger: Option<&Box<dyn Messenger>>)
where
    T: Transport,
{
    let prompt_list_result = 'prompt_list_result: {
        let Ok(resp) = client.request("prompts/list", None).await else {
            tracing::error!("Prompt list query failed for {0}", client.server_name);
            return;
        };
        let Some(result) = resp.result else {
            tracing::warn!("Prompt list query returned no result for {0}", client.server_name);
            return;
        };
        let prompt_list_result = match serde_json::from_value::<PromptsListResult>(result) {
            Ok(res) => res,
            Err(e) => {
                let msg = format!("Failed to deserialize tool result from {}: {:?}", client.server_name, e);
                break 'prompt_list_result Err(eyre::eyre!(msg));
            },
        };
        Ok::<PromptsListResult, eyre::Report>(prompt_list_result)
    };

    if let Some(messenger) = messenger {
        if let Err(e) = messenger.send_prompts_list_result(prompt_list_result).await {
            tracing::error!("Failed to send prompt result through messenger: {:?}", e);
        }
    }
}

#[allow(clippy::borrowed_box)]
async fn fetch_tools_and_notify_with_messenger<T>(client: &Client<T>, messenger: Option<&Box<dyn Messenger>>)
where
    T: Transport,
{
    // TODO: decouple pagination logic from request and have page fetching logic here
    // instead
    let tool_list_result = 'tool_list_result: {
        let resp = match client.request("tools/list", None).await {
            Ok(resp) => resp,
            Err(e) => break 'tool_list_result Err(e.into()),
        };
        if let Some(error) = resp.error {
            let msg = format!("Failed to retrieve tool list for {}: {:?}", client.server_name, error);
            break 'tool_list_result Err(eyre::eyre!(msg));
        }
        let Some(result) = resp.result else {
            let msg = format!("Tool list response from {} is missing result", client.server_name);
            break 'tool_list_result Err(eyre::eyre!(msg));
        };
        let tool_list_result = match serde_json::from_value::<ToolsListResult>(result) {
            Ok(result) => result,
            Err(e) => {
                let msg = format!("Failed to deserialize tool result from {}: {:?}", client.server_name, e);
                break 'tool_list_result Err(eyre::eyre!(msg));
            },
        };
        Ok::<ToolsListResult, eyre::Report>(tool_list_result)
    };

    if let Some(messenger) = messenger {
        if let Err(e) = messenger.send_tools_list_result(tool_list_result).await {
            tracing::error!("Failed to send tool result through messenger {:?}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::Value;

    use super::*;
    const TEST_BIN_OUT_DIR: &str = "target/debug";
    const TEST_SERVER_NAME: &str = "test_mcp_server";

    fn get_workspace_root() -> PathBuf {
        let output = std::process::Command::new("cargo")
            .args(["metadata", "--format-version=1", "--no-deps"])
            .output()
            .expect("Failed to execute cargo metadata");

        let metadata: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("Failed to parse cargo metadata");

        let workspace_root = metadata["workspace_root"]
            .as_str()
            .expect("Failed to find workspace_root in metadata");

        PathBuf::from(workspace_root)
    }

    #[tokio::test(flavor = "multi_thread")]
    // For some reason this test is quite flakey when ran in the CI but not on developer's
    // machines. As a result it is hard to debug, hence we are ignoring it for now.
    #[ignore]
    async fn test_client_stdio() {
        std::process::Command::new("cargo")
            .args(["build", "--bin", TEST_SERVER_NAME])
            .status()
            .expect("Failed to build binary");
        let workspace_root = get_workspace_root();
        let bin_path = workspace_root.join(TEST_BIN_OUT_DIR).join(TEST_SERVER_NAME);
        println!("bin path: {}", bin_path.to_str().unwrap_or("no path found"));

        // Testing 2 concurrent sessions to make sure transport layer does not overlap.
        let client_info_one = serde_json::json!({
          "name": "TestClientOne",
          "version": "1.0.0"
        });
        let client_config_one = ClientConfig {
            server_name: "test_tool".to_owned(),
            bin_path: bin_path.to_str().unwrap().to_string(),
            args: ["1".to_owned()].to_vec(),
            timeout: 120 * 1000,
            client_info: client_info_one.clone(),
            env: {
                let mut map = HashMap::<String, String>::new();
                map.insert("ENV_ONE".to_owned(), "1".to_owned());
                map.insert("ENV_TWO".to_owned(), "2".to_owned());
                Some(map)
            },
            sampling_enabled: false, // Disable sampling for main test
        };
        let client_info_two = serde_json::json!({
          "name": "TestClientTwo",
          "version": "1.0.0"
        });
        let client_config_two = ClientConfig {
            server_name: "test_tool".to_owned(),
            bin_path: bin_path.to_str().unwrap().to_string(),
            args: ["2".to_owned()].to_vec(),
            timeout: 120 * 1000,
            client_info: client_info_two.clone(),
            env: {
                let mut map = HashMap::<String, String>::new();
                map.insert("ENV_ONE".to_owned(), "1".to_owned());
                map.insert("ENV_TWO".to_owned(), "2".to_owned());
                Some(map)
            },
            sampling_enabled: false, // Disable sampling for main test
        };
        let mut client_one = Client::<StdioTransport>::from_config(client_config_one, None).expect("Failed to create client");
        let mut client_two = Client::<StdioTransport>::from_config(client_config_two, None).expect("Failed to create client");
        let client_one_cap = ClientCapabilities::from(client_info_one);
        let client_two_cap = ClientCapabilities::from(client_info_two);

        let (res_one, res_two) = tokio::join!(
            time::timeout(
                time::Duration::from_secs(10),
                test_client_routine(&mut client_one, serde_json::json!(client_one_cap))
            ),
            time::timeout(
                time::Duration::from_secs(10),
                test_client_routine(&mut client_two, serde_json::json!(client_two_cap))
            )
        );
        let res_one = res_one.expect("Client one timed out");
        let res_two = res_two.expect("Client two timed out");
        assert!(res_one.is_ok());
        assert!(res_two.is_ok());
    }

    #[allow(clippy::await_holding_lock)]
    async fn test_client_routine<T: Transport>(
        client: &mut Client<T>,
        cap_sent: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Test init
        let _ = client.init().await.expect("Client init failed");
        tokio::time::sleep(time::Duration::from_millis(1500)).await;
        let client_capabilities_sent = client
            .request("verify_init_ack_sent", None)
            .await
            .expect("Verify init ack mock request failed");
        let has_server_recvd_init_ack = client_capabilities_sent
            .result
            .expect("Failed to retrieve client capabilities sent.");
        assert_eq!(has_server_recvd_init_ack.to_string(), "true");
        let cap_recvd = client
            .request("verify_init_params_sent", None)
            .await
            .expect("Verify init params mock request failed");
        let cap_recvd = cap_recvd
            .result
            .expect("Verify init params mock request does not contain required field (result)");
        assert!(are_json_values_equal(&cap_sent, &cap_recvd));

        // test list tools
        let fake_tool_names = ["get_weather_one", "get_weather_two", "get_weather_three"];
        let mock_result_spec = fake_tool_names.map(create_fake_tool_spec);
        let mock_tool_specs_for_verify = serde_json::json!(mock_result_spec.clone());
        let mock_tool_specs_prep_param = mock_result_spec
            .iter()
            .zip(fake_tool_names.iter())
            .map(|(v, n)| {
                serde_json::json!({
                    "key": (*n).to_string(),
                    "value": v
                })
            })
            .collect::<Vec<serde_json::Value>>();
        let mock_tool_specs_prep_param =
            serde_json::to_value(mock_tool_specs_prep_param).expect("Failed to create mock tool specs prep param");
        let _ = client
            .request("store_mock_tool_spec", Some(mock_tool_specs_prep_param))
            .await
            .expect("Mock tool spec prep failed");
        let tool_spec_recvd = client.request("tools/list", None).await.expect("List tools failed");
        assert!(are_json_values_equal(
            tool_spec_recvd
                .result
                .as_ref()
                .and_then(|v| v.get("tools"))
                .expect("Failed to retrieve tool specs from result received"),
            &mock_tool_specs_for_verify
        ));

        // Test list prompts directly
        let fake_prompt_names = ["code_review_one", "code_review_two", "code_review_three"];
        let mock_result_prompts = fake_prompt_names.map(create_fake_prompts);
        let mock_prompts_for_verify = serde_json::json!(mock_result_prompts.clone());
        let mock_prompts_prep_param = mock_result_prompts
            .iter()
            .zip(fake_prompt_names.iter())
            .map(|(v, n)| {
                serde_json::json!({
                    "key": (*n).to_string(),
                    "value": v
                })
            })
            .collect::<Vec<serde_json::Value>>();
        let mock_prompts_prep_param =
            serde_json::to_value(mock_prompts_prep_param).expect("Failed to create mock prompts prep param");
        let _ = client
            .request("store_mock_prompts", Some(mock_prompts_prep_param))
            .await
            .expect("Mock prompt prep failed");
        let prompts_recvd = client.request("prompts/list", None).await.expect("List prompts failed");
        client.is_prompts_out_of_date.store(false, Ordering::Release);
        assert!(are_json_values_equal(
            prompts_recvd
                .result
                .as_ref()
                .and_then(|v| v.get("prompts"))
                .expect("Failed to retrieve prompts from results received"),
            &mock_prompts_for_verify
        ));

        // Test prompts list changed
        let fake_prompt_names = ["code_review_four", "code_review_five", "code_review_six"];
        let mock_result_prompts = fake_prompt_names.map(create_fake_prompts);
        let mock_prompts_prep_param = mock_result_prompts
            .iter()
            .zip(fake_prompt_names.iter())
            .map(|(v, n)| {
                serde_json::json!({
                    "key": (*n).to_string(),
                    "value": v
                })
            })
            .collect::<Vec<serde_json::Value>>();
        let mock_prompts_prep_param =
            serde_json::to_value(mock_prompts_prep_param).expect("Failed to create mock prompts prep param");
        let _ = client
            .request("store_mock_prompts", Some(mock_prompts_prep_param))
            .await
            .expect("Mock new prompt request failed");
        // After we send the signal for the server to clear prompts, we should be receiving signal
        // to fetch for new prompts, after which we should be getting no prompts.
        let is_prompts_out_of_date = client.is_prompts_out_of_date.clone();
        let wait_for_new_prompts = async move {
            while !is_prompts_out_of_date.load(Ordering::Acquire) {
                tokio::time::sleep(time::Duration::from_millis(100)).await;
            }
        };
        time::timeout(time::Duration::from_secs(5), wait_for_new_prompts)
            .await
            .expect("Timed out while waiting for new prompts");
        let new_prompts = client.prompt_gets.read().expect("Failed to read new prompts");
        for k in new_prompts.keys() {
            assert!(fake_prompt_names.contains(&k.as_str()));
        }

        // Test env var inclusion
        let env_vars = client.request("get_env_vars", None).await.expect("Get env vars failed");
        let env_one = env_vars
            .result
            .as_ref()
            .expect("Failed to retrieve results from env var request")
            .get("ENV_ONE")
            .expect("Failed to retrieve env one from env var request");
        let env_two = env_vars
            .result
            .as_ref()
            .expect("Failed to retrieve results from env var request")
            .get("ENV_TWO")
            .expect("Failed to retrieve env two from env var request");
        let env_one_as_str = serde_json::to_string(env_one).expect("Failed to convert env one to string");
        let env_two_as_str = serde_json::to_string(env_two).expect("Failed to convert env two to string");
        assert_eq!(env_one_as_str, "\"1\"".to_string());
        assert_eq!(env_two_as_str, "\"2\"".to_string());

        Ok(())
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

    fn create_fake_tool_spec(name: &str) -> serde_json::Value {
        serde_json::json!({
            "name": name,
            "description": "Get current weather information for a location",
            "inputSchema": {
              "type": "object",
              "properties": {
                "location": {
                  "type": "string",
                  "description": "City name or zip code"
                }
              },
              "required": ["location"]
            }
        })
    }

    fn create_fake_prompts(name: &str) -> serde_json::Value {
        serde_json::json!({
            "name": name,
            "description": "Asks the LLM to analyze code quality and suggest improvements",
            "arguments": [
              {
                "name": "code",
                "description": "The code to review",
                "required": true
              }
            ]
        })
    }

    #[cfg(windows)]
    mod windows_command_tests {
        use super::*;
        use crate::mcp_client::transport::stdio::JsonRpcStdioTransport as StdioTransport;

        #[test]
        fn test_quote_windows_arg_no_special_chars() {
            let result = Client::<StdioTransport>::quote_windows_arg("simple");
            assert_eq!(result, "simple");
        }

        #[test]
        fn test_quote_windows_arg_with_spaces() {
            let result = Client::<StdioTransport>::quote_windows_arg("with spaces");
            assert_eq!(result, "\"with spaces\"");
        }

        #[test]
        fn test_quote_windows_arg_with_quotes() {
            let result = Client::<StdioTransport>::quote_windows_arg("with \"quotes\"");
            assert_eq!(result, "\"with \\\"quotes\\\"\"");
        }

        #[test]
        fn test_quote_windows_arg_with_backslashes() {
            let result = Client::<StdioTransport>::quote_windows_arg("path\\to\\file");
            assert_eq!(result, "path\\to\\file");
        }

        #[test]
        fn test_quote_windows_arg_with_trailing_backslashes() {
            let result = Client::<StdioTransport>::quote_windows_arg("path\\to\\dir\\");
            assert_eq!(result, "path\\to\\dir\\");
        }

        #[test]
        fn test_quote_windows_arg_with_backslashes_before_quote() {
            let result = Client::<StdioTransport>::quote_windows_arg("path\\\\\"quoted\"");
            assert_eq!(result, "\"path\\\\\\\\\\\"quoted\\\"\"");
        }

        #[test]
        fn test_quote_windows_arg_complex_case() {
            let result = Client::<StdioTransport>::quote_windows_arg("C:\\Program Files\\My App\\bin\\app.exe");
            assert_eq!(result, "\"C:\\Program Files\\My App\\bin\\app.exe\"");
        }

        #[test]
        fn test_quote_windows_arg_with_tabs_and_newlines() {
            let result = Client::<StdioTransport>::quote_windows_arg("with\ttabs\nand\rnewlines");
            assert_eq!(result, "\"with\ttabs\nand\rnewlines\"");
        }

        #[test]
        fn test_quote_windows_arg_edge_case_only_backslashes() {
            let result = Client::<StdioTransport>::quote_windows_arg("\\\\\\");
            assert_eq!(result, "\\\\\\");
        }

        #[test]
        fn test_quote_windows_arg_edge_case_only_quotes() {
            let result = Client::<StdioTransport>::quote_windows_arg("\"\"\"");
            assert_eq!(result, "\"\\\"\\\"\\\"\"");
        }

        // Tests for build_windows_command function
        #[test]
        fn test_build_windows_command_empty_args() {
            let bin_path = "myapp";
            let args = vec![];
            let result = Client::<StdioTransport>::build_windows_command(bin_path, args);
            assert_eq!(result, "myapp");
        }

        #[test]
        fn test_build_windows_command_uvx_example() {
            let bin_path = "uvx";
            let args = vec!["mcp-server-fetch".to_string()];
            let result = Client::<StdioTransport>::build_windows_command(bin_path, args);
            assert_eq!(result, "uvx mcp-server-fetch");
        }

        #[test]
        fn test_build_windows_command_npx_example() {
            let bin_path = "npx";
            let args = vec!["-y".to_string(), "@modelcontextprotocol/server-memory".to_string()];
            let result = Client::<StdioTransport>::build_windows_command(bin_path, args);
            assert_eq!(result, "npx -y @modelcontextprotocol/server-memory");
        }

        #[test]
        fn test_build_windows_command_docker_example() {
            let bin_path = "docker";
            let args = vec![
                "run".to_string(),
                "-i".to_string(),
                "--rm".to_string(),
                "-e".to_string(),
                "GITHUB_PERSONAL_ACCESS_TOKEN".to_string(),
                "ghcr.io/github/github-mcp-server".to_string(),
            ];
            let result = Client::<StdioTransport>::build_windows_command(bin_path, args);
            assert_eq!(
                result,
                "docker run -i --rm -e GITHUB_PERSONAL_ACCESS_TOKEN ghcr.io/github/github-mcp-server"
            );
        }

        #[test]
        fn test_build_windows_command_with_quotes_in_args() {
            let bin_path = "myapp";
            let args = vec!["--config".to_string(), "{\"key\": \"value\"}".to_string()];
            let result = Client::<StdioTransport>::build_windows_command(bin_path, args);
            assert_eq!(result, "myapp --config \"{\\\"key\\\": \\\"value\\\"}\"");
        }

        #[test]
        fn test_build_windows_command_with_spaces_in_path() {
            let bin_path = "C:\\Program Files\\My App\\bin\\app.exe";
            let args = vec!["--input".to_string(), "file with spaces.txt".to_string()];
            let result = Client::<StdioTransport>::build_windows_command(bin_path, args);
            assert_eq!(
                result,
                "\"C:\\Program Files\\My App\\bin\\app.exe\" --input \"file with spaces.txt\""
            );
        }

        #[test]
        fn test_build_windows_command_complex_args() {
            let bin_path = "myapp";
            let args = vec![
                "--config".to_string(),
                "C:\\Users\\test\\config.json".to_string(),
                "--output".to_string(),
                "C:\\Output\\result file.txt".to_string(),
                "--verbose".to_string(),
            ];
            let result = Client::<StdioTransport>::build_windows_command(bin_path, args);
            assert_eq!(
                result,
                "myapp --config C:\\Users\\test\\config.json --output \"C:\\Output\\result file.txt\" --verbose"
            );
        }

        #[test]
        fn test_build_windows_command_with_environment_variables() {
            let bin_path = "cmd";
            let args = vec!["/c".to_string(), "echo %PATH%".to_string()];
            let result = Client::<StdioTransport>::build_windows_command(bin_path, args);
            assert_eq!(result, "cmd /c \"echo %PATH%\"");
        }

        #[test]
        fn test_build_windows_command_real_world_python() {
            let bin_path = "python";
            let args = vec![
                "-m".to_string(),
                "mcp_server".to_string(),
                "--config".to_string(),
                "C:\\configs\\server.json".to_string(),
            ];
            let result = Client::<StdioTransport>::build_windows_command(bin_path, args);
            assert_eq!(result, "python -m mcp_server --config C:\\configs\\server.json");
        }
    }

    // Sampling feature tests
    mod sampling_tests {
        use super::*;
        use crate::mcp_client::facilitator_types::{
            ModelHint,
            ModelPreferences,
            Role,
            SamplingContent,
            SamplingCreateMessageRequest,
            SamplingCreateMessageResponse,
            SamplingMessage,
        };
        use crate::mcp_client::transport::base_protocol::{
            JsonRpcRequest,
            JsonRpcVersion,
        };

        /// Test that ClientCapabilities includes sampling capability
        #[test]
        fn test_client_capabilities_includes_sampling() {
            let client_info = serde_json::json!({
                "name": "TestClient",
                "version": "1.0.0"
            });

            let capabilities = ClientCapabilities::from(client_info);

            // Check that sampling capability is declared
            assert!(capabilities.capabilities.contains_key("sampling"));
            assert_eq!(capabilities.capabilities.get("sampling"), Some(&serde_json::json!({})));
        }

        /// Test successful sampling request handling
        #[tokio::test]
        async fn test_handle_sampling_request_success() {
            let client_info = serde_json::json!({
                "name": "TestClient",
                "version": "1.0.0"
            });

            let client_config = ClientConfig {
                server_name: "test_server".to_string(),
                bin_path: "test".to_string(),
                args: vec![],
                timeout: 5000,
                client_info: client_info.clone(),
                env: None,
                sampling_enabled: true, // Enable sampling for test
            };

            // Use from_config to create the client
            let client = Client::<StdioTransport>::from_config(client_config, None).unwrap();

            // Create a sampling request
            let sampling_request = SamplingCreateMessageRequest {
                messages: vec![SamplingMessage {
                    role: Role::User,
                    content: SamplingContent::Text {
                        text: "What is the capital of France?".to_string(),
                    },
                }],
                model_preferences: Some(ModelPreferences {
                    hints: Some(vec![ModelHint {
                        name: "claude-3-sonnet".to_string(),
                    }]),
                    cost_priority: Some(0.3),
                    speed_priority: Some(0.8),
                    intelligence_priority: Some(0.5),
                }),
                system_prompt: Some("You are a helpful assistant.".to_string()),
                max_tokens: Some(100),
            };

            let request = JsonRpcRequest {
                jsonrpc: JsonRpcVersion::default(),
                id: 1,
                method: "sampling/createMessage".to_string(),
                params: Some(serde_json::to_value(sampling_request).unwrap()),
            };

            // Test the sampling request handler
            let response = client.handle_sampling_request(&request).await.unwrap();

            // Verify response structure
            assert_eq!(response.jsonrpc, JsonRpcVersion::default());
            assert_eq!(response.id, 1);
            assert!(response.result.is_some());
            assert!(response.error.is_none());

            // Verify response content - should indicate no API client available
            let result: SamplingCreateMessageResponse = serde_json::from_value(response.result.unwrap()).unwrap();

            assert_eq!(result.role, Role::Assistant);
            match result.content {
                SamplingContent::Text { text } => {
                    assert!(text.contains("API client not available"));
                },
                _ => panic!("Expected text content"),
            }
            assert_eq!(result.model, Some("amazon-q-cli".to_string()));
            assert_eq!(result.stop_reason, Some("no_api_client".to_string()));
        }

        /// Test sampling request with invalid method
        #[tokio::test]
        async fn test_handle_sampling_request_invalid_method() {
            let client_info = serde_json::json!({
                "name": "TestClient",
                "version": "1.0.0"
            });

            let client_config = ClientConfig {
                server_name: "test_server".to_string(),
                bin_path: "test".to_string(),
                args: vec![],
                timeout: 5000,
                client_info: client_info.clone(),
                env: None,
                sampling_enabled: true, // Enable sampling for test
            };

            let client = Client::<StdioTransport>::from_config(client_config, None).unwrap();

            let request = JsonRpcRequest {
                jsonrpc: JsonRpcVersion::default(),
                id: 1,
                method: "sampling/invalidMethod".to_string(),
                params: Some(serde_json::json!({})),
            };

            // Test with invalid method
            let result = client.handle_sampling_request(&request).await;
            assert!(result.is_err());

            match result.unwrap_err() {
                ClientError::NegotiationError(msg) => {
                    assert!(msg.contains("Unsupported sampling method"));
                },
                _ => panic!("Expected NegotiationError"),
            }
        }

        /// Test sampling request with missing parameters
        #[tokio::test]
        async fn test_handle_sampling_request_missing_params() {
            let client_info = serde_json::json!({
                "name": "TestClient",
                "version": "1.0.0"
            });

            let client_config = ClientConfig {
                server_name: "test_server".to_string(),
                bin_path: "test".to_string(),
                args: vec![],
                timeout: 5000,
                client_info: client_info.clone(),
                env: None,
                sampling_enabled: true, // Enable sampling for test
            };

            let client = Client::<StdioTransport>::from_config(client_config, None).unwrap();

            let request = JsonRpcRequest {
                jsonrpc: JsonRpcVersion::default(),
                id: 1,
                method: "sampling/createMessage".to_string(),
                params: None, // Missing parameters
            };

            // Test with missing parameters
            let result = client.handle_sampling_request(&request).await;
            assert!(result.is_err());

            match result.unwrap_err() {
                ClientError::NegotiationError(msg) => {
                    assert!(msg.contains("Missing parameters"));
                },
                _ => panic!("Expected NegotiationError"),
            }
        }

        /// Test sampling request with malformed parameters
        #[tokio::test]
        async fn test_handle_sampling_request_malformed_params() {
            let client_info = serde_json::json!({
                "name": "TestClient",
                "version": "1.0.0"
            });

            let client_config = ClientConfig {
                server_name: "test_server".to_string(),
                bin_path: "test".to_string(),
                args: vec![],
                timeout: 5000,
                client_info: client_info.clone(),
                env: None,
                sampling_enabled: true, // Enable sampling for test
            };

            let client = Client::<StdioTransport>::from_config(client_config, None).unwrap();

            let request = JsonRpcRequest {
                jsonrpc: JsonRpcVersion::default(),
                id: 1,
                method: "sampling/createMessage".to_string(),
                params: Some(serde_json::json!({
                    "invalid": "structure"
                })),
            };

            // Test with malformed parameters
            let result = client.handle_sampling_request(&request).await;
            assert!(result.is_err());

            match result.unwrap_err() {
                ClientError::Serialization(_) => {
                    // Expected serialization error
                },
                _ => panic!("Expected Serialization error"),
            }
        }

        /// Test sampling request when sampling is disabled
        #[tokio::test]
        async fn test_handle_sampling_request_disabled() {
            let client_info = serde_json::json!({
                "name": "TestClient",
                "version": "1.0.0"
            });

            let client_config = ClientConfig {
                server_name: "test_server".to_string(),
                bin_path: "test".to_string(),
                args: vec![],
                timeout: 5000,
                client_info: client_info.clone(),
                env: None,
                sampling_enabled: false, // Disable sampling
            };

            let client = Client::<StdioTransport>::from_config(client_config, None).unwrap();

            let sampling_request = SamplingCreateMessageRequest {
                messages: vec![SamplingMessage {
                    role: Role::User,
                    content: SamplingContent::Text {
                        text: "Hello, world!".to_string(),
                    },
                }],
                model_preferences: None,
                system_prompt: None,
                max_tokens: None,
            };

            let request = JsonRpcRequest {
                jsonrpc: JsonRpcVersion::default(),
                id: 1,
                method: "sampling/createMessage".to_string(),
                params: Some(serde_json::to_value(sampling_request).unwrap()),
            };

            // Test the sampling request handler
            let response = client.handle_sampling_request(&request).await.unwrap();

            // Verify response structure - should be an error
            assert_eq!(response.jsonrpc, JsonRpcVersion::default());
            assert_eq!(response.id, 1);
            assert!(response.result.is_none());
            assert!(response.error.is_some());

            // Verify error details
            let error = response.error.unwrap();
            assert_eq!(error.code, -32601);
            assert!(error.message.contains("Sampling not enabled"));
            assert!(error.message.contains("sampling: true"));
        }

        /// Test sampling types serialization/deserialization
        #[test]
        fn test_sampling_types_serialization() {
            // Test SamplingCreateMessageRequest
            let request = SamplingCreateMessageRequest {
                messages: vec![
                    SamplingMessage {
                        role: Role::User,
                        content: SamplingContent::Text {
                            text: "Hello".to_string(),
                        },
                    },
                    SamplingMessage {
                        role: Role::Assistant,
                        content: SamplingContent::Image {
                            data: "base64data".to_string(),
                            mime_type: "image/jpeg".to_string(),
                        },
                    },
                ],
                model_preferences: Some(ModelPreferences {
                    hints: Some(vec![
                        ModelHint {
                            name: "claude-3-sonnet".to_string(),
                        },
                        ModelHint {
                            name: "gpt-4".to_string(),
                        },
                    ]),
                    cost_priority: Some(0.2),
                    speed_priority: Some(0.8),
                    intelligence_priority: Some(0.9),
                }),
                system_prompt: Some("You are helpful".to_string()),
                max_tokens: Some(150),
            };

            // Test serialization
            let json = serde_json::to_value(&request).unwrap();
            assert!(json.get("messages").is_some());
            assert!(json.get("modelPreferences").is_some());
            assert!(json.get("systemPrompt").is_some());
            assert!(json.get("maxTokens").is_some());

            // Test deserialization
            let deserialized: SamplingCreateMessageRequest = serde_json::from_value(json).unwrap();
            assert_eq!(deserialized.messages.len(), 2);
            assert!(deserialized.model_preferences.is_some());
            assert_eq!(deserialized.system_prompt, Some("You are helpful".to_string()));
            assert_eq!(deserialized.max_tokens, Some(150));

            // Test SamplingCreateMessageResponse
            let response = SamplingCreateMessageResponse {
                role: Role::Assistant,
                content: SamplingContent::Audio {
                    data: "audiodata".to_string(),
                    mime_type: "audio/wav".to_string(),
                },
                model: Some("claude-3-sonnet-20240307".to_string()),
                stop_reason: Some("endTurn".to_string()),
            };

            // Test serialization/deserialization
            let json = serde_json::to_value(&response).unwrap();
            let deserialized: SamplingCreateMessageResponse = serde_json::from_value(json).unwrap();

            assert_eq!(deserialized.role, Role::Assistant);
            match deserialized.content {
                SamplingContent::Audio { data, mime_type } => {
                    assert_eq!(data, "audiodata");
                    assert_eq!(mime_type, "audio/wav");
                },
                _ => panic!("Expected audio content"),
            }
            assert_eq!(deserialized.model, Some("claude-3-sonnet-20240307".to_string()));
            assert_eq!(deserialized.stop_reason, Some("endTurn".to_string()));
        }

        /// Test ServerCapabilities includes sampling field
        #[test]
        fn test_server_capabilities_sampling_field() {
            let capabilities_json = serde_json::json!({
                "logging": {},
                "prompts": { "listChanged": true },
                "resources": {},
                "tools": { "listChanged": true },
                "sampling": {}
            });

            let capabilities: ServerCapabilities = serde_json::from_value(capabilities_json).unwrap();

            assert!(capabilities.logging.is_some());
            assert!(capabilities.prompts.is_some());
            assert!(capabilities.resources.is_some());
            assert!(capabilities.tools.is_some());
            assert!(capabilities.sampling.is_some());

            // Test serialization back
            let serialized = serde_json::to_value(&capabilities).unwrap();
            assert!(serialized.get("sampling").is_some());
        }

        /// Test Role enum serialization
        #[test]
        fn test_role_serialization() {
            let user_role = Role::User;
            let assistant_role = Role::Assistant;

            // Test serialization
            let user_json = serde_json::to_value(&user_role).unwrap();
            let assistant_json = serde_json::to_value(&assistant_role).unwrap();

            assert_eq!(user_json, serde_json::Value::String("user".to_string()));
            assert_eq!(assistant_json, serde_json::Value::String("assistant".to_string()));

            // Test deserialization
            let user_deserialized: Role = serde_json::from_value(user_json).unwrap();
            let assistant_deserialized: Role = serde_json::from_value(assistant_json).unwrap();

            assert_eq!(user_deserialized, Role::User);
            assert_eq!(assistant_deserialized, Role::Assistant);

            // Test Display trait
            assert_eq!(user_role.to_string(), "user");
            assert_eq!(assistant_role.to_string(), "assistant");
        }

        /// Test SamplingContent variants
        #[test]
        fn test_sampling_content_variants() {
            // Test Text content
            let text_content = SamplingContent::Text {
                text: "Hello world".to_string(),
            };
            let text_json = serde_json::to_value(&text_content).unwrap();
            assert_eq!(text_json["type"], "text");
            assert_eq!(text_json["text"], "Hello world");

            // Test Image content
            let image_content = SamplingContent::Image {
                data: "base64imagedata".to_string(),
                mime_type: "image/png".to_string(),
            };
            let image_json = serde_json::to_value(&image_content).unwrap();
            assert_eq!(image_json["type"], "image");
            assert_eq!(image_json["data"], "base64imagedata");
            assert_eq!(image_json["mimeType"], "image/png");

            // Test Audio content
            let audio_content = SamplingContent::Audio {
                data: "base64audiodata".to_string(),
                mime_type: "audio/mp3".to_string(),
            };
            let audio_json = serde_json::to_value(&audio_content).unwrap();
            assert_eq!(audio_json["type"], "audio");
            assert_eq!(audio_json["data"], "base64audiodata");
            assert_eq!(audio_json["mimeType"], "audio/mp3");

            // Test deserialization
            let text_deserialized: SamplingContent = serde_json::from_value(text_json).unwrap();
            let image_deserialized: SamplingContent = serde_json::from_value(image_json).unwrap();
            let audio_deserialized: SamplingContent = serde_json::from_value(audio_json).unwrap();

            match text_deserialized {
                SamplingContent::Text { text } => assert_eq!(text, "Hello world"),
                _ => panic!("Expected text content"),
            }

            match image_deserialized {
                SamplingContent::Image { data, mime_type } => {
                    assert_eq!(data, "base64imagedata");
                    assert_eq!(mime_type, "image/png");
                },
                _ => panic!("Expected image content"),
            }

            match audio_deserialized {
                SamplingContent::Audio { data, mime_type } => {
                    assert_eq!(data, "base64audiodata");
                    assert_eq!(mime_type, "audio/mp3");
                },
                _ => panic!("Expected audio content"),
            }
        }

        /// Test ModelPreferences with optional fields
        #[test]
        fn test_model_preferences_optional_fields() {
            // Test with all fields
            let full_prefs = ModelPreferences {
                hints: Some(vec![ModelHint {
                    name: "claude".to_string(),
                }]),
                cost_priority: Some(0.5),
                speed_priority: Some(0.7),
                intelligence_priority: Some(0.9),
            };

            let full_json = serde_json::to_value(&full_prefs).unwrap();
            assert!(full_json.get("hints").is_some());
            assert!(full_json.get("costPriority").is_some());
            assert!(full_json.get("speedPriority").is_some());
            assert!(full_json.get("intelligencePriority").is_some());

            // Test with minimal fields
            let minimal_prefs = ModelPreferences {
                hints: None,
                cost_priority: None,
                speed_priority: None,
                intelligence_priority: None,
            };

            let minimal_json = serde_json::to_value(&minimal_prefs).unwrap();
            // Optional fields should not be present when None
            assert!(minimal_json.get("hints").is_none());
            assert!(minimal_json.get("costPriority").is_none());
            assert!(minimal_json.get("speedPriority").is_none());
            assert!(minimal_json.get("intelligencePriority").is_none());

            // Test deserialization
            let full_deserialized: ModelPreferences = serde_json::from_value(full_json).unwrap();
            assert!(full_deserialized.hints.is_some());
            assert_eq!(full_deserialized.cost_priority, Some(0.5));
            assert_eq!(full_deserialized.speed_priority, Some(0.7));
            assert_eq!(full_deserialized.intelligence_priority, Some(0.9));

            let minimal_deserialized: ModelPreferences = serde_json::from_value(minimal_json).unwrap();
            assert!(minimal_deserialized.hints.is_none());
            assert!(minimal_deserialized.cost_priority.is_none());
            assert!(minimal_deserialized.speed_priority.is_none());
            assert!(minimal_deserialized.intelligence_priority.is_none());
        }
    }
}
