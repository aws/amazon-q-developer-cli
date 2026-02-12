//! File and directory reference expansion for @path syntax.
//!
//! This module handles resolving @references in user input to either:
//! - File contents (injected inline)
//! - Directory listings (tree format injected inline)
//!
//! Unresolved references (prompts, typos, etc.) are left unchanged.

use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use agent::tools::MAX_READ_SIZE;
use ignore::WalkBuilder;

/// Maximum depth for directory tree traversal
const MAX_TREE_DEPTH: usize = 3;

/// The prefix character used for @file and @directory references
pub const REFERENCE_PREFIX: char = '@';

/// Maximum items to show per directory level before truncating
const MAX_ITEMS_PER_LEVEL: usize = 10;

/// Maximum total size of all expanded content (1MB).
/// Prevents excessive memory usage when referencing many/large files.
const MAX_TOTAL_EXPANSION_SIZE: usize = 1024 * 1024;

/// Result of resolving a single @reference
#[derive(Debug)]
pub enum ReferenceResolution {
    /// Reference is a readable file
    File { path: String, content: String },
    /// Reference is a file that was truncated due to size
    FileTruncated { path: String, content: String },
    /// Reference is a directory
    Directory {
        path: String,
        tree: String,
        truncated: bool,
    },
    /// File appears to be binary
    FileBinary(String),
    /// Reference not found - leave unchanged in message
    NotFound,
    /// Error reading file/directory
    Error { path: String, error: String },
}

/// Result of expanding all @references in a message
#[derive(Debug)]
pub struct ExpansionResult {
    /// The message with file/directory contents injected
    pub expanded_message: String,
    /// Warnings to display to the user (e.g., files truncated)
    pub warnings: Vec<String>,
    /// Errors to display to the user (e.g., binary files)
    pub errors: Vec<String>,
}

/// Check if file content appears to be binary by looking for null bytes
fn is_binary_content(content: &[u8]) -> bool {
    // Check first 8KB for null bytes (common binary indicator)
    let check_len = content.len().min(8192);
    content[..check_len].contains(&0)
}

/// Entry for directory tree generation
struct TreeEntry {
    name: String,
    path: std::path::PathBuf,
    is_dir: bool,
}

/// Generate a tree listing for a directory using the ignore crate for .gitignore support
fn generate_tree(path: &Path, prefix: &str, depth: usize) -> (String, bool) {
    let mut result = String::new();
    let mut truncated = false;

    if depth >= MAX_TREE_DEPTH {
        return (result, false);
    }

    // Use WalkBuilder with .gitignore support (only immediate children at this level)
    let walker = WalkBuilder::new(path)
        .hidden(false) // Show hidden files (user explicitly requested directory)
        .ignore(true) // Respect .ignore files
        .git_ignore(true) // Respect .gitignore
        .git_global(true) // Respect global gitignore
        .git_exclude(true) // Respect .git/info/exclude
        .max_depth(Some(1)) // Only immediate children
        .build();

    // Collect entries (skip the root directory itself)
    let mut items: Vec<TreeEntry> = walker
        .filter_map(|e| e.ok())
        .filter(|e| e.path() != path) // Skip the root
        .map(|e| TreeEntry {
            name: e.file_name().to_string_lossy().to_string(),
            path: e.path().to_path_buf(),
            is_dir: e.file_type().is_some_and(|ft| ft.is_dir()),
        })
        .collect();

    // Sort: directories first, then alphabetically
    items.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });

    let total_items = items.len();
    let display_count = total_items.min(MAX_ITEMS_PER_LEVEL);

    for (i, entry) in items.iter().take(display_count).enumerate() {
        let is_last = i == display_count - 1 && total_items <= MAX_ITEMS_PER_LEVEL;
        let connector = if is_last { "└── " } else { "├── " };
        let child_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });

        if entry.is_dir {
            result.push_str(&format!("{}{}{}/\n", prefix, connector, entry.name));

            // Recurse into subdirectory
            let (subtree, sub_truncated) = generate_tree(&entry.path, &child_prefix, depth + 1);
            result.push_str(&subtree);

            if sub_truncated {
                truncated = true;
            }
        } else {
            result.push_str(&format!("{}{}{}\n", prefix, connector, entry.name));
        }
    }

    // Show truncation indicator if there are more items
    if total_items > MAX_ITEMS_PER_LEVEL {
        truncated = true;
        let remaining = total_items - MAX_ITEMS_PER_LEVEL;
        result.push_str(&format!("{}└── ... ({} more items)\n", prefix, remaining));
    }

    (result, truncated)
}

