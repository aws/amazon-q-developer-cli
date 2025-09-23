use std::collections::HashSet;

use crate::util::pattern_matching::matches_any_pattern;
use crate::util::MCP_SERVER_TOOL_DELIMITER;
use tracing::info;

/// Checks if a tool is allowed based on the agent's allowed_tools configuration.
/// This function handles both native tools and MCP tools with wildcard pattern support.
pub fn is_tool_allowed(
    allowed_tools: &HashSet<String>,
    tool_name: &str,
    server_name: Option<&str>,
) -> bool {
    let filter_patterns = |predicate: fn(&str) -> bool| -> HashSet<String> {
        allowed_tools
            .iter()
            .filter(|pattern| predicate(pattern))
            .cloned()
            .collect()
    };
    
    match server_name {
        // Native tool
        None => {
            let patterns = filter_patterns(|p| !p.starts_with('@'));
            info!("Native patterns: {:?}", patterns);
            let result = matches_any_pattern(&patterns, tool_name);
            info!("Native tool '{}' permission check result: {}", tool_name, result);
            result
        },
        // MCP tool
        Some(server) => {
            let patterns = filter_patterns(|p| p.starts_with('@'));
            info!("MCP patterns: {:?}", patterns);
            
            // Check server-level permission first: @server_name
            let server_pattern = format!("@{}", server);
            info!("Checking server-level pattern: '{}'", server_pattern);
            if matches_any_pattern(&patterns, &server_pattern) {
                info!("Server-level permission granted for '{}'", server_pattern);
                return true;
            }
            
            // Check tool-specific permission: @server_name/tool_name
            let tool_pattern = format!("@{}{}{}", server, MCP_SERVER_TOOL_DELIMITER, tool_name);
            info!("Checking tool-specific pattern: '{}'", tool_pattern);
            let result = matches_any_pattern(&patterns, &tool_pattern);
            info!("Tool-specific permission result for '{}': {}", tool_pattern, result);
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_native_vs_mcp_separation() {
        let mut allowed = HashSet::new();
        allowed.insert("fs_*".to_string());
        allowed.insert("@git".to_string());
        
        // Native patterns only apply to native tools
        assert!(is_tool_allowed(&allowed, "fs_read", None));
        assert!(!is_tool_allowed(&allowed, "fs_read", Some("server")));
        
        // MCP patterns only apply to MCP tools
        assert!(is_tool_allowed(&allowed, "status", Some("git")));
        assert!(!is_tool_allowed(&allowed, "git", None));
    }

    #[test]
    fn test_mcp_wildcard_patterns() {
        let mut allowed = HashSet::new();
        allowed.insert("@*quip*".to_string());
        allowed.insert("@git/read_*".to_string());
        
        assert!(is_tool_allowed(&allowed, "tool", Some("quip-server")));
        assert!(is_tool_allowed(&allowed, "read_file", Some("git")));
        assert!(!is_tool_allowed(&allowed, "write_file", Some("git")));
    }
}