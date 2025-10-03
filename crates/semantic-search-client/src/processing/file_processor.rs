use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::error::{
    Result,
    SemanticSearchError,
};
use crate::processing::text_chunker::chunk_text;
use crate::types::FileType;

/// Determine the file type based on extension
pub fn get_file_type(path: &Path) -> FileType {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_lowercase())
        .as_deref()
    {
        // Plain text files
        Some("txt") => FileType::Text,

        // Markdown files (including MDX)
        Some("md" | "markdown" | "mdx") => FileType::Markdown,

        // JSON files - now treated as text for better searchability
        Some("json") => FileType::Text,

        // Configuration files
        Some("ini" | "conf" | "cfg" | "properties" | "env") => FileType::Text,

        // Data files
        Some("csv" | "tsv") => FileType::Text,

        // Log files
        Some("log") => FileType::Text,

        // Documentation formats
        Some("rtf" | "tex" | "rst") => FileType::Text,

        // Web and markup formats (text-based)
        Some("svg") => FileType::Text,

        // Code file extensions
        Some("rs") => FileType::Code,
        Some("py") => FileType::Code,
        Some("js" | "jsx" | "ts" | "tsx") => FileType::Code,
        Some("java") => FileType::Code,
        Some("c" | "cpp" | "h" | "hpp") => FileType::Code,
        Some("go") => FileType::Code,
        Some("rb") => FileType::Code,
        Some("php") => FileType::Code,
        Some("swift") => FileType::Code,
        Some("kt" | "kts") => FileType::Code,
        Some("cs") => FileType::Code,
        Some("sh" | "bash" | "zsh") => FileType::Code,
        Some("html" | "htm" | "xml") => FileType::Code,
        Some("css" | "scss" | "sass" | "less") => FileType::Code,
        Some("sql") => FileType::Code,
        Some("yaml" | "yml") => FileType::Code,
        Some("toml") => FileType::Code,

        // Handle files without extensions (common project files)
        None => match path.file_name().and_then(|name| name.to_str()) {
            Some("Dockerfile" | "Makefile" | "LICENSE" | "CHANGELOG" | "README") => FileType::Text,
            Some(name) if name.starts_with('.') => match name {
                ".gitignore" | ".env" | ".dockerignore" => FileType::Text,
                // Treat other dotfiles as text if they appear to be configuration files
                name if name.ends_with("rc") || name.ends_with("config") => FileType::Text,
                // Default other dotfiles to text for better indexing
                _ => FileType::Text,
            },
            _ => FileType::Unknown,
        },

        // Default to unknown (includes office docs, PDFs, etc.)
        _ => FileType::Unknown,
    }
}

/// Process a file and extract its content (backward compatible version)
///
/// # Arguments
///
/// * `path` - Path to the file
///
/// # Returns
///
/// A vector of JSON objects representing the file content
pub fn process_file(path: &Path) -> Result<Vec<Value>> {
    process_file_with_config(path, None, None)
}

/// Process a file with custom chunk configuration
///
/// # Arguments
///
/// * `path` - Path to the file
/// * `chunk_size` - Optional chunk size (uses default if None)
/// * `chunk_overlap` - Optional chunk overlap (uses default if None)
///
/// # Returns
///
/// A vector of JSON objects representing the file content
pub fn process_file_with_config(
    path: &Path,
    chunk_size: Option<usize>,
    chunk_overlap: Option<usize>,
) -> Result<Vec<Value>> {
    if !path.exists() {
        return Err(SemanticSearchError::InvalidPath(format!(
            "File does not exist: {}",
            path.display()
        )));
    }

    let file_type = get_file_type(path);
    let content = fs::read_to_string(path).map_err(|e| {
        SemanticSearchError::IoError(std::io::Error::new(
            e.kind(),
            format!("Failed to read file {}: {}", path.display(), e),
        ))
    })?;

    match file_type {
        FileType::Text | FileType::Markdown | FileType::Code | FileType::Json => {
            // For text-based files (including JSON), chunk the content and create multiple data points
            // Use the configured chunk size and overlap
            let chunks = chunk_text(&content, chunk_size, chunk_overlap);
            let path_str = path.to_string_lossy().to_string();
            let file_type_str = format!("{:?}", file_type);

            let mut results = Vec::new();

            for (i, chunk) in chunks.iter().enumerate() {
                let mut metadata = serde_json::Map::new();
                metadata.insert("text".to_string(), Value::String(chunk.clone()));
                metadata.insert("path".to_string(), Value::String(path_str.clone()));
                metadata.insert("file_type".to_string(), Value::String(file_type_str.clone()));
                metadata.insert("chunk_index".to_string(), Value::Number((i as u64).into()));
                metadata.insert("total_chunks".to_string(), Value::Number((chunks.len() as u64).into()));

                // For code files, add additional metadata
                if file_type == FileType::Code {
                    metadata.insert(
                        "language".to_string(),
                        Value::String(
                            path.extension()
                                .and_then(|ext| ext.to_str())
                                .unwrap_or("unknown")
                                .to_string(),
                        ),
                    );
                }

                results.push(Value::Object(metadata));
            }

            // If no chunks were created (empty file), create at least one entry
            if results.is_empty() {
                let mut metadata = serde_json::Map::new();
                metadata.insert("text".to_string(), Value::String(String::new()));
                metadata.insert("path".to_string(), Value::String(path_str));
                metadata.insert("file_type".to_string(), Value::String(file_type_str));
                metadata.insert("chunk_index".to_string(), Value::Number(0.into()));
                metadata.insert("total_chunks".to_string(), Value::Number(1.into()));

                results.push(Value::Object(metadata));
            }

            Ok(results)
        },
        FileType::Unknown => {
            // For unknown file types, just store the path
            let mut metadata = serde_json::Map::new();
            metadata.insert("path".to_string(), Value::String(path.to_string_lossy().to_string()));
            metadata.insert("file_type".to_string(), Value::String("Unknown".to_string()));

            Ok(vec![Value::Object(metadata)])
        },
    }
}

