use std::sync::RwLock;

use hnsw_rs::hnsw::Hnsw;
use hnsw_rs::prelude::DistCosine;
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
}
