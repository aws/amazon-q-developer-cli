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
use globset::{
    GlobBuilder,
    GlobSetBuilder,
};
use grep_regex::RegexMatcherBuilder;
use grep_searcher::sinks::UTF8;
use grep_searcher::{
    BinaryDetection,
    SearcherBuilder,
};
use ignore::WalkBuilder;
use serde::Deserialize;
use tracing::{
    error,
    warn,
};

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
use crate::cli::chat::util::truncate_safe;
use crate::os::Os;
use crate::theme::StyledText;
use crate::util::paths;
use crate::util::tool_permission_checker::is_tool_in_allowlist;

// Constants: Maximum allowed values (hard limits for safety)
const MAX_ALLOWED_MATCHES_PER_FILE: usize = 30;
const MAX_ALLOWED_FILES: usize = 400;
const MAX_ALLOWED_TOTAL_LINES: usize = 300;
const MAX_ALLOWED_DEPTH: usize = 50;
/// Maximum characters per line to prevent minified files from blowing up output
const MAX_LINE_LENGTH: usize = 500;

// Constants: Default values (conservative defaults for typical usage)
/// Default max matches per file in output
const DEFAULT_MAX_MATCHES_PER_FILE: usize = 5;
/// Default max files to return
const DEFAULT_MAX_FILES: usize = 100;
/// Default max total lines for content mode output
const DEFAULT_MAX_TOTAL_LINES: usize = 100;
/// Default max directory depth
const DEFAULT_MAX_DEPTH: usize = 30;
/// How often to yield to allow cancellation (every N files)
const YIELD_INTERVAL: u32 = 100;

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
    /// Maximum matches to return per file (content mode only).
    #[serde(default)]
    pub max_matches_per_file: Option<usize>,
    /// Maximum number of files to include in results.
    #[serde(default)]
    pub max_files: Option<usize>,
    /// Maximum total lines in output (content mode only).
    #[serde(default)]
    pub max_total_lines: Option<usize>,
    /// Maximum directory depth to traverse.
    #[serde(default)]
    pub max_depth: Option<usize>,
}

impl Grep {
    pub const INFO: ToolInfo = ToolInfo {
        spec_name: "grep",
        preferred_alias: "grep",
        aliases: &["grep"],
    };

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

    pub async fn invoke(&self, os: &Os, output: &mut impl Write) -> Result<InvokeOutput> {
        let base_path = self.get_base_path(os)?;

        if !base_path.exists() {
            return Ok(InvokeOutput {
                output: OutputKind::Json(serde_json::json!({
                    "error": format!("Path does not exist: {}", base_path.display())
                })),
            });
        }

        // Build regex matcher using RegexMatcherBuilder
        let case_sensitive = self.case_sensitive.unwrap_or(false);
        let matcher = match RegexMatcherBuilder::new()
            .case_insensitive(!case_sensitive)
            .build(&self.pattern)
        {
            Ok(m) => m,
            Err(e) => {
                return Ok(InvokeOutput {
                    output: OutputKind::Json(serde_json::json!({
                        "error": format!("Invalid regex: {e}")
                    })),
                });
            },
        };

        // Collect files to search
        let files = self.collect_files(&base_path).await?;

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
            OutputMode::Content => self.search_content(&matcher, &files).await,
            OutputMode::FilesWithMatches => self.search_files_with_matches(&matcher, &files).await,
            OutputMode::Count => self.search_count(&matcher, &files).await,
        };

        // Display result summary
        let num_matches = result["numMatches"].as_u64().unwrap_or(0);
        let num_files = result["numFiles"].as_u64().unwrap_or(0);
        let truncated = result["truncated"].as_bool().unwrap_or(false);
        let search_path = self.path.as_deref().unwrap_or("current directory");
        let summary = if num_matches == 0 {
            format!(
                "No matches found for pattern: {} under {}",
                StyledText::secondary(&self.pattern),
                search_path,
            )
        } else {
            let truncated_suffix = if truncated { " (result is truncated)" } else { "" };
            format!(
                "Successfully found {} in {} under {}{}",
                StyledText::secondary(&format!("{num_matches} matches",)),
                StyledText::secondary(&format!("{num_files} files",)),
                search_path,
                truncated_suffix
            )
        };

