//! Pattern search and matching operations
//!
//! Handles AST-based pattern search using ast-grep syntax for structural code analysis.
//!
//! ## Pattern Syntax
//!
//! Uses ast-grep pattern syntax with variables and wildcards:
//! - `$VAR` - Matches a single AST node (e.g., `$X.unwrap()` matches `foo.unwrap()`)
//! - `$$$` - Matches zero or more nodes (e.g., `fn($$$)` matches any function call)
//! - Literal code - Matches exact syntax (e.g., `unwrap()` matches only `unwrap()`)
//!
//! ## Examples
//!
//! ```ignore
//! // Find all unwrap calls
//! pattern: "$X.unwrap()"
//! matches: foo.unwrap(), bar.unwrap(), etc.
//!
//! // Find async function definitions  
//! pattern: "async fn $NAME($$$)"
//! matches: async fn handler(req: Request), async fn process(), etc.
//!
//! // Find error handling patterns
//! pattern: "if let Err($E) = $X { $$$ }"
//! matches: if let Err(e) = result { return; }, etc.
//! ```

use std::path::Path;
use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};
use std::sync::{
    Arc,
    Mutex,
};

use anyhow::Result;
use ast_grep_core::matcher::Pattern;
use ast_grep_language::{
    LanguageExt,
    SupportLang,
};
use ignore::WalkState;

use crate::model::entities::{
    EnclosingSymbol,
    PatternMatch,
    SourceRange,
};
use crate::model::types::PatternSearchRequest;
use crate::sdk::WorkspaceManager;
use crate::tree_sitter::get_symbol_def;
use crate::tree_sitter::symbol_extractor::find_name;
use crate::utils::traversal::create_code_walker;

/// Maximum files to process in pattern operations
const MAX_FILES_LIMIT: usize = 100_000;

/// Search for AST patterns across workspace
pub async fn search_pattern(
    workspace_manager: &mut WorkspaceManager,
    request: &PatternSearchRequest,
) -> Result<Vec<PatternMatch>> {
    let workspace_root = workspace_manager.workspace_root();
    pattern_search(workspace_root, request.clone()).await
}

/// Search for AST patterns across workspace or single file
pub async fn pattern_search(workspace_root: &Path, request: PatternSearchRequest) -> Result<Vec<PatternMatch>> {
    use tokio_util::sync::CancellationToken;

    let cancel_token = CancellationToken::new();
    let cancel_check = cancel_token.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("Ctrl+C received, cancelling pattern search");
        cancel_check.cancel();
    });

    let lang: SupportLang = request
        .language
        .parse()
        .map_err(|_| anyhow::anyhow!("Unsupported language: {}", request.language))?;
    let lang_name = request.language.to_lowercase();
    let offset = request.offset.unwrap_or(0) as usize;
    let limit = request.limit.unwrap_or(crate::model::types::DEFAULT_SEARCH_RESULTS) as usize;

    let pattern = Pattern::try_new(&request.pattern, lang)
        .map_err(|e| anyhow::anyhow!("Invalid pattern '{}': {}", request.pattern, e))?;

    let extensions: Vec<String> = get_extensions_for_lang(&lang_name)
        .iter()
        .map(|s| s.to_string())
        .collect();

    // Determine search root
    let search_root = if let Some(ref file_path) = request.file_path {
        let path = std::path::PathBuf::from(file_path);
        let path = if path.exists() {
            path
        } else {
            workspace_root.join(file_path)
        };
        if !path.exists() {
            return Err(anyhow::anyhow!("Path not found: {file_path}"));
        }

        // If it's a file, search just that file
        if path.is_file() {
            let matches = search_file(&path, workspace_root, &pattern, &lang, &lang_name);
            return Ok(matches.into_iter().skip(offset).take(limit).collect());
        }

        path
    } else {
        workspace_root.to_path_buf()
    };

    // Parallel directory traversal with file limits
    let all_matches: Mutex<Vec<PatternMatch>> = Mutex::new(Vec::new());
    let processed_files = Arc::new(AtomicUsize::new(0));

    let walker = create_code_walker(&search_root, None).build_parallel();

    walker.run(|| {
        let pattern = &pattern;
        let lang = &lang;
        let lang_name = &lang_name;
        let workspace_root = workspace_root.to_path_buf();
        let extensions = &extensions;
        let all_matches = &all_matches;
        let processed_files = &processed_files;
        let cancel_token = cancel_token.clone();

        Box::new(move |entry| {
            // Check cancellation
            if cancel_token.is_cancelled() {
                return WalkState::Quit;
            }

            // Check file limit
            let current_files = processed_files.load(Ordering::Relaxed);
            if current_files >= MAX_FILES_LIMIT {
                tracing::warn!("Reached maximum file limit: {}", MAX_FILES_LIMIT);
                return WalkState::Quit;
            }

            let entry = match entry {
                Ok(e) => e,
                Err(_) => return WalkState::Continue,
            };

            let path = entry.path();
            if !path.is_file() {
                return WalkState::Continue;
            }

            // Filter by extension
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)) {
                return WalkState::Continue;
            }

            let matches = search_file(path, &workspace_root, pattern, lang, lang_name);
            if !matches.is_empty() {
                all_matches.lock().unwrap().extend(matches);
            }

            processed_files.fetch_add(1, Ordering::Relaxed);
            WalkState::Continue
        })
    });

    if cancel_token.is_cancelled() {
        return Err(anyhow::anyhow!("Pattern search was cancelled"));
    }

    let matches = all_matches.into_inner().unwrap();
    Ok(matches.into_iter().skip(offset).take(limit).collect())
}

