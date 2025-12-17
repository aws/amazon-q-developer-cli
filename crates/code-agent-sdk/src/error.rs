//! Error types for code intelligence operations
//!
//! These errors are designed to be clear and actionable for LLM agents,
//! providing context about what failed and suggestions for recovery.
//!
//! # Error Categories
//!
//! - **LSP Availability**: `LspNotAvailable`, `LspConnectionClosed`, `InitializationFailed`
//! - **Path Issues**: `InvalidPath`, `FileError`
//! - **Operation Failures**: `MethodNotSupported`, `LspError`, `Timeout`
//! - **Multi-language**: `PartialFailure`, `LanguageNotSupported`
//!
//! # Example
//!
//! ```ignore
//! use code_agent_sdk::{CodeIntelligenceError, CodeResult};
//!
//! fn handle_error(err: CodeIntelligenceError) {
//!     match &err {
//!         CodeIntelligenceError::LspNotAvailable { language, server_name, .. } => {
//!             // Suggest installing the language server
//!             println!("Install {} for {} support", server_name.as_deref().unwrap_or("a language server"), language);
//!         }
//!         CodeIntelligenceError::LspConnectionClosed { server_name, .. } => {
//!             // Suggest restarting
//!             println!("Restart {} or reinitialize workspace", server_name);
//!         }
//!         _ => println!("{}", err),
//!     }
//! }
//! ```

use std::fmt;
use std::path::PathBuf;

/// Errors that can occur during code intelligence operations
///
/// Each variant includes context to help understand and recover from the error.
/// The `Display` implementation provides LLM-friendly messages.
#[derive(Debug, Clone)]
pub enum CodeIntelligenceError {
    /// No language server available for the given file type.
    ///
    /// **When it occurs**: File extension not recognized or LSP not installed.
    /// **Recovery**: Install the suggested language server and ensure it's in PATH.
    LspNotAvailable {
        file_path: PathBuf,
        language: String,
        server_name: Option<String>,
    },

    /// Language server connection was closed unexpectedly.
    ///
    /// **When it occurs**: LSP process crashed, was killed, or pipe broken.
    /// **Recovery**: Reinitialize the workspace with `/code init --force`.
    LspConnectionClosed { server_name: String, reason: String },

    /// The requested LSP method is not supported by the server.
    ///
    /// **When it occurs**: Server doesn't implement the requested capability.
    /// **Recovery**: Try an alternative approach or different tool.
    MethodNotSupported { method: String, server_name: String },

    /// Invalid file path provided.
    ///
    /// **When it occurs**: Path doesn't exist, not accessible, or can't be converted to URI.
    /// **Recovery**: Verify the path exists and is within the workspace.
    InvalidPath { path: PathBuf, reason: String },

    /// Language server failed to initialize.
    ///
    /// **When it occurs**: LSP binary not found, wrong arguments, or startup failure.
    /// **Recovery**: Verify the language server is installed correctly.
    InitializationFailed { server_name: String, reason: String },

    /// The language is not supported.
    ///
    /// **When it occurs**: No LSP configured for the requested language.
    /// **Recovery**: Use one of the available languages or configure a new LSP.
    LanguageNotSupported { language: String, available: Vec<String> },

    /// LSP server returned an error response.
    ///
    /// **When it occurs**: Server rejected the request or encountered an internal error.
    /// **Recovery**: Check the error message for details; may need to fix code issues.
    LspError {
        server_name: String,
        code: Option<i32>,
        message: String,
    },

    /// Invalid position (line/column) provided to LSP.
    ///
    /// **When it occurs**: Position is out of bounds or file content changed.
    /// **Recovery**: Verify the position is within file bounds and file is saved.
    InvalidPosition { server_name: String, message: String },

    /// File operation failed.
    ///
    /// **When it occurs**: Read, write, or edit operation failed on a file.
    /// **Recovery**: Check file permissions and disk space.
    FileError {
        path: PathBuf,
        operation: String,
        reason: String,
    },

    /// Operation timed out.
    ///
    /// **When it occurs**: LSP took too long to respond.
    /// **Recovery**: Try again or check if the language server is overloaded.
    Timeout { operation: String, duration_secs: u64 },

    /// Some operations succeeded, some failed (for multi-language operations).
    ///
    /// **When it occurs**: Operation spans multiple LSPs and some failed.
    /// **Recovery**: Review individual errors; partial results may still be useful.
    PartialFailure {
        successful_count: usize,
        errors: Vec<LanguageError>,
    },
}

/// Error specific to a language/LSP in multi-language operations
#[derive(Debug, Clone)]
pub struct LanguageError {
    pub language: String,
    pub error: String,
}

impl std::error::Error for CodeIntelligenceError {}

