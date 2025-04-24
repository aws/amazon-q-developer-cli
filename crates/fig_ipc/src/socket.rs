use std::path::Path;
use std::time::Duration;

use crate::{BufferedReader, ConnectError};

// Re-export platform-specific implementations
#[cfg(unix)]
pub use crate::unix_socket::{
    socket_connect, socket_connect_timeout, validate_socket, BufferedUnixStream as BufferedSocketStream,
};

#[cfg(windows)]
pub use crate::windows_pipe::{
    pipe_connect as socket_connect, 
    pipe_connect_timeout as socket_connect_timeout, 
    validate_pipe as validate_socket,
    BufferedNamedPipeStream as BufferedSocketStream,
};

/// A platform-agnostic wrapper for connecting to a socket or named pipe
pub async fn connect(path: impl AsRef<Path>) -> Result<BufferedSocketStream, ConnectError> {
    BufferedSocketStream::connect(path).await
}

/// A platform-agnostic wrapper for connecting to a socket or named pipe with a timeout
pub async fn connect_timeout(
    path: impl AsRef<Path>,
    timeout: Duration,
) -> Result<BufferedSocketStream, ConnectError> {
    BufferedSocketStream::connect_timeout(path, timeout).await
}
