use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{
    AtomicU64,
    Ordering,
};
use std::time::Duration;

use nix::sys::signal::Signal;
use nix::unistd::Pid;
use serde::Deserialize;
use thiserror::Error;
use tokio::time;

use crate::transport::base_protocol::{
    JsonRpcMessage,
    JsonRpcNotification,
    JsonRpcRequest,
    JsonRpcVersion,
};
use crate::transport::stdio::JsonRpcStdioTransport;
use crate::transport::{
    self,
    Transport,
    TransportError,
};
use crate::{
    PaginationSupportedOps,
    PromptsListResult,
    ResourceTemplatesListResult,
    ResourcesListResult,
    ToolsListResult,
};

pub type ServerCapabilities = serde_json::Value;
pub type StdioTransport = JsonRpcStdioTransport;

#[derive(Debug, Deserialize)]
pub struct ClientConfig {
    pub tool_name: String,
    pub bin_path: String,
    pub args: Vec<String>,
    pub timeout: u64,
    pub init_params: serde_json::Value,
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error(transparent)]
    TransportError(#[from] TransportError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
    #[error(transparent)]
    RuntimeError(#[from] tokio::time::error::Elapsed),
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
}

#[derive(Debug)]
pub struct Client<T: Transport> {
    tool_name: String,
    transport: Arc<T>,
    timeout: u64,
    server_process_id: Pid,
    init_params: serde_json::Value,
    current_id: AtomicU64,
}

impl Client<StdioTransport> {
    pub fn from_config(config: ClientConfig) -> Result<Self, ClientError> {
        let ClientConfig {
            tool_name,
            bin_path,
            args,
            timeout,
            init_params,
        } = config;
        let child = tokio::process::Command::new(bin_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .args(args)
            .spawn()?;
        let server_process_id = child.id().ok_or(ClientError::MissingProcessId)?;
        #[allow(clippy::map_err_ignore)]
        let server_process_id = Pid::from_raw(
            server_process_id
                .try_into()
                .map_err(|_| ClientError::MissingProcessId)?,
        );
        let transport = Arc::new(transport::stdio::JsonRpcStdioTransport::client(child)?);
        Ok(Self {
            tool_name,
            transport,
            timeout,
            server_process_id,
            init_params,
            current_id: AtomicU64::new(0),
        })
    }
}

impl<T> Drop for Client<T>
where
    T: Transport,
{
    fn drop(&mut self) {
        let _ = nix::sys::signal::kill(self.server_process_id, Signal::SIGTERM);
    }
}

impl<T> Client<T>
where
    T: Transport,
{
    /// Exchange of information specified as per https://spec.modelcontextprotocol.io/specification/2024-11-05/basic/lifecycle/#initialization
    ///
    /// Also done is the spawn of a background task that constantly listens for incoming messages
    /// from the server.
    pub async fn init(&self) -> Result<ServerCapabilities, ClientError> {
        let transport_ref = self.transport.clone();
        let tool_name = self.tool_name.clone();

        tokio::spawn(async move {
            loop {
                match transport_ref.monitor().await {
                    Ok(msg) => {
                        match msg {
                            JsonRpcMessage::Request(_req) => {},
                            JsonRpcMessage::Notification(_notif) => {},
                            JsonRpcMessage::Response(_resp) => { /* noop since direct response is handled inside the request api */
                            },
                        }
                    },
                    Err(e) => {
                        tracing::error!("Background listening thread for client {}: {:?}", tool_name, e);
                    },
                }
            }
        });

        let server_capabilities = self.request("initialize", Some(self.init_params.clone())).await?;
        if let Err(e) = examine_server_capabilities(&server_capabilities) {
            let _ = nix::sys::signal::kill(self.server_process_id, Signal::SIGTERM);
            return Err(ClientError::NegotiationError(format!(
                "Client {} has failed to negotiate server capabilities with server: {:?}",
                self.tool_name, e
            )));
        }
        self.notify("initialized", None).await?;

        Ok(server_capabilities)
    }

    /// Sends a request to the server associated.
    /// This call will yield until a response is received.
    pub async fn request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, ClientError> {
        let request = JsonRpcRequest {
            jsonrpc: JsonRpcVersion::default(),
            id: self.get_id(),
            method: method.to_owned(),
            params,
        };
        let msg = JsonRpcMessage::Request(request);
        time::timeout(Duration::from_secs(self.timeout), self.transport.send(&msg)).await??;
        let resp = time::timeout(Duration::from_secs(self.timeout), self.transport.listen()).await??;
        let JsonRpcMessage::Response(mut resp) = resp else {
            return Err(ClientError::UnexpectedMsgType);
        };
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
                    let result = current_resp.result.as_ref().cloned().unwrap();
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
                    let next_request = JsonRpcRequest {
                        jsonrpc: JsonRpcVersion::default(),
                        id: self.get_id(),
                        method: method.to_owned(),
                        params: Some(serde_json::json!({
                            "cursor": next_cursor,
                        })),
                    };
                    let msg = JsonRpcMessage::Request(next_request);
                    time::timeout(Duration::from_secs(self.timeout), self.transport.send(&msg)).await??;
                    let resp = time::timeout(Duration::from_secs(self.timeout), self.transport.listen()).await??;
                    let JsonRpcMessage::Response(resp) = resp else {
                        return Err(ClientError::UnexpectedMsgType);
                    };
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
        Ok(serde_json::to_value(resp)?)
    }

    /// Sends a notification to the server associated.
    /// Notifications are requests that expect no responses.
    pub async fn notify(&self, method: &str, params: Option<serde_json::Value>) -> Result<(), ClientError> {
        let notification = JsonRpcNotification {
            jsonrpc: JsonRpcVersion::default(),
            method: format!("notifications/{}", method),
            params,
        };
        let msg = JsonRpcMessage::Notification(notification);
        Ok(time::timeout(Duration::from_secs(self.timeout), self.transport.send(&msg)).await??)
    }

    fn get_id(&self) -> u64 {
        self.current_id.fetch_add(1, Ordering::SeqCst)
    }
}

fn examine_server_capabilities(ser_cap: &serde_json::Value) -> Result<(), ClientError> {
    // Check the jrpc version.
    // Currently we are only proceeding if the versions are EXACTLY the same.
    let jrpc_version = ser_cap
        .get("jsonrpc")
        .map(|v| {
            v.to_string()
                .trim_matches('"')
                .replace("\\\"", "\"")
                .split(".")
                .map(|n| n.parse::<u32>())
                .collect::<Vec<Result<u32, _>>>()
        })
        .ok_or(ClientError::NegotiationError("Missing jsonrpc from server".to_owned()))?;
    let client_jrpc_version = JsonRpcVersion::default().as_u32_vec();
    for (sv, cv) in jrpc_version.iter().zip(client_jrpc_version.iter()) {
        let sv = sv
            .as_ref()
            .map_err(|e| ClientError::NegotiationError(format!("Failed to parse server jrpc version: {:?}", e)))?;
        if sv != cv {
            return Err(ClientError::NegotiationError(
                "Incompatible jrpc version between server and client".to_owned(),
            ));
        }
    }
    Ok(())
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
    async fn test_client_stdio() {
        std::process::Command::new("cargo")
            .args(["build", "--bin", TEST_SERVER_NAME])
            .status()
            .expect("Failed to build binary");
        let workspace_root = get_workspace_root();
        let bin_path = workspace_root.join(TEST_BIN_OUT_DIR).join(TEST_SERVER_NAME);
        println!("bin path: {}", bin_path.to_str().unwrap_or("no path found"));

        // Testing 2 concurrent sessions to make sure transport layer does not overlap.
        let init_params_one = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
              "roots": {
                "listChanged": true
              },
              "sampling": {}
            },
            "clientInfo": {
              "name": "TestClientOne",
              "version": "1.0.0"
            }
        });
        let client_config_one = ClientConfig {
            tool_name: "test_tool".to_owned(),
            bin_path: bin_path.to_str().unwrap().to_string(),
            args: ["1".to_owned()].to_vec(),
            timeout: 60,
            init_params: init_params_one.clone(),
        };
        let init_params_two = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
              "roots": {
                "listChanged": false
              },
              "sampling": {}
            },
            "clientInfo": {
              "name": "TestClientTwo",
              "version": "1.0.0"
            }
        });
        let client_config_two = ClientConfig {
            tool_name: "test_tool".to_owned(),
            bin_path: bin_path.to_str().unwrap().to_string(),
            args: ["2".to_owned()].to_vec(),
            timeout: 60,
            init_params: init_params_two.clone(),
        };
        let mut client_one = Client::<StdioTransport>::from_config(client_config_one).expect("Failed to create client");
        let mut client_two = Client::<StdioTransport>::from_config(client_config_two).expect("Failed to create client");

        let (res_one, res_two) = tokio::join!(
            time::timeout(
                time::Duration::from_secs(5),
                test_client_routine(&mut client_one, init_params_one)
            ),
            time::timeout(
                time::Duration::from_secs(5),
                test_client_routine(&mut client_two, init_params_two)
            )
        );
        let res_one = res_one.expect("Client one timed out");
        let res_two = res_two.expect("Client two timed out");
        assert!(res_one.is_ok());
        assert!(res_two.is_ok());
    }

    async fn test_client_routine<T: Transport>(
        client: &mut Client<T>,
        cap_sent: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let _ = client.init().await.expect("Client init failed");
        tokio::time::sleep(time::Duration::from_millis(1500)).await;
        let client_capabilities_sent = client
            .request("verify_init_ack_sent", None)
            .await
            .expect("Verify init ack mock request failed");
        let has_server_recvd_init_ack = client_capabilities_sent
            .get("result")
            .expect("Failed to retrieve client capabilities sent.");
        assert_eq!(has_server_recvd_init_ack.to_string(), "true");
        let cap_recvd = client
            .request("verify_init_params_sent", None)
            .await
            .expect("Verify init params mock request failed");
        let cap_recvd = cap_recvd
            .get("result")
            .expect("Verify init params mock request does not contain required field (result)");
        assert!(are_json_values_equal(&cap_sent, cap_recvd));

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
                .get("result")
                .and_then(|v| v.get("tools"))
                .expect("Failed to retrieve tool specs from result received"),
            &mock_tool_specs_for_verify
        ));
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

    fn create_fake_tool_spec(name_to_append: &str) -> serde_json::Value {
        serde_json::json!({
            "name": name_to_append,
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
}
