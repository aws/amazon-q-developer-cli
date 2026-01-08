//! Symbol extraction and analysis
//!
//! Handles parsing individual files and extracting symbol information using TreeSitter.

use std::path::Path;

use ast_grep_language::{
    LanguageExt,
    SupportLang,
};

use crate::model::entities::SymbolInfo;
use crate::tree_sitter::get_symbol_def;

/// Extract symbols from a single file
pub fn parse_file_symbols(path: &Path, workspace_root: &Path, lang: &SupportLang, lang_name: &str) -> Vec<SymbolInfo> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let root = lang.ast_grep(&content);
    extract_symbols_from_root(&root.root(), lang_name, path, workspace_root)
}

/// Extract symbols from already-parsed AST root (avoids re-parsing)
pub fn extract_symbols_from_root<D: ast_grep_core::Doc>(
    root: &ast_grep_core::Node<'_, D>,
    lang_name: &str,
    path: &Path,
    workspace_root: &Path,
) -> Vec<SymbolInfo> {
    let relative_path = path
        .strip_prefix(workspace_root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    let mut symbols = Vec::new();
    extract_symbols(root, lang_name, &relative_path, &mut symbols);
    symbols
}

/// Extract symbols from AST nodes using iterative traversal (avoids stack overflow)
fn extract_symbols<D: ast_grep_core::Doc>(
    root: &ast_grep_core::Node<'_, D>,
    lang_name: &str,
    relative_path: &str,
    symbols: &mut Vec<SymbolInfo>,
) {
    // Stack holds (node, parent_symbol_name)
    let mut stack: Vec<(ast_grep_core::Node<'_, D>, Option<String>)> = vec![(root.clone(), None)];

    while let Some((node, container)) = stack.pop() {
        let kind = node.kind();

        let current_container = if let Some(def) = get_symbol_def(lang_name, &kind)
            && let Some(name) = find_name(&node, &def.name_child)
        {
            let (start_line, start_col) = node.start_pos().byte_point();
            let (end_line, end_col) = node.end_pos().byte_point();

            // Get source line from node text (first line, trimmed)
            let source_line = node.text().lines().next().map(|l| l.trim_end().to_string());

            symbols.push(SymbolInfo {
                name: name.clone(),
                symbol_type: Some(def.symbol_type.clone()),
                file_path: relative_path.to_string(),
                start_row: start_line as u32 + 1,
                end_row: end_line as u32 + 1,
                start_column: start_col as u32 + 1,
                end_column: end_col as u32 + 1,
                container_name: container.clone(),
                detail: None,
                source_line,
                source_code: None,
                language: Some(lang_name.to_string()),
            });
            Some(name)
        } else {
            container.clone()
        };

        // Push children with current container context
        for child in node.children() {
            stack.push((child, current_container.clone()));
        }
    }
}

/// Find name child using iterative traversal (avoids stack overflow)
pub fn find_name<D: ast_grep_core::Doc>(root: &ast_grep_core::Node<'_, D>, target_kind: &str) -> Option<String> {
    let mut stack = vec![root.clone()];

    while let Some(node) = stack.pop() {
        for child in node.children() {
            if child.kind() == target_kind {
                return Some(child.text().to_string());
            }
            stack.push(child);
        }
    }
    None
}
