pub mod command;
pub mod daemon;
pub mod figterm;
pub mod hook;

use std::fmt::Debug;
use std::io::{
    Cursor,
    Write,
};
#[cfg(unix)]
use std::os::unix::net::UnixStream as SyncUnixStream;
use std::path::{
    Path,
    PathBuf,
};
use std::time::Duration;

use anyhow::{
    bail,
    Result,
};
use bytes::BytesMut;
use fig_proto::{
    FigMessage,
    FigProtobufEncodable,
};
use prost::Message;
use prost_reflect::ReflectMessage;
use system_socket::SystemStream;
use thiserror::Error;
use tokio::io::{
    self,
    AsyncRead,
    AsyncReadExt,
    AsyncWrite,
    AsyncWriteExt,
};
use tracing::{
    error,
    trace,
};

/// Get path to "/var/tmp/fig/$USERNAME/fig.socket"
pub fn get_fig_socket_path() -> PathBuf {
    cfg_if::cfg_if! {
        if #[cfg(unix)] {
            if wsl::is_wsl() {
                PathBuf::from("/mnt/c/fig/fig.socket")
            } else {
                [
                    Path::new("/var/tmp/fig"),
                    Path::new(&whoami::username()),
                    Path::new("fig.socket"),
                ]
                .into_iter()
                .collect()
            }
        } else if #[cfg(windows)] {
            PathBuf::from(r"C:\fig\fig.socket")
        } else {
            compile_error!("Unsupported platform");
        }
    }
}

pub async fn connect(socket: impl AsRef<Path>) -> Result<SystemStream> {
    let socket = socket.as_ref();
    let conn = match SystemStream::connect(socket).await {
        Ok(conn) => conn,
        Err(err) => {
            error!("Failed to connect to {socket:?}: {err}");
            bail!("Failed to connect to {socket:?}: {err}");
        },
    };

    #[cfg(target_os = "macos")]
    // When on macOS after the socket connection is made a brief delay is required
    // Not sure why, so this is a workaround
    tokio::time::sleep(Duration::from_millis(2)).await;

    trace!("Connected to {socket:?}");

    Ok(conn)
}

/// Connect to a system socket with a timeout
pub async fn connect_timeout(socket: impl AsRef<Path>, timeout: Duration) -> Result<SystemStream> {
    let socket = socket.as_ref();
    let conn = match tokio::time::timeout(timeout, SystemStream::connect(socket)).await {
        Ok(Ok(conn)) => conn,
        Ok(Err(err)) => {
            error!("Failed to connect to {socket:?}: {err}");
            bail!("Failed to connect to {socket:?}: {err}");
        },
        Err(_) => {
            error!("Timeout while connecting to {socket:?}");
            bail!("Timeout while connecting to {socket:?}");
        },
    };

    #[cfg(target_os = "macos")]
    // When on macOS after the socket connection is made a brief delay is required
    // Not sure why, so this is a workaround
    tokio::time::sleep(Duration::from_millis(2)).await;

    trace!("Connected to {socket:?}");

    Ok(conn)
}

/// Connect to `socket` synchronously without a timeout
#[cfg(unix)]
pub fn connect_sync(socket: impl AsRef<Path>) -> Result<SyncUnixStream> {
    let socket = socket.as_ref();
    let conn = match SyncUnixStream::connect(socket) {
        Ok(conn) => conn,
        Err(err) => {
            error!("Failed to connect to {socket:?}: {err}");
            bail!("Failed to connect to {socket:?}: {err}");
        },
    };

    trace!("Connected to {socket:?}");

    // When on macOS after the socket connection is made a brief delay is required
    // Not sure why, but this is a workaround
    #[cfg(target_os = "macos")]
    std::thread::sleep(std::time::Duration::from_millis(2));

    Ok(conn)
}

