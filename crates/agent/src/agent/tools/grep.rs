use std::borrow::Cow;
use std::path::PathBuf;

use globset::GlobBuilder;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::sinks::UTF8;
use grep_searcher::{
    BinaryDetection,
    SearcherBuilder,
};
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
const MAX_ALLOWED_MATCHES_PER_FILE: usize = 30;
const MAX_ALLOWED_FILES: usize = 400;
const MAX_ALLOWED_TOTAL_LINES: usize = 300;
const MAX_ALLOWED_DEPTH: usize = 50;
const MAX_LINE_LENGTH: usize = 500;

// Defaults
const DEFAULT_MAX_MATCHES_PER_FILE: usize = 5;
const DEFAULT_MAX_FILES: usize = 100;
const DEFAULT_MAX_TOTAL_LINES: usize = 100;
const DEFAULT_MAX_DEPTH: usize = 30;

const GREP_TOOL_DESCRIPTION: &str = r#"
Fast text pattern search in files using regex. Respects .gitignore.

WHEN TO USE:
- Searching for literal text patterns, error messages, TODOs, config values
- Finding files containing specific text

WHEN NOT TO USE:
- For semantic code understanding, use the code tool instead
- For finding symbol definitions or usages, use the code tool

HOW TO USE:
- Provide a regex pattern to search for
- Optionally specify a path to search from (defaults to current directory)
- Optionally specify a file filter glob (e.g., "*.rs", "*.{ts,tsx}")

OUTPUT MODES:
- content: Show matching lines with file path and line number (default)
- files_with_matches: Only show file paths that contain matches
- count: Show count of matches per file
"#;

const GREP_SCHEMA: &str = r#"
{
    "type": "object",
    "properties": {
        "pattern": {
            "type": "string",
            "description": "Regex pattern to search for"
        },
        "path": {
            "type": "string",
            "description": "Directory to search from, defaults to current working directory"
        },
        "include": {
            "type": "string",
            "description": "File filter glob, e.g. \"*.rs\", \"*.{ts,tsx}\""
        },
        "case_sensitive": {
            "type": "boolean",
            "description": "Case-sensitive search, defaults to false"
        },
        "output_mode": {
            "type": "string",
            "enum": ["content", "files_with_matches", "count"],
            "description": "Output format: content (default), files_with_matches, or count"
        },
        "max_matches_per_file": {
            "type": "integer",
            "description": "Maximum matches to return per file (content mode only)"
        },
        "max_files": {
            "type": "integer",
            "description": "Maximum number of files to include in results"
        },
        "max_total_lines": {
            "type": "integer",
            "description": "Maximum total lines in output (content mode only)"
        },
        "max_depth": {
            "type": "integer",
            "description": "Maximum directory depth to traverse"
        }
    },
    "required": ["pattern"]
}
"#;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    #[default]
    Content,
    FilesWithMatches,
    Count,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Grep {
    pub pattern: String,
    pub path: Option<String>,
    pub include: Option<String>,
    #[serde(default)]
    pub case_sensitive: Option<bool>,
    pub output_mode: Option<OutputMode>,
    #[serde(default)]
    pub max_matches_per_file: Option<usize>,
    #[serde(default)]
    pub max_files: Option<usize>,
    #[serde(default)]
    pub max_total_lines: Option<usize>,
    #[serde(default)]
    pub max_depth: Option<usize>,
}

impl BuiltInToolTrait for Grep {
    fn name() -> BuiltInToolName {
        BuiltInToolName::Grep
    }

    fn description() -> Cow<'static, str> {
        GREP_TOOL_DESCRIPTION.into()
    }

    fn input_schema() -> Cow<'static, str> {
        GREP_SCHEMA.into()
    }

    fn aliases() -> Option<&'static [&'static str]> {
        Some(&["grep"])
    }
}

