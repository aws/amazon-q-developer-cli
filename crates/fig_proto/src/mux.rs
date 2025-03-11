use std::io::Read;

use flate2::Compression;
use prost::Message;
use rand::{
    Rng,
    RngCore,
};
use thiserror::Error;

pub use crate::proto::mux::*;

// The current version of the packet, to track backwards incompatible changes
pub const PACKET_VERSION: u32 = 0;

#[derive(Debug, Error)]
pub enum MuxError {
    #[error("Packet version mismatch: expect ({expected}), actual ({actual})")]
    PacketVersionMismatch { expected: u32, actual: u32 },
    #[error("Unknown compression: {}", .0)]
    UnknownCompression(i32),

    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    ProstDecode(#[from] prost::DecodeError),
}

pub struct PacketOptions {
    pub gzip: bool,
}

pub fn message_to_packet<M: Message>(message: M, options: &PacketOptions) -> Result<Packet, MuxError> {
    let mut inner = message.encode_to_vec();

    let mut rng = rand::rng();
    let nonce_length = rng.random_range(16..=32);
    let mut nonce = vec![0; nonce_length];
    rng.fill_bytes(&mut nonce);

    let compression = if options.gzip {
        let mut gz = flate2::bufread::GzEncoder::new(&*inner, Compression::default());
        let mut s = Vec::new();
        gz.read_to_end(&mut s)?;
        inner = s;
        packet::Compression::Gzip.into()
    } else {
        packet::Compression::None.into()
    };

    Ok(Packet {
        version: PACKET_VERSION,
        compression,
        nonce,
        inner,
    })
}

pub fn packet_to_message<M: Message + Default>(packet: Packet) -> Result<M, MuxError> {
    if packet.version != PACKET_VERSION {
        return Err(MuxError::PacketVersionMismatch {
            expected: PACKET_VERSION,
            actual: packet.version,
        });
    }

    let compression = match packet.compression() {
        packet::Compression::Unknown => return Err(MuxError::UnknownCompression(packet.compression)),
        packet::Compression::None => false,
        packet::Compression::Gzip => true,
    };

    let inner = if compression {
        let mut gz = flate2::bufread::GzDecoder::new(&*packet.inner);
        let mut s = Vec::new();
        gz.read_to_end(&mut s)?;
        s
    } else {
        packet.inner
    };

    Ok(Message::decode(&*inner)?)
}

#[cfg(test)]
mod test {
    use super::*;

    fn mock_inner() -> Hostbound {
        Hostbound {
            submessage: Some(hostbound::Submessage::Pong(Pong {
                message_id: uuid::Uuid::new_v4().to_string(),
            })),
        }
    }

    #[test]
    fn test_message_to_packet() {
        let options = super::PacketOptions { gzip: false };
        let inner = mock_inner();
        let packet = super::message_to_packet(inner.clone(), &options).unwrap();
        println!("{packet:?}");

        let new_inner: Hostbound = packet_to_message(packet).unwrap();
        assert_eq!(inner, new_inner);
    }
}
