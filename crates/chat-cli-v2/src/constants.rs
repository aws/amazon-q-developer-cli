//! Centralized constants for user-facing messages

/// Base product name without any qualifiers
pub const PRODUCT_NAME: &str = "Kiro";

/// CLI binary name
pub const CLI_NAME: &str = "kiro-cli";

/// Client name for authentication purposes
pub const CLIENT_NAME: &str = "Kiro CLI";

/// GitHub issues URL for bug reports and feature requests
pub const GITHUB_ISSUES_URL: &str = "https://github.com/kirodotdev/Kiro/issues/new";

/// Default agent name
pub const DEFAULT_AGENT_NAME: &str = "kiro_default";

/// Planner agent name
pub const PLANNER_AGENT_NAME: &str = "kiro_planner";

/// List of all built-in agent names
pub const BUILT_IN_AGENTS: &[&str] = &[DEFAULT_AGENT_NAME, PLANNER_AGENT_NAME];

/// Reserved keyboard shortcuts that cannot be used as agent triggers
pub const RESERVED_KEYBOARD_SHORTCUTS: &[&str] = &[
    "ctrl+c",    // Interrupt
    "ctrl+d",    // EOF/quit
    "ctrl+l",    // Clear screen
    "ctrl+z",    // Suspend
    "ctrl+s",    // Fuzzy search
    "ctrl+r",    // History search
    "ctrl+t",    // Tangent mode (if enabled)
    "shift+tab", // Planner toggle (built-in)
];

/// MCP safety and security documentation URL
pub const MCP_SECURITY_DOC_URL: &str = "https://kiro.dev/docs/cli/mcp/security/";