impl fmt::Display for CodeIntelligenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LspNotAvailable {
                file_path,
                language,
                server_name,
            } => {
                if let Some(name) = server_name {
                    write!(
                        f,
                        "No language server available for {language} files ({}). Install '{name}' and ensure it's in PATH.",
                        file_path.display()
                    )
                } else {
                    write!(
                        f,
                        "No language server configured for {language} files ({}).",
                        file_path.display()
                    )
                }
            },
            Self::LspConnectionClosed { server_name, reason } => {
                write!(
                    f,
                    "Language server '{server_name}' connection closed: {reason}. The server may have crashed or been terminated."
                )
            },
            Self::MethodNotSupported { method, server_name } => {
                write!(
                    f,
                    "The '{method}' operation is not supported by {server_name}. Try a different approach or check if the server supports this feature."
                )
            },
            Self::InvalidPath { path, reason } => {
                write!(
                    f,
                    "Invalid path '{}': {reason}. Ensure the path exists and is accessible.",
                    path.display()
                )
            },
            Self::InitializationFailed { server_name, reason } => {
                write!(
                    f,
                    "Failed to start language server '{server_name}': {reason}. Ensure it's installed and in PATH."
                )
            },
            Self::LanguageNotSupported { language, available } => {
                write!(
                    f,
                    "Language '{language}' is not supported. Available: {}",
                    if available.is_empty() {
                        "none".to_string()
                    } else {
                        available.join(", ")
                    }
                )
            },
            Self::LspError {
                server_name,
                code,
                message,
            } => {
                if let Some(code) = code {
                    write!(f, "LSP error from {server_name} (code {code}): {message}")
                } else {
                    write!(f, "LSP error from {server_name}: {message}")
                }
            },
            Self::InvalidPosition { server_name, message } => {
                write!(
                    f,
                    "Invalid position for {server_name}: {message}. Verify the position is within file bounds."
                )
            },
            Self::FileError {
                path,
                operation,
                reason,
            } => {
                write!(f, "Failed to {operation} '{}': {reason}", path.display())
            },
            Self::Timeout {
                operation,
                duration_secs,
            } => {
                write!(
                    f,
                    "Operation '{operation}' timed out after {duration_secs} seconds. The language server may be overloaded."
                )
            },
            Self::PartialFailure {
                successful_count,
                errors,
            } => {
                let error_summary: Vec<String> =
                    errors.iter().map(|e| format!("{}: {}", e.language, e.error)).collect();
                write!(
                    f,
                    "{successful_count} operation(s) succeeded, {} failed. Failures: {}",
                    errors.len(),
                    error_summary.join("; ")
                )
            },
        }
    }
}

impl CodeIntelligenceError {
    /// Create an LspNotAvailable error with optional server name from config
    pub fn lsp_not_available(file_path: PathBuf, language: &str, server_name: Option<&str>) -> Self {
        Self::LspNotAvailable {
            file_path,
            language: language.to_string(),
            server_name: server_name.map(|s| s.to_string()),
        }
    }

    /// Create a connection closed error
    pub fn connection_closed(server_name: &str, reason: &str) -> Self {
        Self::LspConnectionClosed {
            server_name: server_name.to_string(),
            reason: reason.to_string(),
        }
    }

    /// Create an initialization failed error
    pub fn init_failed(server_name: &str, reason: &str) -> Self {
        Self::InitializationFailed {
            server_name: server_name.to_string(),
            reason: reason.to_string(),
        }
    }

    /// Create an invalid path error
    pub fn invalid_path(path: PathBuf, reason: &str) -> Self {
        Self::InvalidPath {
            path,
            reason: reason.to_string(),
        }
    }

    /// Create an LSP error
    pub fn lsp_error(server_name: &str, code: Option<i32>, message: &str) -> Self {
        Self::LspError {
            server_name: server_name.to_string(),
            code,
            message: message.to_string(),
        }
    }

    /// Create an LSP error, classifying it as InvalidPosition if the message indicates a position
    /// error
    pub fn from_lsp_error(server_name: &str, code: Option<i32>, message: &str) -> Self {
        if Self::is_position_error(message) {
            Self::InvalidPosition {
                server_name: server_name.to_string(),
                message: message.to_string(),
            }
        } else {
            Self::lsp_error(server_name, code, message)
        }
    }

    /// Check if an error message indicates a position-related error
    fn is_position_error(message: &str) -> bool {
        let msg = message.to_lowercase();
        msg.contains("position") || msg.contains("line") || msg.contains("column") || msg.contains("range")
    }

    /// Create a method not supported error
    pub fn method_not_supported(method: &str, server_name: &str) -> Self {
        Self::MethodNotSupported {
            method: method.to_string(),
            server_name: server_name.to_string(),
        }
    }
}

/// Result type alias for code intelligence operations
pub type Result<T> = std::result::Result<T, CodeIntelligenceError>;
