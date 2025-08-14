use std::fs::{
    self,
    File,
};
use std::io::{
    BufReader,
    BufWriter,
};
use std::path::PathBuf;

use crate::error::Result;
use crate::index::VectorIndex;
use crate::types::{
    DataPoint,
    SearchResult,
};

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

        // If we have data points, rebuild the index
        if !context.data_points.is_empty() {
            context.rebuild_index()?;
        }

        Ok(context)
    }

    /// Save data points to disk
    pub fn save(&self) -> Result<()> {
        // Save the data points as JSON
        let file = File::create(&self.data_path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, &self.data_points)?;

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

        // Save to disk
        self.save()?;

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
