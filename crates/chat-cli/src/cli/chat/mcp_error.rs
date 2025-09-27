use std::collections::HashMap;

use crossterm::style::{
    self,
    Color,
};
use crossterm::{
    execute,
    queue,
};
use serde_json::Value;

use crate::cli::chat::tool_manager::PromptBundle;
use crate::cli::chat::{
    ChatError,
    ChatSession,
};

/// Represents parsed MCP error details for generating user-friendly messages.
#[derive(Debug)]
pub struct McpErrorDetails {
    pub code: String,
    pub message: String,
    pub path: Vec<String>,
}

/// Parses MCP error JSON to extract all validation errors for user-friendly messages.
///
/// Attempts to extract JSON error details from MCP server error strings to provide
/// more specific and user-friendly error messages for all validation failures.
///
/// # Arguments
/// * `error_str` - The raw error string from the MCP server
///
/// # Returns
/// * `Vec<McpErrorDetails>` containing all parsed errors, empty if parsing fails
pub fn parse_all_mcp_error_details(error_str: &str) -> Vec<McpErrorDetails> {
    // Try to extract JSON from error string - MCP errors often contain JSON in the message
    let json_start = match error_str.find('[') {
        Some(pos) => pos,
        None => return Vec::new(),
    };
    let json_end = match error_str.rfind(']') {
        Some(pos) => pos + 1,
        None => return Vec::new(),
    };
    let json_str = &error_str[json_start..json_end];

    let error_array: Vec<Value> = match serde_json::from_str(json_str) {
        Ok(array) => array,
        Err(_) => return Vec::new(),
    };

    error_array
        .iter()
        .filter_map(|error_val| {
            let error_obj = error_val.as_object()?;
            let code = error_obj.get("code")?.as_str()?;
            let message = error_obj.get("message")?.as_str().unwrap_or("");
            let path = match error_obj.get("path").and_then(|p| p.as_array()) {
                Some(path_array) => path_array
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect(),
                None => Vec::new(),
            };

            Some(McpErrorDetails {
                code: code.to_string(),
                message: message.to_string(),
                path,
            })
        })
        .collect()
}

/// Handles MCP -32602 (Invalid params) errors with user-friendly messages.
///
/// Parses the error details and displays appropriate error messages based on the
/// specific type of invalid parameter error (missing args, invalid values, etc.).
pub fn handle_mcp_invalid_params_error(
    name: &str,
    error_str: &str,
    prompts: &HashMap<String, Vec<PromptBundle>>,
    session: &mut ChatSession,
) -> Result<(), ChatError> {
    let all_errors = parse_all_mcp_error_details(error_str);

    if !all_errors.is_empty() {
        // Check if this is a missing required arguments error
        if all_errors.len() == 1
            && all_errors[0].code == "invalid_type"
            && all_errors[0].message == "Required"
            && all_errors[0].path.is_empty()
        {
            display_missing_args_error(name, prompts, session)?;
            return Ok(());
        }

        // Display validation errors
        queue!(
            session.stderr,
            style::Print("\n"),
            style::SetForegroundColor(Color::Yellow),
            style::Print("Error: Invalid arguments for prompt '"),
            style::SetForegroundColor(Color::Cyan),
            style::Print(name),
            style::SetForegroundColor(Color::Yellow),
            style::Print("':\n"),
            style::SetForegroundColor(Color::Reset),
        )?;

        for error in &all_errors {
            let field_name = if error.path.is_empty() {
                "unknown field".to_string()
            } else {
                error.path.join(".")
            };

            queue!(
                session.stderr,
                style::Print("  • "),
                style::SetForegroundColor(Color::Yellow),
                style::Print(&field_name),
                style::SetForegroundColor(Color::Reset),
                style::Print(": "),
                style::Print(&error.message),
                style::Print("\n"),
            )?;
        }

        // Show prompt details for reference
        if let Some(bundles) = prompts.get(name) {
            if let Some(bundle) = bundles.first() {
                display_prompt_usage_hint(&bundle.prompt_get.name, &bundle.prompt_get.arguments, session)?;
            }
        }
    } else {
        // Fallback for unparsable errors
        queue!(
            session.stderr,
            style::Print("\n"),
            style::SetForegroundColor(Color::Yellow),
            style::Print("Error: Invalid arguments for prompt '"),
            style::SetForegroundColor(Color::Cyan),
            style::Print(name),
            style::SetForegroundColor(Color::Yellow),
            style::Print("'. "),
            style::Print(error_str),
            style::SetForegroundColor(Color::Reset),
            style::Print("\n"),
        )?;
    }

    execute!(session.stderr)?;
    Ok(())
}

