//! # Token Usage Tracking for Tools
//! 
//! This module provides token cost calculation and display for Amazon Q CLI tools.
//! 
//! ## What are tokens?
//! Tokens represent the computational cost of using a tool, similar to how cloud services
//! measure API usage. Each tool has a token cost based on:
//! - Tool name length
//! - Description length  
//! - Input schema complexity (JSON structure defining tool parameters)
//! 
//! ## Why track token usage?
//! - **User Awareness**: Users can see the relative cost of different tools
//! - **Resource Planning**: Helps users understand which tools are more expensive
//! - **Performance Optimization**: Identifies tools that might need optimization
//! 
//! ## How it works:
//! 1. **Calculation**: Analyzes tool specifications to estimate token cost
//! 2. **Caching**: Results are cached to avoid recalculating the same tools
//! 3. **Display**: Token costs appear next to tool names (e.g., "fs_read (360 tokens)")
//! 4. **Batch Processing**: Multiple tools can be processed efficiently together
//! 
//! ## Example Usage:
//! ```ignore
//! // Calculate costs for multiple tools efficiently
//! let costs = ToolTokenCalculator::calculate_batch_cli_tool_tokens(&tool_specs);
//! ```

use std::collections::HashMap;

use crate::cli::chat::token_counter::{CharCount, TokenCount};
use crate::api_client::model::ToolSpecification as CliToolSpec;

#[cfg(test)]
use crate::cli::chat::token_counter::TokenCounter;

/// Maximum allowed size for a tool specification to prevent resource exhaustion
/// This limit is set to 1MB based on:
/// - Typical tool specifications are 1-10KB
/// - Large complex tools with extensive schemas might reach 100KB
/// - 1MB provides generous headroom while preventing abuse
/// - Matches similar limits used elsewhere in the codebase for content size
const MAX_TOOL_SPEC_SIZE: usize = 1_000_000; // 1MB

/// Result type for token calculation operations
pub type TokenResult<T> = Result<T, TokenCalculationError>;

/// Errors that can occur during token calculation
#[derive(Debug, Clone)]
pub enum TokenCalculationError {
    /// Tool specification is too large
    ToolTooLarge { size: usize, max_size: usize },
}

impl std::fmt::Display for TokenCalculationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenCalculationError::ToolTooLarge { size, max_size } => {
                write!(f, "Tool specification too large: {} bytes (max: {} bytes)", size, max_size)
            }
        }
    }
}

impl std::error::Error for TokenCalculationError {}

/// Service for calculating token costs of tools using character-based estimation.
/// 
/// This calculator provides straightforward token estimation for CLI tools.
/// 
/// ## Features
/// - Direct token calculation from tool specifications
/// - Input validation and safety checks
/// - Thread-safe operation
/// - Batch processing for multiple tools
/// 
/// ## Usage
/// ```ignore
/// // Calculate costs for multiple tools efficiently
/// match ToolTokenCalculator::calculate_batch_cli_tool_tokens(&tool_specs) {
///     Ok(token_costs) => println!("Calculated costs for {} tools", token_costs.len()),
///     Err(e) => eprintln!("Calculation failed: {}", e),
/// }
/// ```
#[derive(Debug)]
pub struct ToolTokenCalculator {
    // No fields needed - this is a stateless calculator
}

impl ToolTokenCalculator {
    /// Create a new token calculator.
    pub fn new() -> Self {
        Self {}
    }

    /// Calculate token costs for multiple CLI tools efficiently
    /// 
    /// This method processes CLI tools and continues processing even if some tools fail.
    /// Failed tools are logged but don't prevent processing of valid tools.
    /// 
    /// # Arguments
    /// * `tools` - Slice of CLI tools to analyze
    /// 
    /// # Returns
    /// * `Ok(HashMap<String, TokenCount>)` - Map of tool names to token counts for successful tools
    /// * `Err(TokenCalculationError)` - Only if critical system error occurs
    pub fn calculate_batch_cli_tool_tokens(tools: &[&CliToolSpec]) -> TokenResult<HashMap<String, TokenCount>> {
        let mut results = HashMap::with_capacity(tools.len());
        let mut failed_tools = Vec::new();
        
        // Calculate all tools (cache is cleared before this method is called)
        for tool in tools {
            let tool_name = &tool.name;
            
            // Skip tools with empty names
            if tool_name.is_empty() {
                failed_tools.push("Tool has empty name".to_string());
                continue;
            }
            
            match Self::calculate_cli_tool_content_length(tool) {
                Ok(char_count) => {
                    // Use the standard system token counting approach (3:1 ratio + rounding)
                    let result = TokenCount::from(CharCount::from(char_count));
                    results.insert(tool.name.clone(), result);
                    tracing::debug!("Calculated tokens for CLI tool '{}': {} tokens (from {} chars)", 
                                  tool.name, result.value(), char_count);
                }
                Err(e) => {
                    failed_tools.push(format!("Tool '{}' calculation failed: {}", tool.name, e));
                    tracing::warn!("Failed to calculate tokens for CLI tool '{}': {}", tool.name, e);
                }
            }
        }
        
        // Log summary
        let successful_count = results.len();
        let failed_count = failed_tools.len();
        
        tracing::info!("Processed {} CLI tools: {} successful, {} failed", 
                      tools.len(), successful_count, failed_count);
        
        if !failed_tools.is_empty() {
            tracing::warn!("Processing failures: {}", failed_tools.join("; "));
        }
        
        Ok(results)
    }

