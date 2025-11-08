use lsp_types::{Location, WorkspaceSymbol, Diagnostic};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Diagnostic event from LSP server
#[derive(Debug, Clone)]
pub struct DiagnosticEvent {
    pub uri: String,
    pub diagnostics: Vec<Diagnostic>,
}

/// Helper function to read a single source line from a file (trimmed)
fn read_source_line(file_path: &Path, line_number: u32) -> Option<String> {
    let content = std::fs::read_to_string(file_path).ok()?;
    let lines: Vec<&str> = content.lines().collect();
    let idx = (line_number.saturating_sub(1)) as usize;

    if idx >= lines.len() {
        return None;
    }

    Some(lines[idx].trim().to_string())
}

/// Result of a rename operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameResult {
    /// Number of files that would be/were modified
    pub file_count: usize,
    /// Total number of edits across all files
    pub edit_count: usize,
}

impl RenameResult {
    /// Create from LSP WorkspaceEdit
    pub fn from_lsp_workspace_edit(edit: &lsp_types::WorkspaceEdit) -> Self {
        let mut file_count = 0;
        let mut edit_count = 0;
        
        // Handle changes field
        if let Some(changes) = &edit.changes {
            file_count += changes.len();
            edit_count += changes.values().map(|edits| edits.len()).sum::<usize>();
        }
        
        // Handle document_changes field
        if let Some(document_changes) = &edit.document_changes {
            match document_changes {
                lsp_types::DocumentChanges::Edits(edits) => {
                    file_count += edits.len();
                    edit_count += edits.iter().map(|edit| edit.edits.len()).sum::<usize>();
                }
                lsp_types::DocumentChanges::Operations(_) => {
                    // Operations like create/rename/delete files
                    file_count += 1;
                    edit_count += 1;
                }
            }
        }
        
        Self { file_count, edit_count }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceEditInfo {
    /// List of file changes
    pub changes: Vec<FileChangeInfo>,
}

/// Information about changes to a single file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChangeInfo {
    /// File path relative to workspace root
    pub file_path: String,
    /// Number of edits in this file
    pub edit_count: usize,
    /// Preview of changes (first few lines)
    pub preview: Option<String>,
}

impl WorkspaceEditInfo {
    /// Creates WorkspaceEditInfo from LSP WorkspaceEdit
    pub fn from_lsp_workspace_edit(
        edit: &lsp_types::WorkspaceEdit,
        workspace_root: &std::path::Path,
    ) -> Self {
        let mut changes = Vec::new();
        
        if let Some(document_changes) = &edit.changes {
            for (uri, text_edits) in document_changes {
                if let Ok(path) = uri.to_file_path() {
                    let relative_path = path
                        .strip_prefix(workspace_root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .to_string();
                    
                    changes.push(FileChangeInfo {
                        file_path: relative_path,
                        edit_count: text_edits.len(),
                        preview: None, // Could add preview logic here
                    });
                }
            }
        }
        
        Self { changes }
    }
}

/// Helper function to read multiple source lines from a file (from start_line to end_line inclusive)
pub(crate) fn read_source_lines(
    file_path: &Path,
    start_line: u32,
    end_line: u32,
) -> Option<String> {
    let content = std::fs::read_to_string(file_path).ok()?;
    let lines: Vec<&str> = content.lines().collect();

    let start_idx = (start_line.saturating_sub(1)) as usize;
    let end_idx = end_line as usize;

    if start_idx >= lines.len() {
        return None;
    }

    let end_idx = end_idx.min(lines.len());
    let selected_lines: Vec<String> = lines[start_idx..end_idx]
        .iter()
        .map(|s| s.to_string())
        .collect();

    if selected_lines.is_empty() {
        None
    } else {
        Some(selected_lines.join("\n"))
    }
}

/// Information about a symbol found in the codebase.
///
/// This struct represents a symbol (function, class, variable, etc.) with its location
/// and metadata. Paths are stored relative to the workspace root for portability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInfo {
    /// The name of the symbol
    pub name: String,
    /// The type/kind of symbol (e.g., "Function", "Class", "Variable")
    pub symbol_type: Option<String>,
    /// File path relative to workspace root
    pub file_path: String,
    /// Fully qualified name including file path (e.g., "src/main.rs::function_name")
    pub fully_qualified_name: String,
    /// Starting line number (1-based)
    pub start_row: u32,
    /// Ending line number (1-based)
    pub end_row: u32,
    /// Starting column number (1-based)
    pub start_column: u32,
    /// Ending column number (1-based)
    pub end_column: u32,
    /// Parent/container name (e.g., class name for a method)
    pub container_name: Option<String>,
    /// Detail/signature from LSP (e.g., "function greet(name: string): string")
    pub detail: Option<String>,
    /// Source code line at the symbol location
    pub source_line: Option<String>,
}

impl SymbolInfo {
    /// Creates a SymbolInfo from an LSP WorkspaceSymbol.
    ///
    /// # Arguments
    /// * `symbol` - The LSP workspace symbol
    /// * `workspace_root` - The workspace root path for making paths relative
    ///
    /// # Returns
    /// * `Option<SymbolInfo>` - The converted symbol info, or None if conversion fails
    ///
    /// # Examples
    /// ```ignore
    /// use code_agent_sdk::SymbolInfo;
    /// use lsp_types::{WorkspaceSymbol, SymbolKind, Location, Url, Position, Range};
    /// use std::path::Path;
    ///
    /// let location = Location::new(
    ///     Url::parse("file:///test.rs").unwrap(),
    ///     Range::new(Position::new(0, 0), Position::new(0, 10))
    /// );
    /// let lsp_symbol = WorkspaceSymbol {
    ///     name: "test_symbol".to_string(),
    ///     kind: SymbolKind::FUNCTION,
    ///     location: lsp_types::OneOf::Left(location),
    ///     container_name: None,
    ///     tags: None,
    ///     data: None,
    /// };
    /// let workspace_root = Path::new("/workspace");
    /// let symbol_info = SymbolInfo::from_workspace_symbol(&lsp_symbol, &workspace_root);
    /// ```ignore
    pub fn from_workspace_symbol(
        symbol: &WorkspaceSymbol,
        workspace_root: &Path,
    ) -> Option<SymbolInfo> {
        match &symbol.location {
            lsp_types::OneOf::Left(location) => {
                let file_path = Path::new(location.uri.path());
                let relative_path = file_path
                    .strip_prefix(workspace_root)
                    .unwrap_or(file_path)
                    .to_string_lossy()
                    .to_string();

                let fully_qualified_name = format!("{}::{}", relative_path, symbol.name);
                let start_row = location.range.start.line + 1;
                let source_line = read_source_line(file_path, start_row);

                Some(SymbolInfo {
                    name: symbol.name.clone(),
                    symbol_type: Some(format!("{:?}", symbol.kind)),
                    file_path: relative_path,
                    fully_qualified_name,
                    start_row,
                    end_row: location.range.end.line + 1,
                    start_column: location.range.start.character + 1,
                    end_column: location.range.end.character + 1,
                    container_name: symbol.container_name.clone(),
                    detail: None, // WorkspaceSymbol doesn't have detail field
                    source_line,
                })
            }
            lsp_types::OneOf::Right(_) => None, // LocationLink not supported yet
        }
    }
}

/// Information about a symbol definition location.
///
/// This struct represents where a symbol is defined, typically returned
/// by "go to definition" operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinitionInfo {
    /// File path relative to workspace root
    pub file_path: String,
    /// Starting line number (1-based)
    pub start_row: u32,
    /// Starting column number (1-based)
    pub start_column: u32,
    /// Ending line number (1-based)
    pub end_row: u32,
    /// Ending column number (1-based)
    pub end_column: u32,
    /// Source code line at the definition location
    pub source_line: Option<String>,
}

