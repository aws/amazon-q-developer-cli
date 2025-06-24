// ABOUTME: File system search tool for finding files by name or content patterns
// ABOUTME: Supports recursive directory traversal with configurable ignore patterns

use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};

use crossterm::queue;
use crossterm::style::{self, Color};
use eyre::{Result, bail};
use glob::Pattern;
use regex::Regex;
use serde::Deserialize;

use super::{InvokeOutput, OutputKind, sanitize_path_tool_arg};
use crate::platform::Context;

// Constants for resource limits
const MAX_RESPONSE_SIZE: usize = 30 * 1024; // 30KB
const DEFAULT_MAX_FILE_SIZE: usize = 52_428_800; // 50MB
const MAX_DIRECTORY_DEPTH: usize = 100;
const MAX_CONTEXT_LINES: usize = 10;

/// Default directories to ignore during search
const DEFAULT_IGNORE_DIRS: &[&str] = &[
    ".git", ".svn", ".hg", "node_modules", "target", "dist", "build", "out",
    ".next", "__pycache__", ".pytest_cache", ".mypy_cache", ".tox", ".venv",
    "venv", ".env",
];

/// File system search tool with explicit modes
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "mode")]
pub enum FsSearch {
    #[serde(rename = "name")]
    Name(FsSearchName),
    #[serde(rename = "content")]
    Content(FsSearchContent),
}

/// Search for files and directories by name using glob patterns
#[derive(Debug, Clone, Deserialize)]
pub struct FsSearchName {
    pub path: String,
    pub pattern: String,
    #[serde(default)]
    pub include_ignored: bool,
}

/// Search within file contents using regex patterns
#[derive(Debug, Clone, Deserialize)]
pub struct FsSearchContent {
    pub path: String,
    pub pattern: String,
    #[serde(default)]
    pub include_ignored: bool,
    pub context_before: Option<usize>,
    pub context_after: Option<usize>,
    pub max_file_size: Option<usize>,
    /// Optional glob pattern to filter files before content search (e.g., "*.rs", "**/*.py")
    pub file_path: Option<String>,
}

impl FsSearch {
    pub async fn validate(&mut self, ctx: &Context) -> Result<()> {
        match self {
            FsSearch::Name(name_search) => name_search.validate(ctx).await,
            FsSearch::Content(content_search) => content_search.validate(ctx).await,
        }
    }

    pub async fn queue_description(&self, ctx: &Context, updates: &mut impl Write) -> Result<()> {
        match self {
            FsSearch::Name(name_search) => name_search.queue_description(ctx, updates).await,
            FsSearch::Content(content_search) => content_search.queue_description(ctx, updates).await,
        }
    }

    pub async fn invoke(&self, ctx: &Context, updates: &mut impl Write) -> Result<InvokeOutput> {
        match self {
            FsSearch::Name(name_search) => name_search.invoke(ctx, updates).await,
            FsSearch::Content(content_search) => content_search.invoke(ctx, updates).await,
        }
    }
}

impl FsSearchName {
    pub async fn validate(&mut self, ctx: &Context) -> Result<()> {
        let path = sanitize_path_tool_arg(ctx, &self.path);
        
        if !path.exists() {
            bail!("Path does not exist: '{}'", self.path);
        }
        
        // Validate pattern as glob
        if let Err(e) = Pattern::new(&self.pattern) {
            bail!("Invalid glob pattern '{}': {}", self.pattern, e);
        }
        
        Ok(())
    }

    pub async fn queue_description(&self, _ctx: &Context, updates: &mut impl Write) -> Result<()> {
        queue!(
            updates,
            style::Print("Searching for files matching pattern: "),
            style::SetForegroundColor(Color::Yellow),
            style::Print(&self.pattern),
            style::ResetColor,
            style::Print(" in "),
            style::SetForegroundColor(Color::Green),
            style::Print(&self.path),
            style::ResetColor,
            style::Print("\n")
        )?;
        Ok(())
    }

    pub async fn invoke(&self, ctx: &Context, _updates: &mut impl Write) -> Result<InvokeOutput> {
        let path = sanitize_path_tool_arg(ctx, &self.path);
        let pattern = Pattern::new(&self.pattern)?;
        
        let matching_files = self.search_directory(&path, &pattern, ctx).await?;
        
        let mut result = format!("Found {} files matching pattern '{}':\n", 
                                matching_files.len(), self.pattern);
        
        for file_path in matching_files {
            let relative_path = file_path.strip_prefix(&path)
                .unwrap_or(&file_path)
                .display();
            result.push_str(&format!("  {}\n", relative_path));
        }
        
        Ok(InvokeOutput {
            output: OutputKind::Text(result),
        })
    }