pub async fn send_message<M, S>(stream: &mut S, message: M) -> Result<()>
where
    M: FigProtobufEncodable,
    S: AsyncWrite + Unpin,
{
    let encoded_message = match message.encode_fig_protobuf() {
        Ok(encoded_message) => encoded_message,
        Err(err) => {
            error!("Failed to encode message: {err}");
            bail!("Failed to encode message: {err}");
        },
    };

    match stream.write_all(&encoded_message).await {
        Ok(()) => {
            trace!("Sent message: {message:?}");
        },
        Err(err) => {
            error!("Failed to write message: {err}");
            bail!("Failed to write message: {err}");
        },
    };

    Ok(())
}

pub fn send_message_sync<M, S>(stream: &mut S, message: M) -> Result<()>
where
    M: FigProtobufEncodable,
    S: Write,
{
    let encoded_message = match message.encode_fig_protobuf() {
        Ok(encoded_message) => encoded_message,
        Err(err) => {
            error!("Failed to encode message: {err}");
            bail!("Failed to encode message: {err}");
        },
    };

    match stream.write_all(&encoded_message) {
        Ok(()) => {
            trace!("Sent message: {message:?}");
        },
        Err(err) => {
            error!("Failed to write message: {err}");
            bail!("Failed to write message: {err}");
        },
    };

    Ok(())
}

#[derive(Debug, Error)]
pub enum RecvError {
    #[error("failed to read from stream: {0}")]
    Io(#[from] io::Error),
    #[error("failed to parse message: {0}")]
    Parse(#[from] fig_proto::FigMessageParseError),
    #[error("failed to decode message: {0}")]
    Decode(#[from] fig_proto::FigMessageDecodeError),
}

impl RecvError {
    pub fn is_disconnect(&self) -> bool {
        if let RecvError::Io(io) = self {
            matches!(io.kind(), std::io::ErrorKind::ConnectionAborted)
        } else {
            false
        }
    }
}

pub async fn recv_message<T, S>(stream: &mut S) -> Result<Option<T>, RecvError>
where
    T: Message + ReflectMessage + Default,
    S: AsyncRead + Unpin,
{
    let mut buffer = BytesMut::with_capacity(1024);

    macro_rules! read_buffer {
        () => {{
            let n = stream.read_buf(&mut buffer).await?;
            if n == 0 {
                if buffer.is_empty() {
                    // If the buffer is empty, we've reached EOF
                    return Ok(None);
                } else {
                    // If the buffer is not empty, the connection was reset
                    return Err(io::Error::from(io::ErrorKind::ConnectionReset).into());
                }
            }
            n
        }};
    }

    // Read into buffer the first time
    read_buffer!();

    loop {
        // Try to parse the message until the buffer is a valid message
        let mut cursor = Cursor::new(buffer.as_ref());
        match FigMessage::parse(&mut cursor) {
            // If the parsed message is valid, return it
            Ok(message) => return Ok(Some(message.decode()?)),
            // If the message is incomplete, read more into the buffer
            Err(fig_proto::FigMessageParseError::Incomplete) => {
                read_buffer!();
            },
            // On any other error, return the error
            Err(err) => {
                return Err(err.into());
            },
        }
    }
}

pub async fn send_recv_message<M, R, S>(stream: &mut S, message: M) -> Result<Option<R>>
where
    M: FigProtobufEncodable,
    R: Message + ReflectMessage + Default,
    S: AsyncReadExt + AsyncWriteExt + Unpin,
{
    send_message(stream, message).await?;
    Ok(recv_message(stream).await?)
}

pub async fn send_recv_message_timeout<M, R, S>(stream: &mut S, message: M, timeout: Duration) -> Result<Option<R>>
where
    M: FigProtobufEncodable,
    R: Message + ReflectMessage + Default,
    S: AsyncReadExt + AsyncWriteExt + Unpin,
{
    send_message(stream, message).await?;
    Ok(match tokio::time::timeout(timeout, recv_message(stream)).await {
        Ok(result) => result?,
        Err(_) => {
            error!("Timeout while receiving message");
            bail!("Timeout while receiving message");
        },
    })
}