    /// Calculate character count for CLI tool specification
    /// 
    /// This method calculates the total character count by analyzing:
    /// - Tool name length
    /// - Description length  
    /// - Input schema JSON serialization length (actual size, not estimated)
    /// 
    /// The character count is then converted to tokens using the standard 3:1 ratio.
    fn calculate_cli_tool_content_length(tool: &CliToolSpec) -> TokenResult<usize> {
        let mut char_count = 0;

        // Add tool name length
        char_count += tool.name.len();

        // Add description length (CLI tools always have descriptions)
        char_count += tool.description.len();

        // Add input schema length - get actual JSON size instead of fixed estimate
        if let Some(json_doc) = &tool.input_schema.json {
            // Try to serialize the FigDocument to get actual JSON size
            match serde_json::to_string(json_doc) {
                Ok(json_string) => {
                    let schema_size = json_string.len();
                    char_count += schema_size;
                    tracing::debug!("CLI tool '{}' actual schema size: {} chars", tool.name, schema_size);
                }
                Err(e) => {
                    // Fall back to a reasonable estimate if serialization fails
                    let fallback_size = 100; // More reasonable than previous 50-char fixed estimate
                    char_count += fallback_size;
                    tracing::debug!("CLI tool '{}' schema serialization failed ({}), using fallback size: {}", 
                                  tool.name, e, fallback_size);
                }
            }
        } else {
            // No JSON content, assume minimal schema
            tracing::debug!("CLI tool schema has no JSON content, using minimal size");
            char_count += 2; // Empty object "{}"
        }
        
        // Validate total size to prevent resource exhaustion
        if char_count > MAX_TOOL_SPEC_SIZE {
            return Err(TokenCalculationError::ToolTooLarge {
                size: char_count,
                max_size: MAX_TOOL_SPEC_SIZE,
            });
        }
        
        tracing::debug!("CLI tool '{}' content length: {} chars (name: {}, desc: {}, schema: actual)", 
                      tool.name, char_count, tool.name.len(), tool.description.len());
        
        Ok(char_count)
    }

}