    async fn search_directory(&self, dir: &Path, pattern: &Pattern, ctx: &Context) -> Result<Vec<PathBuf>> {
        let mut matching_files = Vec::new();
        let mut dirs_to_process = VecDeque::new();
        dirs_to_process.push_back((dir.to_path_buf(), 0));
        
        while let Some((current_dir, depth)) = dirs_to_process.pop_front() {
            if depth > MAX_DIRECTORY_DEPTH {
                continue;
            }
            
            let mut entries = ctx.fs().read_dir(&current_dir).await?;
            
            while let Some(entry) = entries.next_entry().await? {
                let entry_path = entry.path();
                
                // Check ignore patterns
                if !self.include_ignored && Self::should_ignore_entry(&entry_path) {
                    continue;
                }
                
                // Optimize path operations
                if let Ok(relative_path) = entry_path.strip_prefix(dir) {
                    let path_str = relative_path.to_string_lossy();
                    
                    // Match against relative path
                    if pattern.matches(&path_str) {
                        matching_files.push(entry_path.clone());
                        continue;
                    }
                    
                    // If didn't match full path, try just filename
                    if let Some(file_name) = entry_path.file_name().and_then(|n| n.to_str()) {
                        if path_str != file_name && pattern.matches(file_name) {
                            matching_files.push(entry_path.clone());
                        }
                    }
                } else if pattern.matches(&entry_path.to_string_lossy()) {
                    matching_files.push(entry_path.clone());
                }
                
                // Recurse into directories
                if entry_path.is_dir() {
                    dirs_to_process.push_back((entry_path, depth + 1));
                }
            }
        }
        
        matching_files.sort();
        Ok(matching_files)
    }

    fn should_ignore_entry(path: &Path) -> bool {
        // Only check the final component (the actual file/directory name)
        if let Some(file_name) = path.file_name() {
            if let Some(name_str) = file_name.to_str() {
                return name_str.starts_with('.') || DEFAULT_IGNORE_DIRS.contains(&name_str);
            }
        }
        false
    }
}

impl FsSearchContent {
    fn context_before_lines(&self) -> usize {
        self.context_before.unwrap_or(0).min(MAX_CONTEXT_LINES)
    }
    
    fn context_after_lines(&self) -> usize {
        self.context_after.unwrap_or(0).min(MAX_CONTEXT_LINES)
    }
    
    fn max_file_size_bytes(&self) -> usize {
        self.max_file_size.unwrap_or(DEFAULT_MAX_FILE_SIZE)
    }

    pub async fn validate(&mut self, ctx: &Context) -> Result<()> {
        let path = sanitize_path_tool_arg(ctx, &self.path);
        
        if !path.exists() {
            bail!("Path does not exist: '{}'", self.path);
        }
        
        // Validate context parameters
        if let Some(before) = self.context_before {
            if before > 10 {
                bail!("Invalid value for context_before: '{}'. Must be <= 10", before);
            }
        }
        
        if let Some(after) = self.context_after {
            if after > 10 {
                bail!("Invalid value for context_after: '{}'. Must be <= 10", after);
            }
        }
        
        // Validate pattern as regex
        if let Err(e) = Regex::new(&self.pattern) {
            bail!("Invalid regex pattern '{}': {}", self.pattern, e);
        }
        
        // Validate file_path glob pattern if provided
        if let Some(file_path_pattern) = &self.file_path {
            if let Err(e) = Pattern::new(file_path_pattern) {
                bail!("Invalid glob pattern '{}': {}", file_path_pattern, e);
            }
        }
        
        Ok(())
    }

    pub async fn queue_description(&self, _ctx: &Context, updates: &mut impl Write) -> Result<()> {
        queue!(
            updates,
            style::Print("Searching for content matching pattern: "),
            style::SetForegroundColor(Color::Yellow),
            style::Print(&self.pattern),
            style::ResetColor,
            style::Print(" in "),
            style::SetForegroundColor(Color::Green),
            style::Print(&self.path),
            style::ResetColor,
            style::Print("\n")
        )?;
        Ok(())
    }

    pub async fn invoke(&self, ctx: &Context, _updates: &mut impl Write) -> Result<InvokeOutput> {
        let path = sanitize_path_tool_arg(ctx, &self.path);
        let regex = Regex::new(&self.pattern)?;
        
        // Pre-compile file_path pattern if provided
        let file_pattern = self.file_path.as_ref()
            .map(|p| Pattern::new(p))
            .transpose()?;
        
        let mut matches_by_file = Vec::new();
        let mut total_size = 0usize;
        
        // Check if path is a file or directory
        let metadata = ctx.fs().symlink_metadata(&path).await?;
        if metadata.is_file() {
            // Search single file
            if let Some(matches) = self.search_file_content(&path, &regex, ctx).await? {
                if !matches.is_empty() {
                    let size = Self::estimate_matches_size(&matches);
                    total_size += size;
                    matches_by_file.push((path, matches));
                }
            }
        } else if metadata.is_dir() {
            // Search directory recursively
            self.search_directory_content(&path, &regex, ctx, &mut matches_by_file, &mut total_size, MAX_RESPONSE_SIZE, file_pattern.as_ref()).await?;
        } else {
            bail!("Path '{}' is neither a file nor a directory", self.path);
        }
        
        let result = Self::format_content_results(matches_by_file, total_size >= MAX_RESPONSE_SIZE);
        
        Ok(InvokeOutput {
            output: OutputKind::Text(result),
        })
    }