        super::queue_function_result(&summary, output, num_matches == 0, false)?;

        Ok(InvokeOutput {
            output: OutputKind::Json(result),
        })
    }

    fn get_base_path(&self, os: &Os) -> Result<PathBuf> {
        match &self.path {
            Some(p) if !p.is_empty() => Ok(PathBuf::from(p)),
            _ => os.env.current_dir().wrap_err("Failed to get current directory"),
        }
    }

    async fn collect_files(&self, base_path: &PathBuf) -> Result<Vec<PathBuf>> {
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
            .max_depth(Some(self.max_depth()));

        let mut files = Vec::new();
        let include_glob = self.include.as_deref();
        let mut entry_count: u32 = 0;

        for entry in walker.build().flatten() {
            // Yield periodically to allow cancellation (Ctrl+C handling)
            entry_count += 1;
            if entry_count % YIELD_INTERVAL == 0 {
                tokio::task::yield_now().await;
            }

            if entry.file_type().is_some_and(|ft| ft.is_file()) {
                let path = entry.path();

                // Apply glob filter
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

    /// Search and return matching line content in compact ripgrep-like format
    async fn search_content(&self, matcher: &grep_regex::RegexMatcher, files: &[PathBuf]) -> serde_json::Value {
        let mut searcher = SearcherBuilder::new()
            .binary_detection(BinaryDetection::quit(0x00))
            .line_number(true)
            .build();

        // Get configured limits
        let max_matches_per_file = self.max_matches_per_file();
        let max_files = self.max_files();
        let max_total_lines = self.max_total_lines();

        // Collect matches per file with counts
        let mut file_results: Vec<(String, Vec<String>)> = Vec::new();
        let mut total_matches: usize = 0;
        let mut total_files_with_matches: usize = 0;
        let mut file_count: u32 = 0;

        for file_path in files {
            // Yield periodically to allow cancellation
            file_count += 1;
            if file_count % YIELD_INTERVAL == 0 {
                tokio::task::yield_now().await;
            }

            let file_str = file_path.display().to_string();
            let mut file_matches: Vec<String> = Vec::new();

            let _ = searcher.search_path(
                matcher,
                file_path,
                UTF8(|line_num, line| {
                    let trimmed = line.trim_end();
                    // Truncate long lines (protection against minified files)
                    let display_line = if trimmed.len() > MAX_LINE_LENGTH {
                        let truncated = truncate_safe(trimmed, MAX_LINE_LENGTH);
                        let remaining_chars = trimmed[truncated.len()..].chars().count();
                        format!("{truncated}...[+{remaining_chars} chars]")
                    } else {
                        trimmed.to_string()
                    };
                    // Format: "line_num:content"
                    file_matches.push(format!("{line_num}:{display_line}"));
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

        // Build output with file-grouped matches using configured limits
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

    /// Search and return only file paths with matches, sorted by match count
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

        // Collect file paths with their match counts
        let mut file_counts: Vec<(String, usize)> = Vec::new();
        let mut total_matches: usize = 0;
        let mut file_count: u32 = 0;

        for file_path in files {
            // Yield periodically to allow cancellation
            file_count += 1;
            if file_count % YIELD_INTERVAL == 0 {
                tokio::task::yield_now().await;
            }

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
        let truncated = total_files > max_files;

        // Limit output and format as objects with count
        let results: Vec<serde_json::Value> = file_counts
            .into_iter()
            .take(max_files)
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
    async fn search_count(&self, matcher: &grep_regex::RegexMatcher, files: &[PathBuf]) -> serde_json::Value {
        let mut searcher = SearcherBuilder::new()
            .binary_detection(BinaryDetection::quit(0x00))
            .line_number(true)
            .build();

        let max_files = self.max_files();

        let mut file_counts: Vec<(String, usize)> = Vec::new();
        let mut total_count = 0usize;
        let mut file_count: u32 = 0;

        for file_path in files {
            // Yield periodically to allow cancellation
            file_count += 1;
            if file_count % YIELD_INTERVAL == 0 {
                tokio::task::yield_now().await;
            }

            let mut count = 0usize;
            let _ = searcher.search_path(
                matcher,
                file_path,
                UTF8(|_line_num, _line| {
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

        // Sort by count descending
        file_counts.sort_by(|a, b| b.1.cmp(&a.1));

        let total_files = file_counts.len();
        let truncated = total_files > max_files;

        let results: Vec<serde_json::Value> = file_counts
            .into_iter()
            .take(max_files)
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
            if !path.is_empty() {
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

    pub fn eval_perm(&self, os: &Os, agent: &Agent) -> PermissionEvalResult {
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Settings {
            #[serde(default)]
            allowed_paths: Vec<String>,
            #[serde(default)]
            denied_paths: Vec<String>,
            #[serde(default)]
            allow_read_only: bool,
        }

        // Check if tool is in agent's allowlist
        let is_in_allowlist = Self::INFO
            .aliases
            .iter()
            .any(|alias| is_tool_in_allowlist(&agent.allowed_tools, alias, None));

        // Get settings, default to empty object if not configured
        let settings = Self::INFO
            .aliases
            .iter()
            .find_map(|alias| agent.tools_settings.get(*alias))
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));

        let Settings {
            mut allowed_paths,
            denied_paths,
            allow_read_only,
        } = match serde_json::from_value::<Settings>(settings) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to deserialize grep settings: {:?}", e);
                return PermissionEvalResult::Ask;
            },
        };

        // Get and canonicalize the search path
        let search_path = match &self.path {
            Some(p) if !p.is_empty() => p.clone(),
            _ => match os.env.current_dir() {
                Ok(cwd) => cwd.to_string_lossy().to_string(),
                Err(_) => return PermissionEvalResult::Ask,
            },
        };

        let canonical_search_path = match paths::canonicalizes_path(os, &search_path) {
            Ok(p) => p,
            Err(_) => return PermissionEvalResult::Ask,
        };

        // Build deny set
        let deny_set = {
            let mut builder = GlobSetBuilder::new();
            for path in &denied_paths {
                let Ok(canonical_path) = paths::canonicalizes_path(os, path) else {
                    continue;
                };
                if let Err(e) = paths::add_gitignore_globs(&mut builder, &canonical_path) {
                    warn!("Failed to create glob from denied path: {path}: {e}");
                }
            }
            builder.build()
        };

        // 1. Deny check first
        if let Ok(deny_set) = deny_set {
            if deny_set.is_match(&canonical_search_path) {
                return PermissionEvalResult::Deny(vec![format!("Path '{}' is denied", search_path)]);
            }
        }

        // 2. If tool is in allowlist or allow_read_only is true, allow
        if is_in_allowlist || allow_read_only {
            return PermissionEvalResult::Allow;
        }

        // 3. Check allowed_paths + CWD
        if let Ok(cwd) = os.env.current_dir() {
            allowed_paths.push(cwd.to_string_lossy().to_string());
        }

        let allow_set = {
            let mut builder = GlobSetBuilder::new();
            for path in &allowed_paths {
                let Ok(canonical_path) = paths::canonicalizes_path(os, path) else {
                    continue;
                };
                if let Err(e) = paths::add_gitignore_globs(&mut builder, &canonical_path) {
                    warn!("Failed to create glob from allowed path: {path}: {e}");
                }
            }
            builder.build()
        };

        match allow_set {
            Ok(allow_set) if allow_set.is_match(&canonical_search_path) => PermissionEvalResult::Allow,
            _ => PermissionEvalResult::Ask,
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
            max_matches_per_file: None,
            max_files: None,
            max_total_lines: None,
            max_depth: None,
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
            max_matches_per_file: None,
            max_files: None,
            max_total_lines: None,
            max_depth: None,
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
            max_matches_per_file: None,
            max_files: None,
            max_total_lines: None,
            max_depth: None,
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
            max_matches_per_file: None,
            max_files: None,
            max_total_lines: None,
            max_depth: None,
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
            max_matches_per_file: None,
            max_files: None,
            max_total_lines: None,
            max_depth: None,
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
            max_matches_per_file: None,
            max_files: None,
            max_total_lines: None,
            max_depth: None,
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
    async fn test_eval_perm() {
        use std::collections::HashMap;
        use std::path::PathBuf;

        use crate::cli::agent::ToolSettingTarget;

        let os = Os::new().await.unwrap();
        os.env.set_current_dir_for_test(PathBuf::from("/home/user/project"));

        // Case 1: path outside CWD, default agent -> Ask
        let tool_outside_cwd = Grep {
            pattern: "test".to_string(),
            path: Some("/tmp/other/path".to_string()),
            include: None,
            case_sensitive: None,
            output_mode: None,
            max_matches_per_file: None,
            max_files: None,
            max_total_lines: None,
            max_depth: None,
        };

        let default_agent = Agent::default();
        let result = tool_outside_cwd.eval_perm(&os, &default_agent);
        assert!(
            matches!(result, PermissionEvalResult::Ask),
            "Expected Ask for path outside CWD, got {result:?}",
        );

        // Case 2: path inside CWD, default agent -> Allow
        let tool_inside_cwd = Grep {
            pattern: "test".to_string(),
            path: Some("/home/user/project/src".to_string()),
            include: None,
            case_sensitive: None,
            output_mode: None,
            max_matches_per_file: None,
            max_files: None,
            max_total_lines: None,
            max_depth: None,
        };

        let result = tool_inside_cwd.eval_perm(&os, &default_agent);
        assert!(
            matches!(result, PermissionEvalResult::Allow),
            "Expected Allow for path inside CWD, got {result:?}",
        );

        // Case 3: path is None (defaults to CWD) -> Allow
        let tool_no_path = Grep {
            pattern: "test".to_string(),
            path: None,
            include: None,
            case_sensitive: None,
            output_mode: None,
            max_matches_per_file: None,
            max_files: None,
            max_total_lines: None,
            max_depth: None,
        };

        let result = tool_no_path.eval_perm(&os, &default_agent);
        assert!(
            matches!(result, PermissionEvalResult::Allow),
            "Expected Allow when path is CWD, got {result:?}",
        );

        // Case 4: allowReadOnly=true -> Allow anywhere
        let tool_outside_cwd = Grep {
            pattern: "test".to_string(),
            path: Some("/tmp/other/path".to_string()),
            include: None,
            case_sensitive: None,
            output_mode: None,
            max_matches_per_file: None,
            max_files: None,
            max_total_lines: None,
            max_depth: None,
        };

        let agent_allow_read = Agent {
            name: "test".to_string(),
            tools_settings: {
                let mut settings = HashMap::new();
                settings.insert(
                    ToolSettingTarget("grep".to_string()),
                    serde_json::json!({ "allowReadOnly": true }),
                );
                settings
            },
            ..Default::default()
        };

        let result = tool_outside_cwd.eval_perm(&os, &agent_allow_read);
        assert!(
            matches!(result, PermissionEvalResult::Allow),
            "Expected Allow with allowReadOnly=true, got {result:?}",
        );

        // Case 5: denied path -> Deny
        let tool_denied = Grep {
            pattern: "test".to_string(),
            path: Some("/secret/path".to_string()),
            include: None,
            case_sensitive: None,
            output_mode: None,
            max_matches_per_file: None,
            max_files: None,
            max_total_lines: None,
            max_depth: None,
        };

        let agent_with_deny = Agent {
            name: "test".to_string(),
            tools_settings: {
                let mut settings = HashMap::new();
                settings.insert(
                    ToolSettingTarget("grep".to_string()),
                    serde_json::json!({ "deniedPaths": ["/secret"] }),
                );
                settings
            },
            ..Default::default()
        };

        let result = tool_denied.eval_perm(&os, &agent_with_deny);
        assert!(
            matches!(result, PermissionEvalResult::Deny(_)),
            "Expected Deny for denied path, got {result:?}",
        );
    }

    #[test]
    fn test_include_glob_matching() {
        assert!(Grep::match_include("*.rs", "main.rs"));
        assert!(Grep::match_include("*.{ts,tsx}", "component.ts"));
        assert!(Grep::match_include("*.{ts,tsx}", "component.tsx"));
        assert!(!Grep::match_include("*.{ts,tsx}", "component.js"));
    }
    #[tokio::test]
    async fn test_long_line_truncation() {
        let temp_dir = TempDir::new().unwrap();

        let mut file = File::create(temp_dir.path().join("bundle.min.js")).unwrap();
        let long_line = "x".repeat(10_000); // 10k chars, simulating minified content
        writeln!(file, "{}", long_line).unwrap();
        writeln!(file, "normal short line").unwrap();

        let tool = Grep {
            pattern: "x".to_string(),
            path: Some(temp_dir.path().to_string_lossy().to_string()),
            include: None,
            case_sensitive: None,
            output_mode: Some(OutputMode::Content),
            max_matches_per_file: None,
            max_files: None,
            max_total_lines: None,
            max_depth: None,
        };

        let os = Os::new().await.unwrap();
        let mut buf = Vec::new();
        let result = tool.invoke(&os, &mut buf).await.unwrap();

        if let OutputKind::Json(json) = result.output {
            let results = json["results"].as_array().unwrap();
            assert!(!results.is_empty());

            let matches = results[0]["matches"].as_array().unwrap();
            let first_match = matches[0].as_str().unwrap();

            // Line should be truncated: "1:xxx...[+9500 chars]"
            assert!(
                first_match.len() < 1000,
                "Long line should be truncated, got {} chars",
                first_match.len()
            );
            assert!(
                first_match.contains("...[+"),
                "Truncated line should indicate remaining chars, got: {}",
                &first_match[..100.min(first_match.len())]
            );
        } else {
            panic!("Expected JSON output");
        }
    }

    #[tokio::test]
    async fn test_normal_lines_not_truncated() {
        let temp_dir = TempDir::new().unwrap();

        // Create a file with normal length lines
        let mut file = File::create(temp_dir.path().join("normal.rs")).unwrap();
        writeln!(file, "fn main() {{ println!(\"hello world\"); }}").unwrap();
        writeln!(file, "fn other() {{ println!(\"test\"); }}").unwrap();

        let tool = Grep {
            pattern: "fn".to_string(),
            path: Some(temp_dir.path().to_string_lossy().to_string()),
            include: None,
            case_sensitive: None,
            output_mode: Some(OutputMode::Content),
            max_matches_per_file: None,
            max_files: None,
            max_total_lines: None,
            max_depth: None,
        };

        let os = Os::new().await.unwrap();
        let mut buf = Vec::new();
        let result = tool.invoke(&os, &mut buf).await.unwrap();

        if let OutputKind::Json(json) = result.output {
            let results = json["results"].as_array().unwrap();
            let matches = results[0]["matches"].as_array().unwrap();

            // Normal lines should NOT be truncated
            for m in matches {
                let line = m.as_str().unwrap();
                assert!(!line.contains("...[+"), "Normal line should not be truncated: {line}");
            }
        } else {
            panic!("Expected JSON output");
        }
    }
}
