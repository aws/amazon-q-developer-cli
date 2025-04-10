use std::path::Path;
use std::time::Duration;

use tokio::net::UnixStream;
use tracing::{
    debug,
    error,
    trace,
    warn,
};

use crate::{
    BufferedReader,
    ConnectError,
};

struct OctalU32(u32);

impl std::fmt::Display for OctalU32 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:o}", self.0)
    }
}

pub async fn validate_socket(socket: impl AsRef<Path>) -> Result<(), ConnectError> {
    cfg_if::cfg_if! {
            if #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;

            let socket = socket.as_ref();

            match tokio::fs::metadata(socket).await {
                Ok(metadata) => {
                    let mode = metadata.permissions().mode();
                    if validate_mode_bits(mode, 0o600) {
                        debug!(?socket, mode =% OctalU32(mode), "Socket permissions are 0o600");
                        return Ok(());
                    }
                    warn!(?socket, mode =% OctalU32(mode), "Socket permissions are not 0o600");
                },
                Err(err) => {
                    warn!(%err, ?socket, "Failed to get socket metadata, checking parent folder permissions");
                },
            }

            if let Some(parent_path) = socket.parent() {
                let metadata = tokio::fs::metadata(parent_path).await?;
                let mode = metadata.permissions().mode();
                if validate_mode_bits(mode, 0o700) {
                    debug!(?socket, mode =% OctalU32(mode), "Socket folder permissions are 0o700");
                    return Ok(());
                }
                warn!(?socket, mode =% OctalU32(mode), "Socket folder permissions are not 0o700");
            }

            error!(?socket, "Incorrect socket permissions, not connecting to socket");
            Err(ConnectError::IncorrectSocketPermissions)
        } else {
            compile_error!("Unsupported platform");
        }
    }
}

/// Connects to a unix socket
pub async fn socket_connect(socket_path: impl AsRef<Path>) -> Result<UnixStream, ConnectError> {
    let socket_path = socket_path.as_ref();

    validate_socket(&socket_path).await?;

    let stream = match UnixStream::connect(&socket_path).await {
        Ok(stream) => stream,
        Err(err) => {
            error!(%err, ?socket_path, "Failed to connect");
            return Err(err.into());
        },
    };

    // Set lower permissions bits to 0o600
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = tokio::fs::metadata(&socket_path).await {
            let mut permissions = metadata.permissions();
            let new_mode = 0o600 | (permissions.mode() & !0o777);
            if permissions.mode() != new_mode {
                debug!(?socket_path, mode =% OctalU32(new_mode), "Setting socket permissions to 0o600");
                permissions.set_mode(new_mode);
                if let Err(err) = tokio::fs::set_permissions(&socket_path, permissions).await {
                    warn!(%err, ?socket_path, "Failed to set socket permissions");
                }
            }
        }
    }

    trace!(?socket_path, "Connected");

    Ok(stream)
}

/// Connects to a unix socket with a timeout
pub async fn socket_connect_timeout(socket: impl AsRef<Path>, timeout: Duration) -> Result<UnixStream, ConnectError> {
    let socket = socket.as_ref();
    match tokio::time::timeout(timeout, socket_connect(&socket)).await {
        Ok(Ok(conn)) => Ok(conn),
        Ok(Err(err)) => Err(err),
        Err(_) => {
            error!(?socket, ?timeout, "Timeout while connecting");
            Err(ConnectError::Timeout)
        },
    }
}

/// Checks all the lower bits of a permission with the mask 0o777
fn validate_mode_bits(left: u32, right: u32) -> bool {
    left & 0o777 == right & 0o777
}

pub type BufferedUnixStream = BufferedReader<UnixStream>;

impl BufferedUnixStream {
    /// Connect to a unix socket
    pub async fn connect(socket: impl AsRef<Path>) -> Result<Self, ConnectError> {
        Ok(Self::new(socket_connect(socket).await?))
    }

    /// Connect to a unix socket with a timeout
    pub async fn connect_timeout(socket: impl AsRef<Path>, timeout: Duration) -> Result<Self, ConnectError> {
        Ok(Self::new(socket_connect_timeout(socket, timeout).await?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_validate_socket() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("socket.sock");

        let socket_thread = tokio::spawn({
            let socket_path = socket_path.clone();
            async move {
                let listener = tokio::net::UnixListener::bind(&socket_path).unwrap();
                listener.accept().await.unwrap();
            }
        });

        assert!(socket_connect(&socket_path).await.is_err());

        #[cfg(unix)]
        {
            use std::fs::Permissions;
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&dir, Permissions::from_mode(0o700))
                .await
                .unwrap();
        }

        let _stream = socket_connect(&socket_path).await.expect("Failed to validate socket");

        socket_thread.abort();
    }

    /// If this test fails, we need to reevaluate the permissions model design around our sockets
    /// and double check with security
    #[test]
    fn test_validate_mode_bits() {
        let valid = 0o700;

        for i in 0..0o700 {
            assert!(!validate_mode_bits(i, valid));
        }

        for i in 0o701..0o777 {
            assert!(!validate_mode_bits(i, valid));
        }
    }
}