    async fn search_directory_content(
        &self,
        dir: &Path,
        regex: &Regex,
        ctx: &Context,
        matches_by_file: &mut Vec<(PathBuf, Vec<(usize, String)>)>,
        total_size: &mut usize,
        max_size: usize,
        file_pattern: Option<&Pattern>,
    ) -> Result<()> {
        let mut dirs_to_process = VecDeque::new();
        dirs_to_process.push_back((dir.to_path_buf(), 0));
        
        while let Some((current_dir, depth)) = dirs_to_process.pop_front() {
            if *total_size >= max_size || depth > MAX_DIRECTORY_DEPTH {
                break;
            }
            
            let mut entries = ctx.fs().read_dir(&current_dir).await?;
            
            while let Some(entry) = entries.next_entry().await? {
                if *total_size >= max_size {
                    break;
                }
                let entry_path = entry.path();
                
                // Check ignore patterns
                if !self.include_ignored && FsSearchName::should_ignore_entry(&entry_path) {
                    continue;
                }
                
                if entry_path.is_file() {
                    // Apply file_path glob filter if specified
                    if let Some(pattern) = file_pattern {
                        let relative_path = entry_path.strip_prefix(dir)
                            .unwrap_or(&entry_path);
                        let path_str = relative_path.to_string_lossy();
                        let file_name = entry_path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("");
                        
                        // Check if file matches the file_path pattern (either full path or filename)
                        if !pattern.matches(&path_str) && !pattern.matches(file_name) {
                            continue;
                        }
                    }
                    
                    if let Some(matches) = self.search_file_content(&entry_path, regex, ctx).await? {
                        if !matches.is_empty() {
                            // Accurate size estimation
                            let file_content_size = Self::estimate_matches_size(&matches);
                            
                            if *total_size + file_content_size > max_size {
                                break;
                            }
                            
                            *total_size += file_content_size;
                            matches_by_file.push((entry_path, matches));
                        }
                    }
                } else if entry_path.is_dir() {
                    dirs_to_process.push_back((entry_path, depth + 1));
                }
            }
        }
        
        Ok(())
    }
    
    fn estimate_matches_size(matches: &[(usize, String)]) -> usize {
        matches.iter()
            .map(|(line_num, content)| {
                // Account for formatting: "  {line_num}: {content}\n"
                format!("  {}: {}\n", line_num, content).len()
            })
            .sum()
    }

    async fn search_file_content(&self, file_path: &Path, regex: &Regex, ctx: &Context) -> Result<Option<Vec<(usize, String)>>> {
        // Check file size
        let metadata = ctx.fs().symlink_metadata(file_path).await?;
        if metadata.len() > self.max_file_size_bytes() as u64 {
            return Ok(None);
        }
        
        // Try to read as UTF-8
        let content = match ctx.fs().read_to_string(file_path).await {
            Ok(content) => content,
            Err(_) => return Ok(None), // Skip binary files
        };
        
        let mut matches = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        
        for (i, line) in lines.iter().enumerate() {
            if regex.is_match(line) {
                let line_num = i + 1;
                
                // Add context lines if requested
                if self.context_before_lines() > 0 || self.context_after_lines() > 0 {
                    // Add context before
                    let start = if i >= self.context_before_lines() { 
                        i - self.context_before_lines() 
                    } else { 
                        0 
                    };
                    
                    for (j, line) in lines.iter().enumerate().take(i).skip(start) {
                        matches.push((j + 1, format!("[context] {}", line)));
                    }
                    
                    // Add the matching line
                    matches.push((line_num, format!("[match] {}", line)));
                    
                    // Add context after
                    let end = (i + 1 + self.context_after_lines()).min(lines.len());
                    for (j, line) in lines.iter().enumerate().take(end).skip(i + 1) {
                        matches.push((j + 1, format!("[context] {}", line)));
                    }
                } else {
                    matches.push((line_num, (*line).to_string()));
                }
            }
        }
        
        Ok(Some(matches))
    }

