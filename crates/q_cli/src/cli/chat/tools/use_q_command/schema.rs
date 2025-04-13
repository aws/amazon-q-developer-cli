use std::collections::HashMap;

use serde::{
    Deserialize,
    Serialize,
};

/// Tool for executing internal Q commands based on user intent
///
/// This tool allows the AI to execute commands within the Q chat system when a user's
/// natural language query indicates they want to perform a specific action. The AI should
/// use this tool when:
///
/// 1. The user explicitly asks to execute a command (e.g., "clear my chat history")
/// 2. The user asks how to perform an action that maps to a command (e.g., "how do I add context?")
/// 3. The user expresses intent that clearly maps to a command function (e.g., "I want to quit")
/// 4. The user is struggling with command syntax and would benefit from AI assistance
/// 5. The user wants to perform a common action that has a direct command equivalent
///
/// Examples of natural language that should trigger this tool:
/// - "Clear my conversation" -> use_q_command with command="clear"
/// - "I want to add a file as context" -> use_q_command with command="context", subcommand="add"
/// - "Show me the available profiles" -> use_q_command with command="profile", subcommand="list"
/// - "Exit the application" -> use_q_command with command="quit"
/// - "Add this file to my context" -> use_q_command with command="context", subcommand="add",
///   args=["file.txt"]
/// - "How do I switch profiles?" -> use_q_command with command="profile", subcommand="help"
/// - "I need to report a bug" -> use_q_command with command="issue"
/// - "Let me trust the file write tool" -> use_q_command with command="tools", subcommand="trust",
///   args=["fs_write"]
/// - "Show what tools are available" -> use_q_command with command="tools", subcommand="list"
/// - "I want to start fresh" -> use_q_command with command="clear"
/// - "Can you help me create a new profile?" -> use_q_command with command="profile",
///   subcommand="create"
/// - "I'd like to see what context files I have" -> use_q_command with command="context",
///   subcommand="show"
/// - "Remove the second context file" -> use_q_command with command="context", subcommand="rm",
///   args=["2"]
/// - "Trust all tools for this session" -> use_q_command with command="tools",
///   subcommand="trustall"
/// - "Reset tool permissions to default" -> use_q_command with command="tools", subcommand="reset"
/// - "I want to compact the conversation" -> use_q_command with command="compact"
/// - "Show me the help for context commands" -> use_q_command with command="context",
///   subcommand="help"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UseQCommand {
    /// The command to execute (without the leading slash)
    ///
    /// Available commands:
    /// - "quit" - Exit the application When to use: User wants to exit, close, or quit the
    ///   application Example queries: "exit", "quit", "close the app", "I'm done", "goodbye", "bye"
    ///
    /// - "clear" - Clear the conversation history When to use: User wants to start fresh or clear
    ///   the chat history Example queries: "clear chat", "clear history", "start fresh", "reset our
    ///   conversation", "clear the screen", "start over"
    ///
    /// - "help" - Show help information When to use: User asks for help or information about
    ///   available commands Example queries: "help", "what commands can I use?", "show available
    ///   commands", "what can you do?", "how do I use this?"
    ///
    /// - "context" - Manage context files (requires subcommand) When to use: User wants to add,
    ///   remove, view, or clear context files Example queries: "add file to context", "show my
    ///   context", "remove context file", "what context do I have?", "clear all context"
    ///
    /// - "profile" - Manage profiles (requires subcommand) When to use: User wants to manage,
    ///   switch, create, or delete profiles Example queries: "list profiles", "create new profile",
    ///   "switch profile", "delete profile", "what profiles do I have?", "change to a different
    ///   profile"
    ///
    /// - "tools" - Manage tool permissions (requires subcommand) When to use: User wants to view,
    ///   enable, disable, or manage tool permissions Example queries: "show available tools",
    ///   "enable file writing", "trust bash execution", "what tools can you use?", "make all tools
    ///   trusted", "reset tool permissions"
    ///
    /// - "issue" - Report an issue When to use: User wants to report a bug or request a feature
    ///   Example queries: "report a bug", "submit feedback", "I found an issue", "this isn't
    ///   working right", "I want to request a feature"
    ///
    /// - "compact" - Summarize the conversation to free up context space When to use: User wants to
    ///   continue the conversation but free up context space Example queries: "compact the
    ///   conversation", "summarize our chat", "free up context space", "the context is getting too
    ///   long"
    ///
    /// - "editor" - Open an external editor for input When to use: User wants to write a longer
    ///   message or code snippet in an external editor Example queries: "open editor", "I want to
    ///   write a longer message", "let me use vim"
    pub command: String,

    /// Optional subcommand for commands that support them
    ///
    /// Subcommands by command:
    /// - context:
    ///   - "add" - Add a file to context (requires file path as arg) Example: "add README.md to my
    ///     context" -> subcommand="add", args=["README.md"]
    ///   - "rm" - Remove a file from context (requires file path or index as arg) Example: "remove
    ///     the second context file" -> subcommand="rm", args=["2"]
    ///   - "clear" - Clear all context files Example: "clear all my context files" ->
    ///     subcommand="clear"
    ///   - "show" - Display current context files Example: "show me what context I have" ->
    ///     subcommand="show"
    ///   - "help" - Show help for context commands Example: "how do I use context?" ->
    ///     subcommand="help"
    ///
    /// - profile:
    ///   - "list" - List available profiles Example: "show me my profiles" -> subcommand="list"
    ///   - "create" - Create a new profile (requires profile name as arg) Example: "create a work
    ///     profile" -> subcommand="create", args=["work"]
    ///   - "delete" - Delete a profile (requires profile name as arg) Example: "delete my test
    ///     profile" -> subcommand="delete", args=["test"]
    ///   - "set" - Switch to a profile (requires profile name as arg) Example: "switch to my work
    ///     profile" -> subcommand="set", args=["work"]
    ///   - "rename" - Rename a profile (requires old and new names as args) Example: "rename my
    ///     work profile to job" -> subcommand="rename", args=["work", "job"]
    ///   - "help" - Show help for profile commands Example: "how do profiles work?" ->
    ///     subcommand="help"
    ///
    /// - tools:
    ///   - "list" - List available tools and their permission status Example: "what tools are
    ///     available?" -> subcommand="list"
    ///   - "enable" - Enable a tool (requires tool name as arg) Example: "enable the fs_write tool"
    ///     -> subcommand="enable", args=["fs_write"]
    ///   - "disable" - Disable a tool (requires tool name as arg) Example: "disable execute_bash"
    ///     -> subcommand="disable", args=["execute_bash"]
    ///   - "trust" - Trust a tool to run without confirmation (requires tool name as arg) Example:
    ///     "trust fs_write" -> subcommand="trust", args=["fs_write"]
    ///   - "untrust" - Require confirmation for a tool (requires tool name as arg) Example: "make
    ///     fs_write require confirmation" -> subcommand="untrust", args=["fs_write"]
    ///   - "trustall" - Trust all tools to run without confirmation Example: "trust all tools" ->
    ///     subcommand="trustall"
    ///   - "reset" - Reset all tool permissions to defaults Example: "reset tool permissions" ->
    ///     subcommand="reset"
    ///   - "help" - Show help for tools commands Example: "how do I manage tool permissions?" ->
    ///     subcommand="help"
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
    ///
    /// - For profile set: ["my-profile"] - The profile to switch to Example: When user says "switch
    ///   to my work profile", use args=["work"] Example: When user says "use the personal profile",
    ///   use args=["personal"]
    ///
    /// - For profile delete: ["my-profile"] - The profile to delete Example: When user says "delete
    ///   my test profile", use args=["test"]
    ///
    /// - For profile rename: ["old-name", "new-name"] - The old and new profile names Example: When
    ///   user says "rename profile personal to home", use args=["personal", "home"]
    ///
    /// - For tools enable/disable/trust/untrust: ["tool_name"] - The name of the tool Example: When
    ///   user says "trust fs_write", use args=["fs_write"] Example: When user says "disable
    ///   execute_bash", use args=["execute_bash"]
    ///
    /// - For issue: ["Description of the issue"] - The issue description Example: When user says
    ///   "report a bug where context isn't loading", use args=["Context isn't loading properly"]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,

    /// Optional flags for the command
    ///
    /// Examples:
    /// - For context add: {"force": "true"} - Force add even if the file is large Example: When
    ///   user says "force add large-file.log to context", use flags={"force": "true"}
    ///
    /// - For context show: {"global": "true"} - Show only global context files Example: When user
    ///   says "show my global context files", use flags={"global": "true"}
    /// - For context show: {"expand": "true"} - Show expanded file contents Example: When user says
    ///   "show my context with file contents", use flags={"expand": "true"}
    ///
    /// - For tools enable: {"all": "true"} - Enable all tools Example: When user says "enable all
    ///   tools", use flags={"all": "true"}
    ///
    /// - For profile list: {"verbose": "true"} - Show detailed profile information Example: When
    ///   user says "show detailed profile info", use flags={"verbose": "true"}
    ///
    /// - For compact: {"keep": "3"} - Keep the specified number of recent messages Example: When
    ///   user says "compact but keep the last 3 messages", use flags={"keep": "3"}
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<HashMap<String, String>>,

    /// Tool use ID for tracking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_use_q_command_deserialize() {
        // Test with minimal fields
        let json = json!({
            "command": "quit"
        });

        let command: UseQCommand = serde_json::from_value(json).unwrap();
        assert_eq!(command.command, "quit");
        assert!(command.subcommand.is_none());
        assert!(command.args.is_none());
        assert!(command.flags.is_none());
        assert!(command.tool_use_id.is_none());

        // Test with all fields
        let json = json!({
            "command": "context",
            "subcommand": "add",
            "args": ["file.txt"],
            "flags": {
                "force": "true"
            },
            "tool_use_id": "test-id"
        });

        let command: UseQCommand = serde_json::from_value(json).unwrap();
        assert_eq!(command.command, "context");
        assert_eq!(command.subcommand, Some("add".to_string()));
        assert_eq!(command.args, Some(vec!["file.txt".to_string()]));
        assert_eq!(command.flags.as_ref().unwrap().get("force").unwrap(), "true");
        assert_eq!(command.tool_use_id, Some("test-id".to_string()));
    }
}
