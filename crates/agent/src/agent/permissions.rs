use std::collections::HashSet;

use globset::{
    Glob,
    GlobSet,
    GlobSetBuilder,
};

use crate::agent::agent_config::definitions::ToolSettings;
use crate::agent::protocol::PermissionEvalResult;
use crate::agent::tools::{
    BuiltInTool,
    ToolKind,
};
use crate::agent::util::error::UtilError;
use crate::agent::util::glob::matches_any_pattern;
use crate::agent::util::path::canonicalize_path;

pub fn evaluate_tool_permission(
    allowed_tools: &HashSet<String>,
    settings: &ToolSettings,
    tool: &ToolKind,
) -> Result<PermissionEvalResult, UtilError> {
    let tn = tool.canonical_tool_name();
    let tool_name = tn.as_full_name();
    let is_allowed = matches_any_pattern(allowed_tools, &tool_name);

    match tool {
        ToolKind::BuiltIn(built_in) => match built_in {
            BuiltInTool::FileRead(file_read) => evaluate_permission_for_paths(
                &settings.file_read.allowed_paths,
                &settings.file_read.denied_paths,
                file_read.ops.iter().map(|op| &op.path),
                is_allowed,
            ),
            BuiltInTool::FileWrite(file_write) => evaluate_permission_for_paths(
                &settings.file_write.allowed_paths,
                &settings.file_write.denied_paths,
                [file_write.path()],
                is_allowed,
            ),

            // Reuse the same settings for fs read
            BuiltInTool::Ls(ls) => evaluate_permission_for_paths(
                &settings.file_write.allowed_paths,
                &settings.file_write.denied_paths,
                [&ls.path],
                is_allowed,
            ),
            BuiltInTool::ImageRead(image_read) => evaluate_permission_for_paths(
                &settings.file_write.allowed_paths,
                &settings.file_write.denied_paths,
                &image_read.paths,
                is_allowed,
            ),
            BuiltInTool::Grep(_) => Ok(PermissionEvalResult::Allow),

            // Reuse the same settings for fs write
            BuiltInTool::Mkdir(_) => Ok(PermissionEvalResult::Allow),

            BuiltInTool::ExecuteCmd(_) => Ok(PermissionEvalResult::Allow),
            BuiltInTool::Introspect(_) => Ok(PermissionEvalResult::Allow),
            BuiltInTool::SpawnSubagent => Ok(PermissionEvalResult::Allow),
        },
        ToolKind::Mcp(_) => Ok(if is_allowed {
            PermissionEvalResult::Allow
        } else {
            PermissionEvalResult::Ask
        }),
    }
}

fn evaluate_permission_for_paths<T, U>(
    allowed_paths: &[String],
    denied_paths: &[String],
    paths_to_check: T,
    is_allowed: bool,
) -> Result<PermissionEvalResult, UtilError>
where
    T: IntoIterator<Item = U>,
    U: AsRef<str>,
{
    let allowed_paths = canonicalize_paths(allowed_paths);
    let denied_paths = canonicalize_paths(denied_paths);
    let mut ask = false;
    for path in paths_to_check {
        let path = canonicalize_path(path)?;
        match evaluate_permission_for_path(path, allowed_paths.iter(), denied_paths.iter()) {
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

fn canonicalize_paths(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .filter_map(|p| canonicalize_path(p).ok())
        .collect::<Vec<_>>()
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
pub fn create_globset<T, U>(paths: T) -> Result<(Vec<String>, GlobSet), UtilError>
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_evaluate_permission_for_path() {
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
            let path_to_check = canonicalize_path(&test.path_to_check).unwrap();
            let allowed_paths = test
                .allowed_paths
                .iter()
                .map(|p| canonicalize_path(p).unwrap())
                .collect::<Vec<_>>();
            let denied_paths = test
                .denied_paths
                .iter()
                .map(|p| canonicalize_path(p).unwrap())
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
}
