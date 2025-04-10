use std::io;

use async_trait::async_trait;
use bytes::Buf;
use fig_proto::prost::Message;
use fig_proto::{
    FigMessage,
    ReflectMessage,
};
use tokio::io::{
    AsyncRead,
    AsyncReadExt,
};

use crate::BufferedReader;
use crate::error::RecvError;

#[async_trait]
pub trait RecvMessage {
    async fn recv_message<R>(&mut self) -> Result<Option<R>, RecvError>
    where
        R: Message + ReflectMessage + Default;
}

#[async_trait]
impl<T> RecvMessage for BufferedReader<T>
where
    T: AsyncRead + Unpin + Send,
{
    async fn recv_message<M>(&mut self) -> Result<Option<M>, RecvError>
    where
        M: Message + ReflectMessage + Default,
    {
        loop {
            // Try to parse the message until the buffer is a valid message
            let mut cursor = io::Cursor::new(&self.buffer);
            match FigMessage::parse(&mut cursor) {
                // If the parsed message is valid, return it
                Ok((len, message)) => {
                    self.buffer.advance(len);
                    return Ok(Some(message.decode()?));
                },
                // If the message is incomplete, read more into the buffer
                Err(fig_proto::FigMessageParseError::Incomplete(_, _)) => {
                    let bytes = self.inner.read_buf(&mut self.buffer).await?;

                    // If the buffer is empty, we've reached EOF
                    if bytes == 0 {
                        if self.buffer.is_empty() {
                            return Ok(None);
                        } else {
                            return Err(RecvError::Io(io::Error::from(io::ErrorKind::UnexpectedEof)));
                        }
                    }
                },
                // On any other error, return the error
                Err(err) => {
                    // TODO(grant): add resyncing to message boundary
                    let position = cursor.position() as usize;
                    self.buffer.advance(position);
                    return Err(err.into());
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::SendMessage;

    fn mock(initial: Vec<u8>) -> BufferedReader<Cursor<Vec<u8>>> {
        let size = initial.len();
        let mut inner = Cursor::new(initial);
        inner.set_position(size as u64);
        BufferedReader::new(inner)
    }

    fn test_message_small() -> fig_proto::local::LocalMessage {
        fig_proto::hooks::hook_to_message(fig_proto::hooks::new_hide_hook())
    }

    fn test_message_large() -> fig_proto::local::LocalMessage {
        fig_proto::hooks::hook_to_message(fig_proto::hooks::new_edit_buffer_hook(
            None,
            "A".repeat(10000),
            0,
            0,
            None,
        ))
    }

    #[tokio::test]
    async fn single_message_small() {
        let mut mock = mock(vec![]);
        mock.send_message(test_message_small()).await.unwrap();
        mock.inner.set_position(0);
        assert_eq!(mock.recv_message().await.unwrap(), Some(test_message_small()));
    }

    #[tokio::test]
    async fn single_message_large() {
        let mut mock = mock(vec![]);
        mock.send_message(test_message_large()).await.unwrap();
        mock.inner.set_position(0);
        assert_eq!(mock.recv_message().await.unwrap(), Some(test_message_large()));
    }

    #[tokio::test]
    async fn mutlti_message_small() {
        let mut mock = mock(vec![]);
        for _ in 0..500 {
            mock.send_message(test_message_small()).await.unwrap();
        }
        mock.inner.set_position(0);
        for _ in 0..500 {
            assert_eq!(mock.recv_message().await.unwrap(), Some(test_message_small()));
        }
        assert_eq!(mock.read(&mut [0u8]).await.unwrap(), 0);
        assert_eq!(mock.buffer.len(), 0);
    }

    #[tokio::test]
    async fn mutlti_message_large() {
        let mut mock = mock(vec![]);
        for _ in 0..500 {
            mock.send_message(test_message_large()).await.unwrap();
        }
        mock.inner.set_position(0);
        for _ in 0..500 {
            assert_eq!(mock.recv_message().await.unwrap(), Some(test_message_large()));
        }
        assert_eq!(mock.read(&mut [0u8]).await.unwrap(), 0);
        assert_eq!(mock.buffer.len(), 0);
    }

    #[tokio::test]
    async fn invalid_header() {
        let mut mock = mock(vec![b'f', b'o', b'o']);
        mock.inner.set_position(0);
        assert!(mock.recv_message::<fig_proto::local::LocalMessage>().await.is_err());
    }
}
