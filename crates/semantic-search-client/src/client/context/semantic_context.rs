use std::fs::{
    self,
    File,
};
use std::io::{
    BufReader,
    BufWriter,
};
use std::path::PathBuf;

use tracing::debug;

use crate::error::Result;
use crate::index::VectorIndex;
use crate::types::{
    DataPoint,
    SearchResult,
};

const HNSW_BASENAME: &str = "index";

/// A semantic context containing data points and a vector index
pub struct SemanticContext {
    /// The data points stored in the index
    pub(crate) data_points: Vec<DataPoint>,
    /// The vector index for fast approximate nearest neighbor search
    index: Option<VectorIndex>,
    /// Path to save/load the data points
    data_path: PathBuf,
}

impl SemanticContext {
    /// Create a new semantic context
    pub fn new(data_path: PathBuf) -> Result<Self> {
        // Create the directory if it doesn't exist
        if let Some(parent) = data_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Create a new instance
        let mut context = Self {
            data_points: Vec::new(),
            index: None,
            data_path: data_path.clone(),
        };

        // Load data points if the file exists
        if data_path.exists() {
            let file = File::open(&data_path)?;
            let reader = BufReader::new(file);
            context.data_points = serde_json::from_reader(reader)?;
        }

        // If we have data points, try to load persisted index or rebuild
        if !context.data_points.is_empty() {
            context.load_or_rebuild_index()?;
        }

        Ok(context)
    }

    /// Get the directory containing the index files
    fn index_dir(&self) -> Option<&std::path::Path> {
        self.data_path.parent()
    }

    /// Try to load persisted HNSW index, fall back to rebuilding
    fn load_or_rebuild_index(&mut self) -> Result<()> {
        if let Some(dir) = self.index_dir() {
            if let Some(index) = VectorIndex::load(dir, HNSW_BASENAME) {
                debug!("Loaded persisted HNSW index");
                self.index = Some(index);
                return Ok(());
            }
        }
        debug!("No index present, rebuilding");
        // Fall back to rebuilding and save for next time
        self.rebuild_index()?;
        self.save_index();
        Ok(())
    }

    /// Save the HNSW index to disk (best effort, failures are logged)
    fn save_index(&self) {
        if let (Some(index), Some(dir)) = (&self.index, self.index_dir()) {
            index.save(dir, HNSW_BASENAME);
        }
    }

    /// Save data points to disk
    pub fn save(&self) -> Result<()> {
        // Save the data points as JSON
        let file = File::create(&self.data_path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, &self.data_points)?;

        // Also save the HNSW index
        self.save_index();

        Ok(())
    }

    /// Rebuild the index from the current data points
    pub fn rebuild_index(&mut self) -> Result<()> {
        // Create a new index with the current data points
        let index = VectorIndex::new(self.data_points.len().max(100));

        // Add all data points to the index
        for (i, point) in self.data_points.iter().enumerate() {
            index.insert(&point.vector, i);
        }

        // Set the new index
        self.index = Some(index);

        Ok(())
    }

    /// Add data points to the context
    pub fn add_data_points(&mut self, data_points: Vec<DataPoint>) -> Result<usize> {
        // Store the count before extending the data points
        let count = data_points.len();

        if count == 0 {
            return Ok(0);
        }

        // Add the new points to our data store
        let start_idx = self.data_points.len();
        self.data_points.extend(data_points);
        let end_idx = self.data_points.len();

        // Update the index
        self.update_index_by_range(start_idx, end_idx)?;

        Ok(count)
    }

    /// Update the index with data points in a specific range
    pub fn update_index_by_range(&mut self, start_idx: usize, end_idx: usize) -> Result<()> {
        // If we don't have an index yet, or if the index is small and we're adding many points,
        // it might be more efficient to rebuild from scratch
        if self.index.is_none() || (self.data_points.len() < 1000 && (end_idx - start_idx) > self.data_points.len() / 2)
        {
            return self.rebuild_index();
        }

        // Get the existing index
        let index = self.index.as_ref().unwrap();

        // Add only the points in the specified range to the index
        for i in start_idx..end_idx {
            index.insert(&self.data_points[i].vector, i);
        }

        Ok(())
    }

    /// Search for similar items to the given vector
    pub fn search(&self, query_vector: &[f32], limit: usize) -> Result<Vec<SearchResult>> {
        let index = match &self.index {
            Some(idx) => idx,
            None => return Ok(Vec::new()), // Return empty results if no index
        };

        // Search for the nearest neighbors
        let results = index.search(query_vector, limit, 100);

        // Convert the results to our SearchResult type
        let search_results = results
            .into_iter()
            .map(|(id, distance)| {
                let point = self.data_points[id].clone();
                SearchResult::new(point, distance)
            })
            .collect();

        Ok(search_results)
    }

    /// Get the data points for serialization
    pub fn get_data_points(&self) -> &Vec<DataPoint> {
        &self.data_points
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn test_load_without_hnsw_files_rebuilds_index() {
        let dir = tempdir().unwrap();
        let data_path = dir.path().join("data.json");

        // Write only data points JSON (simulating old format without HNSW files)
        let data_points = vec![
            DataPoint {
                id: 0,
                payload: Default::default(),
                vector: vec![1.0, 0.0, 0.0],
            },
            DataPoint {
                id: 1,
                payload: Default::default(),
                vector: vec![0.0, 1.0, 0.0],
            },
        ];
        std::fs::write(&data_path, serde_json::to_string(&data_points).unwrap()).unwrap();

        // Verify no HNSW files exist
        assert!(!dir.path().join("index.hnsw.graph").exists());

        // Load should succeed by rebuilding
        let context = SemanticContext::new(data_path).unwrap();
        assert_eq!(context.data_points.len(), 2);

        // Search should work
        let results = context.search(&[1.0, 0.0, 0.0], 1).unwrap();
        assert_eq!(results.len(), 1);
    }
}
