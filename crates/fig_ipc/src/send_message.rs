use async_trait::async_trait;
use fig_proto::FigProtobufEncodable;
use tokio::io::{
    AsyncWrite,
    AsyncWriteExt,
};
use tracing::{
    error,
    trace,
};

use crate::SendError;

#[async_trait]
pub trait SendMessage {
    async fn send_message<M>(&mut self, message: M) -> Result<(), SendError>
    where
        M: FigProtobufEncodable;
}

#[async_trait]
impl<T> SendMessage for T
where
    T: AsyncWrite + Unpin + Send,
{
    async fn send_message<M>(&mut self, message: M) -> Result<(), SendError>
    where
        M: FigProtobufEncodable,
    {
        let encoded_message = match message.encode_fig_protobuf() {
            Ok(encoded_message) => encoded_message,
            Err(err) => {
                error!(%err, "Failed to encode message");
                return Err(err.into());
            },
        };

        self.write_all(&encoded_message).await?;
        self.flush().await?;

        trace!(?message, "Sent message");

        Ok(())
    }
}
