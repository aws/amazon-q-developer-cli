//! Socket message protocol for Q Chat API mode
//!
//! This module defines the JSON-based message protocol used for communication
//! between Q Chat and external clients through Unix domain sockets.
//! 

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Core message structure for all socket communication
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SocketMessage {
    /// ISO 8601 timestamp when the message was created
    pub timestamp: String,
    /// The specific message type and its data
    #[serde(flatten)]
    pub message_type: MessageType,
    /// Additional metadata or context (optional)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub data: HashMap<String, serde_json::Value>,
}

/// Message types for the remaining active sockets (Input, Output, Thinking, Tools)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum MessageType {
    // Input messages from clients to Q Chat
    UserInput { 
        text: String 
    },
    SlashCommand { 
        command: String, 
        args: Vec<String> 
    },
    
    // Output messages from Q Chat to clients
    Response { 
        content: String, 
        formatted: bool 
    },
    
    // Thinking messages (internal reasoning)
    Thinking { 
        content: String, 
        step: Option<String> 
    },
    
    // Tool execution messages
    ToolRequest { 
        tool_name: String, 
        parameters: serde_json::Value, 
        id: String 
    },
    ToolResponse { 
        id: String, 
        result: serde_json::Value, 
        status: ToolStatus 
    },
    
    // Session lifecycle messages
    SessionEnd { 
        reason: String 
    },
}

/// Status of tool execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ToolStatus {
    Pending,
    Approved,
    Rejected,
    Executing,
    Success,
    Error,
}

impl SocketMessage {
    /// Create a new socket message with the current timestamp
    pub fn new(message_type: MessageType) -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            message_type,
            data: HashMap::new(),
        }
    }

    /// Convert message to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Create message from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}
