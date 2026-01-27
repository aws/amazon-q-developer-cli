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
use std::time::{
    Duration,
    SystemTime,
};

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

/// Cached symbols with modification time for validation
#[derive(Clone)]
struct CachedSymbols {
    symbols: Vec<SymbolInfo>,
    mtime: SystemTime,
}

/// File index for extension-based lookups
pub struct CodeStore {
    /// Extension to file paths index
    extension_index: DashMap<String, Vec<PathBuf>>,
    /// File metadata (hash for change detection)
    metadata: DashMap<PathBuf, FileMetadata>,
    /// LRU cache: file_path -> CachedSymbols (max 500MB, 30min TTL)
    symbol_cache: Arc<Cache<PathBuf, CachedSymbols>>,
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
                    .time_to_live(Duration::from_secs(1800)) // 30 minutes
                    .weigher(|_key: &PathBuf, value: &CachedSymbols| {
                        (value.symbols.len() * 1024) as u32 // ~1KB per symbol
                    })
                    .build(),
            ),
        }
    }

    /// Get cached symbols for a file (validates mtime, returns None if stale)
    pub fn get_cached_symbols(&self, path: &Path) -> Option<Vec<SymbolInfo>> {
        let cached = self.symbol_cache.get(&path.to_path_buf())?;
        let current_mtime = std::fs::metadata(path).ok()?.modified().ok()?;
        if cached.mtime == current_mtime {
            Some(cached.symbols.clone())
        } else {
            self.symbol_cache.invalidate(&path.to_path_buf());
            None
        }
    }

    /// Cache symbols for a file with current mtime
    pub fn cache_symbols(&self, path: &Path, symbols: Vec<SymbolInfo>) {
        if let Ok(mtime) = std::fs::metadata(path).and_then(|m| m.modified()) {
            self.symbol_cache
                .insert(path.to_path_buf(), CachedSymbols { symbols, mtime });
        }
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

#[cfg(test)]
mod tests {
    use std::thread::sleep;

    use super::*;

    #[test]
    fn test_mtime_invalidation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.rs");

        // Create file and cache symbols
        std::fs::write(&file_path, "struct Foo {}").unwrap();
        let store = CodeStore::new();
        let symbols = vec![SymbolInfo {
            name: "Foo".to_string(),
            symbol_type: Some("struct".to_string()),
            file_path: file_path.to_string_lossy().to_string(),
            start_row: 1,
            end_row: 1,
            start_column: 0,
            end_column: 13,
            container_name: None,
            detail: None,
            source_line: None,
            source_code: None,
            language: None,
        }];
        store.cache_symbols(&file_path, symbols.clone());

        // Cache hit - same mtime
        let cached = store.get_cached_symbols(&file_path);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap()[0].name, "Foo");

        // Modify file (sleep to ensure mtime changes)
        sleep(Duration::from_millis(10));
        std::fs::write(&file_path, "struct Bar {}").unwrap();

        // Cache miss - mtime changed
        let cached = store.get_cached_symbols(&file_path);
        assert!(cached.is_none());
    }
}