impl Default for ToolTokenCalculator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_client::model::{ToolInputSchema, FigDocument};
    use aws_smithy_types::Document;

    fn create_test_cli_tool(name: &str, description: &str) -> CliToolSpec {
        use crate::api_client::model::{ToolInputSchema, FigDocument};
        use aws_smithy_types::Document;
        
        CliToolSpec {
            name: name.to_string(),
            description: description.to_string(),
            input_schema: ToolInputSchema {
                json: Some(FigDocument::from(Document::String(serde_json::to_string(&serde_json::json!({
                    "type": "object",
                    "properties": {
                        "test": {"type": "string"}
                    }
                })).unwrap()))),
            },
        }
    }

    #[test]
    fn test_batch_cli_tool_processing_success() {
        let tools: Vec<CliToolSpec> = (0..5).map(|i| {
            create_test_cli_tool(&format!("tool_{}", i), "Test tool")
        }).collect();
        let tool_refs: Vec<&CliToolSpec> = tools.iter().collect();
        
        let batch_results = ToolTokenCalculator::calculate_batch_cli_tool_tokens(&tool_refs).unwrap();
        
        // Should have results for all tools
        assert_eq!(batch_results.len(), 5);
        
        // All results should be positive
        for (tool_name, token_count) in &batch_results {
            assert!(token_count.value() > 0, "Tool {} should have positive token count", tool_name);
        }
    }

    #[test]
    fn test_batch_cli_tool_consistency() {
        let tools: Vec<CliToolSpec> = vec![
            create_test_cli_tool("test_tool", "A test tool"),
            create_test_cli_tool("another_tool", "Another test tool"),
        ];
        let tool_refs: Vec<&CliToolSpec> = tools.iter().collect();
        
        // Multiple calculations should give consistent results
        let result1 = ToolTokenCalculator::calculate_batch_cli_tool_tokens(&tool_refs).unwrap();
        let result2 = ToolTokenCalculator::calculate_batch_cli_tool_tokens(&tool_refs).unwrap();
        
        // Results should be identical
        assert_eq!(result1.len(), result2.len());
        for (tool_name, token_count) in &result1 {
            assert_eq!(result2.get(tool_name), Some(token_count));
        }
    }

    #[test]
    fn test_batch_cli_tool_input_validation() {
        // Test with empty tool name
        let invalid_tools = vec![
            CliToolSpec {
                name: "".to_string(),
                description: "Invalid tool".to_string(),
                input_schema: crate::api_client::model::ToolInputSchema { json: None },
            },
            create_test_cli_tool("valid_tool", "Valid tool"),
        ];
        let tool_refs: Vec<&CliToolSpec> = invalid_tools.iter().collect();
        
        let results = ToolTokenCalculator::calculate_batch_cli_tool_tokens(&tool_refs).unwrap();
        
        // Should only have result for valid tool
        assert_eq!(results.len(), 1);
        assert!(results.contains_key("valid_tool"));
        assert!(!results.contains_key(""));
    }

    #[test]
    fn test_actual_vs_fixed_schema_sizes() {
        // Test the key improvement: actual schema sizes vs fixed estimates
        let simple_tool = create_test_cli_tool("simple", "Simple tool");
        let complex_tool = CliToolSpec {
            name: "complex_tool".to_string(),
            description: "A complex tool with many parameters".to_string(),
            input_schema: ToolInputSchema {
                json: Some(FigDocument::from(Document::String(serde_json::to_string(&serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "File path"},
                        "mode": {"type": "string", "enum": ["read", "write", "append"]},
                        "encoding": {"type": "string", "default": "utf-8"},
                        "options": {
                            "type": "object",
                            "properties": {
                                "recursive": {"type": "boolean"},
                                "follow_symlinks": {"type": "boolean"},
                                "max_depth": {"type": "integer", "minimum": 1}
                            }
                        }
                    },
                    "required": ["path"]
                })).unwrap()))),
            },
        };
        
        let tools = vec![&simple_tool, &complex_tool];
        let results = ToolTokenCalculator::calculate_batch_cli_tool_tokens(&tools).unwrap();
        
        let simple_tokens = results.get("simple").unwrap().value();
        let complex_tokens = results.get("complex_tool").unwrap().value();
        
        // Complex tool should have significantly more tokens due to larger schema
        assert!(complex_tokens > simple_tokens);
        
        // The improvement should be substantial (not just a few tokens difference)
        let improvement_ratio = complex_tokens as f64 / simple_tokens as f64;
        assert!(improvement_ratio > 2.0, "Complex tool should have significantly more tokens than simple tool");
    }

    #[test]
    fn test_consistency_with_standard_token_counting() {
        // Verify our tool token calculator uses the same 3:1 ratio as the rest of the system
        let tool = create_test_cli_tool("test_tool", "Test description");
        let results = ToolTokenCalculator::calculate_batch_cli_tool_tokens(&[&tool]).unwrap();
        let our_tokens = results.get("test_tool").unwrap().value();
        
        // Calculate what the standard approach would give for just name + description
        let name_desc_content = format!("{}{}", tool.name, tool.description);
        let name_desc_tokens = TokenCounter::count_tokens(&name_desc_content);
        
        // Our approach should be higher because it includes schema size
        assert!(our_tokens > name_desc_tokens, 
               "Our approach ({} tokens) should include schema size and be higher than name+desc only ({} tokens)", 
               our_tokens, name_desc_tokens);
        
        // Verify both use the standard 3:1 ratio (not 1:1 char-to-token)
        let name_desc_chars = name_desc_content.len();
        assert!(name_desc_tokens < name_desc_chars, 
               "Standard approach should use 3:1 ratio: {} tokens < {} chars", 
               name_desc_tokens, name_desc_chars);
        
        // Our tokens should be reasonable (not 1:1 with total chars)
        let estimated_total_chars = name_desc_chars + 50; // Rough estimate including schema
        assert!(our_tokens < estimated_total_chars, 
               "Our approach should also use 3:1 ratio, not 1:1: {} tokens < {} chars", 
               our_tokens, estimated_total_chars);
    }
}
