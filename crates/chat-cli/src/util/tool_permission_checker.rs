use std::collections::HashSet;

use tracing::debug;

use crate::util::MCP_SERVER_TOOL_DELIMITER;
use crate::util::pattern_matching::matches_any_pattern;

const BUILT_IN_PREFIX: &str = "@builtin";
const BUILT_IN_PREFIX_WITH_SLASH: &str = "@builtin/";

/// Checks if a tool is allowed based on the agent's allowed_tools configuration.
/// This function handles both native tools and MCP tools with wildcard pattern support.
pub fn is_tool_in_allowlist(allowed_tools: &HashSet<String>, tool_name: &str, server_name: Option<&str>) -> bool {
    let filter_patterns = |predicate: fn(&str) -> bool| -> HashSet<&str> {
        allowed_tools
            .iter()
            .filter(|pattern| predicate(pattern))
            .map(String::as_str)
            .collect()
    };

    match server_name {
        // Native tool
        None => {
            for name in allowed_tools {
                if name
                    .strip_prefix(BUILT_IN_PREFIX)
                    .is_some_and(|n| n.is_empty() || n == "/" || n == "/*")
                {
                    return true;
                }
            }

            let patterns = allowed_tools
                .iter()
                .filter_map(|p| {
                    if !p.starts_with('@') {
                        Some(p.as_str())
                    } else {
                        p.strip_prefix(BUILT_IN_PREFIX_WITH_SLASH)
                    }
                })
                .collect::<HashSet<_>>();

            debug!("Native patterns: {:?}", patterns);
            let result = matches_any_pattern(&patterns, tool_name);
            debug!("Native tool '{}' permission check result: {}", tool_name, result);
            result
        },
        // MCP tool
        Some(server) => {
            let patterns = filter_patterns(|p| p.starts_with('@'));
            debug!("MCP patterns: {:?}", patterns);

            // Check server-level permission first: @server_name
            let server_pattern = format!("@{}", server);
            debug!("Checking server-level pattern: '{}'", server_pattern);
            if matches_any_pattern(&patterns, &server_pattern) {
                debug!("Server-level permission granted for '{}'", server_pattern);
                return true;
            }

            // Check tool-specific permission: @server_name/tool_name
            let tool_pattern = format!("@{}{}{}", server, MCP_SERVER_TOOL_DELIMITER, tool_name);
            debug!("Checking tool-specific pattern: '{}'", tool_pattern);
            let result = matches_any_pattern(&patterns, &tool_pattern);
            debug!("Tool-specific permission result for '{}': {}", tool_pattern, result);
            result
        },
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn test_native_vs_mcp_separation() {
        let mut allowed = HashSet::new();
        allowed.insert("fs_*".to_string());
        allowed.insert("@git".to_string());

        // Native patterns only apply to native tools
        assert!(is_tool_in_allowlist(&allowed, "fs_read", None));
        assert!(!is_tool_in_allowlist(&allowed, "fs_read", Some("server")));

        // MCP patterns only apply to MCP tools
        assert!(is_tool_in_allowlist(&allowed, "status", Some("git")));
        assert!(!is_tool_in_allowlist(&allowed, "git", None));
    }

    #[test]
    fn test_mcp_wildcard_patterns() {
        let mut allowed = HashSet::new();
        allowed.insert("@*quip*".to_string());
        allowed.insert("@git/read_*".to_string());

        assert!(is_tool_in_allowlist(&allowed, "tool", Some("quip-server")));
        assert!(is_tool_in_allowlist(&allowed, "read_file", Some("git")));
        assert!(!is_tool_in_allowlist(&allowed, "write_file", Some("git")));
    }

    #[test]
    fn test_builtin_namespace() {
        let mut allowed = HashSet::new();
        allowed.insert("@builtin".to_string());
        allowed.insert("@builtin/".to_string());
        allowed.insert("@builtin/*".to_string());

        // @builtin should allow all native tools
        assert!(is_tool_in_allowlist(&allowed, "fs_read", None));

        // But should not allow MCP tools
        assert!(!is_tool_in_allowlist(&allowed, "tool", Some("server")));

        allowed.clear();
        allowed.insert("@builtin/fs_read".to_string());

        assert!(is_tool_in_allowlist(&allowed, "fs_read", None));
        assert!(!is_tool_in_allowlist(&allowed, "fs_write", None));

        allowed.clear();
        allowed.insert("@builtin/fs_*".to_string());

        assert!(is_tool_in_allowlist(&allowed, "fs_read", None));
        assert!(is_tool_in_allowlist(&allowed, "fs_write", None));
    }
}
