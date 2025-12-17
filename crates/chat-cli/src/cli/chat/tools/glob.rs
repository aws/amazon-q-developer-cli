use std::io::Write;
use std::path::{
    Path,
    PathBuf,
};

use crossterm::{
    queue,
    style,
};
use eyre::{
    Context,
    Result,
};
use globset::{
    Glob as GlobPattern,
    GlobMatcher,
};
use ignore::WalkBuilder;
use serde::Deserialize;
use tracing::error;

use super::{
    InvokeOutput,
    OutputKind,
    ToolInfo,
    display_tool_use,
};
use crate::cli::agent::{
    Agent,
    PermissionEvalResult,
};
use crate::os::Os;
use crate::theme::StyledText;

/// Default maximum number of results to return
const DEFAULT_MAX_RESULTS: usize = 200;
/// Maximum allowed depth to prevent excessive traversal
const MAX_ALLOWED_DEPTH: usize = 100;
/// Default maximum depth for directory traversal
const DEFAULT_MAX_DEPTH: usize = 50;
/// How often to yield to allow cancellation (every N entries)
const YIELD_INTERVAL: u32 = 500;

#[derive(Debug, Clone, Deserialize)]
pub struct Glob {
    /// Glob pattern, e.g. "**/*.rs", "src/**/*.{ts,tsx}", "target/debug/build/**/*"
    pub pattern: String,
    /// Root directory to search from. Defaults to current working directory.
    pub path: Option<String>,
    /// Maximum number of results to return. Defaults to DEFAULT_MAX_RESULTS.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Maximum directory depth to traverse. Defaults to DEFAULT_MAX_DEPTH.
    #[serde(default)]
    pub max_depth: Option<usize>,
}

impl Glob {
    pub const INFO: ToolInfo = ToolInfo {
        spec_name: "glob",
        preferred_alias: "glob",
        aliases: &["glob"],
    };

    pub async fn invoke(&self, os: &Os, _output: &mut impl Write) -> Result<InvokeOutput> {
        let base_path = self.get_base_path(os)?;

        if !base_path.exists() {
            return Ok(InvokeOutput {
                output: OutputKind::Json(serde_json::json!({
                    "error": format!("Path does not exist: {}", base_path.display())
                })),
            });
        }

        if !base_path.is_dir() {
            return Ok(InvokeOutput {
                output: OutputKind::Json(serde_json::json!({
                    "error": format!("Path is not a directory: {}", base_path.display())
                })),
            });
        }

        // Normalize pattern - extract directory prefix if present
        let (search_base, search_pattern) = self.normalize_pattern(&base_path);

        if !search_base.exists() {
            return Ok(InvokeOutput {
                output: OutputKind::Json(serde_json::json!({
                    "error": format!("Path does not exist: {}", search_base.display())
                })),
            });
        }

        // Build glob matcher
        let glob_matcher = match GlobPattern::new(&search_pattern) {
            Ok(g) => g.compile_matcher(),
            Err(e) => {
                return Ok(InvokeOutput {
                    output: OutputKind::Json(serde_json::json!({
                        "error": format!("Invalid glob pattern: {e}")
                    })),
                });
            },
        };

        // Build walker with gitignore support
        let max_depth = self.max_depth.map_or(DEFAULT_MAX_DEPTH, |d| d.min(MAX_ALLOWED_DEPTH));

        let walker = WalkBuilder::new(&search_base)
            .hidden(false)
            .ignore(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .follow_links(false)
            .max_depth(Some(max_depth))
            .build();

        let max_results = self.limit.unwrap_or(DEFAULT_MAX_RESULTS);
        let mut file_paths: Vec<String> = Vec::new();
        let mut total_files: usize = 0;
        let mut entry_count: u32 = 0;

        for entry in walker.flatten() {
            // Yield periodically to allow cancellation (Ctrl+C handling)
            entry_count += 1;
            if entry_count % YIELD_INTERVAL == 0 {
                tokio::task::yield_now().await;
            }

            if entry.file_type().is_some_and(|ft| ft.is_file()) {
                let path = entry.path();

                // Match against the relative path from search_base
                let relative_path = path.strip_prefix(&search_base).unwrap_or(path);

                if Self::matches_glob(&glob_matcher, relative_path, &search_pattern) {
                    total_files += 1;
                    if file_paths.len() < max_results {
                        file_paths.push(path.display().to_string());
                    }
                }
            }
        }

        let truncated = total_files > file_paths.len();

        if total_files == 0 {
            Ok(InvokeOutput {
                output: OutputKind::Json(serde_json::json!({
                    "filePaths": [],
                    "totalFiles": 0,
                    "truncated": false,
                    "message": format!("No files found matching pattern: {}", self.pattern)
                })),
            })
        } else {
            Ok(InvokeOutput {
                output: OutputKind::Json(serde_json::json!({
                    "filePaths": file_paths,
                    "totalFiles": total_files,
                    "truncated": truncated
                })),
            })
        }
    }

    /// Check if path matches the glob pattern
    fn matches_glob(matcher: &GlobMatcher, path: &Path, pattern: &str) -> bool {
        // For patterns like "*.rs", match against filename only
        // For patterns like "**/*.rs" or "src/**", match against full relative path
        if pattern.contains('/') || pattern.starts_with("**") {
            matcher.is_match(path)
        } else {
            // Simple pattern like "*.rs" - match filename only
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|name| matcher.is_match(name))
        }
    }

