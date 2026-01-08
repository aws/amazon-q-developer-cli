//! Workspace analysis and overview generation
//!
//! Handles high-level codebase analysis with proper type safety.

use std::collections::HashMap;
use std::path::{
    Path,
    PathBuf,
};
use std::sync::Arc;
use std::sync::atomic::{
    AtomicBool,
    AtomicUsize,
    Ordering,
};
use std::time::{
    Duration,
    Instant,
};

use anyhow::Result;
use ast_grep_language::{
    LanguageExt,
    SupportLang,
};
use tokio_util::sync::CancellationToken;

use crate::model::entities::SymbolInfo;
use crate::model::types::{
    CodebaseMapResponse,
    CodebaseOverviewResponse,
    CodebaseOverviewSummary,
    FindSymbolsRequest,
    GenerateCodebaseOverviewRequest,
    SearchCodebaseMapRequest,
};
use crate::sdk::WorkspaceManager;
use crate::tree_sitter::{
    get_call_node_kinds,
    get_import_node_kinds,
    lang_from_extension,
    symbol_extractor,
};
use crate::utils::scoring::calculate_fuzzy_score;
use crate::utils::traversal::create_code_walker;

const MAX_FILES_LIMIT: usize = 1_000_000;
const DEFAULT_TOKEN_BUDGET: usize = 40_000;
const DEFAULT_CODEBASE_MAP_TOKEN_BUDGET: usize = 15_000;

/// Find symbols with timeout handling
pub async fn find_symbols_with_timeout(
    workspace_manager: &mut WorkspaceManager,
    request: &FindSymbolsRequest,
    timeout_secs: u64,
) -> Result<Vec<SymbolInfo>> {
    let workspace_root = workspace_manager.workspace_root().to_path_buf();
    let code_store = workspace_manager.code_store().clone();
    let request = request.clone();

    tokio::select! {
        result = tokio::task::spawn_blocking(move || {
            find_symbols_sync(&workspace_root, &code_store, &request, timeout_secs)
        }) => result?,
        _ = tokio::signal::ctrl_c() => Err(anyhow::anyhow!("Symbol search was cancelled")),
        _ = tokio::time::sleep(Duration::from_secs(timeout_secs)) => {
            Err(anyhow::anyhow!("Symbol search timed out after {timeout_secs} seconds"))
        }
    }
}

/// Synchronous symbol search (runs in blocking task)
fn find_symbols_sync(
    workspace_root: &Path,
    code_store: &Arc<crate::sdk::CodeStore>,
    request: &FindSymbolsRequest,
    timeout_secs: u64,
) -> Result<Vec<SymbolInfo>> {
    let start = Instant::now();
    let query = &request.symbol_name;
    let query_lower = request.symbol_name.to_lowercase();
    let limit = request.limit.unwrap_or(crate::model::types::DEFAULT_SEARCH_RESULTS) as usize;

    let mut all_symbols = Vec::new();
    let mut processed_files = 0;

    let walker = create_code_walker(workspace_root, None).build();

    for entry in walker {
        if start.elapsed().as_secs() >= timeout_secs {
            break;
        }

        if processed_files >= MAX_FILES_LIMIT {
            break;
        }

        let entry = entry?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let lang_name = match lang_from_extension(ext) {
            Some(lang) => lang,
            None => continue,
        };

        // Filter by language if specified
        if let Some(target_lang) = &request.language
            && !lang_name.eq_ignore_ascii_case(target_lang)
        {
            continue;
        }

        // Try cache first
        let file_symbols = if let Some(cached) = code_store.get_cached_symbols(path) {
            cached
        } else {
            let lang: ast_grep_language::SupportLang = match lang_name.parse() {
                Ok(l) => l,
                Err(_) => continue,
            };
            let symbols =
                crate::tree_sitter::symbol_extractor::parse_file_symbols(path, workspace_root, &lang, lang_name);
            code_store.cache_symbols(path, symbols.clone());
            symbols
        };

        // Apply fuzzy matching
        for symbol in file_symbols {
            let score = calculate_fuzzy_score(&query_lower, &symbol.name.to_lowercase(), query, &symbol.name);
            if score >= 0.3 {
                all_symbols.push((score, symbol));
            }
        }

        processed_files += 1;
    }

    // Sort by score and take top results
    all_symbols.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    Ok(all_symbols.into_iter().take(limit).map(|(_, s)| s).collect())
}

