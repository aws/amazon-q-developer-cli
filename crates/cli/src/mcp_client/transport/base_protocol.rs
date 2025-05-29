//! Referencing https://spec.modelcontextprotocol.io/specification/2024-11-05/basic/messages/
//! Protocol Revision 2024-11-05
use serde::{
    Deserialize,
    Serialize,
};

pub type RequestId = u64;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JsonRpcVersion(String);

impl Default for JsonRpcVersion {
    fn default() -> Self {
        JsonRpcVersion("2.0".to_owned())
    }
}

impl JsonRpcVersion {
    pub fn as_u32_vec(&self) -> Vec<u32> {
        self.0
            .split(".")
            .map(|n| n.parse::<u32>().unwrap())
            .collect::<Vec<u32>>()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
#[serde(deny_unknown_fields)]
// DO NOT change the order of these variants. This body of json is [untagged](https://serde.rs/enum-representations.html#untagged)
// The categorization of the deserialization depends on the order in which the variants are
// declared.
pub enum JsonRpcMessage {
    Response(JsonRpcResponse),
    Notification(JsonRpcNotification),
    Request(JsonRpcRequest),
}

impl JsonRpcMessage {
    pub fn is_initialize(&self) -> bool {
        match self {
            JsonRpcMessage::Request(req) => req.method == "initialize",
            _ => false,
        }
    }

    pub fn is_shutdown(&self) -> bool {
        match self {
            JsonRpcMessage::Notification(notif) => notif.method == "notification/shutdown",
            _ => false,
        }
    }

    pub fn id(&self) -> Option<u64> {
        match self {
            JsonRpcMessage::Request(req) => Some(req.id),
            JsonRpcMessage::Response(resp) => Some(resp.id),
            JsonRpcMessage::Notification(_) => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct JsonRpcRequest {
    pub jsonrpc: JsonRpcVersion,
    pub id: RequestId,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct JsonRpcResponse {
    pub jsonrpc: JsonRpcVersion,
    pub id: RequestId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct JsonRpcNotification {
    pub jsonrpc: JsonRpcVersion,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum TransportType {
    #[default]
    Stdio,
    Sse,
    Websocket,
}