    /// Normalize pattern to handle cases like "target/debug/build/*"
    /// Returns (base_path, pattern) tuple
    fn normalize_pattern(&self, base_path: &Path) -> (PathBuf, String) {
        let pattern = &self.pattern;

        // If pattern contains no glob characters, treat it as a directory and add **/*
        if !pattern.contains('*') && !pattern.contains('?') && !pattern.contains('[') {
            let potential_dir = base_path.join(pattern);
            if potential_dir.is_dir() {
                return (potential_dir, "**/*".to_string());
            }
        }

        // Find the first path component with glob characters
        let parts: Vec<&str> = pattern.split('/').collect();
        let mut prefix_parts: Vec<&str> = Vec::new();
        let mut pattern_parts: Vec<&str> = Vec::new();
        let mut found_glob = false;

        for part in parts {
            if found_glob || part.contains('*') || part.contains('?') || part.contains('[') {
                found_glob = true;
                pattern_parts.push(part);
            } else {
                prefix_parts.push(part);
            }
        }

        if prefix_parts.is_empty() {
            // Pattern starts with glob, use base_path as-is
            (base_path.to_path_buf(), pattern.clone())
        } else {
            // Extract directory prefix from pattern
            let prefix = prefix_parts.join("/");
            let new_base = base_path.join(&prefix);

            if new_base.is_dir() && !pattern_parts.is_empty() {
                // Use the prefix as new base and rest as pattern
                let remaining_pattern = pattern_parts.join("/");
                (new_base, remaining_pattern)
            } else {
                // Keep original base and pattern
                (base_path.to_path_buf(), pattern.clone())
            }
        }
    }

    fn get_base_path(&self, os: &Os) -> Result<PathBuf> {
        match &self.path {
            Some(p) if !p.is_empty() && p != "undefined" && p != "null" => Ok(PathBuf::from(p)),
            _ => os.env.current_dir().wrap_err("Failed to get current directory"),
        }
    }

    pub fn queue_description(&self, tool: &super::tool::Tool, output: &mut impl Write) -> Result<()> {
        queue!(output, style::Print("Searching for files: "))?;
        queue!(
            output,
            StyledText::brand_fg(),
            style::Print(&self.pattern),
            StyledText::reset()
        )?;

        if let Some(ref path) = self.path {
            if !path.is_empty() && path != "undefined" && path != "null" {
                queue!(output, style::Print(" in "))?;
                queue!(output, StyledText::brand_fg(), style::Print(path), StyledText::reset())?;
            }
        }

        display_tool_use(tool, output)?;
        Ok(())
    }