/// Generate codebase overview with proper types
pub async fn generate_overview(
    workspace_manager: &mut WorkspaceManager,
    request: &GenerateCodebaseOverviewRequest,
) -> Result<CodebaseOverviewResponse> {
    let workspace_root = request
        .path
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_manager.workspace_root().to_path_buf());
    let token_budget = request.token_budget.unwrap_or(DEFAULT_TOKEN_BUDGET);
    let timeout_secs = request
        .timeout_secs
        .unwrap_or(crate::model::types::DEFAULT_TIMEOUT_SECS);

    let workspace_root_clone = workspace_root.clone();
    tokio::select! {
        result = tokio::task::spawn_blocking(move || {
            let start = Instant::now();
            let cancel_token = CancellationToken::new();
            let (summary, packages, truncated) =
                analyze_workspace_with_packages(&workspace_root_clone, token_budget, start, timeout_secs, cancel_token)?;
            Ok::<_, anyhow::Error>((summary, packages, truncated, workspace_root_clone))
        }) => {
            let (summary, packages, truncated, ws_path) = result??;
            Ok(CodebaseOverviewResponse {
                workspace_path: ws_path.to_string_lossy().to_string(),
                size_category: determine_size_category(summary.total_files),
                summary,
                symbol_key: create_symbol_key(),
                truncated,
                packages,
            })
        }
        _ = tokio::signal::ctrl_c() => Err(anyhow::anyhow!("Overview generation was cancelled")),
        _ = tokio::time::sleep(Duration::from_secs(timeout_secs)) => {
            Err(anyhow::anyhow!("Overview generation timed out after {timeout_secs} seconds"))
        }
    }
}

/// Search codebase map with proper types
pub async fn search_codebase_map(
    workspace_manager: &mut WorkspaceManager,
    request: &SearchCodebaseMapRequest,
) -> Result<CodebaseMapResponse> {
    let workspace_root = workspace_manager.workspace_root().to_path_buf();
    let timeout_secs = request
        .timeout_secs
        .unwrap_or(crate::model::types::DEFAULT_TIMEOUT_SECS);

    // Determine search path
    let search_path = if let Some(path) = &request.path {
        workspace_root.join(path)
    } else {
        workspace_root.clone()
    };

    let token_budget = request.token_budget.unwrap_or(DEFAULT_CODEBASE_MAP_TOKEN_BUDGET);
    let file_filter = request.file_path.clone();

    tokio::select! {
        result = tokio::task::spawn_blocking(move || {
            let start = Instant::now();
            let cancel_token = CancellationToken::new();
            generate_condensed_map(&search_path, file_filter.as_deref(), token_budget, start, timeout_secs, cancel_token)
        }) => {
            let (condensed_map, files_processed, token_count) = result??;
            Ok(CodebaseMapResponse {
                condensed_repomap: condensed_map,
                files_processed,
                token_count,
                truncated: false,
            })
        }
        _ = tokio::signal::ctrl_c() => Err(anyhow::anyhow!("Codebase map generation was cancelled")),
        _ = tokio::time::sleep(Duration::from_secs(timeout_secs)) => {
            Err(anyhow::anyhow!("Codebase map generation timed out after {timeout_secs} seconds"))
        }
    }
}

