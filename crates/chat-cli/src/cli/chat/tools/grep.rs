use std::io::Write;
use std::path::PathBuf;

use crossterm::{
    queue,
    style,
};
use eyre::{
    Context,
    Result,
};
use grep_regex::RegexMatcherBuilder;
use grep_searcher::sinks::UTF8;
use grep_searcher::{
    BinaryDetection,
    SearcherBuilder,
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

/// Default max matches per file in output
const DEFAULT_MAX_MATCHES_PER_FILE: usize = 5;
/// Default max files to return
const DEFAULT_MAX_FILES: usize = 100;
/// Default max total lines for content mode output
const DEFAULT_MAX_TOTAL_LINES: usize = 200;

/// Output mode for grep results
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    /// Show matching lines with file path and line number (default)
    #[default]
    Content,
    /// Only show file paths that contain matches
    FilesWithMatches,
    /// Show count of matches per file
    Count,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Grep {
    /// Regex pattern to search for
    pub pattern: String,
    /// Directory to search from, defaults to cwd
    pub path: Option<String>,
    /// File filter glob, e.g. "*.rs", "*.{ts,tsx}"
    pub include: Option<String>,
    /// Case-sensitive search, defaults to false
    #[serde(default)]
    pub case_sensitive: Option<bool>,
    /// Output mode: "content", "files_with_matches", "count"
    pub output_mode: Option<OutputMode>,
}

impl Grep {
    pub const INFO: ToolInfo = ToolInfo {
        spec_name: "grep",
        preferred_alias: "grep",
        aliases: &["grep"],
    };

    pub async fn invoke(&self, os: &Os, _output: &mut impl Write) -> Result<InvokeOutput> {
        let base_path = self.get_base_path(os)?;

        if !base_path.exists() {
            return Ok(self.error_response(format!("Path does not exist: {}", base_path.display())));
        }

        // Build regex matcher using RegexMatcherBuilder
        let case_sensitive = self.case_sensitive.unwrap_or(false);
        let matcher = match RegexMatcherBuilder::new()
            .case_insensitive(!case_sensitive)
            .build(&self.pattern)
        {
            Ok(m) => m,
            Err(e) => {
                return Ok(self.error_response(format!("Invalid regex: {}", e)));
            },
        };

        // Collect files to search
        let files = self.collect_files(&base_path)?;

        if files.is_empty() {
            return Ok(InvokeOutput {
                output: OutputKind::Json(serde_json::json!({
                    "message": "No files to search"
                })),
            });
        }

        // Execute search based on output mode
        let output_mode = self.output_mode.unwrap_or_default();
        let result = match output_mode {
            OutputMode::Content => self.search_content(&matcher, &files),
            OutputMode::FilesWithMatches => self.search_files_with_matches(&matcher, &files),
            OutputMode::Count => self.search_count(&matcher, &files),
        };

        Ok(InvokeOutput {
            output: OutputKind::Json(result),
        })
    }

    fn get_base_path(&self, os: &Os) -> Result<PathBuf> {
        match &self.path {
            Some(p) if !p.is_empty() && p != "undefined" && p != "null" => Ok(PathBuf::from(p)),
            _ => os.env.current_dir().wrap_err("Failed to get current directory"),
        }
    }

    fn error_response(&self, message: String) -> InvokeOutput {
        InvokeOutput {
            output: OutputKind::Json(serde_json::json!({ "error": message })),
        }
    }

    fn collect_files(&self, base_path: &PathBuf) -> Result<Vec<PathBuf>> {
        // If path is a file, search only that file
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
            .max_depth(Some(50));

        let mut files = Vec::new();
        let include_glob = self.include.as_deref();

        for entry in walker.build() {
            if let Ok(entry) = entry {
                if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                    let path = entry.path();

                    // Apply glob filter
                    if let Some(pattern) = include_glob {
                        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        if !self.match_include(pattern, file_name) {
                            continue;
                        }
                    }

                    files.push(path.to_path_buf());
                }
            }
        }

        Ok(files)
    }

    fn match_include(&self, pattern: &str, file_name: &str) -> bool {
        // Handle {a,b} brace expansion
        if pattern.contains('{') && pattern.contains('}') {
            if let (Some(start), Some(end)) = (pattern.find('{'), pattern.find('}')) {
                let prefix = &pattern[..start];
                let suffix = &pattern[end + 1..];
                let options = &pattern[start + 1..end];

                return options.split(',').any(|opt| {
                    let expanded = format!("{}{}{}", prefix, opt.trim(), suffix);
                    self.match_glob(&expanded, file_name)
                });
            }
        }
        self.match_glob(pattern, file_name)
    }

    fn match_glob(&self, pattern: &str, text: &str) -> bool {
        let mut p = pattern.chars().peekable();
        let mut t = text.chars().peekable();

        while let Some(pc) = p.next() {
            match pc {
                '*' => {
                    // Skip consecutive *
                    while p.peek() == Some(&'*') {
                        p.next();
                    }
                    // If * is at end, match everything
                    if p.peek().is_none() {
                        return true;
                    }
                    // Try matching rest at each position
                    let rest: String = p.collect();
                    while t.peek().is_some() {
                        let remaining: String = t.clone().collect();
                        if self.match_glob(&rest, &remaining) {
                            return true;
                        }
                        t.next();
                    }
                    return self.match_glob(&rest, "");
                },
                '?' => {
                    if t.next().is_none() {
                        return false;
                    }
                },
                c => match t.next() {
                    Some(tc) if tc.eq_ignore_ascii_case(&c) => {},
                    _ => return false,
                },
            }
        }

        t.peek().is_none()
    }

    /// Search and return matching line content in compact ripgrep-like format
    fn search_content(&self, matcher: &grep_regex::RegexMatcher, files: &[PathBuf]) -> serde_json::Value {
        let mut searcher = SearcherBuilder::new()
            .binary_detection(BinaryDetection::quit(0x00))
            .line_number(true)
            .build();

        // Collect matches per file with counts
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
                    // Format: "line_num:content"
                    file_matches.push(format!("{}:{}", line_num, line.trim_end()));
                    Ok(true) // Continue searching to get accurate count
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

        // Sort by match count descending
        file_results.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

        // Build output with file-grouped matches
        // Limit to first N files and M matches per file for output
        let max_files_output = DEFAULT_MAX_FILES.min(file_results.len());
        let mut output_results: Vec<serde_json::Value> = Vec::new();
        let mut output_match_count: usize = 0;

        for (file, matches) in file_results.iter().take(max_files_output) {
            let matches_to_show = matches.len().min(DEFAULT_MAX_MATCHES_PER_FILE);
            let file_output: Vec<String> = matches.iter().take(matches_to_show).cloned().collect();

            output_match_count += file_output.len();

            output_results.push(serde_json::json!({
                "file": file,
                "count": matches.len(),
                "matches": file_output
            }));

            if output_match_count >= DEFAULT_MAX_TOTAL_LINES {
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

    /// Search and return only file paths with matches, sorted by match count
    fn search_files_with_matches(&self, matcher: &grep_regex::RegexMatcher, files: &[PathBuf]) -> serde_json::Value {
        let mut searcher = SearcherBuilder::new()
            .binary_detection(BinaryDetection::quit(0x00))
            .line_number(false)
            .build();

        // Collect file paths with their match counts
        let mut file_counts: Vec<(String, usize)> = Vec::new();
        let mut total_matches: usize = 0;

        for file_path in files {
            let mut match_count = 0usize;
            let _ = searcher.search_path(
                matcher,
                file_path,
                UTF8(|_line_num, _line| {
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

        // Sort by match count descending
        file_counts.sort_by(|a, b| b.1.cmp(&a.1));

        let total_files = file_counts.len();
        let truncated = total_files > DEFAULT_MAX_FILES;

        // Limit output and format as objects with count
        let results: Vec<serde_json::Value> = file_counts
            .into_iter()
            .take(DEFAULT_MAX_FILES)
            .map(|(file, count)| {
                serde_json::json!({
                    "file": file,
                    "count": count
                })
            })
            .collect();

        serde_json::json!({
            "numMatches": total_matches,
            "numFiles": total_files,
            "truncated": truncated,
            "results": results
        })
    }

    /// Search and return match counts per file, sorted by count descending
    fn search_count(&self, matcher: &grep_regex::RegexMatcher, files: &[PathBuf]) -> serde_json::Value {
        let mut searcher = SearcherBuilder::new()
            .binary_detection(BinaryDetection::quit(0x00))
            .line_number(false)
            .build();

        let mut file_counts: Vec<(String, usize)> = Vec::new();
        let mut total_count = 0usize;

        for file_path in files {
            let mut file_count = 0usize;
            let _ = searcher.search_path(
                matcher,
                file_path,
                UTF8(|_line_num, _line| {
                    file_count += 1;
                    Ok(true)
                }),
            );

            if file_count > 0 {
                total_count += file_count;
                file_counts.push((file_path.display().to_string(), file_count));
            }
        }

        if file_counts.is_empty() {
            return serde_json::json!({
                "message": format!("No matches found for pattern: {}", self.pattern),
                "numMatches": 0,
                "numFiles": 0
            });
        }

        // Sort by count descending
        file_counts.sort_by(|a, b| b.1.cmp(&a.1));

        let total_files = file_counts.len();
        let truncated = total_files > DEFAULT_MAX_FILES;

        let results: Vec<serde_json::Value> = file_counts
            .into_iter()
            .take(DEFAULT_MAX_FILES)
            .map(|(file, count)| {
                serde_json::json!({
                    "file": file,
                    "count": count
                })
            })
            .collect();

        serde_json::json!({
            "numMatches": total_count,
            "numFiles": total_files,
            "truncated": truncated,
            "results": results
        })
    }

    pub fn queue_description(&self, tool: &super::tool::Tool, output: &mut impl Write) -> Result<()> {
        queue!(output, style::Print("Searching for: "))?;
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

        if let Some(ref include) = self.include {
            queue!(output, style::Print(" ("))?;
            queue!(
                output,
                StyledText::brand_fg(),
                style::Print(include),
                StyledText::reset()
            )?;
            queue!(output, style::Print(")"))?;
        }

        display_tool_use(tool, output)?;
        queue!(output, style::Print("\n\n"))?;
        Ok(())
    }

    pub async fn validate(&mut self, _os: &Os) -> Result<()> {
        if self.pattern.is_empty() {
            return Err(eyre::eyre!("Search pattern cannot be empty"));
        }

        // Validate regex using RegexMatcherBuilder
        let case_sensitive = self.case_sensitive.unwrap_or(false);
        RegexMatcherBuilder::new()
            .case_insensitive(!case_sensitive)
            .build(&self.pattern)
            .map_err(|e| eyre::eyre!("Invalid regex '{}': {}", self.pattern, e))?;

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
                        error!("Failed to deserialize grep settings: {:?}", e);
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
            // grep is read-only, allow by default
            None => PermissionEvalResult::Allow,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write as IoWrite;

    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn test_grep_content_mode() {
        let temp_dir = TempDir::new().unwrap();
        let mut file = File::create(temp_dir.path().join("test.txt")).unwrap();
        writeln!(file, "Hello world").unwrap();
        writeln!(file, "Goodbye world").unwrap();
        writeln!(file, "Hello again").unwrap();

        let tool = Grep {
            pattern: "Hello".to_string(),
            path: Some(temp_dir.path().to_string_lossy().to_string()),
            include: None,
            case_sensitive: None,
            output_mode: Some(OutputMode::Content),
        };

        let os = Os::new().await.unwrap();
        let mut buf = Vec::new();
        let result = tool.invoke(&os, &mut buf).await.unwrap();

        if let OutputKind::Json(json) = result.output {
            assert_eq!(json["numMatches"], 2);
            assert_eq!(json["numFiles"], 1);
            let results = json["results"].as_array().unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(results[0]["count"], 2);
        } else {
            panic!("Expected JSON output");
        }
    }

    #[tokio::test]
    async fn test_grep_files_with_matches_mode() {
        let temp_dir = TempDir::new().unwrap();

        let mut file1 = File::create(temp_dir.path().join("file1.txt")).unwrap();
        writeln!(file1, "Hello world").unwrap();

        let mut file2 = File::create(temp_dir.path().join("file2.txt")).unwrap();
        writeln!(file2, "No match here").unwrap();

        let mut file3 = File::create(temp_dir.path().join("file3.txt")).unwrap();
        writeln!(file3, "Hello again").unwrap();
        writeln!(file3, "Hello once more").unwrap();

        let tool = Grep {
            pattern: "Hello".to_string(),
            path: Some(temp_dir.path().to_string_lossy().to_string()),
            include: None,
            case_sensitive: None,
            output_mode: Some(OutputMode::FilesWithMatches),
        };

        let os = Os::new().await.unwrap();
        let mut buf = Vec::new();
        let result = tool.invoke(&os, &mut buf).await.unwrap();

        if let OutputKind::Json(json) = result.output {
            assert_eq!(json["numFiles"], 2);
            assert_eq!(json["numMatches"], 3); // 1 + 2 matches
            let results = json["results"].as_array().unwrap();
            // Sorted by count descending, file3 should be first with 2 matches
            assert_eq!(results[0]["count"], 2);
        } else {
            panic!("Expected JSON output");
        }
    }

    #[tokio::test]
    async fn test_grep_count_mode() {
        let temp_dir = TempDir::new().unwrap();
        let mut file = File::create(temp_dir.path().join("test.txt")).unwrap();
        writeln!(file, "Hello world").unwrap();
        writeln!(file, "Hello again").unwrap();
        writeln!(file, "Hello once more").unwrap();

        let tool = Grep {
            pattern: "Hello".to_string(),
            path: Some(temp_dir.path().to_string_lossy().to_string()),
            include: None,
            case_sensitive: None,
            output_mode: Some(OutputMode::Count),
        };

        let os = Os::new().await.unwrap();
        let mut buf = Vec::new();
        let result = tool.invoke(&os, &mut buf).await.unwrap();

        if let OutputKind::Json(json) = result.output {
            assert_eq!(json["numMatches"], 3);
            assert_eq!(json["numFiles"], 1);
            let results = json["results"].as_array().unwrap();
            assert_eq!(results[0]["count"], 3);
        } else {
            panic!("Expected JSON output");
        }
    }

    #[tokio::test]
    async fn test_grep_case_insensitive() {
        let temp_dir = TempDir::new().unwrap();
        let mut file = File::create(temp_dir.path().join("test.txt")).unwrap();
        writeln!(file, "HELLO world").unwrap();
        writeln!(file, "hello there").unwrap();

        let tool = Grep {
            pattern: "hello".to_string(),
            path: Some(temp_dir.path().to_string_lossy().to_string()),
            include: None,
            case_sensitive: Some(false),
            output_mode: Some(OutputMode::Content),
        };

        let os = Os::new().await.unwrap();
        let mut buf = Vec::new();
        let result = tool.invoke(&os, &mut buf).await.unwrap();

        if let OutputKind::Json(json) = result.output {
            assert_eq!(json["numMatches"], 2);
            assert_eq!(json["numFiles"], 1);
        } else {
            panic!("Expected JSON output");
        }
    }

    #[tokio::test]
    async fn test_grep_with_include_filter() {
        let temp_dir = TempDir::new().unwrap();

        let mut rs_file = File::create(temp_dir.path().join("test.rs")).unwrap();
        writeln!(rs_file, "fn main() {{ println!(\"Hello\"); }}").unwrap();

        let mut txt_file = File::create(temp_dir.path().join("test.txt")).unwrap();
        writeln!(txt_file, "Hello world").unwrap();

        let tool = Grep {
            pattern: "Hello".to_string(),
            path: Some(temp_dir.path().to_string_lossy().to_string()),
            include: Some("*.rs".to_string()),
            case_sensitive: None,
            output_mode: Some(OutputMode::FilesWithMatches),
        };

        let os = Os::new().await.unwrap();
        let mut buf = Vec::new();
        let result = tool.invoke(&os, &mut buf).await.unwrap();

        if let OutputKind::Json(json) = result.output {
            assert_eq!(json["numFiles"], 1);
            let results = json["results"].as_array().unwrap();
            assert!(results[0]["file"].as_str().unwrap().ends_with(".rs"));
        } else {
            panic!("Expected JSON output");
        }
    }

    #[tokio::test]
    async fn test_grep_no_matches() {
        let temp_dir = TempDir::new().unwrap();
        let mut file = File::create(temp_dir.path().join("test.txt")).unwrap();
        writeln!(file, "Nothing here").unwrap();

        let tool = Grep {
            pattern: "NotFound".to_string(),
            path: Some(temp_dir.path().to_string_lossy().to_string()),
            include: None,
            case_sensitive: None,
            output_mode: None,
        };

        let os = Os::new().await.unwrap();
        let mut buf = Vec::new();
        let result = tool.invoke(&os, &mut buf).await.unwrap();

        if let OutputKind::Json(json) = result.output {
            assert!(json["message"].as_str().unwrap().contains("No matches found"));
        } else {
            panic!("Expected JSON output");
        }
    }

    #[tokio::test]
    async fn test_eval_perm_default_allow() {
        let tool = Grep {
            pattern: "test".to_string(),
            path: None,
            include: None,
            case_sensitive: None,
            output_mode: None,
        };

        let agent = Agent::default();
        let os = Os::new().await.unwrap();
        let result = tool.eval_perm(&os, &agent);

        // grep is read-only, should allow by default
        assert!(matches!(result, PermissionEvalResult::Allow));
    }

    #[tokio::test]
    async fn test_eval_perm_auto_allow_disabled() {
        use std::collections::HashMap;

        use crate::cli::agent::ToolSettingTarget;

        let tool = Grep {
            pattern: "test".to_string(),
            path: None,
            include: None,
            case_sensitive: None,
            output_mode: None,
        };

        let agent = Agent {
            name: "test".to_string(),
            tools_settings: {
                let mut map = HashMap::new();
                map.insert(
                    ToolSettingTarget("grep".to_string()),
                    serde_json::json!({ "autoAllow": false }),
                );
                map
            },
            ..Default::default()
        };

        let os = Os::new().await.unwrap();
        let result = tool.eval_perm(&os, &agent);

        // Should ask when explicitly disabled
        assert!(matches!(result, PermissionEvalResult::Ask));
    }

    #[test]
    fn test_include_glob_matching() {
        let tool = Grep {
            pattern: "test".to_string(),
            path: None,
            include: None,
            case_sensitive: None,
            output_mode: None,
        };

        assert!(tool.match_include("*.rs", "main.rs"));
        assert!(tool.match_include("*.{ts,tsx}", "component.ts"));
        assert!(tool.match_include("*.{ts,tsx}", "component.tsx"));
        assert!(!tool.match_include("*.{ts,tsx}", "component.js"));
    }
}
