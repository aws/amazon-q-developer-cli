use std::collections::HashMap;

use serde::{
    Deserialize,
    Serialize,
};

/// Schema for the internal_command tool
///
/// This tool allows the AI to suggest commands within the Q chat system
/// when a user's natural language query indicates they want to perform a specific action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalCommand {
    /// The command to execute (without the leading slash)
    ///
    /// Examples:
    /// - "quit" - Exit the application
    /// - "clear" - Clear the conversation
    /// - "help" - Show help information
    /// - "context" - Manage context files
    /// - "profile" - Manage profiles
    /// - "tools" - Manage tools
    /// - "issue" - Create a GitHub issue
    /// - "compact" - Compact the conversation
    /// - "editor" - Open an editor for input
    /// - "usage" - Show token usage statistics
    pub command: String,

    /// Optional subcommand for commands that support them
    ///
    /// Examples:
    /// - For context: "add", "rm", "clear", "show", "hooks"
    /// - For profile: "list", "create", "delete", "set", "rename", "help"
    /// - For tools: "list", "trust", "untrust", "trustall", "reset", "help"
    /// - For prompts: "list", "get", "help"
    /// - For compact: "help"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subcommand: Option<String>,

    /// Optional arguments for the command
    ///
    /// Examples:
    /// - For context add: ["file.txt"] - The file to add as context Example: When user says "add
    ///   README.md to context", use args=["README.md"] Example: When user says "add these files to
    ///   context: file1.txt and file2.txt", use args=["file1.txt", "file2.txt"]
    ///
    /// - For context rm: ["file.txt"] or ["1"] - The file to remove or its index Example: When user
    ///   says "remove README.md from context", use args=["README.md"] Example: When user says
    ///   "remove the first context file", use args=["1"]
    ///
    /// - For profile create: ["my-profile"] - The name of the profile to create Example: When user
    ///   says "create a profile called work", use args=["work"] Example: When user says "make a new
    ///   profile for my personal projects", use args=["personal"]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,

    /// Optional flags for the command
    ///
    /// Examples:
    /// - For context add: {"global": ""} - Add to global context
    /// - For context show: {"expand": ""} - Show expanded context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<HashMap<String, String>>,

    /// Tool use ID for tracking purposes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
}
