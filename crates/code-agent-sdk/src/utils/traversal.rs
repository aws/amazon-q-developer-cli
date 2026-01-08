use std::path::Path;

/// Default maximum depth for directory traversal
pub const DEFAULT_MAX_DEPTH: usize = 15;

/// Maximum depth for finding .gitignore files (shallower since they're typically near root)
pub const GITIGNORE_SEARCH_DEPTH: usize = 5;

/// Directories to always skip regardless of gitignore (cross-language build/cache folders)
/// This is important to avoid undeterminism of difference workspaces and don't heavily depend on
/// excluded patterns or .gitignore. Specially when workspace is the top level workspace or a
/// extremely large folder.
pub const ALWAYS_SKIP_DIRS: &[&str] = &[
    // Build outputs
    "build",
    "dist",
    "out",
    "target",
    "bin",
    "obj",
    // Dependencies
    "node_modules",
    "vendor",
    ".venv",
    "venv",
    "env",
    "__pycache__",
    // Generated
    "generated",
    "generated-sources",
    "gen",
    // IDE/Tools
    ".idea",
    ".vscode",
    ".eclipse",
    ".settings",
    // Package caches
    ".gradle",
    ".maven",
    ".npm",
    ".cargo",
];

/// Check if a directory name should be skipped during traversal
#[inline]
pub fn should_skip_dir(name: &str) -> bool {
    ALWAYS_SKIP_DIRS.contains(&name)
}

/// Create a WalkBuilder with standard filters and skip common build/cache directories
///
/// # Arguments
/// * `root` - Root path to start traversal
/// * `max_depth` - Optional maximum depth (defaults to 15 if None)
pub fn create_code_walker(root: &Path, max_depth: Option<usize>) -> ignore::WalkBuilder {
    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .standard_filters(true)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .max_depth(Some(max_depth.unwrap_or(DEFAULT_MAX_DEPTH)))
        .filter_entry(|e| {
            e.file_type().is_none_or(|ft| !ft.is_dir()) || !should_skip_dir(e.file_name().to_str().unwrap_or(""))
        });
    builder
}