/// Analyze workspace and build packages map with per-file symbols
fn analyze_workspace_with_packages(
    workspace_root: &Path,
    token_budget: usize,
    start: Instant,
    timeout_secs: u64,
    cancel_token: CancellationToken,
) -> Result<(CodebaseOverviewSummary, crate::model::types::PackageBreakdown, bool)> {
    use std::sync::Mutex;

    use ignore::WalkState;
    use rayon::prelude::*;

    use crate::model::types::FileAnalysis;

    // Parallel file discovery
    let code_files = Arc::new(Mutex::new(Vec::new()));
    let total_files = Arc::new(AtomicUsize::new(0));
    let timed_out = Arc::new(AtomicBool::new(false));
    let hit_file_limit = Arc::new(AtomicBool::new(false));

    create_code_walker(workspace_root, None).build_parallel().run(|| {
        let timed_out = timed_out.clone();
        let total_files = total_files.clone();
        let code_files = code_files.clone();
        let hit_file_limit = hit_file_limit.clone();
        let cancel_token = cancel_token.clone();
        Box::new(move |entry| {
            // Check cancellation
            if cancel_token.is_cancelled() {
                tracing::info!("Cancellation detected in file traversal");
                return WalkState::Quit;
            }

            // Check timeout
            if start.elapsed().as_secs() >= timeout_secs {
                timed_out.store(true, Ordering::Relaxed);
                return WalkState::Quit;
            }

            if let Ok(e) = entry
                && e.path().is_file()
            {
                let current = total_files.fetch_add(1, Ordering::Relaxed);
                // Check file limit
                if current >= MAX_FILES_LIMIT {
                    hit_file_limit.store(true, Ordering::Relaxed);
                    return WalkState::Quit;
                }
                if let Some(ext) = e.path().extension().and_then(|x| x.to_str())
                    && lang_from_extension(ext).is_some()
                {
                    code_files
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .push(e.path().to_path_buf());
                }
            }
            WalkState::Continue
        })
    });

    // Check if timed out during traversal
    if timed_out.load(Ordering::Relaxed) {
        tracing::warn!("Overview generation timed out during file traversal");
        return Err(anyhow::anyhow!(
            "Overview generation timed out after {timeout_secs} seconds"
        ));
    }

    let code_files = Arc::try_unwrap(code_files)
        .map(|mutex| mutex.into_inner().unwrap_or_else(|e| e.into_inner()))
        .unwrap_or_else(|arc| arc.lock().unwrap_or_else(|e| e.into_inner()).clone());
    let total_files = total_files.load(Ordering::Relaxed);
    let prioritized_files = code_files.len();

    // Parallel processing - single AST parse per file
    let cancel_for_rayon = cancel_token.clone();
    let cancelled_flag = Arc::new(AtomicBool::new(false));
    let file_data: Vec<_> = code_files
        .par_iter()
        .take(MAX_FILES_LIMIT)
        .filter_map(|path| {
            // Check cancellation first
            if cancel_for_rayon.is_cancelled() {
                if !cancelled_flag.swap(true, Ordering::Relaxed) {
                    tracing::info!("Cancellation detected, stopping parallel processing");
                }
                return None;
            }

            // Check timeout before processing
            if start.elapsed().as_secs() >= timeout_secs {
                return None;
            }

            let ext = path.extension()?.to_str()?;
            let lang_name = lang_from_extension(ext)?;

            // Check cancellation before expensive I/O
            if cancel_for_rayon.is_cancelled() {
                return None;
            }

            // Check timeout before expensive I/O
            if start.elapsed().as_secs() >= timeout_secs {
                return None;
            }
            let content = std::fs::read_to_string(path).ok()?;
            let loc = content.lines().count();

            // Check cancellation before expensive AST parsing
            if cancel_for_rayon.is_cancelled() {
                return None;
            }

            // Check timeout before expensive AST parsing
            if start.elapsed().as_secs() >= timeout_secs {
                return None;
            }
            let lang: SupportLang = lang_name.parse().ok()?;
            let root = lang.ast_grep(&content);

            // Extract everything from single AST parse
            let symbols = symbol_extractor::extract_symbols_from_root(&root.root(), lang_name, path, workspace_root);
            let calls = count_nodes(&root.root(), &get_call_node_kinds(lang_name));
            let imports = count_nodes(&root.root(), &get_import_node_kinds(lang_name));

            // Calculate score
            let mut score = symbols.len() as f64;
            for sym in &symbols {
                if let Some(t) = sym.symbol_type.as_deref() {
                    match t {
                        "Class" | "Struct" | "Interface" | "Trait" | "Enum" => score += 2.0,
                        "Constant" | "Static" | "Variable" => score += 0.5,
                        _ => {},
                    }
                }
            }
            score += calls as f64 * 0.5 + imports as f64 * 0.3;

            if should_deprioritize(path) {
                score *= 0.1;
            }

            let (mut funcs, mut classes) = (Vec::new(), Vec::new());
            for sym in &symbols {
                if let Some(t) = sym.symbol_type.as_deref() {
                    match categorize_symbol(t) {
                        Some("functions") => funcs.push(sym.name.clone()),
                        Some("classes") => classes.push(sym.name.clone()),
                        _ => {},
                    }
                }
            }

            let rel_path = path
                .strip_prefix(workspace_root)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();
            Some((rel_path, score, funcs, classes, loc))
        })
        .collect();

    // Aggregate
    let (mut tf, mut tc, mut total_loc) = (0, 0, 0);
    for (_, _, funcs, classes, loc) in &file_data {
        tf += funcs.len();
        tc += classes.len();
        total_loc += loc;
    }

    // Sort by score
    let mut sorted = file_data;
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Build packages with token budget
    let max_chars = token_budget * 4;
    let mut packages: crate::model::types::PackageBreakdown = HashMap::new();
    let mut est_chars = 500;

    for (rel_path, score, funcs, classes, loc) in sorted {
        let sym_chars: usize = funcs.iter().chain(classes.iter()).map(|s| s.len() + 3).sum();
        if est_chars + rel_path.len() + sym_chars + 60 > max_chars {
            break;
        }
        est_chars += rel_path.len() + sym_chars + 60;

        let p = std::path::Path::new(&rel_path);
        let dir = p.parent().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
        let file_name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();

        packages.entry(dir).or_default().insert(file_name, FileAnalysis {
            f: funcs,
            c: classes,
            loc,
            score: format!("{score:.1}"),
        });
    }

    let truncated = timed_out.load(Ordering::Relaxed) || hit_file_limit.load(Ordering::Relaxed);

    Ok((
        CodebaseOverviewSummary {
            total_files,
            prioritized_files,
            f: tf,
            c: tc,
            i: 0,
            m: 0,
            k: 0,
            loc: total_loc,
        },
        packages,
        truncated,
    ))
}

