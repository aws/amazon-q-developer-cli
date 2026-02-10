use std::collections::HashSet;

use globset::{
    Glob,
    GlobSet,
    GlobSetBuilder,
};
use serde::{
    Deserialize,
    Serialize,
};
use tracing::error;

use super::util::path::canonicalize_path_sys;
use super::util::providers::SystemProvider;
use crate::agent::agent_config::definitions::ToolsSettings;
use crate::agent::agent_config::parse::CanonicalToolName;
use crate::agent::protocol::{
    ApprovalResult,
    PermissionEvalResult,
    PermissionOptionId,
};
use crate::agent::tools::use_aws::UseAws;
use crate::agent::tools::{
    BuiltInTool,
    ToolKind,
};
use crate::agent::util::error::UtilError;
use crate::agent::util::glob::matches_any_pattern;

/// Runtime permissions accumulated during a session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimePermissions {
    filesystem: FileSystemPermissions,
    /// Tools trusted at the tool level (auto-approve all uses)
    #[serde(default)]
    trusted_tools: HashSet<CanonicalToolName>,
    /// Tools denied at the tool level (auto-reject all uses)
    #[serde(default)]
    denied_tools: HashSet<CanonicalToolName>,
}

impl RuntimePermissions {
    /// Set CWD as allowed for read operations.
    pub fn with_cwd(mut self, cwd: &str) -> Self {
        self.filesystem.allowed_read_paths.insert(cwd.to_string());
        self
    }

    /// Grant permission for a path.
    /// Write permission also grants read permission.
    /// Removes the path from the corresponding denied set.
    pub fn grant_path<P: SystemProvider>(&mut self, path: &str, access: PathAccessType, provider: &P) {
        let path = match canonicalize_path_sys(path, provider) {
            Ok(p) => p,
            Err(err) => {
                error!(?err, path, "failed to canonicalize path");
                return;
            },
        };
        match access {
            PathAccessType::Read => {
                self.filesystem.allowed_read_paths.insert(path.clone());
                self.filesystem.denied_read_paths.remove(&path);
            },
            PathAccessType::Write => {
                self.filesystem.allowed_write_paths.insert(path.clone());
                self.filesystem.allowed_read_paths.insert(path.clone());
                self.filesystem.denied_write_paths.remove(&path);
                self.filesystem.denied_read_paths.remove(&path);
            },
        }
    }

    /// Deny permission for a path.
    /// Removes the path from the corresponding allowed set.
    pub fn deny_path<P: SystemProvider>(&mut self, path: &str, access: PathAccessType, provider: &P) {
        let path = match canonicalize_path_sys(path, provider) {
            Ok(p) => p,
            Err(err) => {
                error!(?err, path, "failed to canonicalize path");
                return;
            },
        };
        match access {
            PathAccessType::Read => {
                self.filesystem.denied_read_paths.insert(path.clone());
                self.filesystem.allowed_read_paths.remove(&path);
            },
            PathAccessType::Write => {
                self.filesystem.denied_write_paths.insert(path.clone());
                self.filesystem.allowed_write_paths.remove(&path);
            },
        }
    }
}

/// Filesystem permissions with separate read and write path sets.
/// - Each path supports glob matching
/// - Write permissions imply read permissions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct FileSystemPermissions {
    allowed_read_paths: HashSet<String>,
    allowed_write_paths: HashSet<String>,
    denied_read_paths: HashSet<String>,
    denied_write_paths: HashSet<String>,
}

/// Type of filesystem access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathAccessType {
    Read,
    Write,
}

/// Updates runtime permissions based on an "Always" approval result.
/// - `AllowAlwaysTool`: Trusts the entire tool (auto-approve future uses)
/// - `AllowAlwaysToolArgs`: Grants permission for the tool's arguments (e.g., file paths)
/// - `RejectAlwaysTool`: Denies the entire tool (auto-reject future uses)
/// - `RejectAlwaysToolArgs`: Denies permission for the tool's arguments
///
/// No-op for `AllowOnce` or `RejectOnce`.
pub fn apply_approval_to_permissions<P: SystemProvider>(
    permissions: &mut RuntimePermissions,
    tool: &ToolKind,
    result: &ApprovalResult,
    provider: &P,
) {
    match &result.option_id {
        PermissionOptionId::AllowAlwaysTool => {
            permissions.trusted_tools.insert(tool.canonical_tool_name());
        },
        PermissionOptionId::AllowAlwaysToolArgs => {
            let (paths, access) = extract_paths_from_tool(tool, provider);
            for path in paths {
                permissions.grant_path(&path, access, provider);
            }
        },
        PermissionOptionId::RejectAlwaysTool => {
            permissions.denied_tools.insert(tool.canonical_tool_name());
        },
        PermissionOptionId::RejectAlwaysToolArgs => {
            let (paths, access) = extract_paths_from_tool(tool, provider);
            for path in paths {
                permissions.deny_path(&path, access, provider);
            }
        },
        _ => {},
    }
}

