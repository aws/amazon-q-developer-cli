use std::borrow::Cow;
use std::path::{
    Path,
    PathBuf,
};

use globset::Glob as GlobPattern;
use ignore::WalkBuilder;
use serde::{
    Deserialize,
    Serialize,
};

use super::{
    BuiltInToolName,
    BuiltInToolTrait,
    ToolExecutionError,
    ToolExecutionOutput,
    ToolExecutionOutputItem,
    ToolExecutionResult,
};
use crate::util::path::canonicalize_path_sys;
use crate::util::providers::SystemProvider;

// Hard limits
const MAX_ALLOWED_DEPTH: usize = 50;

// Defaults
const DEFAULT_MAX_RESULTS: usize = 200;
const DEFAULT_MAX_DEPTH: usize = 30;

const GLOB_TOOL_DESCRIPTION: &str = r#"
Find files and directories whose paths match a glob pattern. Respects .gitignore.

WHEN TO USE:
- Finding files by name pattern (e.g., "*.rs", "**/*.tsx")
- Discovering project structure
- Listing files in specific directories

HOW TO USE:
- Provide a glob pattern to match files
- Optionally specify a root directory to search from
- Optionally specify a limit on results and max depth

PATTERNS:
- "*.rs" - All .rs files in current directory
- "**/*.rs" - All .rs files recursively
- "src/**/*.{ts,tsx}" - All TypeScript files under src/
"#;

const GLOB_SCHEMA: &str = r#"
{
    "type": "object",
    "properties": {
        "pattern": {
            "type": "string",
            "description": "Glob pattern, e.g. \"**/*.rs\", \"src/**/*.{ts,tsx}\""
        },
        "path": {
            "type": "string",
            "description": "Root directory to search from. Defaults to current working directory"
        },
        "limit": {
            "type": "integer",
            "description": "Maximum number of results to return"
        },
        "max_depth": {
            "type": "integer",
            "description": "Maximum directory depth to traverse"
        }
    },
    "required": ["pattern"]
}
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Glob {
    pub pattern: String,
    pub path: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub max_depth: Option<usize>,
}

impl BuiltInToolTrait for Glob {
    fn name() -> BuiltInToolName {
        BuiltInToolName::Glob
    }

    fn description() -> Cow<'static, str> {
        GLOB_TOOL_DESCRIPTION.into()
    }

    fn input_schema() -> Cow<'static, str> {
        GLOB_SCHEMA.into()
    }

    fn aliases() -> Option<&'static [&'static str]> {
        Some(&["glob"])
    }
}

impl Glob {
    pub fn get_path<P: SystemProvider>(&self, provider: &P) -> Result<String, ToolExecutionError> {
        match &self.path {
            Some(p) => canonicalize_path_sys(p, provider).map_err(|e| ToolExecutionError::Custom(e.to_string())),
            _ => provider
                .cwd()
                .map(|p| p.to_string_lossy().to_string())
                .map_err(|e| ToolExecutionError::Custom(e.to_string())),
        }
    }

    pub async fn validate<P: SystemProvider>(&self, provider: &P) -> Result<(), String> {
        if self.pattern.is_empty() {
            return Err("Glob pattern cannot be empty".to_string());
        }

        GlobPattern::new(&self.pattern).map_err(|e| format!("Invalid glob pattern '{}': {}", self.pattern, e))?;

        let path = self.get_path(provider).map_err(|e| e.to_string())?;
        let path = PathBuf::from(&path);
        if !path.exists() {
            return Err(format!("Path does not exist: {}", path.display()));
        }
        if !path.is_dir() {
            return Err(format!("Path is not a directory: {}", path.display()));
        }

        Ok(())
    }

