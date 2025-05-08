pub mod schema;
#[cfg(test)]
mod test;
pub mod tool;

pub use schema::InternalCommand;

use crate::cli::chat::command::Command;
use crate::cli::chat::tools::ToolSpec;

/// Get the tool specification for internal_command
///
/// This function builds the tool specification for the internal_command tool
/// with a comprehensive description of available commands.
pub fn get_tool_spec() -> ToolSpec {
    // Build a comprehensive description that includes all commands
    let mut description = "Tool for suggesting internal Q commands based on user intent. ".to_string();
    description.push_str("This tool helps the AI suggest appropriate commands within the Q chat system ");
    description.push_str("when a user's natural language query indicates they want to perform a specific action.\n\n");
    description.push_str("Available commands:\n");

    // Get detailed command descriptions from the Command enum
    let command_descriptions = Command::generate_llm_descriptions();

    // Add each command to the description with its short description
    for (name, cmd_info) in &command_descriptions {
        description.push_str(&format!("- {}: {}\n", name, cmd_info.short_description));
    }

    // Add detailed command information
    description.push_str("\nDetailed command information:\n");
    for (name, cmd_info) in &command_descriptions {
        description.push_str(&format!("\n## {}\n{}\n", name, cmd_info.full_description));
    }

    // Add information about how to access list data for commands that manage lists
    description.push_str("\nList data access commands:\n");
    description.push_str("- For context files: Use '/context show' to see all current context files\n");
    description.push_str("- For profiles: Use '/profile list' to see all available profiles\n");
    description.push_str("- For tools: Use '/tools list' to see all available tools and their status\n");
    description.push_str("These commands can be used to dynamically retrieve the current state of lists.\n");

    // Add examples of natural language that should trigger this tool
    description.push_str("\nExamples of natural language that should trigger this tool:\n");
    description.push_str("- \"Clear my conversation\" -> internal_command with command=\"clear\"\n");
    description.push_str(
        "- \"I want to add a file as context\" -> internal_command with command=\"context\", subcommand=\"add\"\n",
    );
    description.push_str(
        "- \"Show me the available profiles\" -> internal_command with command=\"profile\", subcommand=\"list\"\n",
    );
    description.push_str("- \"Exit the application\" -> internal_command with command=\"quit\"\n");
    description.push_str("- \"Add this file to my context\" -> internal_command with command=\"context\", subcommand=\"add\", args=[\"file.txt\"]\n");
    description.push_str(
        "- \"How do I switch profiles?\" -> internal_command with command=\"profile\", subcommand=\"help\"\n",
    );
    description.push_str("- \"I need to report a bug\" -> internal_command with command=\"issue\"\n");
    description.push_str("- \"Let me trust the file write tool\" -> internal_command with command=\"tools\", subcommand=\"trust\", args=[\"fs_write\"]\n");
    description.push_str(
        "- \"Show what tools are available\" -> internal_command with command=\"tools\", subcommand=\"list\"\n",
    );
    description.push_str("- \"I want to start fresh\" -> internal_command with command=\"clear\"\n");
    description.push_str("- \"Can you help me create a new profile?\" -> internal_command with command=\"profile\", subcommand=\"create\"\n");
    description.push_str("- \"I'd like to see what context files I have\" -> internal_command with command=\"context\", subcommand=\"show\"\n");
    description.push_str("- \"Remove the second context file\" -> internal_command with command=\"context\", subcommand=\"rm\", args=[\"2\"]\n");
    description.push_str(
        "- \"Trust all tools for this session\" -> internal_command with command=\"tools\", subcommand=\"trustall\"\n",
    );
    description.push_str(
        "- \"Reset tool permissions to default\" -> internal_command with command=\"tools\", subcommand=\"reset\"\n",
    );
    description.push_str("- \"I want to compact the conversation\" -> internal_command with command=\"compact\"\n");
    description.push_str("- \"Show me the help for context commands\" -> internal_command with command=\"context\", subcommand=\"help\"\n");
    description.push_str("- \"Show me my token usage\" -> internal_command with command=\"usage\"\n");

    // Create the tool specification
    serde_json::from_value(serde_json::json!({
        "name": "internal_command",
        "description": description,
        "input_schema": {
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The command to execute (without the leading slash). Available commands: quit, clear, help, context, profile, tools, issue, compact, editor, usage"
                },
                "subcommand": {
                    "type": "string",
                    "description": "Optional subcommand for commands that support them (context, profile, tools, prompts)"
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
        }
    })).expect("Failed to create tool spec")
}