impl Grep {
    fn max_matches_per_file(&self) -> usize {
        self.max_matches_per_file.map_or(DEFAULT_MAX_MATCHES_PER_FILE, |v| {
            v.clamp(1, MAX_ALLOWED_MATCHES_PER_FILE)
        })
    }

    fn max_files(&self) -> usize {
        self.max_files
            .map_or(DEFAULT_MAX_FILES, |v| v.clamp(1, MAX_ALLOWED_FILES))
    }

    fn max_total_lines(&self) -> usize {
        self.max_total_lines
            .map_or(DEFAULT_MAX_TOTAL_LINES, |v| v.clamp(1, MAX_ALLOWED_TOTAL_LINES))
    }

    fn max_depth(&self) -> usize {
        self.max_depth
            .map_or(DEFAULT_MAX_DEPTH, |v| v.clamp(1, MAX_ALLOWED_DEPTH))
    }

    pub fn get_path<P: SystemProvider>(&self, provider: &P) -> Result<String, ToolExecutionError> {
        match &self.path {
            Some(p) if !p.is_empty() && p != "undefined" && p != "null" => {
                canonicalize_path_sys(p, provider).map_err(|e| ToolExecutionError::Custom(e.to_string()))
            },
            _ => provider
                .cwd()
                .map(|p| p.to_string_lossy().to_string())
                .map_err(|e| ToolExecutionError::Custom(e.to_string())),
        }
    }

    pub async fn validate<P: SystemProvider>(&self, provider: &P) -> Result<(), String> {
        if self.pattern.is_empty() {
            return Err("Search pattern cannot be empty".to_string());
        }

        let case_sensitive = self.case_sensitive.unwrap_or(false);
        RegexMatcherBuilder::new()
            .case_insensitive(!case_sensitive)
            .build(&self.pattern)
            .map_err(|e| format!("Invalid regex '{}': {}", self.pattern, e))?;

        let path = self.get_path(provider).map_err(|e| e.to_string())?;
        let path = PathBuf::from(&path);
        if !path.exists() {
            return Err(format!("Path does not exist: {}", path.display()));
        }

        Ok(())
    }

