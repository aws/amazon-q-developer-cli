//! Utilities for semantic parsing of agent config values

use std::borrow::Cow;
use std::str::FromStr;

use crate::agent::tools::BuiltInToolName;
use crate::agent::util::path::canonicalize_path_sys;
use crate::agent::util::providers::SystemProvider;

/// Represents a value from the `resources` array in the agent config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceKind<'a> {
    File { original: &'a str, file_path: String },
    FileGlob { original: &'a str, pattern: glob::Pattern },
}

impl<'a> ResourceKind<'a> {
    pub fn parse(value: &'a str, sys: &impl SystemProvider) -> Result<Self, String> {
        if !value.starts_with("file://") {
            return Err("Only file schemes are currently supported".to_string());
        }

        let file_path = value.trim_start_matches("file://");
        if file_path.contains('*') || file_path.contains('?') {
            let canon = canonicalize_path_sys(file_path, sys)
                .map_err(|err| format!("Failed to canonicalize path for {}: {}", file_path, err))?;
            let pattern = glob::Pattern::new(canon.as_str())
                .map_err(|err| format!("Failed to create glob for {}: {}", canon, err))?;
            Ok(Self::FileGlob {
                original: value,
                pattern,
            })
        } else {
            Ok(Self::File {
                original: value,
                file_path: canonicalize_path_sys(file_path, sys)
                    .map_err(|err| format!("Failed to canonicalize path for {}: {}", file_path, err))?,
            })
        }
    }
}

/// Represents the different types of tool name references allowed by the agent
/// configuration `tools` spec.
#[derive(Debug)]
pub enum ToolNameKind<'a> {
    /// All tools. Equal to `*`
    All,
    /// A canonical MCP tool name. Follows the format `@server_name/tool_name`
    McpFullName { server_name: &'a str, tool_name: &'a str },
    /// All tools from an MCP server. Follows the format `@server_name`
    McpServer { server_name: &'a str },
    /// Glob matching for an MCP server. Follows the format `@server_name/glob_part`, where
    /// `glob_part` contains one or more `*`.
    ///
    /// Example: `@myserver/edit_*`
    McpGlob { server_name: &'a str, glob_part: &'a str },
    /// All built-in tools. Equal to `@builtin`
    AllBuiltIn,
    /// Glob matching for a built-in tool.
    BuiltInGlob(&'a str),
    /// A canonical tool name.
    BuiltIn(&'a str),
    /// Glob matching for a specific agent. Follows the format `#agent_glob`, where
    /// `agent_glob` contains one or more `*`.
    AgentGlob(&'a str),
    /// A reference to an agent name. Follows the format `#agent_name`
    Agent(&'a str),
}

impl<'a> ToolNameKind<'a> {
    pub fn parse(name: &'a str) -> Result<Self, String> {
        if name == "*" {
            return Ok(Self::All);
        }

        if name == "@builtin" {
            return Ok(Self::AllBuiltIn);
        }

        // Check for MCP tool
        if let Some(rest) = name.strip_prefix("@") {
            if let Some(i) = rest.find("/") {
                let (server_name, tool_part) = rest.split_at(i);
                if tool_part.contains("*") {
                    return Ok(Self::McpGlob {
                        server_name,
                        glob_part: tool_part,
                    });
                } else {
                    return Ok(Self::McpFullName {
                        server_name,
                        tool_name: tool_part,
                    });
                }
            }

            return Ok(Self::McpServer { server_name: rest });
        }

        // Check for Agent tool
        if let Some(rest) = name.strip_prefix("#") {
            if rest.contains("*") {
                return Ok(Self::AgentGlob(rest));
            } else {
                return Ok(Self::Agent(rest));
            }
        }

        // Rest, must be a built-in
        if name.contains("*") {
            Ok(Self::BuiltInGlob(name))
        } else {
            Ok(Self::BuiltIn(name))
        }
    }
}

/// Represents the authoritative source of a single tool name - essentially, tool names before
/// undergoing any transformations.
///
/// A canonical tool name is one of the following:
/// 1. One of the built-in tool names
/// 2. An MCP server tool name with the format `@server_name/tool_name`
/// 3. An agent name with the format `#agent_name`
///
/// # Background
///
/// Tool names can be presented to the model in some transformed form due to:
/// 1. Tool aliases (usually done to resolve tool name conflicts across different MCP servers)
/// 2. MCP servers providing out-of-spec tool names, which we must transform ourselves
/// 3. Some backend-specific tool name validation - e.g., Bedrock only allows tool names matching
///    `[a-zA-Z0-9_-]+`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CanonicalToolName {
    BuiltIn(BuiltInToolName),
    // todo - make Cow?
    Mcp { server_name: String, tool_name: String },
    Agent { agent_name: String },
}

impl CanonicalToolName {
    pub fn from_mcp_parts(server_name: String, tool_name: String) -> Self {
        Self::Mcp { server_name, tool_name }
    }

    /// Returns the absolute tool name as written in the agent configuration
    pub fn as_full_name(&self) -> Cow<'_, str> {
        match self {
            CanonicalToolName::BuiltIn(name) => name.as_ref().into(),
            CanonicalToolName::Mcp { server_name, tool_name } => format!("@{}/{}", server_name, tool_name).into(),
            CanonicalToolName::Agent { agent_name } => format!("#{}", agent_name).into(),
        }
    }

    /// Returns only tool-name portion of the full name
    ///
    /// # Examples
    ///
    /// - For an MCP name (e.g. `@mcp-server/tool-name`), this would return `tool-name`
    /// - For an agent name (e.g. `#agent-name`), this would return `agent-name`
    pub fn tool_name(&self) -> &str {
        match self {
            CanonicalToolName::BuiltIn(name) => name.as_ref(),
            CanonicalToolName::Mcp { tool_name, .. } => tool_name,
            CanonicalToolName::Agent { agent_name } => agent_name,
        }
    }
}

impl From<BuiltInToolName> for CanonicalToolName {
    fn from(value: BuiltInToolName) -> Self {
        Self::BuiltIn(value)
    }
}

impl FromStr for CanonicalToolName {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match ToolNameKind::parse(s) {
            Ok(kind) => match kind {
                ToolNameKind::McpFullName { server_name, tool_name } => Ok(Self::Mcp {
                    server_name: server_name.to_string(),
                    tool_name: tool_name.to_string(),
                }),
                ToolNameKind::BuiltIn(name) => match name.parse::<BuiltInToolName>() {
                    Ok(name) => Ok(Self::BuiltIn(name)),
                    Err(err) => Err(err.to_string()),
                },
                ToolNameKind::Agent(s) => Ok(Self::Agent {
                    agent_name: s.to_string(),
                }),
                other => Err(format!(
                    "Unexpected format input: {}. {:?} is not a valid name",
                    s, other
                )),
            },
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::util::test::TestProvider;

    #[test]
    fn test_resource_kind_parse_nonfile() {
        assert!(
            ResourceKind::parse("https://google.com", &TestProvider::new()).is_err(),
            "non-file scheme should be an error"
        );
    }

    #[test]
    fn test_resource_kind_parse_file_scheme() {
        let sys = TestProvider::new();

        let resource = "file://project/README.md";
        assert_eq!(ResourceKind::parse(resource, &sys).unwrap(), ResourceKind::File {
            original: resource,
            file_path: "project/README.md".to_string()
        });

        let resource = "file://~/project/**/*.rs";
        assert_eq!(ResourceKind::parse(resource, &sys).unwrap(), ResourceKind::FileGlob {
            original: resource,
            pattern: glob::Pattern::new("/home/testuser/project/**/*.rs").unwrap()
        });
    }
}
