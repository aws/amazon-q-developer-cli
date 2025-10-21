use anyhow::Result;
use lsp_types::{TextEdit, WorkspaceEdit};
use std::path::{Path, PathBuf};

/// Canonicalizes a file path, resolving symbolic links and relative components.
///
/// # Arguments
/// * `path` - The path to canonicalize
///
/// # Returns
/// * `Result<PathBuf>` - The canonicalized absolute path
///
/// # Errors
/// Returns an error if the path doesn't exist or cannot be canonicalized.
///
/// # Examples
/// ```no_run
/// use code_agent_sdk::utils::canonicalize_path;
/// let canonical = canonicalize_path("./src/main.rs").unwrap();
/// ```
pub fn canonicalize_path<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
    path.as_ref().canonicalize().map_err(|e| {
        anyhow::anyhow!(
            "Failed to canonicalize path '{}': {}",
            path.as_ref().display(),
            e
        )
    })
}

/// Ensures a path is absolute, canonicalizing it if necessary.
///
/// # Arguments
/// * `path` - The path to make absolute
///
/// # Returns
/// * `Result<PathBuf>` - The absolute path
///
/// # Examples
/// ```no_run
/// use code_agent_sdk::utils::ensure_absolute_path;
/// let abs_path = ensure_absolute_path("./relative/path").unwrap();
/// assert!(abs_path.is_absolute());
/// ```
pub fn ensure_absolute_path<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
    let path = path.as_ref();
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        canonicalize_path(path)
    }
}

/// Applies a workspace edit containing multiple file changes.
///
/// # Arguments
/// * `workspace_edit` - The LSP workspace edit to apply
///
/// # Returns
/// * `Result<()>` - Success or error
///
/// # Errors
/// Returns an error if any file cannot be written or text edits fail.
///
/// # Examples
/// ```no_run
/// use code_agent_sdk::utils::apply_workspace_edit;
/// use lsp_types::WorkspaceEdit;
/// use std::collections::HashMap;
///
/// let workspace_edit = WorkspaceEdit {
///     changes: Some(HashMap::new()),
///     ..Default::default()
/// };
/// apply_workspace_edit(&workspace_edit).unwrap();
/// ```
pub fn apply_workspace_edit(workspace_edit: &WorkspaceEdit) -> Result<()> {
    let mut applied_files = Vec::new();
    let mut failed_files = Vec::new();

    if let Some(changes) = &workspace_edit.changes {
        for (uri, edits) in changes {
            let file_path = Path::new(uri.path());
            
            // Validate file exists and is writable
            if !file_path.exists() {
                failed_files.push((file_path.to_path_buf(), "File does not exist".to_string()));
                continue;
            }

            if file_path.metadata()?.permissions().readonly() {
                failed_files.push((file_path.to_path_buf(), "File is read-only".to_string()));
                continue;
            }

            // Apply edits with validation
            match apply_text_edits(file_path, edits) {
                Ok(()) => {
                    applied_files.push(file_path.to_path_buf());
                }
                Err(e) => {
                    failed_files.push((file_path.to_path_buf(), e.to_string()));
                }
            }
        }
    }

    // Report results
    if !failed_files.is_empty() {
        let error_msg = failed_files
            .iter()
            .map(|(path, error)| format!("{}: {}", path.display(), error))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(anyhow::anyhow!("Failed to apply edits to {} files: {}", failed_files.len(), error_msg));
    }

    if applied_files.is_empty() {
        return Err(anyhow::anyhow!("No edits were applied"));
    }

    Ok(())
}

