pub mod schema;
#[cfg(test)]
mod test;
pub mod tool;

pub use schema::InternalCommand;

use crate::cli::chat::ToolSpec;

/// Get the tool specification for internal_command
pub fn get_tool_spec() -> ToolSpec {
    serde_json::from_value(serde_json::json!({
        "description": "Execute internal commands within the q chat system. This tool allows the AI assistant to directly execute q commands on behalf of the user.\n\nAvailable commands:\n\n- /clear: Clear the conversation history\n  Usage: /clear\n- /context: Manage context files for the chat session\n  Usage: /context [subcommand]\n- /help: Show help information\n  Usage: /help\n- /quit: Exit the application\n  Usage: /quit\n",
        "name": "internal_command",
        "input_schema": {
            "type": "object",
            "properties": {
                "command": {
                    "description": "The command to execute (e.g., \"quit\", \"context\", \"settings\")",
                    "type": "string"
                },
                "subcommand": {
                    "description": "Optional subcommand (e.g., \"list\", \"add\", \"remove\")",
                    "type": "string"
                },
                "args": {
                    "description": "Optional arguments for the command",
                    "items": {
                        "type": "string"
                    },
                    "type": "array"
                },
                "flags": {
                    "description": "Optional flags for the command",
                    "type": "object"
                },
                "tool_use_id": {
                    "description": "Tool use ID for tracking",
                    "type": "string"
                }
            },
            "required": ["command"],
            "type": "object"
        }
    })).expect("Failed to create tool spec")
}