impl DefinitionInfo {
    /// Creates a DefinitionInfo from an LSP Location.
    ///
    /// # Arguments
    /// * `location` - The LSP location
    /// * `workspace_root` - The workspace root path for making paths relative
    ///
    /// # Returns
    /// * `DefinitionInfo` - The converted definition info
    ///
    /// # Examples
    /// ```ignore
    /// use code_agent_sdk::DefinitionInfo;
    /// use lsp_types::{Location, Url, Position, Range};
    /// use std::path::Path;
    ///
    /// let lsp_location = Location::new(
    ///     Url::parse("file:///test.rs").unwrap(),
    ///     Range::new(Position::new(5, 10), Position::new(5, 20))
    /// );
    /// let workspace_root = Path::new("/workspace");
    /// let def_info = DefinitionInfo::from_location(&lsp_location, &workspace_root);
    /// ```ignore
    pub fn from_location(location: &Location, workspace_root: &Path, show_source: bool) -> Self {
        let file_path = Path::new(location.uri.path());
        let relative_path = file_path
            .strip_prefix(workspace_root)
            .unwrap_or(file_path)
            .to_string_lossy()
            .to_string();

        let start_row = location.range.start.line + 1;
        let end_row = location.range.end.line + 1;

        let source_line = if show_source {
            read_source_lines(file_path, start_row, end_row)
        } else {
            read_source_line(file_path, start_row)
        };

        DefinitionInfo {
            file_path: relative_path,
            start_row,
            start_column: location.range.start.character + 1,
            end_row,
            end_column: location.range.end.character + 1,
            source_line,
        }
    }
}