/// Applies a series of text edits to a file.
///
/// Text edits are applied in reverse order (from end to beginning) to avoid
/// offset issues when multiple edits affect the same file.
///
/// # Arguments
/// * `file_path` - Path to the file to edit
/// * `edits` - Array of LSP text edits to apply
///
/// # Returns
/// * `Result<()>` - Success or error
///
/// # Errors
/// Returns an error if the file cannot be read or written.
///
/// # Examples
/// ```no_run
/// use code_agent_sdk::utils::apply_text_edits;
/// use lsp_types::{TextEdit, Range, Position};
/// use std::path::Path;
///
/// let range = Range::new(Position::new(0, 0), Position::new(0, 5));
/// let edits = vec![TextEdit { range, new_text: "new content".to_string() }];
/// apply_text_edits(Path::new("file.txt"), &edits).unwrap();
/// ```
/// ```
pub fn apply_text_edits(file_path: &Path, edits: &[TextEdit]) -> Result<()> {
    let content = std::fs::read_to_string(file_path)?;
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    // Sort edits by position in reverse order to avoid offset issues
    let mut sorted_edits = edits.to_vec();
    sorted_edits.sort_by(|a, b| {
        b.range
            .start
            .line
            .cmp(&a.range.start.line)
            .then_with(|| b.range.start.character.cmp(&a.range.start.character))
    });

    for edit in sorted_edits {
        let start_line = edit.range.start.line as usize;
        let start_char = edit.range.start.character as usize;
        let end_line = edit.range.end.line as usize;
        let end_char = edit.range.end.character as usize;

        if start_line < lines.len() && end_line < lines.len() {
            if start_line == end_line {
                // Single line edit
                let line = &mut lines[start_line];
                if start_char <= line.len() && end_char <= line.len() {
                    line.replace_range(start_char..end_char, &edit.new_text);
                }
            } else {
                // Multi-line edit (replace from start_line:start_char to end_line:end_char)
                let mut new_content = String::new();

                // Keep beginning of start line
                if start_char < lines[start_line].len() {
                    new_content.push_str(&lines[start_line][..start_char]);
                }

                // Add new text
                new_content.push_str(&edit.new_text);

                // Keep end of end line
                if end_char < lines[end_line].len() {
                    new_content.push_str(&lines[end_line][end_char..]);
                }

                // Replace the range of lines with the new content
                lines.splice(start_line..=end_line, vec![new_content]);
            }
        }
    }

    let new_content = lines.join("\n");
    std::fs::write(file_path, new_content)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Position, Range, Url};
    use std::collections::HashMap;
    use std::fs;
    use tempfile::TempDir;

    fn create_temp_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let file_path = dir.path().join(name);
        fs::write(&file_path, content).unwrap();
        file_path
    }

    #[test]
    fn test_canonicalize_path_existing_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_temp_file(&temp_dir, "test.txt", "content");
        
        let canonical = canonicalize_path(&file_path).unwrap();
        assert!(canonical.is_absolute());
        assert_eq!(canonical, file_path.canonicalize().unwrap());
    }

    #[test]
    fn test_canonicalize_path_nonexistent_file() {
        let nonexistent = Path::new("/nonexistent/path/file.txt");
        let result = canonicalize_path(nonexistent);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to canonicalize path"));
    }

    #[test]
    fn test_ensure_absolute_path_already_absolute() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_temp_file(&temp_dir, "test.txt", "content");
        
        let result = ensure_absolute_path(&file_path).unwrap();
        assert!(result.is_absolute());
        assert_eq!(result, file_path);
    }

    #[test]
    fn test_ensure_absolute_path_relative() {
        // Use current directory as a known existing relative path
        let current_dir = std::env::current_dir().unwrap();
        let relative_path = Path::new(".");
        
        let result = ensure_absolute_path(relative_path).unwrap();
        assert!(result.is_absolute());
        assert_eq!(result, current_dir);
    }

    #[test]
    fn test_ensure_absolute_path_nonexistent() {
        let nonexistent = Path::new("./nonexistent/relative/path");
        let result = ensure_absolute_path(nonexistent);
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_text_edits_single_line_replacement() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_temp_file(&temp_dir, "test.txt", "Hello world!");
        
        let edit = TextEdit {
            range: Range::new(Position::new(0, 6), Position::new(0, 11)),
            new_text: "Rust".to_string(),
        };
        
        apply_text_edits(&file_path, &[edit]).unwrap();
        
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello Rust!");
    }

    #[test]
    fn test_apply_text_edits_single_line_insertion() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_temp_file(&temp_dir, "test.txt", "Hello world!");
        
        let edit = TextEdit {
            range: Range::new(Position::new(0, 5), Position::new(0, 5)),
            new_text: " beautiful".to_string(),
        };
        
        apply_text_edits(&file_path, &[edit]).unwrap();
        
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello beautiful world!");
    }

    #[test]
    fn test_apply_text_edits_single_line_deletion() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_temp_file(&temp_dir, "test.txt", "Hello beautiful world!");
        
        let edit = TextEdit {
            range: Range::new(Position::new(0, 5), Position::new(0, 15)),
            new_text: "".to_string(),
        };
        
        apply_text_edits(&file_path, &[edit]).unwrap();
        
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello world!");
    }

    #[test]
    fn test_apply_text_edits_multiline_content() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_temp_file(&temp_dir, "test.txt", "line1\nline2\nline3");
        
        let edit = TextEdit {
            range: Range::new(Position::new(1, 0), Position::new(1, 5)),
            new_text: "modified".to_string(),
        };
        
        apply_text_edits(&file_path, &[edit]).unwrap();
        
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "line1\nmodified\nline3");
    }

    #[test]
    fn test_apply_text_edits_multiline_replacement() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_temp_file(&temp_dir, "test.txt", "line1\nline2\nline3\nline4");
        
        let edit = TextEdit {
            range: Range::new(Position::new(1, 2), Position::new(2, 2)),
            new_text: "new\ncontent".to_string(),
        };
        
        apply_text_edits(&file_path, &[edit]).unwrap();
        
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "line1\nlinew\ncontentne3\nline4");
    }

    #[test]
    fn test_apply_text_edits_multiple_edits_reverse_order() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_temp_file(&temp_dir, "test.txt", "abc def ghi");
        
        let edits = vec![
            TextEdit {
                range: Range::new(Position::new(0, 0), Position::new(0, 3)),
                new_text: "123".to_string(),
            },
            TextEdit {
                range: Range::new(Position::new(0, 8), Position::new(0, 11)),
                new_text: "789".to_string(),
            },
        ];
        
        apply_text_edits(&file_path, &edits).unwrap();
        
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "123 def 789");
    }

    #[test]
    fn test_apply_text_edits_out_of_bounds() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_temp_file(&temp_dir, "test.txt", "short");
        
        let edit = TextEdit {
            range: Range::new(Position::new(10, 0), Position::new(10, 5)),
            new_text: "replacement".to_string(),
        };
        
        // Should not panic, just ignore out-of-bounds edits
        apply_text_edits(&file_path, &[edit]).unwrap();
        
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "short"); // Unchanged
    }

    #[test]
    fn test_apply_text_edits_nonexistent_file() {
        let nonexistent = Path::new("/nonexistent/file.txt");
        let edit = TextEdit {
            range: Range::new(Position::new(0, 0), Position::new(0, 0)),
            new_text: "test".to_string(),
        };
        
        let result = apply_text_edits(nonexistent, &[edit]);
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_workspace_edit_empty_changes() {
        let workspace_edit = WorkspaceEdit {
            changes: None,
            ..Default::default()
        };
        
        let result = apply_workspace_edit(&workspace_edit);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No edits were applied"));
    }

    #[test]
    fn test_apply_workspace_edit_empty_changes_map() {
        let workspace_edit = WorkspaceEdit {
            changes: Some(HashMap::new()),
            ..Default::default()
        };
        
        let result = apply_workspace_edit(&workspace_edit);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No edits were applied"));
    }

    #[test]
    fn test_apply_workspace_edit_successful() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_temp_file(&temp_dir, "test.txt", "Hello world!");
        let uri = Url::from_file_path(&file_path).unwrap();
        
        let edit = TextEdit {
            range: Range::new(Position::new(0, 6), Position::new(0, 11)),
            new_text: "Rust".to_string(),
        };
        
        let mut changes = HashMap::new();
        changes.insert(uri, vec![edit]);
        
        let workspace_edit = WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        };
        
        apply_workspace_edit(&workspace_edit).unwrap();
        
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello Rust!");
    }

    #[test]
    fn test_apply_workspace_edit_nonexistent_file() {
        let nonexistent = Path::new("/nonexistent/file.txt");
        let uri = Url::from_file_path(nonexistent).unwrap();
        
        let edit = TextEdit {
            range: Range::new(Position::new(0, 0), Position::new(0, 0)),
            new_text: "test".to_string(),
        };
        
        let mut changes = HashMap::new();
        changes.insert(uri, vec![edit]);
        
        let workspace_edit = WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        };
        
        let result = apply_workspace_edit(&workspace_edit);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("File does not exist"));
    }

    #[test]
    fn test_apply_workspace_edit_multiple_files() {
        let temp_dir = TempDir::new().unwrap();
        let file1 = create_temp_file(&temp_dir, "file1.txt", "content1");
        let file2 = create_temp_file(&temp_dir, "file2.txt", "content2");
        
        let uri1 = Url::from_file_path(&file1).unwrap();
        let uri2 = Url::from_file_path(&file2).unwrap();
        
        let edit1 = TextEdit {
            range: Range::new(Position::new(0, 7), Position::new(0, 8)),
            new_text: " modified".to_string(),
        };
        let edit2 = TextEdit {
            range: Range::new(Position::new(0, 7), Position::new(0, 8)),
            new_text: " updated".to_string(),
        };
        
        let mut changes = HashMap::new();
        changes.insert(uri1, vec![edit1]);
        changes.insert(uri2, vec![edit2]);
        
        let workspace_edit = WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        };
        
        apply_workspace_edit(&workspace_edit).unwrap();
        
        let content1 = fs::read_to_string(&file1).unwrap();
        let content2 = fs::read_to_string(&file2).unwrap();
        assert_eq!(content1, "content modified");
        assert_eq!(content2, "content updated");
    }

    #[test]
    fn test_apply_workspace_edit_mixed_success_failure() {
        let temp_dir = TempDir::new().unwrap();
        let existing_file = create_temp_file(&temp_dir, "existing.txt", "content");
        let nonexistent_file = temp_dir.path().join("nonexistent.txt");
        
        let uri1 = Url::from_file_path(&existing_file).unwrap();
        let uri2 = Url::from_file_path(&nonexistent_file).unwrap();
        
        let edit1 = TextEdit {
            range: Range::new(Position::new(0, 0), Position::new(0, 0)),
            new_text: "prefix ".to_string(),
        };
        let edit2 = TextEdit {
            range: Range::new(Position::new(0, 0), Position::new(0, 0)),
            new_text: "test".to_string(),
        };
        
        let mut changes = HashMap::new();
        changes.insert(uri1, vec![edit1]);
        changes.insert(uri2, vec![edit2]);
        
        let workspace_edit = WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        };
        
        let result = apply_workspace_edit(&workspace_edit);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Failed to apply edits to 1 files"));
        assert!(error_msg.contains("File does not exist"));
    }

    #[cfg(unix)]
    #[test]
    fn test_apply_workspace_edit_readonly_file() {
        use std::os::unix::fs::PermissionsExt;
        
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_temp_file(&temp_dir, "readonly.txt", "content");
        
        // Make file read-only
        let mut perms = fs::metadata(&file_path).unwrap().permissions();
        perms.set_mode(0o444);
        fs::set_permissions(&file_path, perms).unwrap();
        
        let uri = Url::from_file_path(&file_path).unwrap();
        let edit = TextEdit {
            range: Range::new(Position::new(0, 0), Position::new(0, 0)),
            new_text: "prefix ".to_string(),
        };
        
        let mut changes = HashMap::new();
        changes.insert(uri, vec![edit]);
        
        let workspace_edit = WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        };
        
        let result = apply_workspace_edit(&workspace_edit);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("File is read-only"));
    }

    #[test]
    fn test_apply_workspace_edit_text_edit_failure() {
        let temp_dir = TempDir::new().unwrap();
        let _file_path = create_temp_file(&temp_dir, "test.txt", "content");
        
        // Create an edit that will cause apply_text_edits to fail by trying to write to a directory
        let dir_path = temp_dir.path().join("directory");
        fs::create_dir(&dir_path).unwrap();
        let uri = Url::from_file_path(&dir_path).unwrap();
        
        let edit = TextEdit {
            range: Range::new(Position::new(0, 0), Position::new(0, 0)),
            new_text: "test".to_string(),
        };
        
        let mut changes = HashMap::new();
        changes.insert(uri, vec![edit]);
        
        let workspace_edit = WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        };
        
        let result = apply_workspace_edit(&workspace_edit);
        assert!(result.is_err());
        // This should trigger the apply_text_edits error path (line 101-102)
    }

    #[test]
    fn test_apply_text_edits_write_failure() {
        // Create a file in a directory, then remove write permissions from the directory
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_temp_file(&temp_dir, "test.txt", "content");
        
        // Remove the file and create a directory with the same name to cause write failure
        fs::remove_file(&file_path).unwrap();
        fs::create_dir(&file_path).unwrap();
        
        let edit = TextEdit {
            range: Range::new(Position::new(0, 0), Position::new(0, 0)),
            new_text: "test".to_string(),
        };
        
        let result = apply_text_edits(&file_path, &[edit]);
        assert!(result.is_err());
        // This should trigger the fs::write error path (line 202)
    }
}
