//! Socket manager for Q Chat API mode
//!
//! This module manages Unix domain sockets for programmatic interaction with Q Chat.
//! It handles socket creation, cleanup, and directory management.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use eyre::{Result, eyre};
use sha2::{Digest, Sha256};
use tokio::net::UnixListener;

/// Types of sockets that can be created
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SocketType {
    Control,
    Input,
    Output,
    Thinking,
    Tools,
    Events,
}

impl SocketType {
    /// Get the filename for this socket type
    pub fn filename(&self) -> &'static str {
        match self {
            SocketType::Control => "control.sock",
            SocketType::Input => "input.sock",
            SocketType::Output => "output.sock",
            SocketType::Thinking => "thinking.sock",
            SocketType::Tools => "tools.sock",
            SocketType::Events => "events.sock",
        }
    }

    /// Get all socket types (excluding Control and Events which are suppressed)
    pub fn all() -> Vec<SocketType> {
        vec![
            // SocketType::Control,  // Suppressed - control socket management disabled
            SocketType::Input,
            SocketType::Output,
            SocketType::Thinking,
            SocketType::Tools,
            // SocketType::Events,   // Suppressed - events socket management disabled
        ]
    }
}

/// Information about a created socket
#[derive(Debug, Clone)]
pub struct SocketInfo {
    pub socket_type: SocketType,
    pub path: PathBuf,
    pub created_at: SystemTime,
    pub listener: Option<Arc<UnixListener>>,
}

/// Manages Unix domain sockets for Q Chat API mode
#[derive(Debug)]
pub struct SocketManager {
    /// Session ID for this socket manager instance
    pub session_id: String,
    /// Working directory hash for socket path generation
    pub working_directory_hash: String,
    /// Directory where sockets are created
    pub socket_directory: PathBuf,
    /// Map of socket types to their information
    pub sockets: HashMap<SocketType, SocketInfo>,
    /// Whether to clean up sockets on drop
    cleanup_on_drop: bool,
}

impl SocketManager {
    /// Create a new SocketManager for the given working directory
    pub fn new(working_directory: &Path) -> Result<Self> {
        let session_id = uuid::Uuid::new_v4().to_string();
        
        // Create a hash of the working directory for unique socket paths
        let mut hasher = Sha256::new();
        hasher.update(working_directory.to_string_lossy().as_bytes());
        let hash = hasher.finalize();
        let working_directory_hash = format!("{:x}", hash)[..16].to_string();
        
        // Create socket directory in /tmp
        let socket_directory = PathBuf::from("/tmp").join(format!("q-chat-{}", working_directory_hash));
        
        // Create the directory if it doesn't exist
        if !socket_directory.exists() {
            fs::create_dir_all(&socket_directory)
                .map_err(|e| eyre!("Failed to create socket directory: {}", e))?;
        }
        
        Ok(Self {
            session_id,
            working_directory_hash,
            socket_directory,
            sockets: HashMap::new(),
            cleanup_on_drop: true,
        })
    }

    /// Get all socket paths as a HashMap
    pub fn get_all_socket_paths(&self) -> HashMap<SocketType, PathBuf> {
        self.sockets.iter()
            .map(|(socket_type, info)| (socket_type.clone(), info.path.clone()))
            .collect()
    }

    /// Get the listener for a specific socket type
    pub fn get_listener(&self, socket_type: &SocketType) -> Option<Arc<UnixListener>> {
        self.sockets.get(socket_type)
            .and_then(|info| info.listener.clone())
    }

    /// Disable cleanup on drop (useful for testing)
    pub fn disable_cleanup_on_drop(&mut self) {
        self.cleanup_on_drop = false;
    }

    /// Clean up all sockets and the socket directory
    pub fn cleanup(&mut self) -> Result<()> {
        // Remove all socket files
        for (_, socket_info) in &self.sockets {
            if socket_info.path.exists() {
                if let Err(e) = fs::remove_file(&socket_info.path) {
                    eprintln!("Warning: Failed to remove socket file {:?}: {}", socket_info.path, e);
                }
            }
        }
        
        // Clear the sockets map
        self.sockets.clear();
        
        // Try to remove the socket directory if it's empty
        if self.socket_directory.exists() {
            if let Err(e) = fs::remove_dir(&self.socket_directory) {
                // It's okay if this fails (directory might not be empty)
                eprintln!("Note: Could not remove socket directory {:?}: {}", self.socket_directory, e);
            }
        }
        
        Ok(())
    }
}

impl Drop for SocketManager {
    fn drop(&mut self) {
        if self.cleanup_on_drop {
            if let Err(e) = self.cleanup() {
                eprintln!("Warning: Failed to cleanup sockets during drop: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_socket_manager_creation() {
        let temp_dir = env::temp_dir();
        let mut manager = SocketManager::new(&temp_dir).unwrap();
        
        manager.disable_cleanup_on_drop();
        assert!(!manager.session_id.is_empty());
        assert_eq!(manager.working_directory_hash.len(), 16);
        assert!(manager.socket_directory.exists());
        assert!(manager.sockets.is_empty());
    }

    #[test]
    fn test_socket_types() {
        assert_eq!(SocketType::Control.filename(), "control.sock");
        assert_eq!(SocketType::Input.filename(), "input.sock");
        assert_eq!(SocketType::Output.filename(), "output.sock");
        assert_eq!(SocketType::Thinking.filename(), "thinking.sock");
        assert_eq!(SocketType::Tools.filename(), "tools.sock");
        assert_eq!(SocketType::Events.filename(), "events.sock");
        
        let all_types = SocketType::all();
        assert_eq!(all_types.len(), 6);
    }
}
