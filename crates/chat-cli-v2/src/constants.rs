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

/// Agent name advertised in ACP InitializeResponse
pub const AGENT_NAME: &str = "Kiro CLI Agent";

/// ACP client name used by the built-in Kiro TUI.
/// The TUI must send this exact value in `InitializeRequest.client_info.name`
/// to be identified as V2 (vs generic ACP).
pub const KIRO_ACP_CLIENT_NAME: &str = "kiro-tui";

#[cfg(test)]
mod tests {
    /// `env!("CARGO_PKG_VERSION")` flows outbound to KRS as the `appVersion`
    /// header, which gates "thinking" on `>= 2.4.0`. The `0.0.0-dev`
    /// placeholder must never leak into a built binary — `build.rs` maps it to
    /// `99.99.99-dev` for local dev. Don't remove without removing that map.
    #[test]
    fn cli_version_is_not_dev_placeholder() {
        assert_ne!(env!("CARGO_PKG_VERSION"), "0.0.0-dev");
    }

    /// Source-level guard: the workspace `Cargo.toml` and `package.json` must
    /// ship the same placeholder so the tag-driven release bumps them together.
    /// Reads the raw `Cargo.toml` version, not `env!("CARGO_PKG_VERSION")`,
    /// which `build.rs` rewrites to `99.99.99-dev` for local dev.
    #[test]
    fn tui_package_json_version_matches_cargo_version() {
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let cargo_toml =
            std::fs::read_to_string(workspace_root.join("Cargo.toml")).expect("failed to read workspace Cargo.toml");
        let cargo_version = cargo_toml
            .lines()
            .skip_while(|l| !l.starts_with("[workspace.package]"))
            .find_map(|l| l.strip_prefix("version = "))
            .map(|v| v.trim().trim_matches('"'))
            .expect("missing [workspace.package] version in Cargo.toml");

        let pkg_json_path = workspace_root.join("packages/tui/package.json");
        let pkg_json: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&pkg_json_path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", pkg_json_path.display())),
        )
        .expect("failed to parse package.json");
        let tui_version = pkg_json["version"].as_str().expect("missing version in package.json");
        assert_eq!(
            cargo_version, tui_version,
            "packages/tui/package.json version ({tui_version}) must match Cargo workspace version ({cargo_version})"
        );
    }
}
