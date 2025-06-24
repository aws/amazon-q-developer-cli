use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to decode base64: {0}")]
    Base64Decode(#[from] base64::DecodeError),
    #[error("Failed to decode message: {0}")]
    ProtoDecode(#[from] fig_proto::prost::DecodeError),
    #[error("timeout")]
    Timeout,
    #[error("no message id")]
    NoMessageId,
}