    pub async fn execute<P: SystemProvider>(&self, provider: &P) -> ToolExecutionResult {
        let base_path = PathBuf::from(self.get_path(provider)?);

        let case_sensitive = self.case_sensitive.unwrap_or(false);
        let matcher = RegexMatcherBuilder::new()
            .case_insensitive(!case_sensitive)
            .build(&self.pattern)
            .map_err(|e| ToolExecutionError::Custom(format!("Invalid regex: {e}")))?;

        let files = self.collect_files(&base_path).await?;
        if files.is_empty() {
            return Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Json(
                serde_json::json!({"message": "No files to search", "numMatches": 0, "numFiles": 0}),
            )]));
        }

        let result = match self.output_mode.unwrap_or_default() {
            OutputMode::Content => self.search_content(&matcher, &files).await,
            OutputMode::FilesWithMatches => self.search_files_with_matches(&matcher, &files).await,
            OutputMode::Count => self.search_count(&matcher, &files).await,
        };

        Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Json(result)]))
    }

    async fn collect_files(&self, base_path: &PathBuf) -> Result<Vec<PathBuf>, ToolExecutionError> {
        if base_path.is_file() {
            return Ok(vec![base_path.clone()]);
        }

        let mut walker = WalkBuilder::new(base_path);
        walker
            .hidden(false)
            .ignore(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .follow_links(false)
            .max_depth(Some(self.max_depth()));

        let mut files = Vec::new();
        let include_glob = self.include.as_deref();

        for entry in walker.build().flatten() {
            if entry.file_type().is_some_and(|ft| ft.is_file()) {
                let path = entry.path();
                if let Some(pattern) = include_glob {
                    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if !Self::match_include(pattern, file_name) {
                        continue;
                    }
                }
                files.push(path.to_path_buf());
            }
        }

        Ok(files)
    }

    fn match_include(pattern: &str, file_name: &str) -> bool {
        GlobBuilder::new(pattern)
            .case_insensitive(false)
            .build()
            .ok()
            .is_some_and(|g| g.compile_matcher().is_match(file_name))
    }

    fn truncate_line(line: &str) -> String {
        let trimmed = line.trim_end();
        if trimmed.len() > MAX_LINE_LENGTH {
            let truncated: String = trimmed.chars().take(MAX_LINE_LENGTH).collect();
            let remaining = trimmed.chars().count() - MAX_LINE_LENGTH;
            format!("{truncated}...[+{remaining} chars]")
        } else {
            trimmed.to_string()
        }
    }

    async fn search_content(&self, matcher: &grep_regex::RegexMatcher, files: &[PathBuf]) -> serde_json::Value {
        let mut searcher = SearcherBuilder::new()
            .binary_detection(BinaryDetection::quit(0x00))
            .line_number(true)
            .build();

        let max_matches_per_file = self.max_matches_per_file();
        let max_files = self.max_files();
        let max_total_lines = self.max_total_lines();

        let mut file_results: Vec<(String, Vec<String>)> = Vec::new();
        let mut total_matches: usize = 0;
        let mut total_files_with_matches: usize = 0;

        for file_path in files {
            let file_str = file_path.display().to_string();
            let mut file_matches: Vec<String> = Vec::new();

            let _ = searcher.search_path(
                matcher,
                file_path,
                UTF8(|line_num, line| {
                    let display_line = Self::truncate_line(line);
                    file_matches.push(format!("{line_num}:{display_line}"));
                    Ok(true)
                }),
            );

            if !file_matches.is_empty() {
                total_matches += file_matches.len();
                total_files_with_matches += 1;
                file_results.push((file_str, file_matches));
            }
        }

        if file_results.is_empty() {
            return serde_json::json!({
                "message": format!("No matches found for pattern: {}", self.pattern),
                "numMatches": 0,
                "numFiles": 0
            });
        }

        file_results.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

        let max_files_output = max_files.min(file_results.len());
        let mut output_results: Vec<serde_json::Value> = Vec::new();
        let mut output_match_count: usize = 0;

        for (file, matches) in file_results.iter().take(max_files_output) {
            let matches_to_show = matches.len().min(max_matches_per_file);
            let file_output: Vec<String> = matches.iter().take(matches_to_show).cloned().collect();
            output_match_count += file_output.len();

            output_results.push(serde_json::json!({
                "file": file,
                "count": matches.len(),
                "matches": file_output
            }));

            if output_match_count >= max_total_lines {
                break;
            }
        }

        let truncated = total_files_with_matches > output_results.len() || total_matches > output_match_count;

        serde_json::json!({
            "numMatches": total_matches,
            "numFiles": total_files_with_matches,
            "truncated": truncated,
            "results": output_results
        })
    }

    async fn search_files_with_matches(
        &self,
        matcher: &grep_regex::RegexMatcher,
        files: &[PathBuf],
    ) -> serde_json::Value {
        let mut searcher = SearcherBuilder::new()
            .binary_detection(BinaryDetection::quit(0x00))
            .line_number(true)
            .build();

        let max_files = self.max_files();
        let mut file_counts: Vec<(String, usize)> = Vec::new();
        let mut total_matches: usize = 0;

        for file_path in files {
            let mut match_count = 0usize;
            let _ = searcher.search_path(
                matcher,
                file_path,
                UTF8(|_, _| {
                    match_count += 1;
                    Ok(true)
                }),
            );

            if match_count > 0 {
                total_matches += match_count;
                file_counts.push((file_path.display().to_string(), match_count));
            }
        }

        if file_counts.is_empty() {
            return serde_json::json!({
                "message": format!("No matches found for pattern: {}", self.pattern),
                "numMatches": 0,
                "numFiles": 0
            });
        }

        file_counts.sort_by(|a, b| b.1.cmp(&a.1));
        let total_files = file_counts.len();
        let truncated = total_files > max_files;

        let results: Vec<serde_json::Value> = file_counts
            .into_iter()
            .take(max_files)
            .map(|(file, count)| serde_json::json!({"file": file, "count": count}))
            .collect();

        serde_json::json!({
            "numMatches": total_matches,
            "numFiles": total_files,
            "truncated": truncated,
            "results": results
        })
    }

    async fn search_count(&self, matcher: &grep_regex::RegexMatcher, files: &[PathBuf]) -> serde_json::Value {
        let mut searcher = SearcherBuilder::new()
            .binary_detection(BinaryDetection::quit(0x00))
            .line_number(true)
            .build();

        let max_files = self.max_files();
        let mut file_counts: Vec<(String, usize)> = Vec::new();
        let mut total_count = 0usize;

        for file_path in files {
            let mut count = 0usize;
            let _ = searcher.search_path(
                matcher,
                file_path,
                UTF8(|_, _| {
                    count += 1;
                    Ok(true)
                }),
            );

            if count > 0 {
                total_count += count;
                file_counts.push((file_path.display().to_string(), count));
            }
        }

        if file_counts.is_empty() {
            return serde_json::json!({
                "message": format!("No matches found for pattern: {}", self.pattern),
                "numMatches": 0,
                "numFiles": 0
            });
        }

        file_counts.sort_by(|a, b| b.1.cmp(&a.1));
        let total_files = file_counts.len();
        let truncated = total_files > max_files;

        let results: Vec<serde_json::Value> = file_counts
            .into_iter()
            .take(max_files)
            .map(|(file, count)| serde_json::json!({"file": file, "count": count}))
            .collect();

        serde_json::json!({
            "numMatches": total_count,
            "numFiles": total_files,
            "truncated": truncated,
            "results": results
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write as IoWrite;

    use super::*;
    use crate::util::test::TestBase;

    #[tokio::test]
    async fn test_grep_content_mode() {
        let test_base = TestBase::new()
            .await
            .with_file(("test.txt", "Hello world\nGoodbye world\nHello again"))
            .await;

        let tool = Grep {
            pattern: "Hello".to_string(),
            path: Some(test_base.join("").to_string_lossy().to_string()),
            include: None,
            case_sensitive: None,
            output_mode: Some(OutputMode::Content),
            max_matches_per_file: None,
            max_files: None,
            max_total_lines: None,
            max_depth: None,
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Json(json) = &result.items[0] {
            assert_eq!(json["numMatches"], 2);
            assert_eq!(json["numFiles"], 1);
        } else {
            panic!("Expected JSON output");
        }
    }

    #[tokio::test]
    async fn test_grep_files_with_matches_mode() {
        let test_base = TestBase::new()
            .await
            .with_file(("file1.txt", "Hello world"))
            .await
            .with_file(("file2.txt", "No match here"))
            .await
            .with_file(("file3.txt", "Hello again\nHello once more"))
            .await;

        let tool = Grep {
            pattern: "Hello".to_string(),
            path: Some(test_base.join("").to_string_lossy().to_string()),
            include: None,
            case_sensitive: None,
            output_mode: Some(OutputMode::FilesWithMatches),
            max_matches_per_file: None,
            max_files: None,
            max_total_lines: None,
            max_depth: None,
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Json(json) = &result.items[0] {
            assert_eq!(json["numFiles"], 2);
            assert_eq!(json["numMatches"], 3);
        } else {
            panic!("Expected JSON output");
        }
    }

    #[tokio::test]
    async fn test_grep_count_mode() {
        let test_base = TestBase::new()
            .await
            .with_file(("test.txt", "Hello world\nHello again\nHello once more"))
            .await;

        let tool = Grep {
            pattern: "Hello".to_string(),
            path: Some(test_base.join("").to_string_lossy().to_string()),
            include: None,
            case_sensitive: None,
            output_mode: Some(OutputMode::Count),
            max_matches_per_file: None,
            max_files: None,
            max_total_lines: None,
            max_depth: None,
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Json(json) = &result.items[0] {
            assert_eq!(json["numMatches"], 3);
            assert_eq!(json["numFiles"], 1);
        } else {
            panic!("Expected JSON output");
        }
    }

    #[tokio::test]
    async fn test_grep_case_insensitive() {
        let test_base = TestBase::new()
            .await
            .with_file(("test.txt", "HELLO world\nhello there"))
            .await;

        let tool = Grep {
            pattern: "hello".to_string(),
            path: Some(test_base.join("").to_string_lossy().to_string()),
            include: None,
            case_sensitive: Some(false),
            output_mode: Some(OutputMode::Content),
            max_matches_per_file: None,
            max_files: None,
            max_total_lines: None,
            max_depth: None,
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Json(json) = &result.items[0] {
            assert_eq!(json["numMatches"], 2);
        } else {
            panic!("Expected JSON output");
        }
    }

    #[tokio::test]
    async fn test_grep_with_include_filter() {
        let test_base = TestBase::new()
            .await
            .with_file(("test.rs", "fn main() { println!(\"Hello\"); }"))
            .await
            .with_file(("test.txt", "Hello world"))
            .await;

        let tool = Grep {
            pattern: "Hello".to_string(),
            path: Some(test_base.join("").to_string_lossy().to_string()),
            include: Some("*.rs".to_string()),
            case_sensitive: None,
            output_mode: Some(OutputMode::FilesWithMatches),
            max_matches_per_file: None,
            max_files: None,
            max_total_lines: None,
            max_depth: None,
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Json(json) = &result.items[0] {
            assert_eq!(json["numFiles"], 1);
            let results = json["results"].as_array().unwrap();
            assert!(results[0]["file"].as_str().unwrap().ends_with(".rs"));
        } else {
            panic!("Expected JSON output");
        }
    }

    #[tokio::test]
    async fn test_grep_no_matches() {
        let test_base = TestBase::new().await.with_file(("test.txt", "Nothing here")).await;

        let tool = Grep {
            pattern: "NotFound".to_string(),
            path: Some(test_base.join("").to_string_lossy().to_string()),
            include: None,
            case_sensitive: None,
            output_mode: None,
            max_matches_per_file: None,
            max_files: None,
            max_total_lines: None,
            max_depth: None,
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Json(json) = &result.items[0] {
            assert!(json["message"].as_str().unwrap().contains("No matches found"));
        } else {
            panic!("Expected JSON output");
        }
    }

    #[tokio::test]
    async fn test_grep_long_line_truncation() {
        let test_base = TestBase::new().await;
        let file_path = test_base.join("bundle.min.js");
        let mut file = File::create(&file_path).unwrap();
        let long_line = "x".repeat(10_000);
        writeln!(file, "{}", long_line).unwrap();

        let tool = Grep {
            pattern: "x".to_string(),
            path: Some(test_base.join("").to_string_lossy().to_string()),
            include: None,
            case_sensitive: None,
            output_mode: Some(OutputMode::Content),
            max_matches_per_file: None,
            max_files: None,
            max_total_lines: None,
            max_depth: None,
        };

        let result = tool.execute(&test_base).await.unwrap();
        if let ToolExecutionOutputItem::Json(json) = &result.items[0] {
            let results = json["results"].as_array().unwrap();
            let matches = results[0]["matches"].as_array().unwrap();
            let first_match = matches[0].as_str().unwrap();
            assert!(first_match.len() < 1000);
            assert!(first_match.contains("...[+"));
        } else {
            panic!("Expected JSON output");
        }
    }

    #[test]
    fn test_include_glob_matching() {
        assert!(Grep::match_include("*.rs", "main.rs"));
        assert!(Grep::match_include("*.{ts,tsx}", "component.ts"));
        assert!(Grep::match_include("*.{ts,tsx}", "component.tsx"));
        assert!(!Grep::match_include("*.{ts,tsx}", "component.js"));
    }
}
