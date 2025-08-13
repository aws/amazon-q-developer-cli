/// File processing utilities for handling different file types and extracting content
pub mod file_processor;
/// Text chunking utilities for breaking down text into manageable pieces for embedding
pub mod text_chunker;

pub use file_processor::{
    get_file_type,
    process_directory,
    process_file,
    process_file_with_config,
};
pub use text_chunker::chunk_text;
