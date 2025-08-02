//! Q Chat API mode implementation
//!
//! This module contains the implementation for Q Chat's API mode,
//! which provides Unix domain sockets for programmatic interaction alongside
//! the normal terminal interface.
//!
//! Simplified version with Control and Events sockets suppressed.

pub mod connection_handler;
pub mod lifecycle_manager;
pub mod message_router;
pub mod protocol;
pub mod socket_manager;

// Re-export commonly used types
pub use connection_handler::ConnectionHandler;
pub use lifecycle_manager::SocketLifecycleManager;
pub use message_router::MessageRouter;
pub use protocol::ToolStatus;
pub use socket_manager::SocketType;