    pub async fn execute<P: SystemProvider>(&self, provider: &P) -> ToolExecutionResult {
        let base_path = PathBuf::from(self.get_path(provider)?);

        let (search_base, search_pattern) = self.normalize_pattern(&base_path);

        if !search_base.exists() {
            return Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Json(
                serde_json::json!({"error": format!("Path does not exist: {}", search_base.display())}),
            )]));
        }

        let glob_matcher = match GlobPattern::new(&search_pattern) {
            Ok(g) => g.compile_matcher(),
            Err(e) => {
                return Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Json(
                    serde_json::json!({"error": format!("Invalid glob pattern: {e}")}),
                )]));
            },
        };

        let max_depth = self.max_depth.map_or(DEFAULT_MAX_DEPTH, |d| d.min(MAX_ALLOWED_DEPTH));
        let max_results = self.limit.unwrap_or(DEFAULT_MAX_RESULTS);

        let walker = WalkBuilder::new(&search_base)
            .hidden(false)
            .ignore(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .follow_links(false)
            .max_depth(Some(max_depth))
            .build();

        let mut file_paths: Vec<String> = Vec::new();
        let mut total_files: usize = 0;

        for entry in walker.flatten() {
            if entry.file_type().is_some_and(|ft| ft.is_file()) {
                let path = entry.path();
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

        let result = if total_files == 0 {
            serde_json::json!({
                "filePaths": [],
                "totalFiles": 0,
                "truncated": false,
                "message": format!("No files found matching pattern: {}", self.pattern)
            })
        } else {
            serde_json::json!({
                "filePaths": file_paths,
                "totalFiles": total_files,
                "truncated": truncated
            })
        };

        Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Json(result)]))
    }

    fn matches_glob(matcher: &globset::GlobMatcher, path: &Path, pattern: &str) -> bool {
        if pattern.contains('/') || pattern.starts_with("**") {
            matcher.is_match(path)
        } else {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|name| matcher.is_match(name))
        }
    }

    fn normalize_pattern(&self, base_path: &Path) -> (PathBuf, String) {
        let pattern = &self.pattern;

        // If pattern contains no glob characters, treat it as a directory
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
            (base_path.to_path_buf(), pattern.clone())
        } else {
            let prefix = prefix_parts.join("/");
            let new_base = base_path.join(&prefix);

            if new_base.is_dir() && !pattern_parts.is_empty() {
                let remaining_pattern = pattern_parts.join("/");
                (new_base, remaining_pattern)
            } else {
                (base_path.to_path_buf(), pattern.clone())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{
        self,
        File,
    };

    use super::*;
    use crate::util::test::TestBase;

    #[tokio::test]
    async fn test_glob_finds_files() {
        let test_base = TestBase::new()
            .await
            .with_file(("test1.rs", ""))
            .await
            .with_file(("test2.rs", ""))
            .await
            .with_file(("other.txt", ""))
            .await;

        let tool = Glob {
            pattern: "*.rs".to_string(),
            path: Some(test_base.join("").to_string_lossy().to_string()),
            limit: None,
            max_depth: None,
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Json(json) = &result.items[0] {
            assert_eq!(json["totalFiles"], 2);
            assert_eq!(json["truncated"], false);
        } else {
            panic!("Expected JSON output");
        }
    }

    #[tokio::test]
    async fn test_glob_recursive() {
        let test_base = TestBase::new().await;
        fs::create_dir_all(test_base.join("src/lib")).unwrap();
        File::create(test_base.join("src/main.rs")).unwrap();
        File::create(test_base.join("src/lib/util.rs")).unwrap();

        let tool = Glob {
            pattern: "**/*.rs".to_string(),
            path: Some(test_base.join("").to_string_lossy().to_string()),
            limit: None,
            max_depth: None,
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Json(json) = &result.items[0] {
            assert_eq!(json["totalFiles"], 2);
        } else {
            panic!("Expected JSON output");
        }
    }

    #[tokio::test]
    async fn test_glob_with_path_prefix() {
        let test_base = TestBase::new().await;
        fs::create_dir_all(test_base.join("target/debug/build/pkg1")).unwrap();
        File::create(test_base.join("target/debug/build/pkg1/file.rs")).unwrap();
        File::create(test_base.join("target/debug/build/root.txt")).unwrap();

        let tool = Glob {
            pattern: "target/debug/build/**/*".to_string(),
            path: Some(test_base.join("").to_string_lossy().to_string()),
            limit: None,
            max_depth: None,
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Json(json) = &result.items[0] {
            assert_eq!(json["totalFiles"], 2);
        } else {
            panic!("Expected JSON output");
        }
    }

    #[tokio::test]
    async fn test_glob_truncation() {
        let test_base = TestBase::new().await;
        for i in 0..10 {
            File::create(test_base.join(format!("file{i}.txt"))).unwrap();
        }

        let tool = Glob {
            pattern: "*.txt".to_string(),
            path: Some(test_base.join("").to_string_lossy().to_string()),
            limit: Some(5),
            max_depth: None,
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Json(json) = &result.items[0] {
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
        let test_base = TestBase::new().await.with_file(("test.txt", "")).await;

        let tool = Glob {
            pattern: "*.rs".to_string(),
            path: Some(test_base.join("").to_string_lossy().to_string()),
            limit: None,
            max_depth: None,
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Json(json) = &result.items[0] {
            assert!(json["message"].as_str().unwrap().contains("No files found"));
            assert_eq!(json["totalFiles"], 0);
        } else {
            panic!("Expected JSON output");
        }
    }
}
