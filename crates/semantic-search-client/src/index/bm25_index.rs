use std::fs::File;
use std::io::{
    BufReader,
    BufWriter,
};
use std::path::Path;
use std::sync::RwLock;

use bm25::{
    Document,
    Language,
    SearchEngine,
    SearchEngineBuilder,
};
use serde::{
    Deserialize,
    Serialize,
};
use tracing::{
    debug,
    info,
};

/// Serializable document for persistence
#[derive(Serialize, Deserialize)]
struct SerializableDocument {
    id: usize,
    contents: String,
}

/// BM25-based search index using native BM25 search engine
#[derive(Debug)]
pub struct BM25Index {
    /// The BM25 search engine
    engine: RwLock<SearchEngine<usize>>,
    /// Counter for document IDs
    next_id: std::sync::atomic::AtomicUsize,
    /// Document count
    doc_count: std::sync::atomic::AtomicUsize,
    /// Average document length used for initialization
    avgdl: f32,
}

impl BM25Index {
    /// Create a new BM25 index
    pub fn new(avgdl: f64) -> Self {
        info!("Creating new BM25 index with avgdl: {}", avgdl);

        let avgdl_f32 = avgdl as f32;
        let engine = SearchEngineBuilder::<usize>::with_avgdl(avgdl_f32)
            .language_mode(Language::English)
            .build();

        debug!("BM25 index created successfully");
        Self {
            engine: RwLock::new(engine),
            next_id: std::sync::atomic::AtomicUsize::new(0),
            doc_count: std::sync::atomic::AtomicUsize::new(0),
            avgdl: avgdl_f32,
        }
    }

    /// Load BM25 index from disk
    pub fn load_from_disk<P: AsRef<Path>>(path: P, avgdl: f64) -> crate::error::Result<Self> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let documents: Vec<SerializableDocument> = serde_json::from_reader(reader)?;

        let mut index = Self::new(avgdl);

        // Rebuild the search engine with loaded documents
        let mut engine = SearchEngineBuilder::<usize>::with_avgdl(avgdl as f32)
            .language_mode(Language::English)
            .build();

        let mut max_id = 0;
        for doc in documents {
            let document = Document {
                id: doc.id,
                contents: doc.contents,
            };
            engine.upsert(document);
            max_id = max_id.max(doc.id);
        }

        index.engine = RwLock::new(engine);
        index.next_id.store(max_id + 1, std::sync::atomic::Ordering::SeqCst);
        index.doc_count.store(max_id + 1, std::sync::atomic::Ordering::SeqCst);

        Ok(index)
    }

    /// Save BM25 index to disk
    pub fn save_to_disk<P: AsRef<Path>>(&self, path: P) -> crate::error::Result<()> {
        // Extract documents from the search engine
        let _engine = self.engine.read().unwrap();
        let documents: Vec<SerializableDocument> = Vec::new();

        // Note: The BM25 crate doesn't expose a way to iterate over documents
        // This is a limitation - we'd need to track documents separately
        // For now, this is a placeholder that would need the BM25 crate to expose document iteration

        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, &documents)?;

        Ok(())
    }

    /// Add a document to the index
    pub fn add_document(&self, content: String) -> usize {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.add_document_with_id(content, id);
        id
    }

    /// Add a document with a specific ID
    pub fn add_document_with_id(&self, content: String, id: usize) {
        let document = Document { id, contents: content };

        let mut engine = self.engine.write().unwrap();
        engine.upsert(document);
        self.doc_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        // Update next_id if this ID is higher
        let current_next = self.next_id.load(std::sync::atomic::Ordering::SeqCst);
        if id >= current_next {
            self.next_id.store(id + 1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    /// Search the index
    pub fn search(&self, query: &str, limit: usize) -> Vec<(usize, f32, String)> {
        let engine = self.engine.read().unwrap();
        let results = engine.search(query, limit);

        results
            .into_iter()
            .map(|result| (result.document.id, result.score, result.document.contents))
            .collect()
    }

    /// Remove a document from the index
    pub fn remove_document(&self, id: usize) {
        let mut engine = self.engine.write().unwrap();
        engine.remove(&id);
        self.doc_count.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }

    /// Get the number of documents in the index
    pub fn len(&self) -> usize {
        self.doc_count.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Check if the index is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the average document length used for BM25 scoring
    pub fn avgdl(&self) -> f32 {
        self.avgdl
    }
}
