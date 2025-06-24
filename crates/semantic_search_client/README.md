# Semantic Search Client

Rust library for managing semantic memory contexts with vector embeddings, enabling semantic search capabilities across text and code.

[![Crate](https://img.shields.io/crates/v/semantic_search_client.svg)](https://crates.io/crates/semantic_search_client)
[![Documentation](https://docs.rs/semantic_search_client/badge.svg)](https://docs.rs/semantic_search_client)

## Features

- **Semantic Memory Management**: Create, store, and search through semantic memory contexts
- **Vector Embeddings**: Generate high-quality text embeddings for semantic similarity search
- **Multi-Platform Support**: Works on macOS, Windows, and Linux with optimized backends
- **Hardware Acceleration**: Uses Metal on macOS and optimized backends on other platforms
- **File Processing**: Process various file types including text, markdown, JSON, and code
- **Persistent Storage**: Save contexts to disk for long-term storage and retrieval
- **Progress Tracking**: Detailed progress reporting for long-running operations
- **Parallel Processing**: Efficiently process large directories with parallel execution
- **Memory Efficient**: Stream large files and directories without excessive memory usage
- **Cross-Platform Compatibility**: Fallback mechanisms for all platforms and architectures

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
semantic_search_client = "0.1.0"
```

## Quick Start

```rust
use semantic_search_client::{SemanticSearchClient, Result};
use std::path::Path;

fn main() -> Result<()> {
    // Create a new memory bank client with default settings
    let mut client = SemanticSearchClient::new_with_default_dir()?;
    
    // Add a context from a directory
    let context_id = client.add_context_from_path(
        Path::new("/path/to/project"),
        "My Project",
        "Code and documentation for my project",
        true, // make it persistent
        None, // no progress callback
    )?;
    
    // Search within the context
    let results = client.search_context(&context_id, "implement authentication", 5)?;
    
    // Print the results
    for result in results {
        println!("Score: {}", result.distance);
        if let Some(text) = result.text() {
            println!("Text: {}", text);
        }
    }
    
    Ok(())
}
```

## Testing

The library includes comprehensive tests for all components. By default, tests use a mock embedder to avoid downloading models.

### Running Tests with Mock Embedders (Default)

```bash
cargo test
```

### Running Tests with Real Embedders

To run tests with real embedders (which will download models), set the `MEMORY_BANK_USE_REAL_EMBEDDERS` environment variable:

```bash
MEMORY_BANK_USE_REAL_EMBEDDERS=1 cargo test
```

## Core Concepts

### Memory Contexts

A memory context is a collection of related text or code that has been processed and indexed for semantic search. Contexts can be created from:

- Files
- Directories
- Raw text

Contexts can be either:

- **Volatile**: Temporary and lost when the program exits
- **Persistent**: Saved to disk and can be reloaded later

### Data Points

Each context contains data points, which are individual pieces of text with associated metadata and vector embeddings. Data points are the atomic units of search.

### Embeddings

Text is converted to vector embeddings using different backends based on platform and architecture:

- **macOS/Windows**: Uses ONNX Runtime with FastEmbed by default
- **Linux (non-ARM)**: Uses Candle for embeddings
- **Linux (ARM64)**: Uses BM25 keyword-based embeddings as a fallback

## Embedding Backends

The library supports multiple embedding backends with automatic selection based on platform compatibility:

1. **ONNX**: Fastest option, available on macOS and Windows
2. **Candle**: Good performance, used on Linux (non-ARM)
3. **BM25**: Fallback option based on keyword matching, used on Linux ARM64

The default selection logic prioritizes performance where possible:
- macOS/Windows: ONNX is the default
- Linux (non-ARM): Candle is the default
- Linux ARM64: BM25 is the default
- ARM64: BM25 is the default

## Detailed Usage

### Creating a Client

```rust
// With default directory (~/.memory_bank)
let client = SemanticSearchClient::new_with_default_dir()?;

// With custom directory
let client = SemanticSearchClient::new("/path/to/storage")?;

// With specific embedding type
use semantic_search_client::embedding::EmbeddingType;
let client = SemanticSearchClient::new_with_embedding_type(EmbeddingType::Candle)?;
```

### Adding Contexts

```rust
// From a file
let file_context_id = client.add_context_from_file(
    "/path/to/document.md",
    "Documentation",
    "Project documentation",
    true, // persistent
    None, // no progress callback
)?;

// From a directory with progress reporting
let dir_context_id = client.add_context_from_directory(
    "/path/to/codebase",
    "Codebase",
    "Project source code",
    true, // persistent
    Some(|status| {
        match status {
            ProgressStatus::CountingFiles => println!("Counting files..."),
            ProgressStatus::StartingIndexing(count) => println!("Starting indexing {} files", count),
            ProgressStatus::Indexing(current, total) => 
                println!("Indexing file {}/{}", current, total),
            ProgressStatus::CreatingSemanticContext => 
                println!("Creating semantic context..."),
            ProgressStatus::GeneratingEmbeddings(current, total) => 
                println!("Generating embeddings {}/{}", current, total),
            ProgressStatus::BuildingIndex => println!("Building index..."),
            ProgressStatus::Finalizing => println!("Finalizing..."),
            ProgressStatus::Complete => println!("Indexing complete!"),
        }
    }),
)?;

// From raw text
let text_context_id = client.add_context_from_text(
    "This is some text to remember",
    "Note",
    "Important information",
    false, // volatile
)?;
```

### Searching

```rust
// Search across all contexts
let all_results = client.search_all("authentication implementation", 5)?;
for (context_id, results) in all_results {
    println!("Results from context {}", context_id);
    for result in results {
        println!("  Score: {}", result.distance);
        if let Some(text) = result.text() {
            println!("  Text: {}", text);
        }
    }
}

// Search in a specific context
let context_results = client.search_context(
    &context_id,
    "authentication implementation",
    5,
)?;
```

### Managing Contexts

```rust
// Get all contexts
let contexts = client.get_all_contexts();
for context in contexts {
    println!("Context: {} ({})", context.name, context.id);
    println!("  Description: {}", context.description);
    println!("  Created: {}", context.created_at);
    println!("  Items: {}", context.item_count);
}

// Make a volatile context persistent
client.make_persistent(
    &context_id,
    "Saved Context",
    "Important information saved for later",
)?;

// Remove a context
client.remove_context_by_id(&context_id, true)?; // true to delete persistent storage
client.remove_context_by_name("My Context", true)?;
client.remove_context_by_path("/path/to/indexed/directory", true)?;
```

## Advanced Features

### Custom Embedding Models

The library supports different embedding backends:

```rust
// Use ONNX (fastest, used on macOS and Windows)
#[cfg(any(target_os = "macos", target_os = "windows"))]
let client = SemanticSearchClient::with_embedding_type(
    "/path/to/storage",
    EmbeddingType::Onnx,
)?;

// Use Candle (used on Linux non-ARM)
#[cfg(all(target_os = "linux", not(target_arch = "aarch64")))]
let client = SemanticSearchClient::with_embedding_type(
    "/path/to/storage",
    EmbeddingType::Candle,
)?;

// Use BM25 (used on Linux ARM64)
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
let client = SemanticSearchClient::with_embedding_type(
    "/path/to/storage",
    EmbeddingType::BM25,
)?;
```

### Parallel Processing

For large directories, the library automatically uses parallel processing to speed up indexing:

```rust
use rayon::prelude::*;

// Configure the global thread pool (optional)
rayon::ThreadPoolBuilder::new()
    .num_threads(8)
    .build_global()
    .unwrap();

// The client will use the configured thread pool
let client = SemanticSearchClient::new_with_default_dir()?;
```

## Performance Considerations

- **Memory Usage**: For very large directories, consider indexing subdirectories separately
- **Disk Space**: Persistent contexts store both the original text and vector embeddings
- **Embedding Speed**: The first embedding operation may be slower as models are loaded
- **Hardware Acceleration**: On macOS, Metal is used for faster embedding generation
- **Platform Differences**: Performance may vary based on the selected embedding backend

## Platform-Specific Features

- **macOS**: Uses Metal for hardware-accelerated embeddings via ONNX Runtime and Candle
- **Windows**: Uses optimized CPU execution via ONNX Runtime and Candle
- **Linux (non-ARM)**: Uses Candle for embeddings
- **Linux ARM64**: Uses BM25 keyword-based embeddings as a fallback

## Error Handling

The library uses a custom error type `MemoryBankError` that implements the standard `Error` trait:

```rust
use semantic_search_client::{SemanticSearchClient, MemoryBankError, Result};

fn process() -> Result<()> {
    let client = SemanticSearchClient::new_with_default_dir()?;
    
    // Handle specific error types
    match client.search_context("invalid-id", "query", 5) {
        Ok(results) => println!("Found {} results", results.len()),
        Err(MemoryBankError::ContextNotFound(id)) => 
            println!("Context not found: {}", id),
        Err(e) => println!("Error: {}", e),
    }
    
    Ok(())
}
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the terms specified in the repository's license file.
