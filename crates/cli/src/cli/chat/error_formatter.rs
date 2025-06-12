use serde_json;

/// Formats an MCP error message to be more user-friendly.
/// 
/// This function extracts nested JSON from the error message and formats it
/// with proper indentation and newlines.
///
/// # Arguments
///
/// * `err` - A reference to a serde_json::Value containing the error information
///
/// # Returns
///
/// A formatted string representation of the error message
pub fn format_mcp_error(err: &serde_json::Value) -> String {
    // Extract the message field from the error JSON
    if let Some(message) = err.get("message").and_then(|m| m.as_str()) {
        // Check if the message contains a nested JSON array
        if let Some(start_idx) = message.find('[') {
            if let Some(end_idx) = message.rfind(']') {
                let prefix = &message[..start_idx].trim();
                let nested_json = &message[start_idx..=end_idx];
                
                // Try to parse the nested JSON
                if let Ok(nested_value) = serde_json::from_str::<serde_json::Value>(nested_json) {
                    // Format the error message with the prefix and pretty-printed nested JSON
                    return format!(
                        "{}\n{}",
                        prefix,
                        serde_json::to_string_pretty(&nested_value)
                            .unwrap_or_else(|_| nested_json.to_string())
                    );
                }
            }
        }
        // If we couldn't extract and parse nested JSON, return the original message
        return message.to_string();
    }
    
    // Fallback to pretty-printing the entire error if message field is missing
    serde_json::to_string_pretty(err).unwrap_or_else(|_| format!("{:?}", err))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_format_mcp_error_with_nested_json() {
        let error = json!({
            "code": -32602,
            "message": "MCP error -32602: Invalid arguments for prompt agent_script_coco_was_sev2_ticket_details_retrieve: [\n  {\n    \"code\": \"invalid_type\",\n    \"expected\": \"object\",\n    \"received\": \"undefined\",\n    \"path\": [],\n    \"message\": \"Required\"\n  }\n]"
        });
        
        let formatted = format_mcp_error(&error);
        
        // Extract the prefix and JSON part from the formatted string
        let parts: Vec<&str> = formatted.split('\n').collect();
        let prefix = parts[0];
        let json_part = &formatted[prefix.len() + 1..];
        
        // Check that the prefix is correct
        assert_eq!(prefix, "MCP error -32602: Invalid arguments for prompt agent_script_coco_was_sev2_ticket_details_retrieve:");
        
        // Parse the JSON part to compare the actual content rather than the exact string
        let parsed_json: serde_json::Value = serde_json::from_str(json_part).expect("Failed to parse JSON part");
        
        // Expected JSON structure
        let expected_json = json!([
            {
                "code": "invalid_type",
                "expected": "object",
                "received": "undefined",
                "path": [],
                "message": "Required"
            }
        ]);
        
        // Compare the parsed JSON values
        assert_eq!(parsed_json, expected_json);
    }

    #[test]
    fn test_format_mcp_error_without_nested_json() {
        let error = json!({
            "code": -32602,
            "message": "MCP error -32602: Invalid arguments for prompt"
        });
        
        let formatted = format_mcp_error(&error);
        
        assert_eq!(formatted, "MCP error -32602: Invalid arguments for prompt");
    }

    #[test]
    fn test_format_mcp_error_non_mcp_error() {
        let error = json!({
            "error": "Unknown error occurred"
        });
        
        let formatted = format_mcp_error(&error);
        
        // Should pretty-print the entire error
        assert_eq!(formatted, "{\n  \"error\": \"Unknown error occurred\"\n}");
    }

    #[test]
    fn test_format_mcp_error_empty_message() {
        let error = json!({
            "code": -32602,
            "message": ""
        });
        
        let formatted = format_mcp_error(&error);
        
        assert_eq!(formatted, "");
    }

    #[test]
    fn test_format_mcp_error_missing_message() {
        let error = json!({
            "code": -32602
        });
        
        let formatted = format_mcp_error(&error);
        
        assert_eq!(formatted, "{\n  \"code\": -32602\n}");
    }

    #[test]
    fn test_format_mcp_error_malformed_nested_json() {
        let error = json!({
            "code": -32602,
            "message": "MCP error -32602: Invalid arguments for prompt: [{\n  \"code\": \"invalid_type\",\n  \"expected\": \"object\",\n  \"received\": \"undefined\",\n  \"path\": [],\n  \"message\": \"Required\"\n"
        });
        
        let formatted = format_mcp_error(&error);
        
        // Should return the original message since the nested JSON is malformed
        assert_eq!(formatted, "MCP error -32602: Invalid arguments for prompt: [{\n  \"code\": \"invalid_type\",\n  \"expected\": \"object\",\n  \"received\": \"undefined\",\n  \"path\": [],\n  \"message\": \"Required\"\n");
    }
}