    pub async fn validate(&mut self, _os: &Os) -> Result<()> {
        if self.pattern.is_empty() {
            return Err(eyre::eyre!("Glob pattern cannot be empty"));
        }

        // Validate glob pattern
        GlobPattern::new(&self.pattern).map_err(|e| eyre::eyre!("Invalid glob pattern '{}': {}", self.pattern, e))?;

        // Clean invalid path values
        if let Some(ref p) = self.path {
            if p == "undefined" || p == "null" || p.is_empty() {
                self.path = None;
            }
        }

        Ok(())
    }

    pub fn eval_perm(&self, _os: &Os, agent: &Agent) -> PermissionEvalResult {
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Settings {
            #[serde(default)]
            denied_paths: Vec<String>,
            #[serde(default = "default_true")]
            auto_allow: bool,
        }

        fn default_true() -> bool {
            true
        }

        match Self::INFO
            .aliases
            .iter()
            .find_map(|alias| agent.tools_settings.get(*alias))
        {
            Some(settings) => {
                let settings: Settings = match serde_json::from_value(settings.clone()) {
                    Ok(s) => s,
                    Err(e) => {
                        error!("Failed to deserialize glob settings: {:?}", e);
                        return PermissionEvalResult::Allow;
                    },
                };

                // Check denied paths
                if let Some(ref search_path) = self.path {
                    for denied in &settings.denied_paths {
                        if search_path.starts_with(denied) {
                            return PermissionEvalResult::Deny(vec![format!("Path '{}' is denied", search_path)]);
                        }
                    }
                }

                if settings.auto_allow {
                    PermissionEvalResult::Allow
                } else {
                    PermissionEvalResult::Ask
                }
            },
            // glob is read-only, allow by default
            None => PermissionEvalResult::Allow,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{
        self,
        File,
    };

    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn test_glob_finds_files() {
        let temp_dir = TempDir::new().unwrap();
        File::create(temp_dir.path().join("test1.rs")).unwrap();
        File::create(temp_dir.path().join("test2.rs")).unwrap();
        File::create(temp_dir.path().join("other.txt")).unwrap();

        let tool = Glob {
            pattern: "*.rs".to_string(),
            path: Some(temp_dir.path().to_string_lossy().to_string()),
            limit: None,
            max_depth: None,
        };

        let os = Os::new().await.unwrap();
        let mut buf = Vec::new();
        let result = tool.invoke(&os, &mut buf).await.unwrap();

        if let OutputKind::Json(json) = result.output {
            assert_eq!(json["totalFiles"], 2);
            assert_eq!(json["truncated"], false);
            let paths = json["filePaths"].as_array().unwrap();
            assert_eq!(paths.len(), 2);
        } else {
            panic!("Expected JSON output");
        }
    }

    #[tokio::test]
    async fn test_glob_recursive() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir_all(temp_dir.path().join("src/lib")).unwrap();
        File::create(temp_dir.path().join("src/main.rs")).unwrap();
        File::create(temp_dir.path().join("src/lib/util.rs")).unwrap();

        let tool = Glob {
            pattern: "**/*.rs".to_string(),
            path: Some(temp_dir.path().to_string_lossy().to_string()),
            limit: None,
            max_depth: None,
        };

        let os = Os::new().await.unwrap();
        let mut buf = Vec::new();
        let result = tool.invoke(&os, &mut buf).await.unwrap();

        if let OutputKind::Json(json) = result.output {
            assert_eq!(json["totalFiles"], 2);
            let paths = json["filePaths"].as_array().unwrap();
            assert_eq!(paths.len(), 2);
        } else {
            panic!("Expected JSON output");
        }
    }

    #[tokio::test]
    async fn test_glob_with_path_prefix() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir_all(temp_dir.path().join("target/debug/build/pkg1")).unwrap();
        File::create(temp_dir.path().join("target/debug/build/pkg1/file.rs")).unwrap();
        File::create(temp_dir.path().join("target/debug/build/root.txt")).unwrap();

        let tool = Glob {
            pattern: "target/debug/build/*".to_string(),
            path: Some(temp_dir.path().to_string_lossy().to_string()),
            limit: None,
            max_depth: None,
        };

        let os = Os::new().await.unwrap();
        let mut buf = Vec::new();
        let result = tool.invoke(&os, &mut buf).await.unwrap();

        if let OutputKind::Json(json) = result.output {
            // Should find root.txt (direct child)
            assert!(json["totalFiles"].as_u64().unwrap() >= 1);
        } else {
            panic!("Expected JSON output");
        }
    }