pub fn evaluate_tool_permission<P: SystemProvider>(
    permissions: &RuntimePermissions,
    allowed_tools: &HashSet<String>,
    settings: &ToolsSettings,
    tool: &ToolKind,
    provider: &P,
) -> Result<PermissionEvalResult, UtilError> {
    let tn = tool.canonical_tool_name();

    // Check tool-level denial first
    if permissions.denied_tools.contains(&tn) {
        return Ok(PermissionEvalResult::Deny {
            reason: "Tool was denied by user".to_string(),
        });
    }

    let tool_name = tn.as_full_name();
    let is_allowed = match tool {
        ToolKind::BuiltIn(built_in_tool) => {
            allowed_tools.contains("@builtin")
                || allowed_tools.contains("@builtin/")
                || allowed_tools.contains("@builtin/*")
                || matches_any_pattern(allowed_tools, tool_name)
                || built_in_tool.aliases().is_some_and(|aliases| {
                    aliases.iter().any(|alias| {
                        matches_any_pattern(allowed_tools, alias)
                            || matches_any_pattern(allowed_tools, format!("@builtin/{alias}"))
                    })
                })
        },
        ToolKind::Mcp(mcp_tool) => {
            let server_name = &mcp_tool.server_name;
            allowed_tools.contains(&format!("@{server_name}"))
                || allowed_tools.contains(&format!("@{server_name}/"))
                || matches_any_pattern(allowed_tools, &tool_name)
        },
    };

    let result = match tool {
        ToolKind::BuiltIn(built_in) => match built_in {
            BuiltInTool::FileRead(file_read) => evaluate_permission_for_paths(
                settings
                    .fs_read
                    .allowed_paths
                    .iter()
                    .chain(&permissions.filesystem.allowed_read_paths),
                settings
                    .fs_read
                    .denied_paths
                    .iter()
                    .chain(&permissions.filesystem.denied_read_paths),
                file_read.ops.iter().map(|op| &op.path),
                is_allowed,
                provider,
            ),
            BuiltInTool::FileWrite(file_write) => evaluate_permission_for_paths(
                settings
                    .fs_write
                    .allowed_paths
                    .iter()
                    .chain(&permissions.filesystem.allowed_write_paths),
                settings
                    .fs_write
                    .denied_paths
                    .iter()
                    .chain(&permissions.filesystem.denied_write_paths),
                [file_write.path()],
                is_allowed,
                provider,
            ),

            // Reuse the same settings for fs read
            BuiltInTool::Ls(ls) => evaluate_permission_for_paths(
                settings
                    .fs_read
                    .allowed_paths
                    .iter()
                    .chain(&permissions.filesystem.allowed_read_paths),
                settings
                    .fs_read
                    .denied_paths
                    .iter()
                    .chain(&permissions.filesystem.denied_read_paths),
                [&ls.path],
                is_allowed,
                provider,
            ),
            BuiltInTool::ImageRead(image_read) => evaluate_permission_for_paths(
                settings
                    .fs_read
                    .allowed_paths
                    .iter()
                    .chain(&permissions.filesystem.allowed_read_paths),
                settings
                    .fs_read
                    .denied_paths
                    .iter()
                    .chain(&permissions.filesystem.denied_read_paths),
                &image_read.paths,
                is_allowed,
                provider,
            ),
            BuiltInTool::Grep(grep) => {
                let path = grep.get_path(provider).map_err(|e| UtilError::Custom(e.to_string()))?;
                evaluate_permission_for_paths(
                    settings
                        .grep
                        .allowed_paths
                        .iter()
                        .chain(&permissions.filesystem.allowed_read_paths),
                    settings
                        .grep
                        .denied_paths
                        .iter()
                        .chain(&permissions.filesystem.denied_read_paths),
                    [path],
                    is_allowed,
                    provider,
                )
            },
            BuiltInTool::Glob(glob) => {
                let path = glob.get_path(provider).map_err(|e| UtilError::Custom(e.to_string()))?;
                evaluate_permission_for_paths(
                    settings
                        .glob
                        .allowed_paths
                        .iter()
                        .chain(&permissions.filesystem.allowed_read_paths),
                    settings
                        .glob
                        .denied_paths
                        .iter()
                        .chain(&permissions.filesystem.denied_read_paths),
                    [path],
                    is_allowed,
                    provider,
                )
            },

            // Reuse the same settings for fs write
            BuiltInTool::Mkdir(_) => Ok(PermissionEvalResult::Allow),

            BuiltInTool::ExecuteCmd(execute_cmd) => evaluate_permission_for_shell_command(
                &settings.shell.allowed_commands,
                &settings.shell.denied_commands,
                &execute_cmd.command,
                is_allowed,
                settings.shell.auto_allow_readonly,
                settings.shell.deny_by_default,
            ),
            BuiltInTool::Introspect(_) => Ok(PermissionEvalResult::Allow),
            BuiltInTool::SpawnSubagent(_) => Ok(PermissionEvalResult::Allow),
            BuiltInTool::Summary(_) => Ok(PermissionEvalResult::Allow),
            BuiltInTool::UseAws(use_aws) => {
                let key = format!("{}:{}", use_aws.service_name, use_aws.operation_name);
                evaluate_permission_for_aws_command(
                    &settings.use_aws.allowed_services,
                    &settings.use_aws.denied_services,
                    &key,
                    is_allowed,
                    settings.use_aws.auto_allow_readonly,
                )
            },
            BuiltInTool::WebFetch(_) => Ok(if is_allowed {
                PermissionEvalResult::Allow
            } else {
                PermissionEvalResult::Ask
            }),
            BuiltInTool::WebSearch(_) => Ok(if is_allowed {
                PermissionEvalResult::Allow
            } else {
                PermissionEvalResult::Ask
            }),
            BuiltInTool::Code(code) => Ok(if !code.is_write_operation() {
                // Read operations always allowed
                PermissionEvalResult::Allow
            } else if is_allowed {
                PermissionEvalResult::Allow
            } else {
                PermissionEvalResult::Ask
            }),
        },
        ToolKind::Mcp(_) => Ok(if is_allowed {
            PermissionEvalResult::Allow
        } else {
            PermissionEvalResult::Ask
        }),
    }?;

    // Deny results take precedence over trusted_tools
    if matches!(result, PermissionEvalResult::Deny { .. }) {
        return Ok(result);
    }

    // Check tool-level trust
    if permissions.trusted_tools.contains(&tn) {
        return Ok(PermissionEvalResult::Allow);
    }

    Ok(result)
}