/// Resolve a single @reference to a file or directory
///
/// # Arguments
/// * `reference` - The reference string (without the @ prefix)
fn resolve_reference(reference: &str) -> ReferenceResolution {
    // First try the path as-is
    let path = Path::new(reference);
    if path.is_file() || path.is_dir() {
        return resolve_path(reference, path);
    }

    // If not found, try fuzzy whitespace matching
    // This handles cases where rustyline drops special characters like U+202F
    if let Some(resolution) = try_fuzzy_whitespace_match(reference) {
        return resolution;
    }

    ReferenceResolution::NotFound
}

/// Remove all whitespace from a string for fuzzy comparison.
/// This handles cases where whitespace characters are dropped entirely.
fn strip_whitespace(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Try to find a file that matches after normalizing Unicode whitespace.
/// This handles cases where:
/// - Rustyline drops special whitespace characters like U+202F
/// - Whitespace is completely missing from the input
fn try_fuzzy_whitespace_match(reference: &str) -> Option<ReferenceResolution> {
    let path = Path::new(reference);
    // path.parent() returns Some("") for files in current directory, which doesn't work with read_dir
    let search_dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let filename = path.file_name()?.to_str()?;
    let stripped_filename = strip_whitespace(filename);

    // List files in directory and find one that matches after normalization
    for entry in fs::read_dir(search_dir).ok()?.flatten() {
        if let Some(entry_name) = entry.file_name().to_str()
            && strip_whitespace(entry_name) == stripped_filename
        {
            // Found a match - use actual filesystem path
            let actual_path = entry.path();
            let display_path = if search_dir == Path::new(".") {
                entry_name.to_string()
            } else {
                actual_path.to_string_lossy().to_string()
            };
            return Some(resolve_path(&display_path, &actual_path));
        }
    }
    None
}

/// Internal helper to resolve a path that we know exists
fn resolve_path(reference: &str, path: &Path) -> ReferenceResolution {
    // Check if it's a file
    if path.is_file() {
        // Get file size to determine if we need to truncate
        let file_size = match fs::metadata(path) {
            Ok(meta) => meta.len(),
            Err(e) => {
                return ReferenceResolution::Error {
                    path: reference.to_string(),
                    error: e.to_string(),
                };
            },
        };

        let will_truncate = file_size > MAX_READ_SIZE as u64;
        let bytes_to_read = if will_truncate {
            MAX_READ_SIZE as usize
        } else {
            file_size as usize
        };

        // Read file content (up to MAX_READ_SIZE bytes)
        let read_result: std::io::Result<Vec<u8>> = File::open(path).and_then(|mut file| {
            let mut buffer = vec![0u8; bytes_to_read];
            file.read_exact(&mut buffer)?;
            Ok(buffer)
        });

        match read_result {
            Ok(bytes) => {
                // Check for binary content
                if is_binary_content(&bytes) {
                    return ReferenceResolution::FileBinary(reference.to_string());
                }

                // Convert to string
                match String::from_utf8(bytes) {
                    Ok(content) if will_truncate => ReferenceResolution::FileTruncated {
                        path: reference.to_string(),
                        content,
                    },
                    Ok(content) => ReferenceResolution::File {
                        path: reference.to_string(),
                        content,
                    },
                    Err(_) => ReferenceResolution::FileBinary(reference.to_string()),
                }
            },
            Err(e) => ReferenceResolution::Error {
                path: reference.to_string(),
                error: e.to_string(),
            },
        }
    }
    // Check if it's a directory
    else if path.is_dir() {
        let (tree_content, truncated) = generate_tree(path, "", 0);
        // Strip trailing slash to avoid double slashes (e.g., "crates/" -> "crates")
        let display_path = reference.trim_end_matches('/');
        let tree = format!("{}/\n{}", display_path, tree_content);

        ReferenceResolution::Directory {
            path: reference.to_string(),
            tree,
            truncated,
        }
    }
    // Should not reach here if called correctly, but handle gracefully
    else {
        ReferenceResolution::NotFound
    }
}

/// Extract all @references from input text that are at word boundaries.
///
/// Only extracts @references that are:
/// - At the start of the input, OR
/// - Preceded by whitespace
///
/// This avoids matching emails like `user@domain.com`.
///
/// Returns a vector of (full_match, reference_without_at, start_position)
fn extract_references(input: &str) -> Vec<(String, String, usize)> {
    let mut results = Vec::new();
    let mut chars = input.char_indices().peekable();
    let mut prev_char: Option<char> = None;

    while let Some((i, c)) = chars.next() {
        // Only match @ at word boundaries (start of input or after whitespace)
        if c == REFERENCE_PREFIX && (prev_char.is_none() || prev_char.is_some_and(|p| p.is_whitespace())) {
            let start = i;
            let mut reference = String::new();

            // Handle quoted paths: @"path with spaces"
            if chars.peek().map(|(_, c)| *c) == Some('"') {
                chars.next(); // consume opening quote
                for (_, c) in chars.by_ref() {
                    if c == '"' {
                        break;
                    }
                    reference.push(c);
                }
                if !reference.is_empty() {
                    let full_match = format!("{}\"{}\"", REFERENCE_PREFIX, reference);
                    results.push((full_match, reference, start));
                }
            } else {
                // Regular path: consume until whitespace
                while let Some(&(_, c)) = chars.peek() {
                    if c.is_whitespace() {
                        break;
                    }
                    reference.push(c);
                    chars.next();
                }

                if !reference.is_empty() {
                    let full_match = format!("{}{}", REFERENCE_PREFIX, reference);
                    results.push((full_match, reference, start));
                }
            }
        }
        prev_char = Some(c);
    }

    results
}

/// Format file content for injection using XML-style tags for clarity
fn format_file_content(path: &str, content: &str, truncated: bool) -> String {
    if truncated {
        format!(
            "<file_content path=\"{}\">\n{}\n[truncated - file exceeds 250KB limit]\n</file_content>",
            path,
            content.trim_end()
        )
    } else {
        format!(
            "<file_content path=\"{}\">\n{}\n</file_content>",
            path,
            content.trim_end()
        )
    }
}

/// Format directory tree for injection using XML-style tags for clarity
fn format_directory_tree(path: &str, tree: &str, truncated: bool) -> String {
    let truncated_note = if truncated { ", truncated" } else { "" };
    format!(
        "<directory_listing path=\"{}\"{}>\n{}</directory_listing>",
        path,
        truncated_note,
        tree.trim_end()
    )
}

/// Expand all @references in a message
///
/// Files and directories are expanded inline. Unresolved references
/// (prompts, typos, etc.) are left unchanged for backwards compatibility.
///
/// # Arguments
/// * `input` - The user's input message
pub fn expand_references(input: &str) -> ExpansionResult {
    let references = extract_references(input);

    if references.is_empty() {
        return ExpansionResult {
            expanded_message: input.to_string(),
            warnings: Vec::new(),
            errors: Vec::new(),
        };
    }

    let mut result = input.to_string();
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    // Track cumulative offset from replacements to adjust positions
    let mut offset: isize = 0;

    // Track total size of expanded content to enforce limit
    let mut total_expansion_size: usize = 0;

    // Process references in forward order, adjusting positions as we go
    for (full_match, reference, start) in references {
        // Use saturating arithmetic to safely handle offset adjustments
        let adjusted_start = (start as isize).saturating_add(offset).max(0) as usize;
        let adjusted_end = adjusted_start.saturating_add(full_match.len()).min(result.len());

        let resolution = resolve_reference(&reference);
        match resolution {
            ReferenceResolution::File { path, content } => {
                // Check if adding this content would exceed the total limit
                if total_expansion_size.saturating_add(content.len()) > MAX_TOTAL_EXPANSION_SIZE {
                    warnings.push(format!("Skipping '{}': total expansion size limit (1MB) reached", path));
                    continue;
                }
                total_expansion_size = total_expansion_size.saturating_add(content.len());

                let formatted = format_file_content(&path, &content, false);
                let old_len = full_match.len();
                let new_len = formatted.len();
                result = format!("{}{}{}", &result[..adjusted_start], formatted, &result[adjusted_end..]);
                offset = offset.saturating_add((new_len as isize).saturating_sub(old_len as isize));
            },
            ReferenceResolution::FileTruncated { path, content } => {
                // Check if adding this content would exceed the total limit
                if total_expansion_size.saturating_add(content.len()) > MAX_TOTAL_EXPANSION_SIZE {
                    warnings.push(format!("Skipping '{}': total expansion size limit (1MB) reached", path));
                    continue;
                }
                total_expansion_size = total_expansion_size.saturating_add(content.len());

                let formatted = format_file_content(&path, &content, true);
                let old_len = full_match.len();
                let new_len = formatted.len();
                result = format!("{}{}{}", &result[..adjusted_start], formatted, &result[adjusted_end..]);
                offset = offset.saturating_add((new_len as isize).saturating_sub(old_len as isize));
                warnings.push(format!("File '{}' was truncated (exceeds 250KB limit)", path));
            },
            ReferenceResolution::Directory { path, tree, truncated } => {
                // Check if adding this tree would exceed the total limit
                if total_expansion_size.saturating_add(tree.len()) > MAX_TOTAL_EXPANSION_SIZE {
                    warnings.push(format!("Skipping '{}': total expansion size limit (1MB) reached", path));
                    continue;
                }
                total_expansion_size = total_expansion_size.saturating_add(tree.len());

                let formatted = format_directory_tree(&path, &tree, truncated);
                let old_len = full_match.len();
                let new_len = formatted.len();
                result = format!("{}{}{}", &result[..adjusted_start], formatted, &result[adjusted_end..]);
                offset = offset.saturating_add((new_len as isize).saturating_sub(old_len as isize));
            },
            ReferenceResolution::FileBinary(path) => {
                let replacement = format!("[binary file: {}]", path);
                let old_len = full_match.len();
                let new_len = replacement.len();
                result = format!(
                    "{}{}{}",
                    &result[..adjusted_start],
                    replacement,
                    &result[adjusted_end..]
                );
                offset = offset.saturating_add((new_len as isize).saturating_sub(old_len as isize));
                errors.push(format!(
                    "File '{}' appears to be binary and cannot be included inline.",
                    path
                ));
            },
            ReferenceResolution::NotFound => {
                // Leave @reference unchanged - could be a prompt or intentional text
                // No warning needed since this is expected behavior
            },
            ReferenceResolution::Error { path, error } => {
                errors.push(format!("Failed to read '{}': {}", path, error));
            },
        }
    }

    ExpansionResult {
        expanded_message: result,
        warnings,
        errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_references_single() {
        let input = "@src/main.rs";
        let refs = extract_references(input);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].1, "src/main.rs");
    }

    #[test]
    fn test_extract_references_multiple() {
        let input = "compare @file1.rs and @file2.rs";
        let refs = extract_references(input);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].1, "file1.rs");
        assert_eq!(refs[1].1, "file2.rs");
    }

    #[test]
    fn test_extract_references_quoted() {
        let input = r#"@"path with spaces/file.rs""#;
        let refs = extract_references(input);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].1, "path with spaces/file.rs");
    }

    #[test]
    fn test_extract_references_mixed() {
        let input = "look at @src and @Cargo.toml then fix it";
        let refs = extract_references(input);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].1, "src");
        assert_eq!(refs[1].1, "Cargo.toml");
    }

    #[test]
    fn test_extract_references_duplicate() {
        let input = "compare @file.rs with @file.rs";
        let refs = extract_references(input);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].1, "file.rs");
        assert_eq!(refs[1].1, "file.rs");
        // Verify positions are different
        assert_ne!(refs[0].2, refs[1].2);
    }

    #[test]
    fn test_extract_references_ignores_emails() {
        let input = "email me at user@domain.com about @Cargo.toml";
        let refs = extract_references(input);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].1, "Cargo.toml");
    }

    #[test]
    fn test_is_binary_content() {
        assert!(!is_binary_content(b"Hello, world!"));
        assert!(!is_binary_content(b"fn main() {\n    println!(\"test\");\n}"));
        assert!(is_binary_content(b"\x00\x01\x02\x03"));
        assert!(is_binary_content(b"PNG\x00\x00\x00"));
    }

    #[test]
    fn test_resolve_reference_not_found() {
        // Non-existent file should return NotFound
        let result = resolve_reference("nonexistent_file_12345.rs");
        assert!(matches!(result, ReferenceResolution::NotFound));
    }

    #[test]
    fn test_format_file_content() {
        let formatted = format_file_content("test.rs", "fn main() {}", false);
        assert!(formatted.starts_with("<file_content path=\"test.rs\">"));
        assert!(formatted.contains("fn main() {}"));
    }

    #[test]
    fn test_format_file_content_truncated() {
        let formatted = format_file_content("test.rs", "content", true);
        assert!(formatted.contains("[truncated"));
        assert!(formatted.contains("</file_content>"));
    }

    #[test]
    fn test_extract_references_unicode_whitespace() {
        // U+202F is narrow no-break space used in macOS screenshot filenames
        let input = "what's in @\"Screenshot 2026-01-29 at 8.47.56\u{202F}AM.png\"";
        let refs = extract_references(input);
        assert_eq!(refs.len(), 1);
        let (full_match, reference, _) = &refs[0];
        assert_eq!(full_match, "@\"Screenshot 2026-01-29 at 8.47.56\u{202F}AM.png\"");
        assert_eq!(reference, "Screenshot 2026-01-29 at 8.47.56\u{202F}AM.png");
        assert!(reference.contains('\u{202F}'), "Should preserve narrow no-break space");
    }
}
