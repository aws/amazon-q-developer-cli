//! This is a bin used solely for testing the client
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{
    AtomicU8,
    Ordering,
};

use chat_cli::{
    self,
    JsonRpcRequest,
    JsonRpcResponse,
    JsonRpcStdioTransport,
    PreServerRequestHandler,
    Response,
    Server,
    ServerError,
    ServerRequestHandler,
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
    prompts: Mutex<HashMap<String, Response>>,
    prompt_key_list: Mutex<Vec<String>>,
    prompt_list_call_no: AtomicU8,
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
                    let tool_spec_key_list = self.tool_spec_key_list.lock().await;
                    let tool_spec = self.tool_spec.lock().await;
                    let first_key = tool_spec_key_list
                        .first()
                        .expect("First key missing from tool specs")
                        .clone();
                    let first_value = tool_spec
                        .get(&first_key)
                        .expect("First value missing from tool specs")
                        .clone();
                    let second_key = tool_spec_key_list
                        .get(1)
                        .expect("Second key missing from tool specs")
                        .clone();
                    return Ok(Some(serde_json::json!({
                        "tools": [first_value],
                        "nextCursor": second_key
                    })));
                };
            },
            "get_env_vars" => {
                let kv = std::env::vars().fold(HashMap::<String, String>::new(), |mut acc, (k, v)| {
                    acc.insert(k, v);
                    acc
                });
                Ok(Some(serde_json::json!(kv)))
            },
            "discover_tools" => {
                let Some(ref _send_request) = self.send_request else {
                    return Err(ServerError::MissingMethod);
                };
                
                let task_description = if let Some(params) = params {
                    params.get("task_description")
                        .and_then(|t| t.as_str())
                        .unwrap_or("unknown task")
                        .to_string()
                } else {
                    "unknown task".to_string()
                };
                
                let sampling_params = Some(serde_json::json!({
                  "messages": [
                    {
                      "role": "user",
                      "content": {
                        "type": "text",
                        "text": format!("You are a tool selection assistant. Based on this task: '{}', should I enable data processing tools? Respond with 'YES' if the task involves data, files, or processing. Otherwise respond 'NO'.", task_description)
                      }
                    }
                  ],
                  "maxTokens": 10
                }));
                
                self.send_request.as_ref().unwrap()("sampling/createMessage", sampling_params)?;
                Ok(Some(serde_json::json!({
                    "message": format!("Tool discovery initiated for task: {}", task_description)
                })))
            },
            // This is a test path relevant only to sampling
            "trigger_server_request" => {
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
                self.send_request.as_ref().unwrap()("sampling/createMessage", params)?;
                Ok(None)
            },
            "store_mock_prompts" => {
                let Some(params) = params else {
                    eprintln!("Params missing from store mock prompts");
                    return Ok(None);
                };
                // expecting a mock_prompts: { key: String, value: serde_json::Value }[];
                let Ok(mock_prompts) = serde_json::from_value::<Vec<serde_json::Value>>(params) else {
                    eprintln!("Failed to convert to mock specs from value");
                    return Ok(None);
                };
                let mut self_prompts = self.prompts.lock().await;
                let mut self_prompt_key_list = self.prompt_key_list.lock().await;
                let is_first_mock = self_prompts.is_empty();
                self_prompts.clear();
                self_prompt_key_list.clear();
                let _ = mock_prompts.iter().fold(self_prompts, |mut acc, spec| {
                    let Some(key) = spec.get("key").cloned() else {
                        return acc;
                    };
                    let Ok(key) = serde_json::from_value::<String>(key) else {
                        eprintln!("Failed to convert serde value to string for key");
                        return acc;
                    };
                    self_prompt_key_list.push(key.clone());
                    acc.insert(key, spec.get("value").cloned());
                    acc
                });
                if !is_first_mock {
                    if let Some(sender) = &self.send_request {
                        let _ = sender("notifications/prompts/list_changed", None);
                    }
                }
                Ok(None)
            },
            "prompts/list" => {
                // We expect this method to be called after the mock prompts have already been
                // stored.
                self.prompt_list_call_no.fetch_add(1, Ordering::Relaxed);
                if let Some(params) = params {
                    if let Some(cursor) = params.get("cursor").cloned() {
                        let Ok(cursor) = serde_json::from_value::<String>(cursor) else {
                            eprintln!("Failed to convert cursor to string: {:#?}", params);
                            return Ok(None);
                        };
                        let self_prompt_key_list = self.prompt_key_list.lock().await;
                        let self_prompts = self.prompts.lock().await;
                        let (next_cursor, spec) = {
                            'blk: {
                                for (i, item) in self_prompt_key_list.iter().enumerate() {
                                    if item == &cursor {
                                        break 'blk (
                                            self_prompt_key_list.get(i + 1).cloned(),
                                            self_prompts.get(&cursor).cloned().unwrap(),
                                        );
                                    }
                                }
                                (None, None)
                            }
                        };
                        if let Some(next_cursor) = next_cursor {
                            return Ok(Some(serde_json::json!({
                                "prompts": [spec.unwrap()],
                                "nextCursor": next_cursor,
                            })));
                        } else {
                            return Ok(Some(serde_json::json!({
                                "prompts": [spec.unwrap()],
                            })));
                        }
                    } else {
                        eprintln!("Params exist but cursor is missing");
                        return Ok(None);
                    }
                } else {
                    // If there is no parameter, this is the request to retrieve the first page
                    let prompt_key_list = self.prompt_key_list.lock().await;
                    let prompts = self.prompts.lock().await;
                    let first_key = prompt_key_list.first().expect("first key missing");
                    let first_value = prompts.get(first_key).cloned().unwrap().unwrap();
                    let second_key = prompt_key_list.get(1).expect("second key missing");
                    return Ok(Some(serde_json::json!({
                        "prompts": [first_value],
                        "nextCursor": second_key
                    })));
                };
            },
            "get_prompt_list_call_no" => Ok(Some(
                serde_json::to_value::<u8>(self.prompt_list_call_no.load(Ordering::Relaxed))
                    .expect("Failed to convert list call no to u8"),
            )),
            "tools/call" => {
                // Handle MCP tools/call method
                if let Some(params) = params {
                    if let Some(tool_name) = params.get("name").and_then(|n| n.as_str()) {
                        let tool_args = params.get("arguments");
                        // Delegate to the specific tool handler
                        return self.handle_incoming(tool_name, tool_args.cloned()).await;
                    }
                }
                Err(ServerError::MissingMethod)
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
    let test_server = Server::<JsonRpcStdioTransport, _>::new(handler, stdin, stdout).expect("Failed to create server");
    let _ = test_server.init().expect("Test server failed to init").await;
}
