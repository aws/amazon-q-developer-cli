use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Connect(#[from] ConnectError),
    #[error(transparent)]
    Send(#[from] SendError),
    #[error(transparent)]
    Recv(#[from] RecvError),
    #[error("timeout")]
    Timeout,
    #[error(transparent)]
    Dir(#[from] fig_util::directories::DirectoryError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[cfg(unix)]
    #[error(transparent)]
    Nix(#[from] nix::Error),
}

#[derive(Debug, Error)]
pub enum ConnectError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("timeout connecting to socket")]
    Timeout,
    #[error("invalid permissions on socket dir")]
    IncorrectSocketPermissions,
}

#[derive(Debug, Error)]
pub enum SendError {
    #[error(transparent)]
    Encode(#[from] fig_proto::FigMessageEncodeError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Error)]
pub enum RecvError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Parse(#[from] fig_proto::FigMessageParseError),
    #[error(transparent)]
    Decode(#[from] fig_proto::FigMessageDecodeError),
    #[error("invalid message type")]
    InvalidMessageType,
}

impl RecvError {
    pub fn is_disconnect(&self) -> bool {
        if let RecvError::Io(io) = self {
            #[cfg(windows)]
            {
                // Windows error code
                let wsaeconnreset = 10054;
                if let Some(err) = io.raw_os_error() {
                    if err == wsaeconnreset {
                        return true;
                    }
                }
            }
            matches!(io.kind(), std::io::ErrorKind::ConnectionAborted)
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_disconnect() {
        assert!(!RecvError::Decode(fig_proto::FigMessageDecodeError::NameNotValid("test".to_string())).is_disconnect());
        assert!(RecvError::Io(std::io::Error::new(std::io::ErrorKind::ConnectionAborted, "error")).is_disconnect());
        assert!(!RecvError::Io(std::io::Error::new(std::io::ErrorKind::WouldBlock, "error")).is_disconnect());
    }
}