/// Evaluate permission for shell commands using the new shell_permission system.
fn evaluate_permission_for_shell_command(
    allowed_commands: &[String],
    denied_commands: &[String],
    command: &str,
    is_allowed: bool,
    auto_allow_readonly: bool,
    deny_by_default: bool,
) -> Result<PermissionEvalResult, UtilError> {
    let settings = super::shell_permission::ShellPermissionSettings {
        allowed_commands: allowed_commands.to_vec(),
        denied_commands: denied_commands.to_vec(),
        auto_allow_readonly,
        deny_by_default,
        is_tool_allowed: is_allowed,
    };
    Ok(super::shell_permission::evaluate_shell_permission(command, &settings))
}

/// Evaluate permission for AWS commands using glob patterns.
fn evaluate_permission_for_aws_command(
    allowed_commands: &[String],
    denied_commands: &[String],
    command: &str,
    is_allowed: bool,
    auto_allow_readonly: bool,
) -> Result<PermissionEvalResult, UtilError> {
    let allow = create_globset(allowed_commands.iter());
    let deny = create_globset(denied_commands.iter());

    let (Ok((_, allow_set)), Ok((deny_items, deny_set))) = (allow, deny) else {
        return Ok(PermissionEvalResult::Ask);
    };

    let denied_matches = deny_set.matches(command);
    if !denied_matches.is_empty() {
        let mut matched = Vec::new();
        for i in denied_matches {
            if let Some(item) = deny_items.get(i) {
                matched.push(item.clone());
            }
        }
        return Ok(PermissionEvalResult::Deny {
            reason: matched.join(", "),
        });
    }

    if !allow_set.matches(command).is_empty() {
        return Ok(PermissionEvalResult::Allow);
    }

    if auto_allow_readonly && UseAws::is_readonly(command) {
        return Ok(PermissionEvalResult::Allow);
    }

    Ok(if is_allowed {
        PermissionEvalResult::Allow
    } else {
        PermissionEvalResult::Ask
    })
}

fn evaluate_permission_for_paths<A, B, C, P>(
    allowed_paths: A,
    denied_paths: B,
    paths_to_check: C,
    is_allowed: bool,
    provider: &P,
) -> Result<PermissionEvalResult, UtilError>
where
    A: IntoIterator,
    A::Item: AsRef<str>,
    B: IntoIterator,
    B::Item: AsRef<str>,
    C: IntoIterator,
    C::Item: AsRef<str>,
    P: SystemProvider,
{
    let allowed_paths = canonicalize_paths(allowed_paths, provider);
    let denied_paths = canonicalize_paths(denied_paths, provider);
    let mut ask = false;
    for path in paths_to_check {
        let path = canonicalize_path_sys(path, provider)?;
        match evaluate_permission_for_path(&path, allowed_paths.iter(), denied_paths.iter()) {
            PermissionCheckResult::Denied(items) => {
                return Ok(PermissionEvalResult::Deny {
                    reason: items.join(", "),
                });
            },
            PermissionCheckResult::Ask => ask = true,
            PermissionCheckResult::Allow => (),
        }
    }
    Ok(if ask && !is_allowed {
        PermissionEvalResult::Ask
    } else {
        PermissionEvalResult::Allow
    })
}

