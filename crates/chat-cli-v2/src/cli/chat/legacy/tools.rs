//! Minimal tool types needed by cli/agent for agent config validation.

use std::borrow::Borrow;

use serde::{
    Deserialize,
    Serialize,
};

pub const DEFAULT_APPROVE: [&str; 0] = [];

#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub spec_name: &'static str,
    pub preferred_alias: &'static str,
    pub aliases: &'static [&'static str],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    #[serde(alias = "inputSchema")]
    pub input_schema: serde_json::Value,
    #[serde(skip_serializing, default)]
    pub tool_origin: ToolOrigin,
}

#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub enum ToolOrigin {
    #[default]
    Native,
    McpServer(String),
}

impl std::hash::Hash for ToolOrigin {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        if let Self::McpServer(name) = self {
            name.hash(state);
        }
    }
}

impl Borrow<str> for ToolOrigin {
    fn borrow(&self) -> &str {
        match self {
            Self::McpServer(name) => name.as_str(),
            Self::Native => "native",
        }
    }
}

impl<'de> Deserialize<'de> for ToolOrigin {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if s == "native___" {
            Ok(ToolOrigin::Native)
        } else {
            Ok(ToolOrigin::McpServer(s))
        }
    }
}

impl Serialize for ToolOrigin {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            ToolOrigin::Native => serializer.serialize_str("native___"),
            ToolOrigin::McpServer(server) => serializer.serialize_str(server),
        }
    }
}

impl std::fmt::Display for ToolOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolOrigin::Native => write!(f, "Built-in"),
            ToolOrigin::McpServer(server) => write!(f, "{server} (MCP)"),
        }
    }
}

/// Proxy for accessing tool metadata
pub struct ToolMetadata;

impl ToolMetadata {
    const ALL: &[&ToolInfo] = &[
        &Self::FS_READ,
        &Self::FS_WRITE,
        &Self::EXECUTE_COMMAND,
        &Self::USE_AWS,
        &Self::GH_ISSUE,
        &Self::INTROSPECT,
        &Self::KNOWLEDGE,
        &Self::THINKING,
        &Self::TODO,
        &Self::DELEGATE,
        &Self::GREP,
        &Self::GLOB,
        &Self::CODE,
        &Self::WEB_SEARCH,
        &Self::WEB_FETCH,
        &Self::USE_SUBAGENT,
        &Self::SWITCH_TO_EXECUTION,
    ];
    pub const CODE: ToolInfo = ToolInfo {
        spec_name: "code",
        preferred_alias: "code",
        aliases: &["code", "code/read", "code/write"],
    };
    pub const DELEGATE: ToolInfo = ToolInfo {
        spec_name: "delegate",
        preferred_alias: "delegate",
        aliases: &["delegate"],
    };
    pub const EXECUTE_COMMAND: ToolInfo = ToolInfo {
        spec_name: "execute_bash",
        preferred_alias: "shell",
        aliases: &["execute_bash", "execute_cmd", "shell"],
    };
    pub const FS_READ: ToolInfo = ToolInfo {
        spec_name: "fs_read",
        preferred_alias: "read",
        aliases: &["fs_read", "read"],
    };
    pub const FS_WRITE: ToolInfo = ToolInfo {
        spec_name: "fs_write",
        preferred_alias: "write",
        aliases: &["fs_write", "write"],
    };
    pub const GH_ISSUE: ToolInfo = ToolInfo {
        spec_name: "report_issue",
        preferred_alias: "report",
        aliases: &["gh_issue", "report_issue", "report"],
    };
    pub const GLOB: ToolInfo = ToolInfo {
        spec_name: "glob",
        preferred_alias: "glob",
        aliases: &["glob"],
    };
    pub const GREP: ToolInfo = ToolInfo {
        spec_name: "grep",
        preferred_alias: "grep",
        aliases: &["grep"],
    };
    pub const INTROSPECT: ToolInfo = ToolInfo {
        spec_name: "introspect",
        preferred_alias: "introspect",
        aliases: &["introspect"],
    };
    pub const KNOWLEDGE: ToolInfo = ToolInfo {
        spec_name: "knowledge",
        preferred_alias: "knowledge",
        aliases: &["knowledge"],
    };
    pub const SWITCH_TO_EXECUTION: ToolInfo = ToolInfo {
        spec_name: "switch_to_execution",
        preferred_alias: "switch_to_execution",
        aliases: &["switch_to_execution"],
    };
    pub const THINKING: ToolInfo = ToolInfo {
        spec_name: "thinking",
        preferred_alias: "thinking",
        aliases: &["thinking"],
    };
    pub const TODO: ToolInfo = ToolInfo {
        spec_name: "todo_list",
        preferred_alias: "todo",
        aliases: &["todo_list", "todo"],
    };
    pub const USE_AWS: ToolInfo = ToolInfo {
        spec_name: "use_aws",
        preferred_alias: "aws",
        aliases: &["use_aws", "aws"],
    };
    pub const USE_SUBAGENT: ToolInfo = ToolInfo {
        spec_name: "use_subagent",
        preferred_alias: "subagent",
        aliases: &["use_subagent", "subagent"],
    };
    pub const WEB_FETCH: ToolInfo = ToolInfo {
        spec_name: "web_fetch",
        preferred_alias: "web_fetch",
        aliases: &["web_fetch"],
    };
    pub const WEB_SEARCH: ToolInfo = ToolInfo {
        spec_name: "web_search",
        preferred_alias: "web_search",
        aliases: &["web_search"],
    };

    pub fn get_by_any_alias(alias: &str) -> Option<&'static ToolInfo> {
        Self::ALL.iter().copied().find(|info| info.aliases.contains(&alias))
    }

    pub fn get_by_spec_name(spec_name: &str) -> Option<&'static ToolInfo> {
        Self::ALL.iter().copied().find(|info| info.spec_name == spec_name)
    }
}

pub fn is_native_tool(name: &str) -> bool {
    ToolMetadata::ALL.iter().any(|info| info.aliases.contains(&name))
}

/// All native tool aliases for validation
pub const NATIVE_TOOL_ALIASES: &[&str] = &[
    "fs_read",
    "read",
    "fs_write",
    "write",
    "execute_bash",
    "execute_cmd",
    "shell",
    "use_aws",
    "aws",
    "gh_issue",
    "report_issue",
    "report",
    "introspect",
    "knowledge",
    "thinking",
    "todo_list",
    "todo",
    "delegate",
    "grep",
    "glob",
    "code",
    "code/read",
    "code/write",
    "web_search",
    "web_fetch",
    "use_subagent",
    "subagent",
    "switch_to_execution",
];