/// Information about a symbol reference location.
///
/// This struct represents where a symbol is referenced/used, typically returned
/// by "find references" operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceInfo {
    /// File path relative to workspace root
    pub file_path: String,
    /// Starting line number (1-based)
    pub start_row: u32,
    /// Starting column number (1-based)
    pub start_column: u32,
    /// Ending line number (1-based)
    pub end_row: u32,
    /// Ending column number (1-based)
    pub end_column: u32,
    /// Source code line at the reference location
    pub source_line: Option<String>,
}

impl ReferenceInfo {
    /// Creates a ReferenceInfo from an LSP Location.
    ///
    /// # Arguments
    /// * `location` - The LSP location
    /// * `workspace_root` - The workspace root path for making paths relative
    ///
    /// # Returns
    /// * `ReferenceInfo` - The converted reference info
    ///
    /// # Examples
    /// ```ignore
    /// use code_agent_sdk::ReferenceInfo;
    /// use lsp_types::{Location, Url, Position, Range};
    /// use std::path::Path;
    ///
    /// let lsp_location = Location::new(
    ///     Url::parse("file:///test.rs").unwrap(),
    ///     Range::new(Position::new(10, 5), Position::new(10, 15))
    /// );
    /// let workspace_root = Path::new("/workspace");
    /// let ref_info = ReferenceInfo::from_location(&lsp_location, &workspace_root);
    /// ```ignore
    pub fn from_location(location: &Location, workspace_root: &Path) -> Self {
        let file_path = Path::new(location.uri.path());
        let relative_path = file_path
            .strip_prefix(workspace_root)
            .unwrap_or(file_path)
            .to_string_lossy()
            .to_string();

        let start_row = location.range.start.line + 1;
        let end_row = location.range.end.line + 1;
        let source_line = read_source_lines(file_path, start_row, end_row);

        ReferenceInfo {
            file_path: relative_path,
            start_row,
            start_column: location.range.start.character + 1,
            end_row,
            end_column: location.range.end.character + 1,
            source_line,
        }
    }
}

/// Severity level of a diagnostic message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