/// Determine size category based on file count
fn determine_size_category(file_count: usize) -> String {
    match file_count {
        0..=10 => "XS",
        11..=50 => "S",
        51..=200 => "M",
        201..=1000 => "L",
        1001..=5000 => "XL",
        _ => "XXL",
    }
    .to_string()
}

/// Create symbol key explanations
fn create_symbol_key() -> HashMap<String, String> {
    let mut key = HashMap::new();
    key.insert("f".to_string(), "Functions".to_string());
    key.insert("c".to_string(), "Classes/Structs/Enums".to_string());
    key.insert("i".to_string(), "Interfaces/Traits".to_string());
    key.insert("m".to_string(), "Modules".to_string());
    key.insert("k".to_string(), "Constants/Statics".to_string());
    key.insert("loc".to_string(), "Lines of Code".to_string());
    key
}

/// Generate condensed directory map with intelligent file scoring
fn generate_condensed_map(
    search_path: &Path,
    file_filter: Option<&str>,
    token_budget: usize,
    start: Instant,
    timeout_secs: u64,
    cancel_token: CancellationToken,
) -> Result<(String, usize, usize)> {
    let mut scored_files = Vec::new();
    let mut processed_files = 0;

    let walker = create_code_walker(search_path, None).build();

    for entry in walker {
        // Check cancellation
        if cancel_token.is_cancelled() {
            return Err(anyhow::anyhow!("Codebase map generation was cancelled"));
        }

        // Check timeout
        if start.elapsed().as_secs() >= timeout_secs {
            return Err(anyhow::anyhow!(
                "Codebase map generation timed out after {timeout_secs} seconds"
            ));
        }

        // Check file limit
        if processed_files >= MAX_FILES_LIMIT {
            break;
        }

        let entry = entry?;
        let path = entry.path();

        if let Some(filter) = file_filter
            && !path.to_string_lossy().contains(filter)
        {
            continue;
        }

        if path.is_file() {
            processed_files += 1;
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if lang_from_extension(ext).is_some() {
                let score = calculate_file_score(path, search_path);
                scored_files.push((path.to_path_buf(), score));
            }
        }
    }

    // Sort by score (highest first)
    scored_files.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Generate repomap with configurable token budget
    let mut repomap = String::new();
    let mut tokens = 0usize;
    let mut count = 0usize;

    for (path, score) in &scored_files {
        // Check cancellation during processing
        if cancel_token.is_cancelled() {
            break;
        }

        // Check timeout during processing
        if start.elapsed().as_secs() >= timeout_secs {
            break;
        }

        let symbols = parse_file_symbols_safe(path, search_path);
        if let Ok(r) = generate_repomap(path, &symbols, *score) {
            let t = r.len() / 4; // ~4 chars per token
            if tokens + t <= token_budget {
                repomap.push_str(&r);
                repomap.push('\n');
                count += 1;
                tokens += t;
            } else {
                break;
            }
        }
    }

    Ok((repomap, count, tokens))
}