    #[tokio::test]
    async fn test_glob_with_path_prefix_recursive() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir_all(temp_dir.path().join("target/debug/build/pkg1")).unwrap();
        fs::create_dir_all(temp_dir.path().join("target/debug/build/pkg2")).unwrap();
        File::create(temp_dir.path().join("target/debug/build/pkg1/file1.rs")).unwrap();
        File::create(temp_dir.path().join("target/debug/build/pkg2/file2.rs")).unwrap();
        File::create(temp_dir.path().join("target/debug/build/root.txt")).unwrap();

        let tool = Glob {
            pattern: "target/debug/build/**/*".to_string(),
            path: Some(temp_dir.path().to_string_lossy().to_string()),
            limit: None,
            max_depth: None,
        };

        let os = Os::new().await.unwrap();
        let mut buf = Vec::new();
        let result = tool.invoke(&os, &mut buf).await.unwrap();

        if let OutputKind::Json(json) = result.output {
            // Should find all 3 files
            assert_eq!(json["totalFiles"], 3);
            let paths = json["filePaths"].as_array().unwrap();
            assert_eq!(paths.len(), 3);
        } else {
            panic!("Expected JSON output");
        }
    }

    #[tokio::test]
    async fn test_glob_truncation() {
        let temp_dir = TempDir::new().unwrap();
        for i in 0..10 {
            File::create(temp_dir.path().join(format!("file{i}.txt"))).unwrap();
        }

        let tool = Glob {
            pattern: "*.txt".to_string(),
            path: Some(temp_dir.path().to_string_lossy().to_string()),
            limit: Some(5),
            max_depth: None,
        };

        let os = Os::new().await.unwrap();
        let mut buf = Vec::new();
        let result = tool.invoke(&os, &mut buf).await.unwrap();

        if let OutputKind::Json(json) = result.output {
            assert_eq!(json["totalFiles"], 10);
            assert_eq!(json["truncated"], true);
            let paths = json["filePaths"].as_array().unwrap();
            assert_eq!(paths.len(), 5);
        } else {
            panic!("Expected JSON output");
        }
    }

    #[tokio::test]
    async fn test_glob_no_matches() {
        let temp_dir = TempDir::new().unwrap();
        File::create(temp_dir.path().join("test.txt")).unwrap();

        let tool = Glob {
            pattern: "*.rs".to_string(),
            path: Some(temp_dir.path().to_string_lossy().to_string()),
            limit: None,
            max_depth: None,
        };

        let os = Os::new().await.unwrap();
        let mut buf = Vec::new();
        let result = tool.invoke(&os, &mut buf).await.unwrap();

        if let OutputKind::Json(json) = result.output {
            assert!(json["message"].as_str().unwrap().contains("No files found"));
            assert_eq!(json["totalFiles"], 0);
            assert_eq!(json["truncated"], false);
        } else {
            panic!("Expected JSON output");
        }
    }

    #[tokio::test]
    async fn test_eval_perm() {
        use std::collections::HashMap;

        use crate::cli::agent::ToolSettingTarget;

        let tool = Glob {
            pattern: "*.rs".to_string(),
            path: None,
            limit: None,
            max_depth: None,
        };

        let os = Os::new().await.unwrap();

        // Case 1: default agent -> Allow
        let default_agent = Agent::default();
        let result = tool.eval_perm(&os, &default_agent);
        assert!(matches!(result, PermissionEvalResult::Allow));

        // Case 2: agent disables autoAllow -> Ask
        let agent_with_restriction = Agent {
            name: "test".to_string(),
            tools_settings: {
                let mut settings = HashMap::new();
                settings.insert(
                    ToolSettingTarget("glob".to_string()),
                    serde_json::json!({ "autoAllow": false }),
                );
                settings
            },
            ..Default::default()
        };

        let result = tool.eval_perm(&os, &agent_with_restriction);

        assert!(matches!(result, PermissionEvalResult::Ask));
    }
}
