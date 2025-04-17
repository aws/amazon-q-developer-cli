pub mod schema;
#[cfg(test)]
mod test;
pub mod tool;

pub use schema::InternalCommand;

use crate::cli::chat::ToolSpec;
use crate::cli::chat::commands::registry::CommandRegistry;

/// Get the tool specification for internal_command
///
/// This function dynamically builds the tool specification for the internal_command tool
/// using the command registry to include all available commands and their descriptions.
pub fn get_tool_spec() -> ToolSpec {
    // Get the command registry
    let registry = CommandRegistry::global();

    // Generate LLM descriptions for all commands
    let command_descriptions = registry.generate_llm_descriptions();

    // Build a comprehensive description that includes all commands
    let mut description = "Tool for executing internal Q commands based on user intent. ".to_string();
    description.push_str("This tool allows the AI to execute commands within the Q chat system ");
    description.push_str("when a user's natural language query indicates they want to perform a specific action.\n\n");
    description.push_str("Available commands:\n");

    // Add each command to the description
    for name in registry.command_names() {
        if let Some(handler) = registry.get(name) {
            description.push_str(&format!("- {}: {}\n", handler.usage(), handler.description()));
        }
    }

    // Create the tool specification
    serde_json::from_value(serde_json::json!({
        "name": "internal_command",
        "description": description,
        "input_schema": {
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The command to execute (without the leading slash). Available commands: quit, clear, help, context, profile, tools, issue, compact, editor"
                },
                "subcommand": {
                    "type": "string",
                    "description": "Optional subcommand for commands that support them (context, profile, tools)"
                },
                "args": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "description": "Optional arguments for the command"
                },
                "flags": {
                    "type": "object",
                    "additionalProperties": {
                        "type": "string"
                    },
                    "description": "Optional flags for the command"
                }
            },
            "required": ["command"]
        },
        "command_details": command_descriptions
    })).expect("Failed to create tool spec")
}