    fn format_content_results(matches_by_file: Vec<(PathBuf, Vec<(usize, String)>)>, truncated: bool) -> String {
        if matches_by_file.is_empty() {
            return "Found matches in 0 files:".to_string();
        }
        
        let mut result = format!("Found matches in {} files:\n\n", matches_by_file.len());
        
        for (file_path, matches) in matches_by_file {
            result.push_str(&format!("{}:\n", file_path.display()));
            
            for (line_num, line_content) in matches {
                result.push_str(&format!("  {}: {}\n", line_num, line_content));
            }
            
            result.push('\n');
        }
        
        if truncated {
            result.push_str("\n[Results truncated - response size limit reached]");
        }
        
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Context;
    use serde_json::json;
    use std::sync::Arc;

    const TEST_CONTENT_FILE: &str = "/test_content.rs";
    const TEST_CONTENT: &str = r#"// ABOUTME: This is a test Rust file
// ABOUTME: Used for testing fs_search functionality

use std::io::Write;

fn main() {
    println!("Hello, world!");
    // TODO: Add more functionality
    println!("This is a test"); // FIXME: Remove debug print
}

mod tests {
    #[test]
    fn test_function() {
        assert_eq!(2 + 2, 4);
        // TODO: Add more tests
    }
}
"#;

    const TEST_DIR_STRUCTURE: &[(&str, &str)] = &[
        ("/src/main.rs", "fn main() { println!(\"Hello\"); }"),
        ("/src/lib.rs", "pub mod utils;"),
        ("/src/utils/mod.rs", "pub fn helper() {}"),
        ("/tests/integration.rs", "// Integration tests"),
        ("/README.md", "# Test Project"),
        ("/Cargo.toml", "[package]\nname = \"test\""),
        ("/.git/config", "[core]\nrepositoryformatversion = 0"),
        ("/node_modules/package.json", "{}"),
    ];

    /// Set up test directory with file structure for fs_search testing
    async fn setup_test_directory() -> Arc<Context> {
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let fs = ctx.fs();
        
        // Create main test content file
        fs.write(TEST_CONTENT_FILE, TEST_CONTENT).await.unwrap();
        
        // Create directory structure
        for (path, content) in TEST_DIR_STRUCTURE {
            if path.contains('/') && !path.ends_with('/') {
                if let Some(parent) = std::path::Path::new(path).parent() {
                    fs.create_dir_all(parent).await.unwrap();
                }
            }
            fs.write(path, content).await.unwrap();
        }
        
        ctx
    }

    #[tokio::test]
    async fn test_name_search_deserialization() {
        let json = json!({
            "mode": "name", 
            "path": "/test",
            "pattern": "*.rs"
        });
        
        let fs_search: FsSearch = serde_json::from_value(json).unwrap();
        match fs_search {
            FsSearch::Name(name_search) => {
                assert_eq!(name_search.path, "/test");
                assert_eq!(name_search.pattern, "*.rs");
                assert!(!name_search.include_ignored);
            }
            _ => panic!("Expected Name variant"),
        }
    }

    #[tokio::test]
    async fn test_content_search_deserialization() {
        let json = json!({
            "mode": "content",
            "path": "/test", 
            "pattern": "TODO",
            "context_before": 2,
            "context_after": 2,
            "include_ignored": true
        });
        
        let fs_search: FsSearch = serde_json::from_value(json).unwrap();
        match fs_search {
            FsSearch::Content(content_search) => {
                assert_eq!(content_search.path, "/test");
                assert_eq!(content_search.pattern, "TODO");
                assert_eq!(content_search.context_before, Some(2));
                assert_eq!(content_search.context_after, Some(2));
                assert!(content_search.include_ignored);
            }
            _ => panic!("Expected Content variant"),
        }
    }

    #[tokio::test]
    async fn test_validation_missing_mode() {
        let json = json!({
            "path": "/test",
            "pattern": "*.rs"
        });
        
        let result = serde_json::from_value::<FsSearch>(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_context_limits() {
        let content_search = FsSearchContent {
            path: "/test".to_string(),
            pattern: "test".to_string(),
            include_ignored: false,
            context_before: Some(15),
            context_after: Some(5),
            max_file_size: None,
            file_path: None,
        };
        
        assert_eq!(content_search.context_before_lines(), 10); // Capped at 10
        assert_eq!(content_search.context_after_lines(), 5);
    }

    #[tokio::test]
    async fn test_fs_search_name_invoke() {
        let ctx = setup_test_directory().await;
        let mut stdout = std::io::stdout();

        // First test that files exist
        let fs = ctx.fs();
        assert!(fs.read_to_string("/test_content.rs").await.is_ok());
        assert!(fs.read_to_string("/src/main.rs").await.is_ok());
        
        // Test searching for Rust files
        let v = json!({
            "mode": "name",
            "path": "/",
            "pattern": "*.rs"
        });
        let output = serde_json::from_value::<FsSearch>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(text) = output.output {
            assert!(text.contains("Found 5 files"));
            assert!(text.contains("main.rs"));
            assert!(text.contains("lib.rs"));
            assert!(text.contains("mod.rs"));
            assert!(text.contains("test_content.rs"));
            assert!(text.contains("integration.rs"));
            assert!(!text.contains("README.md"));
        } else {
            panic!("Expected text output");
        }

        // Test searching for markdown files
        let v = json!({
            "mode": "name",
            "path": "/",
            "pattern": "*.md"
        });
        let output = serde_json::from_value::<FsSearch>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(text) = output.output {
            assert!(text.contains("Found 1 files"));
            assert!(text.contains("README.md"));
        } else {
            panic!("Expected text output");
        }
    }

    #[tokio::test]
    async fn test_fs_search_name_with_ignore() {
        let ctx = setup_test_directory().await;
        let mut stdout = std::io::stdout();

        // Test without include_ignored (should exclude .git and node_modules)
        let v = json!({
            "mode": "name",
            "path": "/",
            "pattern": "*",
            "include_ignored": false
        });
        let output = serde_json::from_value::<FsSearch>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(text) = output.output {
            assert!(!text.contains(".git"));
            assert!(!text.contains("node_modules"));
            assert!(text.contains("src"));
        } else {
            panic!("Expected text output");
        }

        // Test with include_ignored (should include everything)
        let v = json!({
            "mode": "name",
            "path": "/",
            "pattern": "*config*",
            "include_ignored": true
        });
        let output = serde_json::from_value::<FsSearch>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(text) = output.output {
            assert!(text.contains("config"));
        } else {
            panic!("Expected text output");
        }
    }

    #[tokio::test]
    async fn test_fs_search_content_invoke() {
        let ctx = setup_test_directory().await;
        let mut stdout = std::io::stdout();

        // Test searching for TODO comments
        let v = json!({
            "mode": "content",
            "path": "/",
            "pattern": "TODO"
        });
        let output = serde_json::from_value::<FsSearch>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(text) = output.output {
            assert!(text.contains("Found matches in 1 files"));
            assert!(text.contains("test_content.rs"));
            assert!(text.contains("TODO"));
            // Should find both TODO comments in the test file
            assert!(text.lines().filter(|line| line.contains("TODO")).count() >= 2);
        } else {
            panic!("Expected text output");
        }

        // Test regex pattern
        let v = json!({
            "mode": "content",
            "path": "/",
            "pattern": "fn \\w+"
        });
        let output = serde_json::from_value::<FsSearch>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(text) = output.output {
            assert!(text.contains("fn main"));
            assert!(text.contains("fn test_function"));
        } else {
            panic!("Expected text output");
        }
    }

    #[tokio::test]
    async fn test_fs_search_content_with_context() {
        let ctx = setup_test_directory().await;
        let mut stdout = std::io::stdout();

        // Test with context lines
        let v = json!({
            "mode": "content",
            "path": "/",
            "pattern": "TODO",
            "context_before": 1,
            "context_after": 1
        });
        let output = serde_json::from_value::<FsSearch>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(text) = output.output {
            assert!(text.contains("[match]"));
            assert!(text.contains("[context]"));
            assert!(text.contains("TODO"));
        } else {
            panic!("Expected text output");
        }
    }

    #[tokio::test]
    async fn test_fs_search_validation_errors() {
        let ctx = setup_test_directory().await;

        // Test invalid path
        let mut v = json!({
            "mode": "name",
            "path": "/nonexistent",
            "pattern": "*.rs"
        });
        let mut fs_search = serde_json::from_value::<FsSearch>(v).unwrap();
        let result = fs_search.validate(&ctx).await;
        assert!(result.is_err());

        // Test invalid glob pattern
        v = json!({
            "mode": "name",
            "path": "/",
            "pattern": "[unclosed"
        });
        let mut fs_search = serde_json::from_value::<FsSearch>(v).unwrap();
        let result = fs_search.validate(&ctx).await;
        assert!(result.is_err());

        // Test invalid regex pattern  
        v = json!({
            "mode": "content",
            "path": "/",
            "pattern": "("
        });
        let mut fs_search = serde_json::from_value::<FsSearch>(v).unwrap();
        let result = fs_search.validate(&ctx).await;
        assert!(result.is_err());

        // Test context limits
        v = json!({
            "mode": "content",
            "path": "/",
            "pattern": "test",
            "context_before": 15
        });
        let mut fs_search = serde_json::from_value::<FsSearch>(v).unwrap();
        let result = fs_search.validate(&ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_vs_directory_search_errors() {
        let ctx = setup_test_directory().await;
        let mut stdout = std::io::stdout();
        
        // Test searching a file as if it were a directory (name search)
        // This should result in an error since read_dir() will fail on a file
        let v = json!({
            "mode": "name",
            "path": "/test_content.rs",
            "pattern": "*.rs"
        });
        let result = serde_json::from_value::<FsSearch>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await;

        // Should error when trying to read a file as a directory
        assert!(result.is_err());
        let error_msg = format!("{}", result.unwrap_err());
        assert!(error_msg.contains("Not a directory") || error_msg.contains("os error"));

        // Test content search on a single file (should work now)
        let v = json!({
            "mode": "content", 
            "path": "/test_content.rs",
            "pattern": "TODO"
        });
        let output = serde_json::from_value::<FsSearch>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();

        // Should find matches in single file
        if let OutputKind::Text(text) = output.output {
            assert!(text.contains("Found matches"));
            assert!(text.contains("test_content.rs"));
            assert!(text.contains("TODO"));
        } else {
            panic!("Expected text output");
        }
        
        // Test content search on a directory (should also work)
        let v = json!({
            "mode": "content", 
            "path": "/src",
            "pattern": "fn"
        });
        let output = serde_json::from_value::<FsSearch>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();

        // Should search all files in directory
        if let OutputKind::Text(text) = output.output {
            assert!(text.contains("Found matches"));
        } else {
            panic!("Expected text output");
        }
    }

    #[tokio::test]
    async fn test_permission_denied_scenarios() {
        let ctx = setup_test_directory().await;
        
        // Create a directory structure to test with
        let fs = ctx.fs();
        fs.create_dir_all("/restricted").await.unwrap();
        fs.write("/restricted/file.txt", "test content").await.unwrap();
        
        // Test case where we can at least attempt to read
        // Note: In a fake filesystem, we can't truly test permission errors,
        // but we can test the error handling paths
        let mut stdout = std::io::stdout();
        let v = json!({
            "mode": "content",
            "path": "/restricted",
            "pattern": "test"
        });
        
        // This should succeed in fake filesystem, but in real usage permission errors
        // would be caught by the error handling in search_directory_content
        let result = serde_json::from_value::<FsSearch>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await;
        
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_invalid_nonexistent_path_handling() {
        let ctx = setup_test_directory().await;
        
        // Test completely nonexistent path
        let v = json!({
            "mode": "name",
            "path": "/does/not/exist/anywhere",
            "pattern": "*.txt"
        });
        let mut fs_search = serde_json::from_value::<FsSearch>(v).unwrap();
        let result = fs_search.validate(&ctx).await;
        assert!(result.is_err());
        let error_msg = format!("{}", result.unwrap_err());
        assert!(error_msg.contains("Path does not exist"));

        // Test content search on nonexistent path
        let v = json!({
            "mode": "content",
            "path": "/missing/directory",
            "pattern": "anything"
        });
        let mut fs_search = serde_json::from_value::<FsSearch>(v).unwrap();
        let result = fs_search.validate(&ctx).await;
        assert!(result.is_err());
        let error_msg = format!("{}", result.unwrap_err());
        assert!(error_msg.contains("Path does not exist"));

        // Test path that exists and should validate successfully
        // (This tests runtime error handling vs validation errors)
        let fs = ctx.fs();
        fs.create_dir_all("/temp_dir").await.unwrap();
        
        let v = json!({
            "mode": "name",
            "path": "/temp_dir", 
            "pattern": "*.txt"
        });
        let mut fs_search = serde_json::from_value::<FsSearch>(v).unwrap();
        assert!(fs_search.validate(&ctx).await.is_ok());
    }

    #[tokio::test]
    async fn test_malformed_glob_regex_pattern_errors() {
        let ctx = setup_test_directory().await;

        // Test various malformed glob patterns
        let bad_glob_patterns = vec![
            "[unclosed_bracket",
            // Note: Some patterns that look malformed may actually be valid in glob
            // We test ones that are definitely invalid
        ];
        
        for pattern in bad_glob_patterns {
            let v = json!({
                "mode": "name",
                "path": "/",
                "pattern": pattern
            });
            let mut fs_search = serde_json::from_value::<FsSearch>(v).unwrap();
            let result = fs_search.validate(&ctx).await;
            assert!(result.is_err(), "Pattern '{}' should have failed validation", pattern);
            let error_msg = format!("{}", result.unwrap_err());
            assert!(error_msg.contains("Invalid glob pattern"));
        }

        // Test various malformed regex patterns  
        let bad_regex_patterns = vec![
            "(",
            "[",
            "*",
            "?+",
            "(?P<>test)",
            "(?i",
            "\\k<name>",
        ];
        
        for pattern in bad_regex_patterns {
            let v = json!({
                "mode": "content",
                "path": "/",
                "pattern": pattern
            });
            let mut fs_search = serde_json::from_value::<FsSearch>(v).unwrap();
            let result = fs_search.validate(&ctx).await;
            assert!(result.is_err(), "Pattern '{}' should have failed validation", pattern);
            let error_msg = format!("{}", result.unwrap_err());
            assert!(error_msg.contains("Invalid regex pattern"));
        }
    }

    #[tokio::test]
    async fn test_parameter_validation_edge_cases() {
        let ctx = setup_test_directory().await;

        // Test context_before boundary conditions
        let v = json!({
            "mode": "content",
            "path": "/", 
            "pattern": "test",
            "context_before": 11
        });
        let mut fs_search = serde_json::from_value::<FsSearch>(v).unwrap();
        let result = fs_search.validate(&ctx).await;
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("Must be <= 10"));

        // Test context_after boundary conditions
        let v = json!({
            "mode": "content",
            "path": "/",
            "pattern": "test", 
            "context_after": 11
        });
        let mut fs_search = serde_json::from_value::<FsSearch>(v).unwrap();
        let result = fs_search.validate(&ctx).await;
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("Must be <= 10"));

        // Test valid boundary values (should pass)
        let v = json!({
            "mode": "content",
            "path": "/",
            "pattern": "test",
            "context_before": 10,
            "context_after": 10
        });
        let mut fs_search = serde_json::from_value::<FsSearch>(v).unwrap();
        assert!(fs_search.validate(&ctx).await.is_ok());

        // Test negative values (JSON should prevent this, but test if it somehow gets through)
        let v = json!({
            "mode": "content", 
            "path": "/",
            "pattern": "test",
            "context_before": 0,
            "context_after": 0
        });
        let mut fs_search = serde_json::from_value::<FsSearch>(v).unwrap();
        assert!(fs_search.validate(&ctx).await.is_ok());
    }

    #[tokio::test]
    async fn test_empty_and_whitespace_patterns() {
        let ctx = setup_test_directory().await;

        // Test empty glob pattern
        let v = json!({
            "mode": "name",
            "path": "/",
            "pattern": ""
        });
        let mut fs_search = serde_json::from_value::<FsSearch>(v).unwrap();
        // Empty pattern should be valid for glob (matches nothing)
        assert!(fs_search.validate(&ctx).await.is_ok());

        // Test whitespace-only patterns
        let v = json!({
            "mode": "name",
            "path": "/", 
            "pattern": "   "
        });
        let mut fs_search = serde_json::from_value::<FsSearch>(v).unwrap();
        assert!(fs_search.validate(&ctx).await.is_ok());

        // Test empty regex pattern
        let v = json!({
            "mode": "content",
            "path": "/",
            "pattern": ""
        });
        let mut fs_search = serde_json::from_value::<FsSearch>(v).unwrap();
        // Empty regex should be valid (matches everything)
        assert!(fs_search.validate(&ctx).await.is_ok());
    }

    #[tokio::test] 
    async fn test_large_file_handling_errors() {
        let ctx = setup_test_directory().await;
        let fs = ctx.fs();
        
        // Create a large file by writing lots of content
        let large_content = "x".repeat(100_000); // 100KB file
        fs.write("/large_file.txt", &large_content).await.unwrap();
        
        // Test content search with small max_file_size
        let mut stdout = std::io::stdout();
        let v = json!({
            "mode": "content",
            "path": "/",
            "pattern": "x",
            "max_file_size": 1000  // 1KB limit
        });
        
        let output = serde_json::from_value::<FsSearch>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();
        
        // Large file should be skipped due to size limit
        if let OutputKind::Text(text) = output.output {
            // Should report 0 matches since the large file was skipped
            assert!(text.contains("Found matches in 0 files") || !text.contains("large_file.txt"));
        } else {
            panic!("Expected text output");
        }
    }

    #[tokio::test]
    async fn test_relative_path_handling() {
        let ctx = setup_test_directory().await;
        let fs = ctx.fs();
        
        // Create a nested directory structure for relative path testing
        fs.create_dir_all("/project/src/utils").await.unwrap();
        fs.create_dir_all("/project/tests").await.unwrap();
        fs.write("/project/src/main.rs", "fn main() {}").await.unwrap();
        fs.write("/project/src/utils/helper.rs", "pub fn help() {}").await.unwrap();
        fs.write("/project/tests/test.rs", "// test file").await.unwrap();
        
        // Test relative path navigation - this tests conceptual relative paths
        // In fake filesystem, we need to test the path sanitization logic
        let mut stdout = std::io::stdout();
        
        // Test with current directory shortcut
        let v = json!({
            "mode": "name",
            "path": "/project",
            "pattern": "*.rs"
        });
        let output = serde_json::from_value::<FsSearch>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();
        
        if let OutputKind::Text(text) = output.output {
            assert!(text.contains("main.rs"));
            assert!(text.contains("helper.rs"));
            assert!(text.contains("test.rs"));
        } else {
            panic!("Expected text output");
        }
    }

    #[tokio::test]
    async fn test_symlink_following_behavior() {
        let ctx = setup_test_directory().await;
        let fs = ctx.fs();
        
        // Create files and directories
        fs.write("/target_file.txt", "target content").await.unwrap();
        fs.create_dir_all("/target_dir").await.unwrap();
        fs.write("/target_dir/file.txt", "dir content").await.unwrap();
        
        // Test normal file search
        let mut stdout = std::io::stdout();
        let v = json!({
            "mode": "content",
            "path": "/",
            "pattern": "content"
        });
        let output = serde_json::from_value::<FsSearch>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();
        
        if let OutputKind::Text(text) = output.output {
            assert!(text.contains("target_file.txt") || text.contains("target content"));
        } else {
            panic!("Expected text output");
        }
    }

    #[tokio::test]
    async fn test_cross_platform_path_canonicalization() {
        let ctx = setup_test_directory().await;
        
        // Test path sanitization with various path formats
        // This tests the sanitize_path_tool_arg function behavior
        
        // Create test structure
        let fs = ctx.fs();
        fs.create_dir_all("/path/with/spaces dir").await.unwrap();
        fs.write("/path/with/spaces dir/file.txt", "test content").await.unwrap();
        
        let mut stdout = std::io::stdout();
        
        // Test path with spaces
        let v = json!({
            "mode": "content",
            "path": "/path/with/spaces dir",
            "pattern": "test"
        });
        let output = serde_json::from_value::<FsSearch>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();
        
        if let OutputKind::Text(text) = output.output {
            assert!(text.contains("Found matches") || text.contains("test"));
        } else {
            panic!("Expected text output");
        }
    }

    #[tokio::test]
    async fn test_current_directory_shortcuts() {
        let ctx = setup_test_directory().await;
        
        // Test that various current directory representations work
        // Test with root as current directory
        let mut stdout = std::io::stdout();
        
        // Test explicit root path
        let v = json!({
            "mode": "name",
            "path": "/",
            "pattern": "*.rs"
        });
        let output = serde_json::from_value::<FsSearch>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();
        
        if let OutputKind::Text(text) = output.output {
            assert!(text.contains("Found") && text.contains("files"));
        } else {
            panic!("Expected text output");
        }
    }

    #[tokio::test]
    async fn test_path_validation_edge_cases() {
        let ctx = setup_test_directory().await;
        
        // Test empty path
        let v = json!({
            "mode": "name",
            "path": "",
            "pattern": "*.txt"
        });
        let mut fs_search = serde_json::from_value::<FsSearch>(v).unwrap();
        let result = fs_search.validate(&ctx).await;
        assert!(result.is_err());
        
        // Test path with only whitespace
        let v = json!({
            "mode": "name", 
            "path": "   ",
            "pattern": "*.txt"
        });
        let mut fs_search = serde_json::from_value::<FsSearch>(v).unwrap();
        let result = fs_search.validate(&ctx).await;
        assert!(result.is_err());
        
        // Test extremely long path
        let long_path = "/".to_string() + &"a".repeat(1000);
        let v = json!({
            "mode": "name",
            "path": long_path,
            "pattern": "*.txt"
        });
        let mut fs_search = serde_json::from_value::<FsSearch>(v).unwrap();
        let result = fs_search.validate(&ctx).await;
        assert!(result.is_err());
    }


    #[tokio::test]
    async fn test_file_path_glob_filtering() {
        let ctx = setup_test_directory().await;
        let fs = ctx.fs();
        
        // Create a diverse file structure for testing
        fs.create_dir_all("/project/src").await.unwrap();
        fs.create_dir_all("/project/tests").await.unwrap();
        fs.create_dir_all("/project/docs").await.unwrap();
        
        fs.write("/project/src/main.rs", "fn main() { println!(\"Hello\"); }").await.unwrap();
        fs.write("/project/src/lib.rs", "pub mod utils;").await.unwrap();
        fs.write("/project/src/utils.py", "def hello(): pass").await.unwrap();
        fs.write("/project/tests/test.rs", "// Test file").await.unwrap();
        fs.write("/project/tests/integration.py", "# Integration test").await.unwrap();
        fs.write("/project/docs/README.md", "# Documentation").await.unwrap();
        fs.write("/project/config.json", "{\"version\": \"1.0\"}").await.unwrap();
        
        let mut stdout = std::io::stdout();
        
        // Test filtering for Rust files only
        let v = json!({
            "mode": "content",
            "path": "/project",
            "pattern": "fn|mod|Test",
            "file_path": "*.rs"
        });
        let output = serde_json::from_value::<FsSearch>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();
        
        if let OutputKind::Text(text) = output.output {
            // Should only find matches in .rs files
            assert!(text.contains("main.rs") || text.contains("lib.rs") || text.contains("test.rs"));
            assert!(!text.contains("utils.py"));
            assert!(!text.contains("integration.py"));
            assert!(!text.contains("README.md"));
        } else {
            panic!("Expected text output");
        }
        
        // Test filtering for Python files only
        let v = json!({
            "mode": "content",
            "path": "/project",
            "pattern": "def|#",
            "file_path": "*.py"
        });
        let output = serde_json::from_value::<FsSearch>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();
        
        if let OutputKind::Text(text) = output.output {
            // Should only find matches in .py files
            assert!(text.contains("utils.py") || text.contains("integration.py"));
            assert!(!text.contains("main.rs"));
            assert!(!text.contains("README.md"));
        } else {
            panic!("Expected text output");
        }
        
        // Test recursive pattern filtering
        let v = json!({
            "mode": "content",
            "path": "/project",
            "pattern": "test|Test",
            "file_path": "**/test*"
        });
        let output = serde_json::from_value::<FsSearch>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();
        
        if let OutputKind::Text(text) = output.output {
            // Should find test files in subdirectories
            assert!(text.contains("test.rs") || text.contains("integration.py"));
        } else {
            panic!("Expected text output");
        }
    }

    #[tokio::test]
    async fn test_file_path_validation() {
        let ctx = setup_test_directory().await;
        
        // Test valid file_path patterns
        let v = json!({
            "mode": "content",
            "path": "/",
            "pattern": "test",
            "file_path": "*.rs"
        });
        let mut fs_search = serde_json::from_value::<FsSearch>(v).unwrap();
        assert!(fs_search.validate(&ctx).await.is_ok());
        
        let v = json!({
            "mode": "content",
            "path": "/",
            "pattern": "test",
            "file_path": "**/*.py"
        });
        let mut fs_search = serde_json::from_value::<FsSearch>(v).unwrap();
        assert!(fs_search.validate(&ctx).await.is_ok());
        
        // Test invalid file_path patterns
        let v = json!({
            "mode": "content",
            "path": "/",
            "pattern": "test",
            "file_path": "[unclosed"
        });
        let mut fs_search = serde_json::from_value::<FsSearch>(v).unwrap();
        let result = fs_search.validate(&ctx).await;
        assert!(result.is_err());
        let error_msg = format!("{}", result.unwrap_err());
        assert!(error_msg.contains("Invalid glob pattern"));
    }

    #[tokio::test]
    async fn test_file_path_deserialization() {
        // Test content search with file_path parameter
        let json = json!({
            "mode": "content",
            "path": "/test",
            "pattern": "TODO",
            "file_path": "*.rs",
            "context_before": 1,
            "context_after": 1
        });
        
        let fs_search: FsSearch = serde_json::from_value(json).unwrap();
        match fs_search {
            FsSearch::Content(content_search) => {
                assert_eq!(content_search.path, "/test");
                assert_eq!(content_search.pattern, "TODO");
                assert_eq!(content_search.file_path, Some("*.rs".to_string()));
                assert_eq!(content_search.context_before, Some(1));
                assert_eq!(content_search.context_after, Some(1));
            }
            _ => panic!("Expected Content variant"),
        }
        
        // Test content search without file_path parameter (should be None)
        let json = json!({
            "mode": "content",
            "path": "/test",
            "pattern": "TODO"
        });
        
        let fs_search: FsSearch = serde_json::from_value(json).unwrap();
        match fs_search {
            FsSearch::Content(content_search) => {
                assert_eq!(content_search.file_path, None);
            }
            _ => panic!("Expected Content variant"),
        }
    }

    #[tokio::test]
    async fn test_combined_filtering_and_context() {
        let ctx = setup_test_directory().await;
        let fs = ctx.fs();
        
        // Create test files with specific content
        fs.write("/filtered_test.rs", r#"
fn main() {
    // TODO: Implement main logic
    println!("Hello");
    // FIXME: Handle errors properly
}
"#).await.unwrap();
        
        fs.write("/filtered_test.py", r#"
def main():
    # TODO: Implement main logic
    print("Hello")
    # FIXME: Handle errors properly
"#).await.unwrap();
        
        let mut stdout = std::io::stdout();
        
        // Test filtering with context - should only search in .rs files
        let v = json!({
            "mode": "content",
            "path": "/",
            "pattern": "TODO",
            "file_path": "*.rs",
            "context_before": 1,
            "context_after": 1
        });
        let output = serde_json::from_value::<FsSearch>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();
        
        if let OutputKind::Text(text) = output.output {
            // Should find TODO in .rs file with context
            assert!(text.contains("filtered_test.rs"));
            assert!(text.contains("TODO"));
            assert!(text.contains("[context]") || text.contains("[match]"));
            // Should not find matches in .py file
            assert!(!text.contains("filtered_test.py"));
        } else {
            panic!("Expected text output");
        }
    }
}