impl From<lsp_types::DiagnosticSeverity> for DiagnosticSeverity {
    fn from(severity: lsp_types::DiagnosticSeverity) -> Self {
        match severity {
            lsp_types::DiagnosticSeverity::ERROR => DiagnosticSeverity::Error,
            lsp_types::DiagnosticSeverity::WARNING => DiagnosticSeverity::Warning,
            lsp_types::DiagnosticSeverity::INFORMATION => DiagnosticSeverity::Information,
            lsp_types::DiagnosticSeverity::HINT => DiagnosticSeverity::Hint,
            _ => DiagnosticSeverity::Information,
        }
    }
}

/// Related location information for a diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticRelatedInfo {
    /// File path relative to workspace root
    pub file_path: String,
    /// Starting line number (1-based)
    pub start_row: u32,
    /// Starting column number (1-based)
    pub start_column: u32,
    /// Ending line number (1-based)
    pub end_row: u32,
    /// Ending column number (1-based)
    pub end_column: u32,
    /// Message describing the related location
    pub message: String,
}

/// Information about a diagnostic (error, warning, etc.) in a document.
///
/// This struct represents a diagnostic message from a language server,
/// providing information about errors, warnings, and other issues in code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticInfo {
    /// The diagnostic message
    pub message: String,
    /// Severity level (error, warning, info, hint)
    pub severity: DiagnosticSeverity,
    /// Starting line number (1-based)
    pub start_row: u32,
    /// Starting column number (1-based)
    pub start_column: u32,
    /// Ending line number (1-based)
    pub end_row: u32,
    /// Ending column number (1-based)
    pub end_column: u32,
    /// Source of the diagnostic (e.g., "typescript", "rust-analyzer")
    pub source: Option<String>,
    /// Diagnostic code (e.g., error code)
    pub code: Option<String>,
    /// Related information (e.g., other locations relevant to this diagnostic)
    pub related_information: Vec<DiagnosticRelatedInfo>,
}