/// Search for patterns in a single file
fn search_file(
    path: &Path,
    workspace_root: &Path,
    pattern: &Pattern,
    lang: &SupportLang,
    lang_name: &str,
) -> Vec<PatternMatch> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let root = lang.ast_grep(&content);
    let relative_path = path
        .strip_prefix(workspace_root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    root.root()
        .find_all(pattern)
        .map(|node_match| {
            let (start_line, start_col) = node_match.start_pos().byte_point();
            let (end_line, end_col) = node_match.end_pos().byte_point();
            let enclosing_symbols = find_enclosing_symbols(&node_match, lang_name);

            PatternMatch {
                file_path: relative_path.clone(),
                matched_code: node_match.text().to_string(),
                start_row: start_line as u32 + 1,
                start_column: start_col as u32 + 1,
                end_row: end_line as u32 + 1,
                end_column: end_col as u32 + 1,
                enclosing_symbols,
            }
        })
        .collect()
}

/// Find enclosing symbols for pattern match context
fn find_enclosing_symbols<D: ast_grep_core::Doc>(
    node: &ast_grep_core::Node<'_, D>,
    lang: &str,
) -> Vec<EnclosingSymbol> {
    let mut symbols = Vec::new();
    let mut current = node.clone();

    while let Some(parent) = current.parent() {
        let kind = parent.kind();
        if let Some(def) = get_symbol_def(lang, &kind)
            && let Some(name) = find_name(&parent, &def.name_child)
        {
            let (start_line, start_col) = parent.start_pos().byte_point();
            let (end_line, end_col) = parent.end_pos().byte_point();

            symbols.push(EnclosingSymbol {
                name: name.clone(),
                kind: def.symbol_type.clone(),
                range: SourceRange::new(start_line, start_col, end_line, end_col),
            });
        }
        current = parent;
    }

    symbols
}

/// Find name child recursively
/// Get extensions for language
fn get_extensions_for_lang(lang: &str) -> Vec<&'static str> {
    crate::tree_sitter::get_extensions(lang)
        .map(|exts| exts.iter().map(|s| s.as_str()).collect())
        .unwrap_or_default()
}
