use std::path::Path;
use std::time::Duration;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite};
use tracing::{debug, error, trace, warn};
use pin_project_lite::pin_project;

use crate::{BufferedReader, ConnectError};

// Windows named pipe implementation
pin_project! {
    /// A wrapper around a Windows named pipe client
    pub struct NamedPipeStream {
        #[pin]
        client: NamedPipeClient,
    }
}

// Simple wrapper for the Windows named pipe client
pub struct NamedPipeClient {
    inner: windows_named_pipe::PipeClient,
}

impl NamedPipeClient {
    pub fn new(client: windows_named_pipe::PipeClient) -> Self {
        Self { inner: client }
    }

    pub fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }

    pub fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }
}

impl NamedPipeStream {
    /// Creates a new named pipe client
    pub fn new(client: NamedPipeClient) -> Self {
        Self { client }
    }
}

impl AsyncRead for NamedPipeStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // Use the underlying client for reading
        let this = self.project();
        let slice = buf.initialize_unfilled();
        match this.client.read(slice) {
            Ok(n) => {
                buf.advance(n);
                Poll::Ready(Ok(()))
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

impl AsyncWrite for NamedPipeStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        // Use the underlying client for writing
        let this = self.project();
        match this.client.write(buf) {
            Ok(n) => Poll::Ready(Ok(n)),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        // Named pipes don't need explicit flushing
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        // No explicit shutdown needed
        Poll::Ready(Ok(()))
    }
}

/// Converts a path to a Windows named pipe name
pub fn path_to_pipe_name(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    let path_str = path.to_string_lossy();
    
    // Replace forward slashes with backslashes
    let path_str = path_str.replace("/", "\\");
    
    // Create a valid pipe name
    format!(r"\\.\pipe\{}", path_str)
}

/// Validates that the pipe path is valid
pub async fn validate_pipe(_pipe: impl AsRef<Path>) -> Result<(), ConnectError> {
    // Windows named pipes don't need the same permission validation as Unix sockets
    Ok(())
}

/// Connects to a Windows named pipe
pub async fn pipe_connect(pipe_path: impl AsRef<Path>) -> Result<NamedPipeStream, ConnectError> {
    let pipe_path = pipe_path.as_ref();
    let pipe_name = path_to_pipe_name(pipe_path);

    debug!(?pipe_path, ?pipe_name, "Connecting to named pipe");

    // Try to connect to the pipe
    match windows_named_pipe::PipeClient::connect(&pipe_name) {
        Ok(client) => {
            trace!(?pipe_path, "Connected to named pipe");
            Ok(NamedPipeStream::new(NamedPipeClient::new(client)))
        }
        Err(err) => {
            error!(%err, ?pipe_path, "Failed to connect to named pipe");
            Err(ConnectError::Io(err))
        }
    }
}

/// Connects to a Windows named pipe with a timeout
pub async fn pipe_connect_timeout(
    pipe: impl AsRef<Path>,
    timeout: Duration,
) -> Result<NamedPipeStream, ConnectError> {
    let pipe = pipe.as_ref();
    match tokio::time::timeout(timeout, async {
        // Try connecting with retries
        let mut attempts = 0;
        let max_attempts = 5;
        let retry_delay = Duration::from_millis(100);

        loop {
            match pipe_connect(&pipe).await {
                Ok(stream) => return Ok(stream),
                Err(err) => {
                    attempts += 1;
                    if attempts >= max_attempts {
                        return Err(err);
                    }
                    warn!(%err, ?pipe, attempts, "Retrying pipe connection");
                    tokio::time::sleep(retry_delay).await;
                }
            }
        }
    })
    .await
    {
        Ok(Ok(conn)) => Ok(conn),
        Ok(Err(err)) => Err(err),
        Err(_) => {
            error!(?pipe, ?timeout, "Timeout while connecting to named pipe");
            Err(ConnectError::Timeout)
        }
    }
}

pub type BufferedNamedPipeStream = BufferedReader<NamedPipeStream>;

impl BufferedNamedPipeStream {
    /// Connect to a Windows named pipe
    pub async fn connect(pipe: impl AsRef<Path>) -> Result<Self, ConnectError> {
        Ok(Self::new(pipe_connect(pipe).await?))
    }

    /// Connect to a Windows named pipe with a timeout
    pub async fn connect_timeout(
        pipe: impl AsRef<Path>,
        timeout: Duration,
    ) -> Result<Self, ConnectError> {
        Ok(Self::new(pipe_connect_timeout(pipe, timeout).await?))
    }
}

// Create Windows-compatible versions of the Unix socket functions
pub use pipe_connect as socket_connect;
pub use pipe_connect_timeout as socket_connect_timeout;
pub use validate_pipe as validate_socket;
pub use BufferedNamedPipeStream as BufferedUnixStream;

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::sync::oneshot;

    #[test]
    pub fn test_path_to_pipe_name() {
        assert_eq!(
            path_to_pipe_name(r"C:\temp\socket.sock"),
            r"\\.\pipe\C:\temp\socket.sock"
        );
        assert_eq!(
            path_to_pipe_name("/tmp/socket.sock"),
            r"\\.\pipe\\tmp\socket.sock"
        );
        
        // Test with special characters
        assert_eq!(
            path_to_pipe_name(r"C:\temp\socket with spaces.sock"),
            r"\\.\pipe\C:\temp\socket with spaces.sock"
        );
        
        // Test with long paths
        let long_path = "C:\\".to_string() + &"a".repeat(200) + "\\socket.sock";
        assert_eq!(
            path_to_pipe_name(&long_path),
            format!(r"\\.\pipe\{}", long_path)
        );
    }

    // This test will only run on Windows
    #[cfg(windows)]
    #[tokio::test]
    pub async fn test_pipe_connect() {
        use windows_named_pipe::{PipeOptions, PipeMode};
        use std::thread;
        
        // Create a unique pipe name for this test
        let pipe_name = format!(r"\\.\pipe\test_pipe_{}", uuid::Uuid::new_v4());
        let pipe_path = Path::new(&pipe_name);
        
        // Start a server in a separate thread
        let server_pipe_name = pipe_name.clone();
        let server_thread = thread::spawn(move || {
            // Create a named pipe server
            let server = windows_named_pipe::PipeServer::create(
                &server_pipe_name,
                PipeOptions::new().mode(PipeMode::Message),
            ).expect("Failed to create pipe server");
            
            // Wait for a client to connect
            server.connect().expect("Failed to connect to client");
            
            // Write a test message
            let message = b"Hello from pipe server";
            server.write(message).expect("Failed to write to pipe");
            
            // Read the response
            let mut buffer = [0u8; 1024];
            let bytes_read = server.read(&mut buffer).expect("Failed to read from pipe");
            
            assert_eq!(&buffer[..bytes_read], b"Hello from pipe client");
        });
        
        // Give the server time to start
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Connect to the pipe
        let mut stream = pipe_connect(pipe_path).await.expect("Failed to connect to pipe");
        
        // Create a buffered reader
        let mut buffered_stream = BufferedReader::new(stream);
        
        // Read the message
        let mut buffer = [0u8; 1024];
        let bytes_read = buffered_stream.get_mut().read(&mut buffer).await.expect("Failed to read from pipe");
        
        assert_eq!(&buffer[..bytes_read], b"Hello from pipe server");
        
        // Write a response
        buffered_stream.get_mut().write(b"Hello from pipe client").await.expect("Failed to write to pipe");
        
        // Wait for the server to finish
        server_thread.join().expect("Server thread panicked");
    }

    // This test will only run on Windows
    #[cfg(windows)]
    #[tokio::test]
    pub async fn test_pipe_connect_timeout() {
        // Test connecting to a non-existent pipe with timeout
        let pipe_path = Path::new(r"\\.\pipe\non_existent_pipe");
        let result = pipe_connect_timeout(pipe_path, Duration::from_millis(100)).await;
        
        assert!(result.is_err());
        match result {
            Err(ConnectError::Timeout) => {}, // Expected
            Err(e) => panic!("Expected timeout error, got: {:?}", e),
            Ok(_) => panic!("Expected error, got success"),
        }
    }

    // Mock test that can run on any platform
    #[test]
    pub fn test_named_pipe_client_mock() {
        use std::io::{Read, Write};
        
        // Create a mock implementation for testing
        struct MockPipeClient {
            data: Vec<u8>,
        }
        
        impl MockPipeClient {
            fn new() -> Self {
                Self { data: Vec::new() }
            }
            
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                let len = std::cmp::min(buf.len(), self.data.len());
                if len == 0 {
                    return Err(io::Error::new(io::ErrorKind::WouldBlock, "No data"));
                }
                
                buf[..len].copy_from_slice(&self.data[..len]);
                self.data = self.data[len..].to_vec();
                Ok(len)
            }
            
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.data.extend_from_slice(buf);
                Ok(buf.len())
            }
        }
        
        // Create a mock client
        let mut client = MockPipeClient::new();
        
        // Write data
        let write_result = client.write(b"Test data").unwrap();
        assert_eq!(write_result, 9);
        
        // Read data
        let mut buffer = [0u8; 4];
        let read_result = client.read(&mut buffer).unwrap();
        assert_eq!(read_result, 4);
        assert_eq!(&buffer, b"Test");
        
        // Read more data
        let read_result = client.read(&mut buffer).unwrap();
        assert_eq!(read_result, 4);
        assert_eq!(&buffer, b" dat");
        
        // Read remaining data
        let mut buffer = [0u8; 1];
        let read_result = client.read(&mut buffer).unwrap();
        assert_eq!(read_result, 1);
        assert_eq!(&buffer, b"a");
        
        // Try to read when no data is available
        let result = client.read(&mut buffer);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::WouldBlock);
    }
}