/// Generate repomap (condensed signature-only view) for a file
fn generate_repomap(path: &Path, symbols: &[SymbolInfo], score: f64) -> Result<String> {
    let source = std::fs::read_to_string(path)?;
    let lines: Vec<&str> = source.lines().collect();
    let mut repomap = format!("// {} (score: {:.1})\n\n", path.display(), score);

    for symbol in symbols {
        let row = symbol.start_row.saturating_sub(1) as usize;
        if let Some(line) = lines.get(row) {
            let trimmed = line.trim();
            if let Some(pos) = trimmed.find('{') {
                repomap.push_str(&trimmed[..pos]);
                repomap.push_str("{}\n");
            } else {
                repomap.push_str(trimmed);
                repomap.push_str(" {}\n");
            }
        }
    }
    Ok(repomap)
}

/// Count calls in a file (outgoing calls)
pub fn count_calls(path: &Path) -> Result<usize> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let lang_name = lang_from_extension(ext).ok_or_else(|| anyhow::anyhow!("Unsupported extension: {ext}"))?;
    let lang: SupportLang = lang_name
        .parse()
        .map_err(|_| anyhow::anyhow!("Failed to parse language: {lang_name}"))?;

    let content = std::fs::read_to_string(path)?;
    let root = lang.ast_grep(&content);

    let call_kinds = get_call_node_kinds(lang_name);
    Ok(count_nodes(&root.root(), &call_kinds))
}

/// Count imports in a file (dependencies)
pub fn count_imports(path: &Path) -> Result<usize> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let lang_name = lang_from_extension(ext).ok_or_else(|| anyhow::anyhow!("Unsupported extension: {ext}"))?;
    let lang: SupportLang = lang_name
        .parse()
        .map_err(|_| anyhow::anyhow!("Failed to parse language: {lang_name}"))?;

    let content = std::fs::read_to_string(path)?;
    let root = lang.ast_grep(&content);

    let import_kinds = get_import_node_kinds(lang_name);
    Ok(count_nodes(&root.root(), &import_kinds))
}