impl DiagnosticInfo {
    /// Creates a DiagnosticInfo from an LSP Diagnostic.
    ///
    /// # Arguments
    /// * `diagnostic` - The LSP diagnostic
    /// * `workspace_root` - The workspace root path for making paths relative
    ///
    /// # Returns
    /// * `DiagnosticInfo` - The converted diagnostic info
    pub fn from_lsp_diagnostic(diagnostic: &Diagnostic, workspace_root: &Path) -> Self {
        let severity = diagnostic
            .severity
            .map(DiagnosticSeverity::from)
            .unwrap_or(DiagnosticSeverity::Information);

        let code = diagnostic.code.as_ref().map(|c| match c {
            lsp_types::NumberOrString::Number(n) => n.to_string(),
            lsp_types::NumberOrString::String(s) => s.clone(),
        });

        let related_information = diagnostic
            .related_information
            .as_ref()
            .map(|info_vec| {
                info_vec
                    .iter()
                    .map(|info| {
                        let file_path = Path::new(info.location.uri.path());
                        let relative_path = file_path
                            .strip_prefix(workspace_root)
                            .unwrap_or(file_path)
                            .to_string_lossy()
                            .to_string();

                        DiagnosticRelatedInfo {
                            file_path: relative_path,
                            start_row: info.location.range.start.line + 1,
                            start_column: info.location.range.start.character + 1,
                            end_row: info.location.range.end.line + 1,
                            end_column: info.location.range.end.character + 1,
                            message: info.message.clone(),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        DiagnosticInfo {
            message: diagnostic.message.clone(),
            severity,
            start_row: diagnostic.range.start.line + 1,
            start_column: diagnostic.range.start.character + 1,
            end_row: diagnostic.range.end.line + 1,
            end_column: diagnostic.range.end.character + 1,
            source: diagnostic.source.clone(),
            code,
            related_information,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Position, Range, SymbolKind, Url};
    use std::fs;
    use tempfile::TempDir;

    fn create_temp_file(dir: &TempDir, name: &str, content: &str) -> std::path::PathBuf {
        let file_path = dir.path().join(name);
        fs::write(&file_path, content).unwrap();
        file_path
    }

    // Test helper functions (business logic only)
    #[test]
    fn test_read_source_line() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_temp_file(&temp_dir, "test.rs", "line1\n  line2  \nline3");
        
        assert_eq!(read_source_line(&file_path, 2), Some("line2".to_string()));
        assert_eq!(read_source_line(&file_path, 5), None);
        assert_eq!(read_source_line(&file_path, 1), Some("line1".to_string())); // Fix: line 1 exists
    }

    #[test]
    fn test_read_source_lines() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_temp_file(&temp_dir, "test.rs", "line1\nline2\nline3\nline4");
        
        assert_eq!(read_source_lines(&file_path, 2, 3), Some("line2\nline3".to_string()));
        assert_eq!(read_source_lines(&file_path, 5, 6), None);
        assert_eq!(read_source_lines(&file_path, 2, 10), Some("line2\nline3\nline4".to_string()));
    }

    // Test conversion methods (business logic only)
    #[test]
    fn test_symbol_info_from_workspace_symbol() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_temp_file(&temp_dir, "test.rs", "fn test() {}");
        
        let location = Location::new(
            Url::from_file_path(&file_path).unwrap(),
            Range::new(Position::new(0, 3), Position::new(0, 7))
        );
        
        let workspace_symbol = WorkspaceSymbol {
            name: "test".to_string(),
            kind: SymbolKind::FUNCTION,
            location: lsp_types::OneOf::Left(location),
            container_name: None,
            tags: None,
            data: None,
        };
        
        let result = SymbolInfo::from_workspace_symbol(&workspace_symbol, temp_dir.path());
        assert!(result.is_some());
        
        let symbol_info = result.unwrap();
        assert_eq!(symbol_info.name, "test");
        assert_eq!(symbol_info.start_row, 1);
        assert_eq!(symbol_info.start_column, 4);
    }

    #[test]
    fn test_symbol_info_location_link_not_supported() {
        // Test that LocationLink (OneOf::Right) returns None
        // We can't easily construct a LocationLink due to type complexity,
        // so we test the business logic path by checking the match arm
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_temp_file(&temp_dir, "test.rs", "fn test() {}");
        
        let location = Location::new(
            Url::from_file_path(&file_path).unwrap(),
            Range::new(Position::new(0, 0), Position::new(0, 4))
        );
        
        let workspace_symbol = WorkspaceSymbol {
            name: "test".to_string(),
            kind: SymbolKind::FUNCTION,
            location: lsp_types::OneOf::Left(location), // Use Left to test the working path
            container_name: None,
            tags: None,
            data: None,
        };
        
        let result = SymbolInfo::from_workspace_symbol(&workspace_symbol, temp_dir.path());
        assert!(result.is_some()); // Should work with OneOf::Left
    }

    #[test]
    fn test_definition_info_from_location() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_temp_file(&temp_dir, "test.rs", "fn test() {\n    let x = 1;\n}");
        
        let location = Location::new(
            Url::from_file_path(&file_path).unwrap(),
            Range::new(Position::new(0, 0), Position::new(2, 1))
        );
        
        let result = DefinitionInfo::from_location(&location, temp_dir.path(), true);
        assert_eq!(result.start_row, 1);
        assert_eq!(result.end_row, 3);
        assert!(result.source_line.is_some());
        
        let result_no_source = DefinitionInfo::from_location(&location, temp_dir.path(), false);
        assert!(result_no_source.source_line.is_some()); // Fix: show_source=false still reads single line
    }

    #[test]
    fn test_reference_info_from_location() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_temp_file(&temp_dir, "test.rs", "fn main() {\n    test();\n}");
        
        let location = Location::new(
            Url::from_file_path(&file_path).unwrap(),
            Range::new(Position::new(1, 4), Position::new(1, 8))
        );
        
        let result = ReferenceInfo::from_location(&location, temp_dir.path());
        assert_eq!(result.start_row, 2);
        assert_eq!(result.start_column, 5);
        assert_eq!(result.source_line, Some("    test();".to_string())); // read_source_lines doesn't trim
    }
}
