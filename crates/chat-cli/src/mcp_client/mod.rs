#![allow(dead_code)]

mod client;
mod facilitator_types;
mod server;
mod transport;

pub use client::*;
pub use facilitator_types::*;
#[allow(unused_imports)]
pub use server::*;
pub use transport::*;

/// Error codes as defined in the MCP protocol.
///
/// These error codes are based on the JSON-RPC 2.0 specification with additional
/// MCP-specific error codes in the -32000 to -32099 range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum McpError {
    /// Invalid JSON was received by the server.
    /// An error occurred on the server while parsing the JSON text.
    ParseError           = -32700,

    /// The JSON sent is not a valid Request object.
    InvalidRequest       = -32600,

    /// The method does not exist / is not available.
    MethodNotFound       = -32601,

    /// Invalid method parameter(s).
    InvalidParams        = -32602,

    /// Internal JSON-RPC error.
    InternalError        = -32603,

    /// Server has not been initialized.
    /// This error is returned when a request is made before the server
    /// has been properly initialized.
    ServerNotInitialized = -32002,

    /// Unknown error code.
    /// This error is returned when an error code is received that is not
    /// recognized by the implementation.
    UnknownErrorCode     = -32001,

    /// Request failed.
    /// This error is returned when a request fails for a reason not covered
    /// by other error codes.
    RequestFailed        = -32000,
}

impl From<i32> for McpError {
    fn from(code: i32) -> Self {
        match code {
            -32700 => McpError::ParseError,
            -32600 => McpError::InvalidRequest,
            -32601 => McpError::MethodNotFound,
            -32602 => McpError::InvalidParams,
            -32603 => McpError::InternalError,
            -32002 => McpError::ServerNotInitialized,
            -32001 => McpError::UnknownErrorCode,
            -32000 => McpError::RequestFailed,
            _ => McpError::UnknownErrorCode,
        }
    }
}

impl From<McpError> for i32 {
    fn from(code: McpError) -> Self {
        code as i32
    }
}

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
