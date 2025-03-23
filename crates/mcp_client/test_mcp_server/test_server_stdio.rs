//! This is a bin used solely for testing the client
use std::collections::HashMap;
use std::str::FromStr;

use mcp_client::server::{
    self,
    PreServerRequestHandler,
    Response,
    ServerError,
    ServerRequestHandler,
};
use mcp_client::transport::{
    JsonRpcRequest,
    JsonRpcResponse,
    JsonRpcStdioTransport,
};
use tokio::sync::Mutex;

#[derive(Default)]
struct Handler {
    pending_request: Option<Box<dyn Fn(u64) -> Option<JsonRpcRequest> + Send + Sync>>,
    #[allow(clippy::type_complexity)]
    send_request: Option<Box<dyn Fn(&str, Option<serde_json::Value>) -> Result<(), ServerError> + Send + Sync>>,
    storage: Mutex<HashMap<String, serde_json::Value>>,
    tool_spec: Mutex<HashMap<String, Response>>,
    tool_spec_key_list: Mutex<Vec<String>>,
}

impl PreServerRequestHandler for Handler {
    fn register_pending_request_callback(
        &mut self,
        cb: impl Fn(u64) -> Option<JsonRpcRequest> + Send + Sync + 'static,
    ) {
        self.pending_request = Some(Box::new(cb));
    }

    fn register_send_request_callback(
        &mut self,
        cb: impl Fn(&str, Option<serde_json::Value>) -> Result<(), ServerError> + Send + Sync + 'static,
    ) {
        self.send_request = Some(Box::new(cb));
    }
}

#[async_trait::async_trait]
impl ServerRequestHandler for Handler {
    async fn handle_initialize(&self, params: Option<serde_json::Value>) -> Result<Response, ServerError> {
        let mut storage = self.storage.lock().await;
        if let Some(params) = params {
            storage.insert("client_cap".to_owned(), params);
        }
        let capabilities = serde_json::json!({
          "protocolVersion": "2024-11-05",
          "capabilities": {
            "logging": {},
            "prompts": {
              "listChanged": true
            },
            "resources": {
              "subscribe": true,
              "listChanged": true
            },
            "tools": {
              "listChanged": true
            }
          },
          "serverInfo": {
            "name": "TestServer",
            "version": "1.0.0"
          }
        });
        Ok(Some(capabilities))
    }

    async fn handle_incoming(&self, method: &str, params: Option<serde_json::Value>) -> Result<Response, ServerError> {
        match method {
            "notifications/initialized" => {
                {
                    let mut storage = self.storage.lock().await;
                    storage.insert(
                        "init_ack_sent".to_owned(),
                        serde_json::Value::from_str("true").expect("Failed to convert string to value"),
                    );
                }
                Ok(None)
            },
            "verify_init_params_sent" => {
                let client_capabilities = {
                    let storage = self.storage.lock().await;
                    storage.get("client_cap").cloned()
                };
                Ok(client_capabilities)
            },
            "verify_init_ack_sent" => {
                let result = {
                    let storage = self.storage.lock().await;
                    storage.get("init_ack_sent").cloned()
                };
                Ok(result)
            },
            "store_mock_tool_spec" => {
                let Some(params) = params else {
                    eprintln!("Params missing from store mock tool spec");
                    return Ok(None);
                };
                // expecting a mock_specs: { key: String, value: serde_json::Value }[];
                let Ok(mock_specs) = serde_json::from_value::<Vec<serde_json::Value>>(params) else {
                    eprintln!("Failed to convert to mock specs from value");
                    return Ok(None);
                };
                let self_tool_specs = self.tool_spec.lock().await;
                let mut self_tool_spec_key_list = self.tool_spec_key_list.lock().await;
                let _ = mock_specs.iter().fold(self_tool_specs, |mut acc, spec| {
                    let Some(key) = spec.get("key").cloned() else {
                        return acc;
                    };
                    let Ok(key) = serde_json::from_value::<String>(key) else {
                        eprintln!("Failed to convert serde value to string for key");
                        return acc;
                    };
                    self_tool_spec_key_list.push(key.clone());
                    acc.insert(key, spec.get("value").cloned());
                    acc
                });
                Ok(None)
            },
            "tools/list" => {
                if let Some(params) = params {
                    if let Some(cursor) = params.get("cursor").cloned() {
                        let Ok(cursor) = serde_json::from_value::<String>(cursor) else {
                            eprintln!("Failed to convert cursor to string: {:#?}", params);
                            return Ok(None);
                        };
                        let self_tool_spec_key_list = self.tool_spec_key_list.lock().await;
                        let self_tool_spec = self.tool_spec.lock().await;
                        let (next_cursor, spec) = {
                            'blk: {
                                for (i, item) in self_tool_spec_key_list.iter().enumerate() {
                                    if item == &cursor {
                                        break 'blk (
                                            self_tool_spec_key_list.get(i + 1).cloned(),
                                            self_tool_spec.get(&cursor).cloned().unwrap(),
                                        );
                                    }
                                }
                                (None, None)
                            }
                        };
                        if let Some(next_cursor) = next_cursor {
                            return Ok(Some(serde_json::json!({
                                "tools": [spec.unwrap()],
                                "nextCursor": next_cursor,
                            })));
                        } else {
                            return Ok(Some(serde_json::json!({
                                "tools": [spec.unwrap()],
                            })));
                        }
                    } else {
                        eprintln!("Params exist but cursor is missing");
                        return Ok(None);
                    }
                } else {
                    let first_key = self
                        .tool_spec_key_list
                        .lock()
                        .await
                        .first()
                        .expect("First key missing from tool specs")
                        .clone();
                    let first_value = self
                        .tool_spec
                        .lock()
                        .await
                        .get(&first_key)
                        .expect("First value missing from tool specs")
                        .clone();
                    let second_key = self
                        .tool_spec_key_list
                        .lock()
                        .await
                        .get(1)
                        .expect("Second key missing from tool specs")
                        .clone();
                    return Ok(Some(serde_json::json!({
                        "tools": [first_value],
                        "nextCursor": second_key
                    })));
                };
            },
            // This is a test path relevant only to sampling
            "trigger_server_request" => {
                let Some(ref send_request) = self.send_request else {
                    return Err(ServerError::MissingMethod);
                };
                let params = Some(serde_json::json!({
                  "messages": [
                    {
                      "role": "user",
                      "content": {
                        "type": "text",
                        "text": "What is the capital of France?"
                      }
                    }
                  ],
                  "modelPreferences": {
                    "hints": [
                      {
                        "name": "claude-3-sonnet"
                      }
                    ],
                    "intelligencePriority": 0.8,
                    "speedPriority": 0.5
                  },
                  "systemPrompt": "You are a helpful assistant.",
                  "maxTokens": 100
                }));
                send_request("sampling/createMessage", params)?;
                Ok(None)
            },
            _ => Err(ServerError::MissingMethod),
        }
    }

    // This is a test path relevant only to sampling
    async fn handle_response(&self, resp: JsonRpcResponse) -> Result<(), ServerError> {
        let JsonRpcResponse { id, .. } = resp;
        let _pending = self.pending_request.as_ref().and_then(|f| f(id));
        Ok(())
    }

    async fn handle_shutdown(&self) -> Result<(), ServerError> {
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let handler = Handler::default();
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let test_server =
        server::Server::<JsonRpcStdioTransport, _>::new(handler, stdin, stdout).expect("Failed to create server");
    let _ = test_server.init().expect("Test server failed to init").await;
}