/// Handles MCP -32603 (Internal error) errors with structured information display.
///
/// Attempts to parse structured error information from the server response
/// and displays it in a user-friendly format.
pub fn handle_mcp_internal_error(name: &str, error_str: &str, session: &mut ChatSession) -> Result<(), ChatError> {
    // Try to parse JSON error response
    if let Some(json_start) = error_str.find('{') {
        if let Some(json_end) = error_str.rfind('}') {
            let json_str = &error_str[json_start..=json_end];
            if let Ok(error_obj) = serde_json::from_str::<Value>(json_str) {
                if let Some(error_field) = error_obj.get("error") {
                    let message = error_field
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("Internal server error");

                    queue!(
                        session.stderr,
                        style::Print("\n"),
                        style::SetForegroundColor(Color::Red),
                        style::Print("❌ Server Error: "),
                        style::SetForegroundColor(Color::Cyan),
                        style::Print(name),
                        style::SetForegroundColor(Color::Red),
                        style::Print(" - "),
                        style::Print(message),
                        style::SetForegroundColor(Color::Reset),
                        style::Print("\n"),
                    )?;

                    // Show additional details if available
                    if let Some(data) = error_field.get("data") {
                        if let Some(details) = data.get("details").and_then(|d| d.as_str()) {
                            queue!(
                                session.stderr,
                                style::SetForegroundColor(Color::DarkGrey),
                                style::Print("  Details: "),
                                style::Print(details),
                                style::SetForegroundColor(Color::Reset),
                                style::Print("\n"),
                            )?;
                        }
                    }

                    execute!(session.stderr)?;
                    return Ok(());
                }
            }
        }
    }

    // Fallback for unparsable internal errors
    queue!(
        session.stderr,
        style::Print("\n"),
        style::SetForegroundColor(Color::Red),
        style::Print("❌ Internal server error for prompt "),
        style::SetForegroundColor(Color::Cyan),
        style::Print(name),
        style::SetForegroundColor(Color::Red),
        style::Print(". "),
        style::Print(error_str),
        style::SetForegroundColor(Color::Reset),
        style::Print("\n"),
    )?;
    execute!(session.stderr)?;
    Ok(())
}

/// Displays a user-friendly error message for missing required arguments.
///
/// Shows usage information and lists all required and optional arguments
/// with descriptions when available.
pub fn display_missing_args_error(
    prompt_name: &str,
    prompts: &HashMap<String, Vec<PromptBundle>>,
    session: &mut ChatSession,
) -> Result<(), ChatError> {
    queue!(
        session.stderr,
        style::Print("\n"),
        style::SetForegroundColor(Color::Yellow),
        style::Print("Error: Missing required arguments for prompt "),
        style::SetForegroundColor(Color::Cyan),
        style::Print(prompt_name),
        style::SetForegroundColor(Color::Reset),
        style::Print("\n\n"),
    )?;

    // Extract the actual prompt name from server/prompt format if needed
    let actual_prompt_name = if let Some((_, name)) = prompt_name.split_once('/') {
        name
    } else {
        prompt_name
    };

    if let Some(bundles) = prompts.get(actual_prompt_name) {
        if let Some(bundle) = bundles.first() {
            if let Some(args) = &bundle.prompt_get.arguments {
                let required_args: Vec<_> = args.iter().filter(|arg| arg.required == Some(true)).collect();
                let optional_args: Vec<_> = args.iter().filter(|arg| arg.required != Some(true)).collect();

                // Usage line
                queue!(
                    session.stderr,
                    style::Print("Usage: "),
                    style::SetForegroundColor(Color::Cyan),
                    style::Print("@"),
                    style::Print(prompt_name),
                )?;

                for arg in &required_args {
                    queue!(
                        session.stderr,
                        style::Print(" <"),
                        style::Print(&arg.name),
                        style::Print(">"),
                    )?;
                }
                for arg in &optional_args {
                    queue!(
                        session.stderr,
                        style::Print(" ["),
                        style::Print(&arg.name),
                        style::Print("]"),
                    )?;
                }

                queue!(
                    session.stderr,
                    style::SetForegroundColor(Color::Reset),
                    style::Print("\n"),
                )?;

                if !args.is_empty() {
                    queue!(session.stderr, style::Print("\nArguments:\n"),)?;

                    // Show required arguments first
                    for arg in required_args {
                        queue!(
                            session.stderr,
                            style::Print("  "),
                            style::SetForegroundColor(Color::Red),
                            style::Print("(required) "),
                            style::SetForegroundColor(Color::Cyan),
                            style::Print(&arg.name),
                            style::SetForegroundColor(Color::Reset),
                        )?;
                        if let Some(desc) = &arg.description {
                            if !desc.trim().is_empty() {
                                queue!(session.stderr, style::Print(" - "), style::Print(desc),)?;
                            }
                        }
                        queue!(session.stderr, style::Print("\n"))?;
                    }

                    // Then show optional arguments
                    for arg in optional_args {
                        queue!(
                            session.stderr,
                            style::Print("  "),
                            style::SetForegroundColor(Color::DarkGrey),
                            style::Print("(optional) "),
                            style::SetForegroundColor(Color::Cyan),
                            style::Print(&arg.name),
                            style::SetForegroundColor(Color::Reset),
                        )?;
                        if let Some(desc) = &arg.description {
                            if !desc.trim().is_empty() {
                                queue!(session.stderr, style::Print(" - "), style::Print(desc),)?;
                            }
                        }
                        queue!(session.stderr, style::Print("\n"))?;
                    }
                }
            }
        }
    }

    execute!(session.stderr)?;
    Ok(())
}

