use std::path::Path;
use std::sync::RwLock;

use hnsw_rs::hnsw::Hnsw;
use hnsw_rs::hnswio::HnswIo;
use hnsw_rs::prelude::{
    AnnT,
    DistCosine,
};
use tracing::{
    debug,
    info,
};

/// Vector index for fast approximate nearest neighbor search
pub struct VectorIndex {
    /// The HNSW index protected by RwLock for thread safety
    index: RwLock<Hnsw<'static, f32, DistCosine>>,
    /// Counter to track the number of elements
    count: std::sync::atomic::AtomicUsize,
    /// HnswIo holder - must outlive the index when loaded from disk
    /// Using Box to keep it on heap and stable address
    _hnsw_io: Option<Box<HnswIo>>,
}

impl VectorIndex {
    /// Create a new empty vector index
    ///
    /// # Arguments
    ///
    /// * `max_elements` - Maximum number of elements the index can hold
    ///
    /// # Returns
    ///
    /// A new VectorIndex instance
    pub fn new(max_elements: usize) -> Self {
        info!("Creating new vector index with max_elements: {}", max_elements);

        let index = Hnsw::new(
            16,                    // Max number of connections per layer
            max_elements.max(100), // Maximum elements
            16,                    // Max layer
            100,                   // ef_construction (size of the dynamic candidate list)
            DistCosine {},
        );

        debug!("Vector index created successfully");
        Self {
            index: RwLock::new(index),
            count: std::sync::atomic::AtomicUsize::new(0),
            _hnsw_io: None,
        }
    }

    /// Insert a vector into the index
    ///
    /// # Arguments
    ///
    /// * `vector` - The vector to insert
    /// * `id` - The ID associated with the vector
    pub fn insert(&self, vector: &[f32], id: usize) {
        let index = self.index.read().unwrap();
        index.insert((vector, id));
        self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    /// Search for nearest neighbors
    ///
    /// # Arguments
    ///
    /// * `query` - The query vector
    /// * `limit` - Maximum number of results to return
    /// * `ef_search` - Size of the dynamic candidate list for search
    ///
    /// # Returns
    ///
    /// A vector of (id, distance) pairs
    pub fn search(&self, query: &[f32], limit: usize, ef_search: usize) -> Vec<(usize, f32)> {
        let index = self.index.read().unwrap();
        let results = index.search(query, limit, ef_search);

        results
            .into_iter()
            .map(|neighbor| (neighbor.d_id, neighbor.distance))
            .collect()
    }

    /// Get the number of elements in the index
    ///
    /// # Returns
    ///
    /// The number of elements in the index
    pub fn len(&self) -> usize {
        self.count.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Check if the index is empty
    ///
    /// # Returns
    ///
    /// `true` if the index is empty, `false` otherwise
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Save the HNSW index to disk (best effort, logs errors)
    pub fn save(&self, dir: &Path, basename: &str) {
        let index = match self.index.read() {
            Ok(guard) => guard,
            Err(e) => {
                debug!("Failed to acquire lock for HNSW save: {}", e);
                return;
            },
        };
        if let Err(e) = index.file_dump(dir, basename) {
            debug!("Failed to save HNSW index: {}", e);
        }
    }

    /// Load an HNSW index from disk
    ///
    /// # Arguments
    ///
    /// * `dir` - Directory containing the index files
    /// * `basename` - Base name of the index files
    ///
    /// # Returns
    ///
    /// A VectorIndex if files exist and load successfully, None otherwise
    pub fn load(dir: &Path, basename: &str) -> Option<Self> {
        let graph_file = dir.join(format!("{basename}.hnsw.graph"));
        if !graph_file.exists() {
            return None;
        }

        // Box the HnswIo so it has a stable address
        let mut hnsw_io = Box::new(HnswIo::new(dir, basename));

        // Load the index - the lifetime is tied to hnsw_io
        // SAFETY: We store hnsw_io in the struct, ensuring it outlives the index.
        // The index only references hnsw_io's mmap data (if mmap is used), and we
        // use default options (no mmap), so data is copied into the Hnsw struct.
        let index: Hnsw<'_, f32, DistCosine> = match hnsw_io.load_hnsw::<f32, DistCosine>() {
            Ok(idx) => idx,
            Err(e) => {
                debug!("Failed to load HNSW index: {}", e);
                return None;
            },
        };

        let count = index.get_nb_point();
        info!("Loaded HNSW index with {} points", count);

        // SAFETY: We're extending the lifetime to 'static because we store hnsw_io
        // in the same struct, guaranteeing it lives as long as the index.
        let index: Hnsw<'static, f32, DistCosine> = unsafe { std::mem::transmute(index) };

        Some(Self {
            index: RwLock::new(index),
            count: std::sync::atomic::AtomicUsize::new(count),
            _hnsw_io: Some(hnsw_io),
        })
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn test_save_and_load() {
        let dir = tempdir().unwrap();

        // Create and populate index
        let index = VectorIndex::new(100);
        index.insert(&[1.0, 0.0, 0.0], 0);
        index.insert(&[0.0, 1.0, 0.0], 1);
        index.insert(&[0.0, 0.0, 1.0], 2);

        // Save
        index.save(dir.path(), "test");

        // Load
        let loaded = VectorIndex::load(dir.path(), "test").unwrap();
        assert_eq!(loaded.len(), 3);

        // Verify search works on loaded index
        let results = loaded.search(&[1.0, 0.0, 0.0], 1, 50);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, 0);
    }

    #[test]
    fn test_load_nonexistent() {
        let dir = tempdir().unwrap();
        assert!(VectorIndex::load(dir.path(), "missing").is_none());
    }
}
