//! Utilities for semantic parsing of agent config values

use std::borrow::Cow;
use std::str::FromStr;

use serde::{
    Deserialize,
    Serialize,
};

use crate::agent::tools::BuiltInToolName;
use crate::agent::util::path::{
    canonicalize_path_sys,
    expand_path,
};
use crate::agent::util::providers::SystemProvider;

/// Represents a value from the `resources` array in the agent config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceKind<'a> {
    File { original: &'a str, file_path: String },
    FileGlob { original: &'a str, pattern: glob::Pattern },
    Skill { original: &'a str, file_path: String },
    SkillGlob { original: &'a str, pattern: glob::Pattern },
}

impl<'a> ResourceKind<'a> {
    pub fn parse(value: &'a str, sys: &impl SystemProvider) -> Result<Self, String> {
        let (scheme, path) = if let Some(p) = value.strip_prefix("file://") {
            ("file", p)
        } else if let Some(p) = value.strip_prefix("skill://") {
            ("skill", p)
        } else {
            return Err(format!("resource must start with file:// or skill://, got: {value}"));
        };

        let is_glob = path.contains('*') || path.contains('?');

        if is_glob {
            // For glob patterns: expand ~ and env vars, make absolute, but do NOT
            // canonicalize. On Windows, canonicalization resolves mapped drive letters
            // (e.g. P:\) to UNC paths (\\server\share\...), but glob traversal returns
            // paths using the original drive letter — causing a mismatch that prevents
            // any files from matching. See V2240114729.
            let expanded = expand_path(path, sys).map_err(|err| format!("Failed to expand path {path}: {err}"))?;
            let expanded_path = std::path::Path::new(expanded.as_ref());
            let abs = if expanded_path.is_absolute() {
                expanded_path.to_path_buf()
            } else {
                sys.cwd()
                    .map_err(|err| format!("Failed to get cwd: {err}"))?
                    .join(expanded_path)
            };
            let pattern_str = abs.to_string_lossy().to_string();
            // On Windows, shellexpand may leave forward slashes from the original
            // pattern (e.g. "~/project/**/*.rs" → "C:\home/project/**/*.rs").
            // Normalize to the platform separator so glob matching is consistent
            // with filesystem traversal results.
            #[cfg(windows)]
            let pattern_str = pattern_str.replace('/', "\\");
            let pattern = glob::Pattern::new(&pattern_str)
                .map_err(|err| format!("Failed to create glob for {pattern_str}: {err}"))?;

            match scheme {
                "file" => Ok(Self::FileGlob {
                    original: value,
                    pattern,
                }),
                "skill" => Ok(Self::SkillGlob {
                    original: value,
                    pattern,
                }),
                _ => unreachable!(),
            }
        } else {
            // For single file paths: full canonicalization for dedup and symlink resolution.
            let canon = canonicalize_path_sys(path, sys)
                .map_err(|err| format!("Failed to canonicalize path for {path}: {err}"))?;

            match scheme {
                "file" => Ok(Self::File {
                    original: value,
                    file_path: canon,
                }),
                "skill" => Ok(Self::Skill {
                    original: value,
                    file_path: canon,
                }),
                _ => unreachable!(),
            }
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

        if matches!(name, "@builtin" | "@builtin/" | "@builtin/*") {
            return Ok(Self::AllBuiltIn);
        } else if let Some(rest) = name.strip_prefix("@builtin/") {
            if rest.contains("*") {
                return Ok(Self::BuiltInGlob(rest));
            } else {
                return Ok(Self::BuiltIn(rest));
            }
        }

        // Check for MCP tool
        if let Some(rest) = name.strip_prefix("@") {
            if let Some((server_name, tool_part)) = rest.split_once("/") {
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
            CanonicalToolName::Mcp { server_name, tool_name } => format!("@{server_name}/{tool_name}").into(),
            CanonicalToolName::Agent { agent_name } => format!("#{agent_name}").into(),
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
                other => Err(format!("Unexpected format input: {s}. {other:?} is not a valid name")),
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
        let home = TestProvider::default_home();

        let resource = "file://project/README.md";
        let expected_path = if cfg!(windows) {
            format!("{home}\\project\\README.md")
        } else {
            format!("{home}/project/README.md")
        };
        assert_eq!(ResourceKind::parse(resource, &sys).unwrap(), ResourceKind::File {
            original: resource,
            file_path: expected_path,
        });

        let resource = "file://~/project/**/*.rs";
        let expected_pattern = if cfg!(windows) {
            format!("{home}\\project\\**\\*.rs")
        } else {
            format!("{home}/project/**/*.rs")
        };
        assert_eq!(ResourceKind::parse(resource, &sys).unwrap(), ResourceKind::FileGlob {
            original: resource,
            pattern: glob::Pattern::new(&expected_pattern).unwrap()
        });
    }

    #[test]
    fn test_resource_kind_parse_skill_scheme() {
        let sys = TestProvider::new();
        let home = TestProvider::default_home();

        let resource = "skill://skills/my-skill.md";
        let expected_path = if cfg!(windows) {
            format!("{home}\\skills\\my-skill.md")
        } else {
            format!("{home}/skills/my-skill.md")
        };
        assert_eq!(ResourceKind::parse(resource, &sys).unwrap(), ResourceKind::Skill {
            original: resource,
            file_path: expected_path,
        });

        let resource = "skill://~/skills/helper.md";
        let expected_path = if cfg!(windows) {
            format!("{home}\\skills\\helper.md")
        } else {
            format!("{home}/skills/helper.md")
        };
        assert_eq!(ResourceKind::parse(resource, &sys).unwrap(), ResourceKind::Skill {
            original: resource,
            file_path: expected_path,
        });
    }

    #[test]
    fn test_resource_kind_parse_skill_glob() {
        let sys = TestProvider::new();
        let home = TestProvider::default_home();

        let resource = "skill://.kiro/skills/**/SKILL.md";
        let expected_pattern = if cfg!(windows) {
            format!("{home}\\.kiro\\skills\\**\\SKILL.md")
        } else {
            format!("{home}/.kiro/skills/**/SKILL.md")
        };
        assert_eq!(ResourceKind::parse(resource, &sys).unwrap(), ResourceKind::SkillGlob {
            original: resource,
            pattern: glob::Pattern::new(&expected_pattern).unwrap()
        });
    }

    /// Glob patterns must NOT be canonicalized — they should retain the original
    /// path scheme (drive letter on Windows) so that glob traversal results match.
    /// This is the regression test for V2240114729 (mapped drive skills bug).
    #[test]
    fn test_glob_pattern_preserves_drive_letter_path() {
        // Simulate a Windows mapped drive: home is P:\users\dev
        let drive_home = if cfg!(windows) {
            "P:\\users\\dev"
        } else {
            "/mnt/network/users/dev"
        };
        let sys = TestProvider::new_with_base(drive_home);

        let resource = "skill://~/.kiro/skills/**/*.md";
        let result = ResourceKind::parse(resource, &sys).unwrap();

        match &result {
            ResourceKind::SkillGlob { pattern, .. } => {
                let pat = pattern.as_str();
                // Pattern must start with the drive letter / mount path, NOT a UNC path
                assert!(
                    pat.starts_with(drive_home),
                    "Glob pattern should preserve the original path scheme. Got: {pat}"
                );
                // Must not contain UNC prefix
                assert!(
                    !pat.contains("\\\\?\\"),
                    "Glob pattern must not contain verbatim path prefix. Got: {pat}"
                );
                assert!(
                    !pat.contains("\\\\UNC\\"),
                    "Glob pattern must not be converted to UNC. Got: {pat}"
                );
            },
            other => panic!("Expected SkillGlob, got: {:?}", other),
        }
    }

    /// Non-glob paths should still be fully canonicalized (for dedup/identity).
    #[test]
    fn test_non_glob_paths_are_canonicalized() {
        let sys = TestProvider::new();
        let home = TestProvider::default_home();

        // Single file (no wildcards) → should go through canonicalize
        let resource = "file://~/project/README.md";
        let result = ResourceKind::parse(resource, &sys).unwrap();
        match &result {
            ResourceKind::File { file_path, .. } => {
                let expected = if cfg!(windows) {
                    format!("{home}\\project\\README.md")
                } else {
                    format!("{home}/project/README.md")
                };
                assert_eq!(file_path, &expected);
            },
            other => panic!("Expected File, got: {:?}", other),
        }
    }

    /// Glob patterns with forward slashes on Windows must have separators normalized.
    #[test]
    fn test_glob_normalizes_separators() {
        let sys = TestProvider::new();

        // Input uses forward slashes (common in YAML configs)
        let resource = "file://~/.kiro/skills/**/*.md";
        let result = ResourceKind::parse(resource, &sys).unwrap();

        match &result {
            ResourceKind::FileGlob { pattern, .. } => {
                let pat = pattern.as_str();
                if cfg!(windows) {
                    // On Windows, all separators should be backslashes
                    assert!(
                        !pat.contains('/'),
                        "Windows glob pattern should not contain forward slashes. Got: {pat}"
                    );
                } else {
                    // On Unix, forward slashes are native
                    assert!(
                        !pat.contains('\\'),
                        "Unix glob pattern should not contain backslashes. Got: {pat}"
                    );
                }
            },
            other => panic!("Expected FileGlob, got: {:?}", other),
        }
    }

    /// Relative glob patterns (no ~) should be made absolute using cwd.
    #[test]
    fn test_glob_relative_path_made_absolute() {
        let sys = TestProvider::new();
        let home = TestProvider::default_home();

        let resource = "file://.kiro/skills/**/*.md";
        let result = ResourceKind::parse(resource, &sys).unwrap();

        match &result {
            ResourceKind::FileGlob { pattern, .. } => {
                let pat = pattern.as_str();
                // Must be absolute (starts with / on Unix, drive letter on Windows)
                assert!(
                    std::path::Path::new(pat.split('*').next().unwrap_or(pat)).is_absolute(),
                    "Glob pattern should be absolute. Got: {pat}"
                );
                // Must contain the cwd prefix (which equals home for TestProvider)
                assert!(
                    pat.starts_with(home),
                    "Glob pattern should be rooted at cwd. Got: {pat}, expected prefix: {home}"
                );
            },
            other => panic!("Expected FileGlob, got: {:?}", other),
        }
    }

    /// Question mark is also detected as a glob character.
    #[test]
    fn test_question_mark_detected_as_glob() {
        let sys = TestProvider::new();

        let resource = "file://~/project/file?.txt";
        let result = ResourceKind::parse(resource, &sys).unwrap();

        assert!(
            matches!(result, ResourceKind::FileGlob { .. }),
            "Path with ? should be parsed as FileGlob"
        );
    }

    /// Environment variables in glob patterns should be expanded.
    #[test]
    fn test_glob_expands_env_vars() {
        let sys = TestProvider::new().with_var("KIRO_SKILLS", "/custom/skills");

        let resource = "skill://$KIRO_SKILLS/**/*.md";
        let result = ResourceKind::parse(resource, &sys).unwrap();

        match &result {
            ResourceKind::SkillGlob { pattern, .. } => {
                let pat = pattern.as_str();
                // The env var value uses forward slashes; on Windows our separator
                // normalization converts them to backslashes. Check platform-aware.
                let expected_fragment = if cfg!(windows) {
                    "custom\\skills"
                } else {
                    "custom/skills"
                };
                assert!(
                    pat.contains(expected_fragment),
                    "Env var should be expanded in glob pattern. Got: {pat}"
                );
                assert!(
                    !pat.contains("$KIRO_SKILLS"),
                    "Raw env var reference should not remain. Got: {pat}"
                );
            },
            other => panic!("Expected SkillGlob, got: {:?}", other),
        }
    }

    /// Absolute glob paths should be used as-is (no cwd prepend).
    #[test]
    fn test_glob_absolute_path_unchanged() {
        let abs_pattern = if cfg!(windows) {
            "file://C:\\projects\\**\\*.rs"
        } else {
            "file:///projects/**/*.rs"
        };
        let sys = TestProvider::new();
        let result = ResourceKind::parse(abs_pattern, &sys).unwrap();

        match &result {
            ResourceKind::FileGlob { pattern, .. } => {
                let pat = pattern.as_str();
                if cfg!(windows) {
                    assert!(
                        pat.starts_with("C:\\projects"),
                        "Absolute Windows glob should not be prefixed with cwd. Got: {pat}"
                    );
                } else {
                    assert!(
                        pat.starts_with("/projects"),
                        "Absolute Unix glob should not be prefixed with cwd. Got: {pat}"
                    );
                }
            },
            other => panic!("Expected FileGlob, got: {:?}", other),
        }
    }

    /// Bracket patterns `[abc]` are NOT currently detected as globs.
    /// This documents the pre-existing limitation (not introduced by our fix).
    #[test]
    fn test_bracket_pattern_not_detected_as_glob() {
        let sys = TestProvider::new();

        // [abc] is valid glob syntax but our parser only checks * and ?
        let resource = "file://~/project/file[abc].txt";
        let result = ResourceKind::parse(resource, &sys).unwrap();

        // Documents current behavior: treated as a regular file path, not a glob.
        // Follow-up could add path.contains('[') to is_glob check.
        assert!(
            matches!(result, ResourceKind::File { .. }),
            "Bracket patterns are currently treated as File (known limitation)"
        );
    }

    /// Parsing the same path with and without wildcards produces different variants.
    #[test]
    fn test_glob_vs_non_glob_same_base_path() {
        let sys = TestProvider::new();

        let non_glob = "file://~/project/README.md";
        let glob_ver = "file://~/project/*.md";

        let non_glob_result = ResourceKind::parse(non_glob, &sys).unwrap();
        let glob_result = ResourceKind::parse(glob_ver, &sys).unwrap();

        assert!(
            matches!(non_glob_result, ResourceKind::File { .. }),
            "Path without wildcards should be File"
        );
        assert!(
            matches!(glob_result, ResourceKind::FileGlob { .. }),
            "Path with wildcards should be FileGlob"
        );
    }

    #[test]
    fn test_tool_name_kind_parse() {
        // Test wildcard for all tools
        match ToolNameKind::parse("*").unwrap() {
            ToolNameKind::All => {},
            _ => panic!("Expected All variant"),
        }

        // Test built-in all variants
        match ToolNameKind::parse("@builtin").unwrap() {
            ToolNameKind::AllBuiltIn => {},
            _ => panic!("Expected AllBuiltIn variant"),
        }
        match ToolNameKind::parse("@builtin/").unwrap() {
            ToolNameKind::AllBuiltIn => {},
            _ => panic!("Expected AllBuiltIn variant"),
        }
        match ToolNameKind::parse("@builtin/*").unwrap() {
            ToolNameKind::AllBuiltIn => {},
            _ => panic!("Expected AllBuiltIn variant"),
        }

        // Test built-in glob
        match ToolNameKind::parse("@builtin/edit_*").unwrap() {
            ToolNameKind::BuiltInGlob(glob) => assert_eq!(glob, "edit_*"),
            _ => panic!("Expected BuiltInGlob variant"),
        }

        // Test built-in specific tool
        match ToolNameKind::parse("@builtin/bash").unwrap() {
            ToolNameKind::BuiltIn(name) => assert_eq!(name, "bash"),
            _ => panic!("Expected BuiltIn variant"),
        }

        // Test MCP full name
        match ToolNameKind::parse("@myserver/mytool").unwrap() {
            ToolNameKind::McpFullName { server_name, tool_name } => {
                assert_eq!(server_name, "myserver");
                assert_eq!(tool_name, "mytool");
            },
            _ => panic!("Expected McpFullName variant"),
        }

        // Test MCP glob
        match ToolNameKind::parse("@myserver/edit_*").unwrap() {
            ToolNameKind::McpGlob { server_name, glob_part } => {
                assert_eq!(server_name, "myserver");
                assert_eq!(glob_part, "edit_*");
            },
            _ => panic!("Expected McpGlob variant"),
        }

        // Test MCP server only
        match ToolNameKind::parse("@myserver").unwrap() {
            ToolNameKind::McpServer { server_name } => assert_eq!(server_name, "myserver"),
            _ => panic!("Expected McpServer variant"),
        }

        // Test plain built-in glob (no prefix)
        match ToolNameKind::parse("bash_*").unwrap() {
            ToolNameKind::BuiltInGlob(glob) => assert_eq!(glob, "bash_*"),
            _ => panic!("Expected BuiltInGlob variant"),
        }

        // Test plain built-in (no prefix)
        match ToolNameKind::parse("bash").unwrap() {
            ToolNameKind::BuiltIn(name) => assert_eq!(name, "bash"),
            _ => panic!("Expected BuiltIn variant"),
        }
    }

    #[test]
    fn test_canonical_tool_name_from_mcp_parts() {
        let n = CanonicalToolName::from_mcp_parts("srv".into(), "tool".into());
        assert!(matches!(n, CanonicalToolName::Mcp { .. }));
    }

    #[test]
    fn test_canonical_tool_name_as_full_name_mcp() {
        let n = CanonicalToolName::from_mcp_parts("srv".into(), "tool".into());
        assert_eq!(n.as_full_name(), "@srv/tool");
    }

    #[test]
    fn test_canonical_tool_name_as_full_name_agent() {
        let n = CanonicalToolName::Agent {
            agent_name: "myagent".into(),
        };
        assert_eq!(n.as_full_name(), "#myagent");
    }

    #[test]
    fn test_canonical_tool_name_as_full_name_built_in() {
        let n = CanonicalToolName::BuiltIn(BuiltInToolName::FsRead);
        let full_name = n.as_full_name();
        assert!(!full_name.is_empty());
    }

    #[test]
    fn test_canonical_tool_name_serde() {
        let n = CanonicalToolName::Mcp {
            server_name: "s".into(),
            tool_name: "t".into(),
        };
        let json = serde_json::to_string(&n).unwrap();
        let parsed: CanonicalToolName = serde_json::from_str(&json).unwrap();
        assert_eq!(n, parsed);
    }

    #[test]
    fn test_canonical_tool_name_eq_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(CanonicalToolName::Mcp {
            server_name: "s".into(),
            tool_name: "t".into(),
        });
        assert!(set.contains(&CanonicalToolName::Mcp {
            server_name: "s".into(),
            tool_name: "t".into(),
        }));
    }

    #[test]
    fn test_tool_name_kind_parse_empty() {
        // Empty string should parse to BuiltIn(empty) per the implementation
        let _result = ToolNameKind::parse("");
        // Just verify it doesn't panic
    }

    #[test]
    fn test_tool_name_kind_parse_mcp_server_only() {
        match ToolNameKind::parse("@myserver").unwrap() {
            ToolNameKind::McpServer { server_name } => assert_eq!(server_name, "myserver"),
            _ => panic!("expected McpServer"),
        }
    }

    #[test]
    fn test_tool_name_kind_parse_agent() {
        match ToolNameKind::parse("#myagent").unwrap() {
            ToolNameKind::Agent(name) => assert_eq!(name, "myagent"),
            _ => panic!("expected Agent"),
        }
    }

    #[test]
    fn test_tool_name_kind_parse_agent_glob() {
        match ToolNameKind::parse("#agent_*").unwrap() {
            ToolNameKind::AgentGlob(name) => assert_eq!(name, "agent_*"),
            _ => panic!("expected AgentGlob"),
        }
    }

    #[test]
    fn test_tool_name_kind_parse_built_in_glob() {
        match ToolNameKind::parse("fs_*").unwrap() {
            ToolNameKind::BuiltInGlob(g) => assert_eq!(g, "fs_*"),
            _ => panic!("expected BuiltInGlob"),
        }
    }

    #[test]
    fn test_canonical_tool_name_tool_name_method() {
        let mcp = CanonicalToolName::from_mcp_parts("server".into(), "tool".into());
        assert_eq!(mcp.tool_name(), "tool");

        let agent = CanonicalToolName::Agent {
            agent_name: "ag".to_string(),
        };
        assert_eq!(agent.tool_name(), "ag");
    }

    #[test]
    fn test_canonical_tool_name_from_built_in_tool_name() {
        let n: CanonicalToolName = BuiltInToolName::FsRead.into();
        assert!(matches!(n, CanonicalToolName::BuiltIn(_)));
    }

    #[test]
    fn test_canonical_tool_name_from_str_built_in() {
        let n: CanonicalToolName = "fs_read".parse().unwrap();
        assert!(matches!(n, CanonicalToolName::BuiltIn(_)));
    }

    #[test]
    fn test_canonical_tool_name_from_str_mcp() {
        let n: CanonicalToolName = "@server/tool".parse().unwrap();
        assert!(matches!(n, CanonicalToolName::Mcp { .. }));
    }

    #[test]
    fn test_canonical_tool_name_from_str_agent() {
        let n: CanonicalToolName = "#myagent".parse().unwrap();
        match n {
            CanonicalToolName::Agent { agent_name } => assert_eq!(agent_name, "myagent"),
            _ => panic!("expected Agent"),
        }
    }

    #[test]
    fn test_canonical_tool_name_from_str_invalid_built_in() {
        let result: Result<CanonicalToolName, _> = "nonexistent_built_in".parse();
        assert!(result.is_err());
    }

    #[test]
    fn test_canonical_tool_name_from_str_unsupported_kind() {
        // McpServer is not supported in CanonicalToolName::from_str
        let result: Result<CanonicalToolName, _> = "@server".parse();
        assert!(result.is_err());
    }
}