/// Displays a usage hint for a prompt with its arguments.
pub fn display_prompt_usage_hint(
    name: &str,
    arguments: &Option<Vec<rmcp::model::PromptArgument>>,
    session: &mut ChatSession,
) -> Result<(), ChatError> {
    queue!(
        session.stderr,
        style::Print("\n"),
        style::SetForegroundColor(Color::DarkGrey),
        style::Print("Usage: "),
        style::SetForegroundColor(Color::Cyan),
        style::Print("@"),
        style::Print(name),
    )?;

    if let Some(args) = arguments {
        for arg in args {
            match arg.required {
                Some(true) => {
                    queue!(
                        session.stderr,
                        style::Print(" <"),
                        style::Print(&arg.name),
                        style::Print(">"),
                    )?;
                },
                _ => {
                    queue!(
                        session.stderr,
                        style::Print(" ["),
                        style::Print(&arg.name),
                        style::Print("]"),
                    )?;
                },
            }
        }
    }

    queue!(
        session.stderr,
        style::SetForegroundColor(Color::Reset),
        style::Print("\n"),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_all_mcp_error_details() {
        // Test parsing multiple validation errors
        let error_str = r#"MCP error -32602: Invalid arguments for prompt validation-test: [
  {
    "validation": "regex",
    "code": "invalid_string",
    "message": "Must be a valid email ending in .com",
    "path": [
      "email"
    ]
  },
  {
    "validation": "regex",
    "code": "invalid_string",
    "message": "Must be a positive number",
    "path": [
      "count"
    ]
  }
]"#;

        let errors = parse_all_mcp_error_details(error_str);
        assert_eq!(errors.len(), 2);

        // First error
        assert_eq!(errors[0].code, "invalid_string");
        assert_eq!(errors[0].message, "Must be a valid email ending in .com");
        assert_eq!(errors[0].path, vec!["email"]);

        // Second error
        assert_eq!(errors[1].code, "invalid_string");
        assert_eq!(errors[1].message, "Must be a positive number");
        assert_eq!(errors[1].path, vec!["count"]);

        // Test empty array
        let empty_error = "MCP error -32602: Invalid arguments for prompt test: []";
        let empty_errors = parse_all_mcp_error_details(empty_error);
        assert_eq!(empty_errors.len(), 0);

        // Test invalid JSON
        let invalid_error = "Not a valid MCP error";
        let invalid_errors = parse_all_mcp_error_details(invalid_error);
        assert_eq!(invalid_errors.len(), 0);
    }

    #[test]
    fn test_parse_32603_error_with_data() {
        // Test parsing -32603 error with data object
        let error_str = r#"MCP error -32603: {
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32603,
                "message": "Tool execution failed",
                "data": {
                    "tool": "get_weather",
                    "reason": "API service unavailable"
                }
            }
        }"#;

        // Extract JSON part
        let json_start = error_str.find('{').unwrap();
        let json_end = error_str.rfind('}').unwrap();
        let json_str = &error_str[json_start..=json_end];

        let error_obj: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let error_field = error_obj.get("error").unwrap();

        let message = error_field.get("message").unwrap().as_str().unwrap();
        assert_eq!(message, "Tool execution failed");

        let data = error_field.get("data").unwrap();
        assert_eq!(data.get("tool").unwrap().as_str().unwrap(), "get_weather");
        assert_eq!(data.get("reason").unwrap().as_str().unwrap(), "API service unavailable");
    }
}
