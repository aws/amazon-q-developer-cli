use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};

use anyhow::Result;
use ast_grep_core::matcher::Pattern;
use ast_grep_language::{
    LanguageExt,
    SupportLang,
};
use dashmap::DashMap;
use ignore::WalkState;

use crate::model::entities::RewriteResult;
use crate::model::types::PatternRewriteRequest;
use crate::sdk::WorkspaceManager;
use crate::utils::traversal::create_code_walker;

pub struct TreeSitterCodingService;

impl TreeSitterCodingService {
    pub fn new() -> Self {
        Self
    }

    pub async fn pattern_rewrite(
        &self,
        workspace_manager: &mut WorkspaceManager,
        request: PatternRewriteRequest,
    ) -> Result<RewriteResult> {
        let workspace_root = workspace_manager.workspace_root().to_path_buf();
        let lang: SupportLang = request
            .language
            .parse()
            .map_err(|_| anyhow::anyhow!("Unsupported language: {}", request.language))?;
        let lang_name = request.language.to_lowercase();
        let limit = request.limit.unwrap_or(crate::model::types::DEFAULT_SEARCH_RESULTS) as usize;

        let pattern = Pattern::try_new(&request.pattern, lang)
            .map_err(|e| anyhow::anyhow!("Invalid pattern '{}': {}", request.pattern, e))?;

        let extensions: Vec<String> = crate::tree_sitter::get_extensions(&lang_name)
            .unwrap_or(&[])
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

            // If it's a file, rewrite just that file
            if path.is_file() {
                let (f, r) = Self::rewrite_file(&path, &pattern, &lang, &request.replacement, request.dry_run)?;
                let modified_files = if f > 0 {
                    vec![
                        path.strip_prefix(&workspace_root)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .to_string(),
                    ]
                } else {
                    vec![]
                };
                return Ok(RewriteResult {
                    files_modified: f,
                    replacements: r,
                    modified_files,
                    dry_run: request.dry_run,
                });
            }

            // If it's a directory, use it as search root
            path
        } else {
            workspace_root.clone()
        };

        // Walk directory with gitignore support
        let walker = create_code_walker(&search_root, None).build_parallel();

        let files_modified = AtomicUsize::new(0);
        let replacements = AtomicUsize::new(0);
        let modified_files: DashMap<String, ()> = DashMap::new();

        walker.run(|| {
            let pattern = &pattern;
            let lang = &lang;
            let replacement = &request.replacement;
            let dry_run = request.dry_run;
            let extensions = &extensions;
            let files_modified = &files_modified;
            let replacements = &replacements;
            let modified_files = &modified_files;
            let workspace_root = &workspace_root;

            Box::new(move |entry| {
                // Early exit if limit reached
                if files_modified.load(Ordering::Relaxed) >= limit {
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

                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if !extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)) {
                    return WalkState::Continue;
                }

                if let Ok((f, r)) = Self::rewrite_file(path, pattern, lang, replacement, dry_run)
                    && f > 0
                {
                    let relative_path = path
                        .strip_prefix(workspace_root)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .to_string();
                    modified_files.insert(relative_path, ());
                    files_modified.fetch_add(f, Ordering::Relaxed);
                    replacements.fetch_add(r, Ordering::Relaxed);
                }

                WalkState::Continue
            })
        });

        let files_modified = files_modified.load(Ordering::Relaxed);
        let replacements = replacements.load(Ordering::Relaxed);
        let modified_files: Vec<String> = modified_files.into_iter().map(|(k, _)| k).collect();

        Ok(RewriteResult {
            files_modified,
            replacements,
            modified_files,
            dry_run: request.dry_run,
        })
    }

    fn rewrite_file(
        path: &std::path::Path,
        pattern: &Pattern,
        lang: &SupportLang,
        replacement: &str,
        dry_run: bool,
    ) -> Result<(usize, usize)> {
        let content = std::fs::read_to_string(path)?;
        let root = lang.ast_grep(&content);

        // Get all edits at once (no re-parsing between replacements)
        let edits = root.root().replace_all(pattern, replacement);
        let file_replacements = edits.len();

        if file_replacements > 0 && !dry_run {
            // Apply all edits to source
            let mut result = content.clone();
            // Apply edits in reverse order to preserve positions
            for edit in edits.into_iter().rev() {
                let start = edit.position;
                let end = start + edit.deleted_length;
                let text = String::from_utf8_lossy(&edit.inserted_text);
                result.replace_range(start..end, &text);
            }
            std::fs::write(path, result)?;
        }

        Ok(if file_replacements > 0 {
            (1, file_replacements)
        } else {
            (0, 0)
        })
    }
}

impl Default for TreeSitterCodingService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let service = TreeSitterCodingService::new();
        assert!(std::mem::size_of_val(&service) == 0); // Zero-sized type
    }

    #[test]
    fn test_parse_language_typescript() {
        let lang: Result<SupportLang, _> = "typescript".parse();
        assert!(lang.is_ok());
    }
}