fn canonicalize_paths<I, P>(paths: I, provider: &P) -> Vec<String>
where
    I: IntoIterator,
    I::Item: AsRef<str>,
    P: SystemProvider,
{
    paths
        .into_iter()
        .filter_map(|p| canonicalize_path_sys(p.as_ref(), provider).ok())
        .collect()
}

/// Result of checking a path against allowed and denied paths
#[derive(Debug, Clone, PartialEq, Eq)]
enum PermissionCheckResult {
    Denied(Vec<String>),
    Ask,
    Allow,
}

fn evaluate_permission_for_path<A, B, T>(
    path_to_check: impl AsRef<str>,
    allowed_paths: A,
    denied_paths: B,
) -> PermissionCheckResult
where
    A: Iterator<Item = T>,
    B: Iterator<Item = T>,
    T: AsRef<str>,
{
    let path_to_check = path_to_check.as_ref();
    let allow = create_globset(allowed_paths);
    let deny = create_globset(denied_paths);

    let (Ok((_, allow_set)), Ok((deny_items, deny_set))) = (allow, deny) else {
        return PermissionCheckResult::Ask;
    };

    let denied_matches = deny_set.matches(path_to_check);
    if !denied_matches.is_empty() {
        let mut matched = Vec::new();
        for i in denied_matches {
            if let Some(item) = deny_items.get(i) {
                matched.push(item.clone());
            }
        }
        return PermissionCheckResult::Denied(matched);
    }

    if !allow_set.matches(path_to_check).is_empty() {
        return PermissionCheckResult::Allow;
    }

    PermissionCheckResult::Ask
}

/// Creates a [GlobSet] from a list of strings, returning a list of the strings that were added as
/// part of the glob set (this is required for making use of the [GlobSet::matches] API).
///
/// Paths that fail to be created into a [Glob] are skipped.
fn create_globset<T, U>(paths: T) -> Result<(Vec<String>, GlobSet), UtilError>
where
    T: Iterator<Item = U>,
    U: AsRef<str>,
{
    let mut glob_paths = Vec::new();
    let mut builder = GlobSetBuilder::new();

    for path in paths {
        let path = path.as_ref();
        let Ok(glob_for_file) = Glob::new(path) else {
            continue;
        };

        // remove existing slash in path so we don't end up with double slash
        // Glob doesn't normalize the path so it doesn't work with double slash
        let dir_pattern: String = format!("{}/**", path.trim_end_matches('/'));
        let Ok(glob_for_dir) = Glob::new(&dir_pattern) else {
            continue;
        };

        glob_paths.push(path.to_string());
        glob_paths.push(path.to_string());
        builder.add(glob_for_file);
        builder.add(glob_for_dir);
    }

    Ok((glob_paths, builder.build()?))
}