/// Calculate file score based on symbols, calls, and imports
///
/// This scoring algorithm is based on network centrality metrics from software
/// engineering research, particularly:
///
/// > "Betweenness centrality and **out-degree** metric yield an impressive accuracy
/// > for bug prediction and test prioritization."
/// > — He et al., "Using Software Dependency to Bug Prediction" (2013)
/// > DOI: 10.1155/2013/869356
///
/// # Metrics Used
///
/// ## Out-Degree (calls × 0.5)
/// Number of outgoing function calls from this file. Files with high out-degree
/// are "orchestrators" that coordinate many other components. Research shows
/// these files:
/// - Have higher bug proneness (more complex integration logic)
/// - Change more frequently during development
/// - Are critical entry points (controllers, main modules)
///
/// ## Symbol Complexity
/// - Classes/Structs/Interfaces/Traits/Enums: +3.0 (1.0 base + 2.0 weight)
/// - Functions/Methods: +1.0 (base symbol weight)
/// - Constants/Statics/Variables: +1.5 (1.0 base + 0.5 weight)
/// - Modules: +1.0 (base only - no bonus to maintain precision)
/// - Other symbols: +1.0 (base symbol weight)
///
/// ## Import Dependencies (imports × 0.3)
/// Number of import/use statements. Higher imports indicate:
/// - Greater coupling to external modules
/// - More complex dependency management
/// - Higher maintenance burden
///
/// ## Test File Penalty (× 0.1)
/// Test files receive 10% of normal score to deprioritize them in
/// architectural analysis while still maintaining visibility.
pub fn calculate_file_score(path: &Path, workspace_root: &Path) -> f64 {
    let symbols = parse_file_symbols_safe(path, workspace_root);
    let calls = count_calls(path).unwrap_or(0);
    let imports = count_imports(path).unwrap_or(0);

    let mut score = 0.0;
    for symbol in &symbols {
        score += 1.0; // Base symbol weight
        if let Some(t) = symbol.symbol_type.as_deref() {
            match t {
                // High complexity architectural elements (+3.0 total)
                "Class" | "Struct" | "Interface" | "Trait" | "Enum" => score += 2.0,

                // Standard complexity executable elements (+1.0 total - base only)
                "Function" | "Method" => {},

                // Low complexity data elements (+1.5 total)
                "Constant" | "Static" | "Variable" => score += 0.5,

                // Modules get NO bonus (precision issue) - just base score (+1.0 total)
                "Module" => {},

                // Everything else gets base score only
                _ => {},
            }
        }
    }
    score += calls as f64 * 0.5;
    score += imports as f64 * 0.3;

    if should_deprioritize(path) {
        score *= 0.1;
    }
    score
}

/// Categorize symbol type for analysis
///
/// Note: "Variable" intentionally returns None because local variables are not
/// useful in codebase overview output. However, Variables DO contribute to file
/// scoring (+0.5) in calculate_file_score() since files with more state tend to
/// be more complex. This asymmetry is intentional.
pub fn categorize_symbol(symbol_type: &str) -> Option<&'static str> {
    match symbol_type {
        "Function" | "Method" => Some("functions"),
        "Class" | "Struct" | "Enum" => Some("classes"), // All types grouped together
        "Interface" | "Trait" => Some("interfaces"),
        "Module" => Some("modules"),
        "Constant" | "Static" => Some("constants"), // Global constants/statics only
        // "Variable" intentionally excluded - scored but not categorized (see doc comment)
        _ => None,
    }
}

/// Check if file should be deprioritized (test, generated, minified)
fn should_deprioritize(path: &Path) -> bool {
    let path_str = path.to_string_lossy().to_lowercase();
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();

    // Check if in test directory
    let in_test_dir = path_str.contains("/test/")
        || path_str.contains("/tests/")
        || path_str.contains("/tst/")
        || path_str.contains("/spec/");

    // Get filename without extension
    let name_without_ext = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();

    // Check if test file by naming convention
    let is_test_file = name_without_ext.starts_with("test")
        || name_without_ext.ends_with("test")
        || name_without_ext.ends_with("spec");

    in_test_dir
        || is_test_file
        || path_str.contains("/generated")
        || path_str.contains("generated-sources")
        || file_name.ends_with(".min.js")
        || file_name.ends_with(".min.css")
}

/// Helper function to count AST nodes iteratively (avoids stack overflow)
fn count_nodes<D: ast_grep_core::Doc>(root: &ast_grep_core::Node<'_, D>, kinds: &[&str]) -> usize {
    let mut count = 0;
    let mut stack = vec![root.clone()];

    while let Some(node) = stack.pop() {
        if kinds.iter().any(|k| *k == node.kind()) {
            count += 1;
        }
        for child in node.children() {
            stack.push(child);
        }
    }
    count
}

