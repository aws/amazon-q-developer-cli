# Semantic Search Client

Rust library for managing semantic memory contexts with vector embeddings, enabling semantic search capabilities across text and code.

[![Crate](https://img.shields.io/crates/v/semantic_search_client.svg)](https://crates.io/crates/semantic_search_client)
[![Documentation](https://docs.rs/semantic_search_client/badge.svg)](https://docs.rs/semantic_search_client)

## Features

- **Async-First Design**: Built for modern async Rust applications with tokio
- **Semantic Memory Management**: Create, store, and search through semantic memory contexts
- **Pattern Filtering**: Include/exclude files using glob-style patterns during indexing
- **Vector Embeddings**: Generate high-quality text embeddings for semantic similarity search
- **Multi-Platform Support**: Works on macOS, Windows, and Linux with optimized backends
- **Hardware Acceleration**: Uses Metal on macOS and optimized backends on other platforms
- **File Processing**: Process various file types including text, markdown, JSON, and code
- **Persistent Storage**: Save contexts to disk for long-term storage and retrieval
- **Background Processing**: Non-blocking indexing with progress tracking and cancellation
- **Parallel Processing**: Efficiently process large directories with parallel execution
- **Memory Efficient**: Stream large files and directories without excessive memory usage
- **Cross-Platform Compatibility**: Fallback mechanisms for all platforms and architectures
- **🆕 Configurable File Limits**: Built-in protection against indexing too many files (default: 5,000 files)
- **🆕 Database Settings Integration**: Configurable chunk sizes, overlap, and limits

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
semantic_search_client = "0.1.0"
```

## Quick Start

```rust
use semantic_search_client::{AsyncSemanticSearchClient, AddContextRequest, Result};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    // Create a new async client with default settings
    let client = AsyncSemanticSearchClient::new_with_default_dir().await?;
    
    // Add a context using the new structured request API
    let request = AddContextRequest {
        path: PathBuf::from("/path/to/project"),
        name: "My Project".to_string(),
        description: "Code and documentation for my project".to_string(),
        persistent: true,
        include_patterns: Some(vec!["**/*.rs".to_string(), "**/*.md".to_string()]),
        exclude_patterns: Some(vec!["target/**".to_string(), "**/.git/**".to_string()]),
    };
    
    let (operation_id, _cancel_token) = client.add_context(request).await?;
    println!("Started indexing with operation ID: {}", operation_id);
    
    // Search across all contexts
    let results = client.search_all("implement authentication", None).await?;
    
    // Print the results
    for (context_id, context_results) in results {
        println!("Results from context {}", context_id);
        for result in context_results {
            println!("Score: {}", result.distance);
            if let Some(text) = result.text() {
                println!("Text: {}", text);
            }
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
// Create async client with default settings
let client = AsyncSemanticSearchClient::new_with_default_dir().await?;

// With custom directory
let client = AsyncSemanticSearchClient::new("/path/to/storage").await?;

// With custom configuration
let config = SemanticSearchConfig::default()
    .set_max_files(10000)      // Allow up to 10,000 files
    .set_chunk_size(1024);     // Custom chunk size
let client = AsyncSemanticSearchClient::with_config("/path/to/storage", config).await?;
```
```

### Configuration Options

The `SemanticSearchConfig` struct provides various configuration options:

```rust
let config = SemanticSearchConfig {
    chunk_size: 512,           // Size of text chunks for processing
    chunk_overlap: 128,        // Overlap between chunks
    default_results: 5,        // Default number of search results
    model_name: "all-MiniLM-L6-v2".to_string(),
    timeout: 30000,            // 30 seconds
    base_dir: PathBuf::from("/path/to/storage"),
    max_files: 5000,          // Maximum files allowed in a directory
};

// Or use builder methods
let config = SemanticSearchConfig::default()
    .set_max_files(10000)     // Custom file limit
    .set_chunk_size(1024)     // Custom chunk size
    .set_chunk_overlap(256);  // Custom overlap

// Just set file limit
let config = SemanticSearchConfig::with_max_files(15000);
```

### File Limit Protection

The client includes built-in protection against indexing too many files:

```rust
// This will fail if the directory contains more than 5,000 files
let result = client.add_context_from_directory(
    "/huge/workspace",
    "Workspace",
    "Too many files",
    true,
    None,
);

// Check for specific error
match result {
    Err(SemanticSearchError::InvalidArgument(msg)) => {
        println!("Directory exceeds file limit: {}", msg);
        // Example message:
        // "Directory contains 12,847 files, which exceeds the maximum 
        //  limit of 5,000 files. Please choose a smaller directory or 
        //  increase the max_files limit in the configuration."
    }
    _ => { /* handle other cases */ }
}

// To handle larger directories, increase the limit:
let config = SemanticSearchConfig::with_max_files(20000);
let client = SemanticSearchClient::with_config(path, config)?;
```

### Adding Contexts

```rust
use semantic_search_client::AddContextRequest;
use std::path::PathBuf;

// Add context with pattern filtering
let request = AddContextRequest {
    path: PathBuf::from("/path/to/codebase"),
    name: "Rust Codebase".to_string(),
    description: "Main Rust project source code".to_string(),
    persistent: true,
    include_patterns: Some(vec![
        "**/*.rs".to_string(),
        "**/*.toml".to_string(),
        "**/*.md".to_string(),
    ]),
    exclude_patterns: Some(vec![
        "target/**".to_string(),
        "**/.git/**".to_string(),
        "**/node_modules/**".to_string(),
    ]),
};

let (operation_id, cancel_token) = client.add_context(request).await?;
println!("Started indexing with operation ID: {}", operation_id);

// Add context without pattern filtering
let simple_request = AddContextRequest {
    path: PathBuf::from("/path/to/docs"),
    name: "Documentation".to_string(),
    description: "Project documentation".to_string(),
    persistent: true,
    include_patterns: None,
    exclude_patterns: None,
};

let (operation_id, _) = client.add_context(simple_request).await?;

// Monitor progress (the client runs indexing in background)
let status = client.get_status_data().await?;
println!("Active operations: {}", status.active_count);
```
```

### Searching

```rust
// Search across all contexts
let all_results = client.search_all("authentication implementation", None).await?;
for (context_id, results) in all_results {
    println!("Results from context {}", context_id);
    for result in results {
        println!("  Score: {}", result.distance);
        if let Some(text) = result.text() {
            println!("  Text: {}", text);
        }
    }
}

// Get all contexts first, then search specific ones
let contexts = client.get_contexts().await;
for context in &contexts {
    println!("Available context: {} ({})", context.name, context.id);
}
```
```

### Managing Contexts

```rust
// Get all contexts
let contexts = client.get_contexts().await;
for context in contexts {
    println!("Context: {} ({})", context.name, context.id);
    println!("  Description: {}", context.description);
    println!("  Created: {}", context.created_at);
    println!("  Items: {}", context.item_count);
    println!("  Include patterns: {:?}", context.include_patterns);
    println!("  Exclude patterns: {:?}", context.exclude_patterns);
}

// Remove contexts
client.remove_context_by_id("context-id").await?;
client.remove_context_by_name("Context Name").await?;
client.remove_context_by_path("/path/to/indexed/directory").await?;

// Cancel ongoing operations
client.cancel_operation(operation_id).await?;
client.cancel_all_operations().await?;

// Get system status
let status = client.get_status_data().await?;
println!("Total contexts: {}", status.total_contexts);
println!("Active operations: {}", status.active_count);
```
```

## Pattern Filtering

The library supports glob-style pattern filtering to control which files are indexed:

### Include Patterns
Only files matching these patterns will be indexed:
```rust
let request = AddContextRequest {
    // ... other fields
    include_patterns: Some(vec![
        "**/*.rs".to_string(),      // All Rust files
        "**/*.md".to_string(),      // All Markdown files
        "src/**/*.toml".to_string(), // TOML files in src directory
    ]),
    exclude_patterns: None,
};
```

### Exclude Patterns
Files matching these patterns will be skipped:
```rust
let request = AddContextRequest {
    // ... other fields
    include_patterns: None,
    exclude_patterns: Some(vec![
        "target/**".to_string(),        // Build artifacts
        "**/.git/**".to_string(),       // Git metadata
        "**/node_modules/**".to_string(), // Node.js dependencies
        "**/*.log".to_string(),         // Log files
    ]),
};
```

### Combined Filtering
Use both include and exclude patterns for precise control:
```rust
let request = AddContextRequest {
    // ... other fields
    include_patterns: Some(vec![
        "**/*.rs".to_string(),
        "**/*.toml".to_string(),
        "**/*.md".to_string(),
    ]),
    exclude_patterns: Some(vec![
        "target/**".to_string(),
        "**/tests/**".to_string(),  // Skip test files
        "**/*_test.rs".to_string(), // Skip test files
    ]),
};
```

### Pattern Syntax
- `**` matches any number of directories
- `*` matches any characters within a single path segment
- `?` matches a single character
- `[abc]` matches any character in the set
- `{a,b,c}` matches any of the alternatives

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

## File Limit Management

The semantic search client includes built-in protection against indexing too many files, which can overwhelm system resources and cause performance issues.

### Default Behavior

- **Default limit**: 5,000 files per directory
- **Early detection**: Files are counted before indexing begins
- **Clear error messages**: Users are informed of the limit and how to resolve it
- **Configurable**: Limits can be adjusted based on your needs

### Configuring File Limits

```rust
// Default limit (5,000 files)
let client = SemanticSearchClient::new_with_default_dir()?;

// Custom limit for larger projects
let config = SemanticSearchConfig::with_max_files(15000);
let client = SemanticSearchClient::with_config(path, config)?;

// No limit (use with caution!)
let config = SemanticSearchConfig::with_max_files(usize::MAX);
let client = SemanticSearchClient::with_config(path, config)?;

// Chainable configuration
let config = SemanticSearchConfig::default()
    .set_max_files(10000)
    .set_chunk_size(1024);
```

### Handling File Limit Errors

```rust
match client.add_context_from_directory(path, "name", "desc", true, None) {
    Ok(context_id) => {
        println!("Successfully indexed directory: {}", context_id);
    }
    Err(SemanticSearchError::InvalidArgument(msg)) if msg.contains("exceeds the maximum limit") => {
        println!("Directory has too many files: {}", msg);
        
        // Options to resolve:
        // 1. Choose a smaller subdirectory
        // 2. Increase the file limit
        // 3. Index subdirectories separately
    }
    Err(e) => {
        println!("Other error: {}", e);
    }
}
```

### Best Practices

1. **Start with subdirectories**: Instead of indexing entire workspaces, focus on specific components
2. **Use appropriate limits**: 
   - Small projects: 1,000-5,000 files
   - Medium projects: 5,000-15,000 files  
   - Large projects: 15,000+ files (consider splitting)
3. **Monitor performance**: Larger indexes take more memory and longer to search
4. **Exclude unnecessary files**: The client automatically skips common build artifacts and hidden files

### Example: Handling Large Codebases

```rust
// Instead of indexing the entire workspace
// let context = client.add_context_from_directory("/workspace", ...); // Might fail!

// Index important subdirectories separately
let src_context = client.add_context_from_directory("/workspace/src", "Source Code", "Main source code", true, None)?;
let docs_context = client.add_context_from_directory("/workspace/docs", "Documentation", "Project docs", true, None)?;
let tests_context = client.add_context_from_directory("/workspace/tests", "Tests", "Test files", true, None)?;

// Or increase the limit for the entire workspace
let config = SemanticSearchConfig::with_max_files(25000);
let client = SemanticSearchClient::with_config(path, config)?;
let workspace_context = client.add_context_from_directory("/workspace", "Full Workspace", "Complete codebase", true, None)?;
```

## Performance Considerations

- **File Count Limits**: The default 5,000 file limit prevents system overload. Adjust based on your hardware:
  - 8GB RAM: 5,000-10,000 files
  - 16GB RAM: 10,000-20,000 files
  - 32GB+ RAM: 20,000+ files
- **Memory Usage**: Each file adds to memory usage. Monitor system resources when indexing large directories
- **Disk Space**: Persistent contexts store both the original text and vector embeddings
- **Embedding Speed**: The first embedding operation may be slower as models are loaded
- **Hardware Acceleration**: On macOS, Metal is used for faster embedding generation
- **Platform Differences**: Performance may vary based on the selected embedding backend
- **Indexing Time**: Larger file counts increase indexing time exponentially

## Platform-Specific Features

- **macOS**: Uses Metal for hardware-accelerated embeddings via ONNX Runtime and Candle
- **Windows**: Uses optimized CPU execution via ONNX Runtime and Candle
- **Linux (non-ARM)**: Uses Candle for embeddings
- **Linux ARM64**: Uses BM25 keyword-based embeddings as a fallback

## Error Handling

The library uses a custom error type `SemanticSearchError` that implements the standard `Error` trait:

```rust
use semantic_search_client::{SemanticSearchClient, SemanticSearchError, Result};

fn process() -> Result<()> {
    let client = SemanticSearchClient::new_with_default_dir()?;
    
    // Handle specific error types
    match client.search_context("invalid-id", "query", 5) {
        Ok(results) => println!("Found {} results", results.len()),
        Err(SemanticSearchError::ContextNotFound(id)) => 
            println!("Context not found: {}", id),
        Err(e) => println!("Error: {}", e),
    }
    
    // Handle file limit errors
    match client.add_context_from_directory("/large/directory", "name", "desc", true, None) {
        Ok(context_id) => println!("Successfully indexed: {}", context_id),
        Err(SemanticSearchError::InvalidArgument(msg)) if msg.contains("exceeds the maximum limit") => {
            println!("File limit exceeded: {}", msg);
            // Suggest solutions:
            // 1. Use a smaller directory
            // 2. Increase max_files in configuration
            // 3. Index subdirectories separately
        }
        Err(SemanticSearchError::InvalidPath(path)) => 
            println!("Invalid path: {}", path),
        Err(e) => println!("Other error: {}", e),
    }
    
    Ok(())
}
```

### Common Error Types

- `InvalidArgument`: Configuration issues, including file limit violations
- `InvalidPath`: Path doesn't exist or isn't accessible
- `ContextNotFound`: Trying to access a non-existent context
- `OperationFailed`: General operation failures
- `IoError`: File system or network errors
- `EmbeddingError`: Issues with embedding generation
```

## Migration Guide

### Upgrading from Previous Versions

If you're upgrading from a version without file limits, your existing code will continue to work with the default 5,000 file limit. However, you may encounter new errors if you were previously indexing large directories.

#### Before (Previous Versions)
```rust
// This would index any size directory
let client = SemanticSearchClient::new_with_default_dir()?;
let context = client.add_context_from_directory("/huge/workspace", "name", "desc", true, None)?;
```

#### After (Current Version)
```rust
// This may now fail if directory has > 5,000 files
let client = SemanticSearchClient::new_with_default_dir()?;
match client.add_context_from_directory("/huge/workspace", "name", "desc", true, None) {
    Ok(context) => println!("Success: {}", context),
    Err(SemanticSearchError::InvalidArgument(msg)) if msg.contains("exceeds the maximum limit") => {
        // Handle file limit error - increase limit or use smaller directory
        let config = SemanticSearchConfig::with_max_files(20000);
        let client = SemanticSearchClient::with_config(path, config)?;
        let context = client.add_context_from_directory("/huge/workspace", "name", "desc", true, None)?;
    }
    Err(e) => return Err(e),
}
```

#### Recommended Migration Strategy
1. **Test your existing code** with the new version
2. **Identify directories** that exceed the 5,000 file limit
3. **Choose an approach**:
   - Increase the limit: `SemanticSearchConfig::with_max_files(N)`
   - Split large directories into smaller contexts
   - Focus on specific subdirectories instead of entire workspaces

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the terms specified in the repository's license file.
