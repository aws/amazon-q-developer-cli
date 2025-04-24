pub mod local;

mod error;

mod buffered_reader;
mod codec;
mod recv_message;
mod send_message;
mod send_recv_message;
#[cfg(unix)]
mod unix_socket;
#[cfg(windows)]
mod windows_pipe;

pub use buffered_reader::BufferedReader;
pub use codec::Base64LineCodec;
pub use error::{
    ConnectError,
    Error,
    RecvError,
    SendError,
};
pub use recv_message::RecvMessage;
pub use send_message::SendMessage;
pub use send_recv_message::SendRecvMessage;

// Export platform-specific implementations
#[cfg(unix)]
pub use unix_socket::{
    BufferedUnixStream,
    socket_connect,
    socket_connect_timeout,
    validate_socket,
};

#[cfg(windows)]
pub use windows_pipe::{
    BufferedUnixStream,
    socket_connect,
    socket_connect_timeout,
    validate_socket,
};

// Re-export tests for Windows pipe module
#[cfg(test)]
pub mod tests {
    #[cfg(windows)]
    pub use crate::windows_pipe::tests::*;
}