/// Safe symbol parsing that doesn't fail
fn parse_file_symbols_safe(path: &Path, workspace_root: &Path) -> Vec<SymbolInfo> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if let Some(lang_name) = lang_from_extension(ext)
        && let Ok(lang) = lang_name.parse::<SupportLang>()
    {
        return symbol_extractor::parse_file_symbols(path, workspace_root, &lang, lang_name);
    }
    Vec::new()
}
#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_count_calls() {
        let temp_dir = TempDir::new().unwrap();
        let rust_file = temp_dir.path().join("test.rs");
        fs::write(
            &rust_file,
            r#"
fn main() {
    println!("hello");
    foo();
    bar();
}
fn foo() {}
fn bar() {}
"#,
        )
        .unwrap();

        let calls = count_calls(&rust_file).unwrap();
        println!("Found {} calls", calls);
        assert!(calls >= 2); // foo(), bar() - println! might not be detected as call
    }

    #[test]
    fn test_count_imports() {
        let temp_dir = TempDir::new().unwrap();
        let rust_file = temp_dir.path().join("test.rs");
        fs::write(
            &rust_file,
            r#"
use std::collections::HashMap;
use serde::Serialize;

fn main() {}
"#,
        )
        .unwrap();

        let imports = count_imports(&rust_file).unwrap();
        assert_eq!(imports, 2);
    }

    #[test]
    fn test_calculate_file_score() {
        let temp_dir = TempDir::new().unwrap();

        // High score file with functions and calls
        let high_score_file = temp_dir.path().join("high.rs");
        fs::write(
            &high_score_file,
            r#"
use std::collections::HashMap;

struct MyStruct {}

impl MyStruct {
    fn method1(&self) { self.method2(); }
    fn method2(&self) { println!("test"); }
}

fn main() {
    let s = MyStruct{};
    s.method1();
}
"#,
        )
        .unwrap();

        // Low score file
        let low_score_file = temp_dir.path().join("low.rs");
        fs::write(
            &low_score_file,
            r#"
// Just a comment
"#,
        )
        .unwrap();

        let high_score = calculate_file_score(&high_score_file, temp_dir.path());
        let low_score = calculate_file_score(&low_score_file, temp_dir.path());

        assert!(high_score > low_score);
        assert!(high_score > 5.0); // Should have struct + methods + calls + imports
    }

    #[test]
    fn test_test_file_penalty() {
        let temp_dir = TempDir::new().unwrap();

        let normal_file = temp_dir.path().join("normal.rs");
        let test_file = temp_dir.path().join("tests").join("test_file.rs");

        // Create tests directory
        fs::create_dir_all(test_file.parent().unwrap()).unwrap();

        let code = r#"
struct TestStruct {}
impl TestStruct {
    fn test_method(&self) {}
}
"#;

        fs::write(&normal_file, code).unwrap();
        fs::write(&test_file, code).unwrap();

        let normal_score = calculate_file_score(&normal_file, temp_dir.path());
        let test_score = calculate_file_score(&test_file, temp_dir.path());

        println!("Normal score: {}, Test score: {}", normal_score, test_score);

        // Test file should have 10% of normal score
        assert!(test_score < normal_score);
        assert!(normal_score > 0.0); // Ensure we have a baseline
        if normal_score > 0.0 {
            let ratio = test_score / normal_score;
            assert!(ratio < 0.2); // Should be significantly less (around 0.1)
        }
    }

    #[test]
    fn test_categorize_symbol() {
        assert_eq!(categorize_symbol("Function"), Some("functions"));
        assert_eq!(categorize_symbol("Method"), Some("functions"));
        assert_eq!(categorize_symbol("Class"), Some("classes"));
        assert_eq!(categorize_symbol("Struct"), Some("classes"));
        assert_eq!(categorize_symbol("Enum"), Some("classes")); // Enums count as types
        assert_eq!(categorize_symbol("Interface"), Some("interfaces"));
        assert_eq!(categorize_symbol("Trait"), Some("interfaces"));
        assert_eq!(categorize_symbol("Module"), Some("modules"));
        assert_eq!(categorize_symbol("Constant"), Some("constants"));
        assert_eq!(categorize_symbol("Static"), Some("constants"));
        assert_eq!(categorize_symbol("Variable"), None); // Local vars not counted
    }

    #[test]
    fn test_exact_0_1_deprioritization_factor() {
        let temp_dir = TempDir::new().unwrap();

        let code = r#"
struct MyStruct { field: i32 }
impl MyStruct {
    fn method1(&self) { self.method2(); }
    fn method2(&self) { println!("test"); }
}
fn main() {
    let s = MyStruct { field: 42 };
    s.method1();
}
"#;

        let normal_file = temp_dir.path().join("src").join("normal.rs");
        let test_file = temp_dir.path().join("tests").join("test_normal.rs");

        fs::create_dir_all(normal_file.parent().unwrap()).unwrap();
        fs::create_dir_all(test_file.parent().unwrap()).unwrap();
        fs::write(&normal_file, code).unwrap();
        fs::write(&test_file, code).unwrap();

        let normal_score = calculate_file_score(&normal_file, temp_dir.path());
        let test_score = calculate_file_score(&test_file, temp_dir.path());

        println!("Normal file score: {}", normal_score);
        println!("Test file score: {}", test_score);
        println!("Ratio: {}", test_score / normal_score);

        assert!(normal_score > 0.0, "Normal file should have positive score");

        let expected = normal_score * 0.1;
        assert!(
            (test_score - expected).abs() < 0.001,
            "Test score should be 0.1x normal. Expected: {}, Got: {}",
            expected,
            test_score
        );
    }

    #[test]
    fn test_files_with_test_in_name_not_deprioritized() {
        let temp_dir = TempDir::new().unwrap();

        let code = "class ABTestingHelper { void method() {} }";

        let file = temp_dir.path().join("src").join("ABTestingHelper.java");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, code).unwrap();

        let score = calculate_file_score(&file, temp_dir.path());

        assert!(
            score > 1.0,
            "ABTestingHelper should NOT be deprioritized (score: {})",
            score
        );
        assert!(
            !should_deprioritize(&file),
            "Files with 'test' in name should not be deprioritized"
        );
    }

    #[test]
    fn test_deprioritize_starts_with_test() {
        let temp_dir = TempDir::new().unwrap();

        let test_file = temp_dir.path().join("TestHelper.java");
        fs::write(&test_file, "class TestHelper {}").unwrap();

        assert!(
            should_deprioritize(&test_file),
            "TestHelper.java should be deprioritized"
        );
    }

    #[test]
    fn test_deprioritize_ends_with_test() {
        let temp_dir = TempDir::new().unwrap();

        let test_file = temp_dir.path().join("HelperTest.java");
        fs::write(&test_file, "class HelperTest {}").unwrap();

        assert!(
            should_deprioritize(&test_file),
            "HelperTest.java should be deprioritized"
        );
    }

    #[test]
    fn test_deprioritize_ends_with_spec() {
        let temp_dir = TempDir::new().unwrap();

        let spec_file = temp_dir.path().join("helper.spec.ts");
        fs::write(&spec_file, "describe('helper', () => {})").unwrap();

        assert!(
            should_deprioritize(&spec_file),
            "helper.spec.ts should be deprioritized"
        );
    }

    #[tokio::test]
    async fn test_generate_overview_respects_path_parameter() {
        use std::fs;

        // Create two temp directories with different content
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();

        // Create a file in dir1
        fs::write(temp_dir1.path().join("file1.rs"), "fn main() {}").unwrap();

        // Create a file in dir2
        fs::write(temp_dir2.path().join("file2.rs"), "fn test() {}").unwrap();

        // Initialize workspace manager with dir1
        let mut workspace_manager = WorkspaceManager::new(temp_dir1.path().to_path_buf());

        // Request overview for dir2 (different from workspace root)
        let request = GenerateCodebaseOverviewRequest {
            path: Some(temp_dir2.path().to_string_lossy().to_string()),
            timeout_secs: Some(10),
            token_budget: None,
        };

        let result = generate_overview(&mut workspace_manager, &request).await;
        assert!(result.is_ok(), "Overview generation should succeed");

        let overview = result.unwrap();
        // The workspace_path should reflect the requested path, not the workspace root
        assert_eq!(
            overview.workspace_path,
            temp_dir2.path().to_string_lossy().to_string(),
            "Overview should analyze the requested path, not workspace root"
        );
    }
}
