use fastembed::{
    EmbeddingModel as FastEmbeddingModel,
    InitOptions,
    TextEmbedding,
};
use tracing::{
    debug,
    info,
};

use crate::error::Result;

/// Text embedding generator
pub struct TextEmbedder {
    /// The embedding model
    model: TextEmbedding,
}

impl TextEmbedder {
    /// Create a new TextEmbedder with the default model
    ///
    /// # Returns
    ///
    /// A new TextEmbedder instance
    pub fn new() -> Result<Self> {
        Self::with_model(FastEmbeddingModel::AllMiniLML6V2Q)
    }

    /// Create a new TextEmbedder with a specific model
    ///
    /// # Arguments
    ///
    /// * `model` - The embedding model to use
    ///
    /// # Returns
    ///
    /// A new TextEmbedder instance
    pub fn with_model(model: FastEmbeddingModel) -> Result<Self> {
        info!("Initializing text embedder with model: {:?}", model);

        let model = TextEmbedding::try_new(InitOptions::new(model).with_show_download_progress(false))?;

        debug!("Text embedder initialized successfully");
        Ok(Self { model })
    }

    /// Generate an embedding for a text
    ///
    /// # Arguments
    ///
    /// * `text` - The text to embed
    ///
    /// # Returns
    ///
    /// A vector of floats representing the text embedding
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let documents = vec![text];
        let embeddings = self.model.embed(documents, None)?;
        Ok(embeddings.into_iter().next().unwrap())
    }

    /// Generate embeddings for multiple texts
    ///
    /// # Arguments
    ///
    /// * `texts` - The texts to embed
    ///
    /// # Returns
    ///
    /// A vector of embeddings
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let documents: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        let embeddings = self.model.embed(documents, None)?;
        Ok(embeddings)
    }
}
