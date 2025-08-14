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
use crate::index::BM25Index;
use crate::types::BM25DataPoint;

/// BM25 context for managing persistent BM25 search data
#[derive(Debug)]
pub struct BM25Context {
    /// Data points stored in this context
    data_points: Vec<BM25DataPoint>,

    /// BM25 search index (rebuilt from data points)
    index: Option<BM25Index>,

    /// Path to the data file
    data_path: PathBuf,

    /// Average document length for BM25
    avgdl: f64,
}

impl BM25Context {
    /// Create a new BM25 context
    pub fn new(data_path: PathBuf, avgdl: f64) -> Result<Self> {
        // Create the directory if it doesn't exist
        if let Some(parent) = data_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Create a new instance
        let mut context = Self {
            data_points: Vec::new(),
            index: None,
            data_path: data_path.clone(),
            avgdl,
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
        let file = File::create(&self.data_path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, &self.data_points)?;
        Ok(())
    }

    /// Rebuild the index from the current data points
    pub fn rebuild_index(&mut self) -> Result<()> {
        let index = BM25Index::new(self.avgdl);

        // Add all data points to the index
        for point in &self.data_points {
            index.add_document_with_id(point.content.clone(), point.id);
        }

        self.index = Some(index);
        Ok(())
    }

    /// Add data points to the context
    pub fn add_data_points(&mut self, data_points: Vec<BM25DataPoint>) -> Result<usize> {
        let count = data_points.len();

        // Add to our data points
        self.data_points.extend(data_points);

        // Always rebuild index when we have data points
        if !self.data_points.is_empty() {
            self.rebuild_index()?;
        }

        // Save to disk
        self.save()?;

        Ok(count)
    }

    /// Search the context
    pub fn search(&self, query: &str, limit: usize) -> Vec<(usize, f32)> {
        match &self.index {
            Some(index) => index
                .search(query, limit)
                .into_iter()
                .map(|(id, score, _)| (id, score))
                .collect(),
            None => Vec::new(),
        }
    }

    /// Get data points
    pub fn get_data_points(&self) -> &[BM25DataPoint] {
        &self.data_points
    }

    /// Get a specific data point by index
    pub fn get_data_point(&self, index: usize) -> Option<&BM25DataPoint> {
        self.data_points.get(index)
    }
}