/// Process a directory and extract content from all files
///
/// # Arguments
///
/// * `dir_path` - Path to the directory
/// * `chunk_size` - Optional chunk size (uses default if None)
/// * `chunk_overlap` - Optional chunk overlap (uses default if None)
///
/// # Returns
///
/// A vector of JSON objects representing the content of all files
pub fn process_directory(
    dir_path: &Path,
    chunk_size: Option<usize>,
    chunk_overlap: Option<usize>,
) -> Result<Vec<Value>> {
    let mut results = Vec::new();

    for entry in walkdir::WalkDir::new(dir_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();

        // Skip hidden files, but allow common configuration files
        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            if filename.starts_with('.') {
                // Allow common configuration files
                let is_config_file = filename.ends_with("rc") 
                    || filename.ends_with("config")
                    || matches!(filename, ".gitignore" | ".env" | ".dockerignore");
                
                if !is_config_file {
                    continue;
                }
            }
        }

        // Process the file
        if let Ok(mut items) = process_file_with_config(path, chunk_size, chunk_overlap) {
            results.append(&mut items);
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::types::FileType;

    #[test]
    fn test_file_type_detection() {
        let test_cases = [
            // Code files
            ("main.rs", FileType::Code),
            ("script.py", FileType::Code),
            ("app.js", FileType::Code),
            ("component.tsx", FileType::Code),
            ("Main.java", FileType::Code),
            ("main.c", FileType::Code),
            ("index.html", FileType::Code),
            ("styles.css", FileType::Code),
            ("config.yaml", FileType::Code),
            ("Cargo.toml", FileType::Code),
            // Markdown files
            ("README.md", FileType::Markdown),
            ("doc.markdown", FileType::Markdown),
            ("component.mdx", FileType::Markdown),
            // Text files
            ("notes.txt", FileType::Text),
            ("data.json", FileType::Text),
            ("config.ini", FileType::Text),
            ("data.csv", FileType::Text),
            ("Dockerfile", FileType::Text),
            ("LICENSE", FileType::Text),
            (".gitignore", FileType::Text),
            (".bashrc", FileType::Text),
            (".vimrc", FileType::Text),
            (".raprc", FileType::Text),
            (".testrc", FileType::Text),
            (".zshconfig", FileType::Text),
            (".someconfig", FileType::Text),
            (".randomdotfile", FileType::Text),
            // Case insensitive
            ("Main.RS", FileType::Code),
            ("README.MD", FileType::Markdown),
            ("notes.TXT", FileType::Text),
            // Unknown files
            ("image.png", FileType::Unknown),
            ("document.pdf", FileType::Unknown),
            ("binary.exe", FileType::Unknown),
            ("unknown_file", FileType::Unknown),
        ];

        for (filename, expected) in test_cases {
            assert_eq!(
                get_file_type(&PathBuf::from(filename)),
                expected,
                "Failed for {}",
                filename
            );
        }
    }

    #[test]
    fn test_unknown_file_types() {
        // Binary files and unsupported formats
        assert_eq!(get_file_type(&PathBuf::from("image.png")), FileType::Unknown);
        assert_eq!(get_file_type(&PathBuf::from("document.pdf")), FileType::Unknown);
        assert_eq!(get_file_type(&PathBuf::from("archive.zip")), FileType::Unknown);
        assert_eq!(get_file_type(&PathBuf::from("binary.exe")), FileType::Unknown);
        assert_eq!(get_file_type(&PathBuf::from("data.db")), FileType::Unknown);
    }
}
