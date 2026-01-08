//! CodeStore - File index for fast extension-based lookups
//!
//! Tracks file paths by extension for efficient file discovery.

use std::path::{
    Path,
    PathBuf,
};
use std::sync::Arc;
use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};
use std::time::SystemTime;

use dashmap::DashMap;
use moka::sync::Cache;
use sha2::{
    Digest,
    Sha256,
};

use crate::model::entities::SymbolInfo;

/// Metadata about an indexed file
#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub last_modified: SystemTime,
    pub content_hash: [u8; 32],
    pub file_size: u64,
}

/// File index for extension-based lookups
pub struct CodeStore {
    /// Extension to file paths index
    extension_index: DashMap<String, Vec<PathBuf>>,
    /// File metadata (hash for change detection)
    metadata: DashMap<PathBuf, FileMetadata>,
    /// LRU cache: file_path -> Vec<SymbolInfo> (max 500MB)
    symbol_cache: Arc<Cache<PathBuf, Vec<SymbolInfo>>>,
}

impl Default for CodeStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeStore {
    pub fn new() -> Self {
        Self {
            extension_index: DashMap::new(),
            metadata: DashMap::new(),
            symbol_cache: Arc::new(
                Cache::builder()
                    .max_capacity(500 * 1024 * 1024) // 500MB
                    .weigher(|_key: &PathBuf, value: &Vec<SymbolInfo>| {
                        (value.len() * 1024) as u32 // ~1KB per symbol
                    })
                    .build(),
            ),
        }
    }

    /// Get cached symbols for a file
    pub fn get_cached_symbols(&self, path: &Path) -> Option<Vec<SymbolInfo>> {
        self.symbol_cache.get(&path.to_path_buf())
    }

    /// Cache symbols for a file
    pub fn cache_symbols(&self, path: &Path, symbols: Vec<SymbolInfo>) {
        self.symbol_cache.insert(path.to_path_buf(), symbols);
    }

    /// Invalidate symbol cache for a file
    pub fn invalidate_symbols(&self, path: &Path) {
        self.symbol_cache.invalidate(&path.to_path_buf());
    }

    /// Get cache statistics
    pub fn cache_size_mb(&self) -> f64 {
        self.symbol_cache.weighted_size() as f64 / (1024.0 * 1024.0)
    }

    pub fn cache_entry_count(&self) -> u64 {
        self.symbol_cache.entry_count()
    }

    /// Index a file (stores metadata only)
    pub fn index_file(&self, path: &Path, content: String) {
        let content_hash = Self::compute_hash(&content);
        let file_size = content.len() as u64;
        let last_modified = SystemTime::now();

        let extension = path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase());

        // Check if unchanged
        if let Some(existing) = self.metadata.get(path) {
            if existing.content_hash == content_hash {
                return;
            }
            // File changed - invalidate cache
            self.invalidate_symbols(path);
        }

        // Update extension index
        if let Some(ref ext) = extension {
            self.extension_index
                .entry(ext.clone())
                .or_default()
                .push(path.to_path_buf());
        }

        // Store metadata
        self.metadata.insert(path.to_path_buf(), FileMetadata {
            last_modified,
            content_hash,
            file_size,
        });
    }

    /// Remove a file from index
    pub fn remove_file(&self, path: &Path) {
        for mut entry in self.extension_index.iter_mut() {
            entry.value_mut().retain(|p| p != path);
        }
        self.metadata.remove(path);
        self.invalidate_symbols(path);
    }

    /// Get files by extension
    pub fn get_files_by_extension(&self, extension: &str) -> Vec<PathBuf> {
        let ext_lower = extension.to_lowercase();
        self.extension_index
            .get(&ext_lower)
            .map(|r| r.clone())
            .unwrap_or_default()
    }

    /// Get files matching any of the given extensions
    pub fn get_files_by_extensions(&self, extensions: &[&str]) -> Vec<PathBuf> {
        let mut result = Vec::new();
        for ext in extensions {
            result.extend(self.get_files_by_extension(ext));
        }
        result
    }

    /// Get all indexed file paths
    pub fn get_all_indexed_files(&self) -> Vec<PathBuf> {
        self.metadata.iter().map(|r| r.key().clone()).collect()
    }

    /// Number of indexed files
    pub fn len(&self) -> usize {
        self.metadata.len()
    }

    pub fn is_empty(&self) -> bool {
        self.metadata.is_empty()
    }

    /// Index directory (parallel)
    pub fn index_directory(&self, root: &Path) -> usize {
        use ignore::WalkState;

        use crate::utils::traversal::create_code_walker;

        let indexed_count = AtomicUsize::new(0);

        let walker = create_code_walker(root, None).build_parallel();

        walker.run(|| {
            Box::new(|entry| {
                let entry = match entry {
                    Ok(e) => e,
                    Err(_) => return WalkState::Continue,
                };

                let path = entry.path();
                if !path.is_file() {
                    return WalkState::Continue;
                }

                if let Ok(content) = std::fs::read_to_string(path) {
                    self.index_file(path, content);
                    indexed_count.fetch_add(1, Ordering::Relaxed);
                }

                WalkState::Continue
            })
        });

        indexed_count.load(Ordering::Relaxed)
    }

    fn compute_hash(content: &str) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        hasher.finalize().into()
    }
}