fn extract_paths_from_tool<P: SystemProvider>(tool: &ToolKind, provider: &P) -> (Vec<String>, PathAccessType) {
    match tool {
        ToolKind::BuiltIn(built_in) => match built_in {
            BuiltInTool::FileRead(t) => (t.ops.iter().map(|op| op.path.clone()).collect(), PathAccessType::Read),
            BuiltInTool::FileWrite(t) => (vec![t.path().to_string()], PathAccessType::Write),
            BuiltInTool::Ls(t) => (vec![t.path.clone()], PathAccessType::Read),
            BuiltInTool::ImageRead(t) => (t.paths.clone(), PathAccessType::Read),
            BuiltInTool::Grep(t) => match t.get_path(provider) {
                Ok(p) => (vec![p], PathAccessType::Read),
                Err(_) => (vec![], PathAccessType::Read),
            },
            BuiltInTool::Glob(t) => match t.get_path(provider) {
                Ok(p) => (vec![p], PathAccessType::Read),
                Err(_) => (vec![], PathAccessType::Read),
            },
            _ => (vec![], PathAccessType::Read),
        },
        ToolKind::Mcp(_) => (vec![], PathAccessType::Read),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::agent_config::definitions::{
        GlobSettings,
        GrepSettings,
    };
    use crate::tools::fs_read::{
        FsRead,
        FsReadOp,
    };
    use crate::tools::fs_write::{
        FileCreate,
        FsWrite,
    };
    use crate::tools::glob::Glob;
    use crate::tools::grep::Grep;
    use crate::tools::mcp::McpTool;
    use crate::util::test::TestProvider;

    #[derive(Debug)]
    struct TestCase {
        path_to_check: String,
        allowed_paths: Vec<String>,
        denied_paths: Vec<String>,
        expected: PermissionCheckResult,
    }

    impl<T, U> From<(T, U, U, PermissionCheckResult)> for TestCase
    where
        T: AsRef<str>,
        U: IntoIterator<Item = T>,
    {
        fn from(value: (T, U, U, PermissionCheckResult)) -> Self {
            Self {
                path_to_check: value.0.as_ref().to_string(),
                allowed_paths: value.1.into_iter().map(|v| v.as_ref().to_string()).collect(),
                denied_paths: value.2.into_iter().map(|v| v.as_ref().to_string()).collect(),
                expected: value.3,
            }
        }
    }

    #[test]
    fn test_evaluate_basic_tool_permission() {
        let provider = TestProvider::new();
        let fs_read_tool = ToolKind::BuiltIn(BuiltInTool::FileRead(FsRead { ops: vec![] }));
        let perms = RuntimePermissions::default();
        let mut allowed_tools = HashSet::new();

        // Test builtin tool with @builtin wildcard
        allowed_tools.insert("@builtin".to_string());
        let settings = ToolsSettings::default();
        let result = evaluate_tool_permission(&perms, &allowed_tools, &settings, &fs_read_tool, &provider);
        assert!(matches!(result, Ok(PermissionEvalResult::Allow)));

        // Test builtin tool with @builtin/ prefix
        allowed_tools.clear();
        allowed_tools.insert("@builtin/".to_string());
        let result = evaluate_tool_permission(&perms, &allowed_tools, &settings, &fs_read_tool, &provider);
        assert!(matches!(result, Ok(PermissionEvalResult::Allow)));

        // Test builtin tool with @builtin/* pattern
        allowed_tools.clear();
        allowed_tools.insert("@builtin/*".to_string());
        let result = evaluate_tool_permission(&perms, &allowed_tools, &settings, &fs_read_tool, &provider);
        assert!(matches!(result, Ok(PermissionEvalResult::Allow)));

        // Test builtin tool with specific tool name
        allowed_tools.clear();
        allowed_tools.insert("@builtin/fs_read".to_string());
        let result = evaluate_tool_permission(&perms, &allowed_tools, &settings, &fs_read_tool, &provider);
        assert!(matches!(result, Ok(PermissionEvalResult::Allow)));

        // Test builtin tool with wildcard
        allowed_tools.clear();
        allowed_tools.insert("@builtin/fs_*".to_string());
        let result_fs_read = evaluate_tool_permission(&perms, &allowed_tools, &settings, &fs_read_tool, &provider);
        let result_fs_write = evaluate_tool_permission(&perms, &allowed_tools, &settings, &fs_read_tool, &provider);
        assert!(matches!(result_fs_read, Ok(PermissionEvalResult::Allow)));
        assert!(matches!(result_fs_write, Ok(PermissionEvalResult::Allow)));

        allowed_tools.clear();
        allowed_tools.insert("fs_*".to_string());
        let result_fs_read = evaluate_tool_permission(&perms, &allowed_tools, &settings, &fs_read_tool, &provider);
        let result_fs_write = evaluate_tool_permission(&perms, &allowed_tools, &settings, &fs_read_tool, &provider);
        assert!(matches!(result_fs_read, Ok(PermissionEvalResult::Allow)));
        assert!(matches!(result_fs_write, Ok(PermissionEvalResult::Allow)));

        // Test MCP tool with server name
        allowed_tools.clear();
        allowed_tools.insert("@test_server".to_string());
        let mcp_tool = ToolKind::Mcp(McpTool {
            server_name: "test_server".to_string(),
            tool_name: "test_tool".to_string(),
            params: None,
        });
        let result = evaluate_tool_permission(&perms, &allowed_tools, &settings, &mcp_tool, &provider);
        assert!(matches!(result, Ok(PermissionEvalResult::Allow)));

        // Test MCP tool with server name and slash
        allowed_tools.clear();
        allowed_tools.insert("@test_server/".to_string());
        let result = evaluate_tool_permission(&perms, &allowed_tools, &settings, &mcp_tool, &provider);
        assert!(matches!(result, Ok(PermissionEvalResult::Allow)));

        // Test MCP tool with full tool name pattern
        allowed_tools.clear();
        allowed_tools.insert("@test_server/*".to_string());
        let result = evaluate_tool_permission(&perms, &allowed_tools, &settings, &mcp_tool, &provider);
        assert!(matches!(result, Ok(PermissionEvalResult::Allow)));

        // Test MCP tool not allowed should ask
        allowed_tools.clear();
        let result = evaluate_tool_permission(&perms, &allowed_tools, &settings, &mcp_tool, &provider);
        assert!(matches!(result, Ok(PermissionEvalResult::Ask)));
    }

    #[test]
    fn test_evaluate_permission_for_path() {
        let sys = TestProvider::new();

        // Test case format: (path_to_check, allowed_paths, denied_paths, expected)
        let test_cases: Vec<TestCase> = [
            ("src/main.rs", vec!["src"], vec![], PermissionCheckResult::Allow),
            (
                "tests/test_file",
                vec!["tests/**"],
                vec![],
                PermissionCheckResult::Allow,
            ),
            (
                "~/home_allow/sub_path",
                vec!["~/home_allow/"],
                vec![],
                PermissionCheckResult::Allow,
            ),
            (
                "denied_dir/sub_path",
                vec![],
                vec!["denied_dir/**/*"],
                PermissionCheckResult::Denied(vec!["denied_dir/**/*".to_string()]),
            ),
            (
                "denied_dir/sub_path",
                vec!["denied_dir"],
                vec!["denied_dir"],
                PermissionCheckResult::Denied(vec!["denied_dir".to_string()]),
            ),
            (
                "denied_dir/allowed/hi",
                vec!["denied_dir/allowed"],
                vec!["denied_dir"],
                PermissionCheckResult::Denied(vec!["denied_dir".to_string()]),
            ),
            (
                "denied_dir/key_id_ecdsa",
                vec![],
                vec!["denied_dir", "*id_ecdsa*"],
                PermissionCheckResult::Denied(vec!["denied_dir".to_string(), "*id_ecdsa*".to_string()]),
            ),
            (
                "denied_dir",
                vec![],
                vec!["denied_dir/**/*"],
                PermissionCheckResult::Ask,
            ),
        ]
        .into_iter()
        .map(TestCase::from)
        .collect();

        for test in test_cases {
            let actual =
                evaluate_permission_for_path(&test.path_to_check, test.allowed_paths.iter(), test.denied_paths.iter());
            assert_eq!(
                actual, test.expected,
                "Received actual result: {:?} for test case: {:?}",
                actual, test,
            );

            // Next, test using canonical paths.
            let path_to_check = canonicalize_path_sys(&test.path_to_check, &sys).unwrap();
            let allowed_paths = test
                .allowed_paths
                .iter()
                .map(|p| canonicalize_path_sys(p, &sys).unwrap())
                .collect::<Vec<_>>();
            let denied_paths = test
                .denied_paths
                .iter()
                .map(|p| canonicalize_path_sys(p, &sys).unwrap())
                .collect::<Vec<_>>();
            let actual = evaluate_permission_for_path(&path_to_check, allowed_paths.iter(), denied_paths.iter());
            assert_eq!(
                std::mem::discriminant(&actual),
                std::mem::discriminant(&test.expected),
                "Received actual result: {:?} for test case: {:?}.\n\nExpanded paths:\n  {}\n  {:?}\n  {:?}",
                actual,
                test,
                path_to_check,
                allowed_paths,
                denied_paths
            );
        }
    }

    #[test]
    fn test_evaluate_permission_for_commands() {
        // Test denied commands (should short circuit)
        // Note: patterns are now regex, so "git push.*" matches "git push origin main"
        let result = evaluate_permission_for_shell_command(
            &["git status".to_string()],
            &["git push.*".to_string()],
            "git push origin main",
            true,
            false,
            false,
        )
        .unwrap();
        assert!(matches!(result, PermissionEvalResult::Deny { .. }));

        // Test allowed commands (regex pattern)
        let result =
            evaluate_permission_for_shell_command(&["git status".to_string()], &[], "git status", false, false, false)
                .unwrap();
        assert!(matches!(result, PermissionEvalResult::Allow));
        let result =
            evaluate_permission_for_shell_command(&["git.*".to_string()], &[], "git status", false, false, false)
                .unwrap();
        assert!(matches!(result, PermissionEvalResult::Allow));

        // Test auto_allow_readonly
        let result = evaluate_permission_for_shell_command(&[], &[], "ls -la", false, true, false).unwrap();
        assert!(matches!(result, PermissionEvalResult::Allow));

        // Test deny_by_default
        let result = evaluate_permission_for_shell_command(&[], &[], "rm file.txt", false, false, true).unwrap();
        assert!(matches!(result, PermissionEvalResult::Deny { .. }));

        // Test normal ask behavior
        let result = evaluate_permission_for_shell_command(&[], &[], "rm file.txt", false, false, false).unwrap();
        assert!(matches!(result, PermissionEvalResult::Ask));
    }

    #[test]
    fn test_evaluate_grep_permission() {
        let provider = TestProvider::new();
        let allowed_tools = HashSet::new();

        let grep_tool = ToolKind::BuiltIn(BuiltInTool::Grep(Grep {
            pattern: "test".to_string(),
            path: Some("/some/path".to_string()),
            include: None,
            case_sensitive: None,
            output_mode: None,
            max_matches_per_file: None,
            max_files: None,
            max_total_lines: None,
            max_depth: None,
        }));

        let mut settings = ToolsSettings {
            grep: GrepSettings {
                allowed_paths: vec!["/some".to_string()],
                denied_paths: vec![],
            },
            ..Default::default()
        };

        let result = evaluate_tool_permission(
            &RuntimePermissions::default(),
            &allowed_tools,
            &settings,
            &grep_tool,
            &provider,
        );
        assert!(matches!(result, Ok(PermissionEvalResult::Allow)));

        // Test grep with denied path
        settings.grep = GrepSettings {
            allowed_paths: vec![],
            denied_paths: vec!["/some".to_string()],
        };

        let result = evaluate_tool_permission(
            &RuntimePermissions::default(),
            &allowed_tools,
            &settings,
            &grep_tool,
            &provider,
        );
        assert!(matches!(result, Ok(PermissionEvalResult::Deny { .. })));

        // Test grep with allowed path
        settings.grep = GrepSettings {
            allowed_paths: vec!["/some".to_string()],
            denied_paths: vec![],
        };

        let result = evaluate_tool_permission(
            &RuntimePermissions::default(),
            &allowed_tools,
            &settings,
            &grep_tool,
            &provider,
        );
        assert!(matches!(result, Ok(PermissionEvalResult::Allow)));
    }

    #[test]
    fn test_evaluate_glob_permission() {
        let provider = TestProvider::new();
        let allowed_tools = HashSet::new();

        let glob_tool = ToolKind::BuiltIn(BuiltInTool::Glob(Glob {
            pattern: "*.rs".to_string(),
            path: Some("/some/path".to_string()),
            limit: None,
            max_depth: None,
        }));

        let mut settings = ToolsSettings {
            glob: GlobSettings {
                allowed_paths: vec!["/some".to_string()],
                denied_paths: vec![],
            },
            ..Default::default()
        };

        let result = evaluate_tool_permission(
            &RuntimePermissions::default(),
            &allowed_tools,
            &settings,
            &glob_tool,
            &provider,
        );
        assert!(matches!(result, Ok(PermissionEvalResult::Allow)));

        // Test glob with denied path
        settings.glob = GlobSettings {
            allowed_paths: vec![],
            denied_paths: vec!["/some".to_string()],
        };

        let result = evaluate_tool_permission(
            &RuntimePermissions::default(),
            &allowed_tools,
            &settings,
            &glob_tool,
            &provider,
        );
        assert!(matches!(result, Ok(PermissionEvalResult::Deny { .. })));

        // Test glob with allowed path
        settings.glob = GlobSettings {
            allowed_paths: vec!["/some".to_string()],
            denied_paths: vec![],
        };

        let result = evaluate_tool_permission(
            &RuntimePermissions::default(),
            &allowed_tools,
            &settings,
            &glob_tool,
            &provider,
        );
        assert!(matches!(result, Ok(PermissionEvalResult::Allow)));
    }

    #[test]
    fn test_runtime_permissions() {
        let provider = TestProvider::new();
        let allowed_tools = HashSet::new();
        let settings = ToolsSettings::default();

        // Use ~ path to test canonicalization
        let fs_read_tool = ToolKind::BuiltIn(BuiltInTool::FileRead(FsRead {
            ops: vec![FsReadOp {
                path: "~/file.txt".to_string(),
                limit: None,
                offset: None,
            }],
        }));
        let fs_write_tool = ToolKind::BuiltIn(BuiltInTool::FileWrite(FsWrite::Create(FileCreate {
            path: "~/file.txt".to_string(),
            content: "test".to_string(),
            start_line: None,
        })));

        let mut permissions = RuntimePermissions::default();

        // No permissions - should ask
        let result = evaluate_tool_permission(&permissions, &allowed_tools, &settings, &fs_read_tool, &provider);
        assert!(matches!(result, Ok(PermissionEvalResult::Ask)));

        // Grant read - should allow read, still ask for write
        permissions.grant_path("~/file.txt", PathAccessType::Read, &provider);
        assert!(
            permissions
                .filesystem
                .allowed_read_paths
                .contains("/home/testuser/file.txt"),
            "~ should be canonicalized to absolute path"
        );
        let result = evaluate_tool_permission(&permissions, &allowed_tools, &settings, &fs_read_tool, &provider);
        assert!(matches!(result, Ok(PermissionEvalResult::Allow)));
        let result = evaluate_tool_permission(&permissions, &allowed_tools, &settings, &fs_write_tool, &provider);
        assert!(matches!(result, Ok(PermissionEvalResult::Ask)));

        // Deny read - should deny (evicts from allowed)
        permissions.deny_path("~/file.txt", PathAccessType::Read, &provider);
        let result = evaluate_tool_permission(&permissions, &allowed_tools, &settings, &fs_read_tool, &provider);
        assert!(matches!(result, Ok(PermissionEvalResult::Deny { .. })));

        // Grant read again - should allow (evicts from denied)
        permissions.grant_path("~/file.txt", PathAccessType::Read, &provider);
        let result = evaluate_tool_permission(&permissions, &allowed_tools, &settings, &fs_read_tool, &provider);
        assert!(matches!(result, Ok(PermissionEvalResult::Allow)));

        // Grant write - should allow both read and write
        permissions.grant_path("~/file.txt", PathAccessType::Write, &provider);
        let result = evaluate_tool_permission(&permissions, &allowed_tools, &settings, &fs_read_tool, &provider);
        assert!(matches!(result, Ok(PermissionEvalResult::Allow)));
        let result = evaluate_tool_permission(&permissions, &allowed_tools, &settings, &fs_write_tool, &provider);
        assert!(matches!(result, Ok(PermissionEvalResult::Allow)));

        // Deny write - should deny write but still allow read
        permissions.deny_path("~/file.txt", PathAccessType::Write, &provider);
        let result = evaluate_tool_permission(&permissions, &allowed_tools, &settings, &fs_read_tool, &provider);
        assert!(matches!(result, Ok(PermissionEvalResult::Allow)));
        let result = evaluate_tool_permission(&permissions, &allowed_tools, &settings, &fs_write_tool, &provider);
        assert!(matches!(result, Ok(PermissionEvalResult::Deny { .. })));
    }

    #[test]
    fn test_fs_read_cwd_auto_allowed() {
        use crate::util::providers::CwdProvider;

        let provider = TestProvider::new();
        let cwd = provider.cwd().unwrap();
        let cwd_str = cwd.to_string_lossy();
        let allowed_tools = HashSet::new();
        let settings = ToolsSettings::default();
        let permissions = RuntimePermissions::default().with_cwd(&cwd_str);

        let make_fs_read = |path: &str| {
            ToolKind::BuiltIn(BuiltInTool::FileRead(FsRead {
                ops: vec![FsReadOp {
                    path: path.to_string(),
                    limit: None,
                    offset: None,
                }],
            }))
        };

        // In CWD - should allow
        let result = evaluate_tool_permission(
            &permissions,
            &allowed_tools,
            &settings,
            &make_fs_read(&format!("{}/file.txt", cwd_str)),
            &provider,
        );
        assert!(matches!(result, Ok(PermissionEvalResult::Allow)), "in CWD: {result:?}");

        // In CWD subdirectory - should allow
        let result = evaluate_tool_permission(
            &permissions,
            &allowed_tools,
            &settings,
            &make_fs_read(&format!("{}/project/src/main.rs", cwd_str)),
            &provider,
        );
        assert!(
            matches!(result, Ok(PermissionEvalResult::Allow)),
            "in CWD subdir: {result:?}"
        );

        // Outside CWD - should ask
        let result = evaluate_tool_permission(
            &permissions,
            &allowed_tools,
            &settings,
            &make_fs_read("/tmp/other/file.txt"),
            &provider,
        );
        assert!(
            matches!(result, Ok(PermissionEvalResult::Ask)),
            "outside CWD: {result:?}"
        );

        // Parent of CWD - should ask
        let result = evaluate_tool_permission(
            &permissions,
            &allowed_tools,
            &settings,
            &make_fs_read("/home/other.txt"),
            &provider,
        );
        assert!(
            matches!(result, Ok(PermissionEvalResult::Ask)),
            "parent of CWD: {result:?}"
        );

        // CWD itself - should allow
        let result = evaluate_tool_permission(
            &permissions,
            &allowed_tools,
            &settings,
            &make_fs_read(&cwd_str),
            &provider,
        );
        assert!(
            matches!(result, Ok(PermissionEvalResult::Allow)),
            "CWD itself: {result:?}"
        );
    }

    #[test]
    fn test_with_cwd() {
        let cwd = "/home/testuser";
        let permissions = RuntimePermissions::default().with_cwd(cwd);

        assert!(permissions.filesystem.allowed_read_paths.contains(cwd));
        assert!(permissions.filesystem.allowed_write_paths.is_empty());
        assert!(permissions.filesystem.denied_read_paths.is_empty());
        assert!(permissions.filesystem.denied_write_paths.is_empty());
    }

    #[test]
    fn test_denied_tools_takes_precedence_over_trusted() {
        let provider = TestProvider::new();
        let allowed_tools = HashSet::new();
        let settings = ToolsSettings::default();
        let fs_read_tool = ToolKind::BuiltIn(BuiltInTool::FileRead(FsRead { ops: vec![] }));
        let tool_name = fs_read_tool.canonical_tool_name();

        let mut permissions = RuntimePermissions::default();
        permissions.trusted_tools.insert(tool_name.clone());
        permissions.denied_tools.insert(tool_name);

        // denied_tools should take precedence over trusted_tools
        let result = evaluate_tool_permission(&permissions, &allowed_tools, &settings, &fs_read_tool, &provider);
        assert!(matches!(result, Ok(PermissionEvalResult::Deny { .. })));
    }

    #[test]
    fn test_path_deny_takes_precedence_over_trusted_tool() {
        let provider = TestProvider::new();
        let allowed_tools = HashSet::new();
        let mut settings = ToolsSettings::default();
        settings.fs_read.denied_paths.push("/denied".to_string());

        let fs_read_tool = ToolKind::BuiltIn(BuiltInTool::FileRead(FsRead {
            ops: vec![FsReadOp {
                path: "/denied/file.txt".to_string(),
                limit: None,
                offset: None,
            }],
        }));
        let tool_name = fs_read_tool.canonical_tool_name();

        let mut permissions = RuntimePermissions::default();
        permissions.trusted_tools.insert(tool_name);

        // Path-level deny should take precedence over tool-level trust
        let result = evaluate_tool_permission(&permissions, &allowed_tools, &settings, &fs_read_tool, &provider);
        assert!(matches!(result, Ok(PermissionEvalResult::Deny { .. })));
    }
}
