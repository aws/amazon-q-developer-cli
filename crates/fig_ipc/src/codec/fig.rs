use std::io;

use bytes::{
    Buf,
    BytesMut,
};
use fig_proto::{
    FigMessage,
    FigMessageEncodeError,
    FigMessageParseError,
};
use tokio_util::codec::{
    Decoder,
    Encoder,
};

#[derive(Debug)]
pub struct FigCodec {
    _internal: (),
}

impl FigCodec {
    pub fn new() -> FigCodec {
        FigCodec { _internal: () }
    }
}

impl Decoder for FigCodec {
    type Error = FigMessageParseError;
    type Item = FigMessage;

    fn decode(&mut self, src: &mut bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let mut cursor = io::Cursor::new(&src);
        match FigMessage::parse(&mut cursor) {
            Ok((len, message)) => {
                src.advance(len);
                Ok(Some(message))
            },
            Err(FigMessageParseError::Incomplete(_, needed)) => {
                src.reserve(needed);
                Ok(None)
            },
            Err(err) => Err(err),
        }
    }
}

impl Encoder<FigMessage> for FigCodec {
    type Error = FigMessageEncodeError;

    fn encode(&mut self, item: FigMessage, dst: &mut BytesMut) -> Result<(), Self::Error> {
        item.encode_buf(dst)
    }
}
