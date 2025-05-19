use crate::error::Result;

/// Mock text embedder for testing
pub struct MockTextEmbedder {
    /// Fixed embedding dimension
    dimension: usize,
}

impl MockTextEmbedder {
    /// Create a new MockTextEmbedder
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
    }

    /// Generate a deterministic embedding for a text
    ///
    /// # Arguments
    ///
    /// * `text` - The text to embed
    ///
    /// # Returns
    ///
    /// A vector of floats representing the text embedding
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Generate a deterministic embedding based on the text
        // This avoids downloading any models while providing consistent results
        let mut embedding = Vec::with_capacity(self.dimension);

        // Use a simple hash of the text to seed the embedding values
        let hash = text.chars().fold(0u32, |acc, c| acc.wrapping_add(c as u32));

        for i in 0..self.dimension {
            // Generate a deterministic but varied value for each dimension
            let value = ((hash.wrapping_add(i as u32)).wrapping_mul(16807) % 65536) as f32 / 65536.0;
            embedding.push(value);
        }

        // Normalize the embedding to unit length
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        for value in &mut embedding {
            *value /= norm;
        }

        Ok(embedding)
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
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text)?);
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_embed_single() {
        let embedder = MockTextEmbedder::new(384);
        let embedding = embedder.embed("This is a test sentence.").unwrap();

        // Check dimension
        assert_eq!(embedding.len(), 384);

        // Check that the embedding is normalized (L2 norm ≈ 1.0)
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_mock_embed_batch() {
        let embedder = MockTextEmbedder::new(384);
        let texts = vec![
            "The cat sits outside".to_string(),
            "A man is playing guitar".to_string(),
        ];
        let embeddings = embedder.embed_batch(&texts).unwrap();

        assert_eq!(embeddings.len(), 2);
        assert_eq!(embeddings[0].len(), 384);
        assert_eq!(embeddings[1].len(), 384);

        // Check that embeddings are different
        let mut different = false;
        for i in 0..384 {
            if (embeddings[0][i] - embeddings[1][i]).abs() > 1e-5 {
                different = true;
                break;
            }
        }
        assert!(different);

        // Check determinism - same input should give same output
        let embedding1 = embedder.embed("The cat sits outside").unwrap();
        let embedding2 = embedder.embed("The cat sits outside").unwrap();

        for i in 0..384 {
            assert_eq!(embedding1[i], embedding2[i]);
        }
    }
}
