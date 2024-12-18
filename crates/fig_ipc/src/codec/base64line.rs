use std::io::{
    Error,
    Write,
};
use std::marker::PhantomData;

use base64::prelude::*;
use bytes::BytesMut;
use fig_proto::prost::Message;
use flate2::Compression;
use tokio_util::codec::{
    AnyDelimiterCodec,
    AnyDelimiterCodecError,
    Decoder,
    Encoder,
};

#[derive(Debug, Clone)]
pub struct Base64LineCodec<T: Message> {
    line_delinited: AnyDelimiterCodec,
    compressed: bool,
    _a: PhantomData<T>,
}

impl<T: Message> Base64LineCodec<T> {
    pub fn new() -> Base64LineCodec<T> {
        Base64LineCodec {
            line_delinited: AnyDelimiterCodec::new(b"\r\n".into(), b"\n".into()),
            compressed: false,
            _a: PhantomData,
        }
    }

    pub fn compressed(mut self) -> Self {
        self.compressed = true;
        self
    }
}

impl<T: Message + Default> Decoder for Base64LineCodec<T> {
    type Error = Error;
    type Item = T;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let line = match self.line_delinited.decode(src) {
            Ok(Some(line)) => line,
            Ok(None) => return Ok(None),
            Err(AnyDelimiterCodecError::Io(io)) => return Err(io),
            Err(err @ AnyDelimiterCodecError::MaxChunkLengthExceeded) => {
                return Err(Error::new(std::io::ErrorKind::Other, err.to_string()));
            },
        };
        let base64_decoded = BASE64_STANDARD.decode(line).unwrap();
        let message = T::decode(&*base64_decoded).unwrap();
        Ok(Some(message))
    }
}

impl<T: Message> Encoder<T> for Base64LineCodec<T> {
    type Error = Error;

    fn encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let mut encoded_message = item.encode_to_vec();

        if self.compressed {
            let mut f = flate2::write::GzEncoder::new(Vec::new(), Compression::default());
            f.write_all(&encoded_message).unwrap();
            encoded_message = f.finish().unwrap();
        }

        let base64_encoded = BASE64_STANDARD.encode(encoded_message);
        match self.line_delinited.encode(&base64_encoded, dst) {
            Ok(()) => Ok(()),
            Err(AnyDelimiterCodecError::Io(io)) => Err(io),
            Err(err @ AnyDelimiterCodecError::MaxChunkLengthExceeded) => {
                Err(Error::new(std::io::ErrorKind::Other, err.to_string()))
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use fig_proto::fig::{
        EnvironmentVariable,
        ShellContext,
    };
    use fig_proto::local::PromptHook;
    use fig_proto::remote::{
        Hostbound,
        hostbound,
    };

    use super::*;

    fn mock_message() -> Hostbound {
        let environment_variables = include_str!("../../test/env.txt")
            .lines()
            .filter(|line| !line.starts_with('#'))
            .map(|line| {
                let (left, right) = line.split_once('=').unwrap();
                EnvironmentVariable {
                    key: left.into(),
                    value: Some(right.into()),
                }
            })
            .collect();

        Hostbound {
            packet: Some(hostbound::Packet::Request(hostbound::Request {
                nonce: None,
                request: Some(hostbound::request::Request::Prompt(PromptHook {
                    context: Some(ShellContext {
                        pid: Some(123456),
                        ttys: Some("/dev/pts/1".into()),
                        process_name: Some("zsh".into()),
                        current_working_directory: Some("/home/cloudshell-user".into()),
                        session_id: Some(uuid::Uuid::new_v4().to_string()),
                        terminal: Some("VSCode".into()),
                        hostname: Some("cloudshell-user@127.0.0.1.ec2.internal".into()),
                        shell_path: Some("/usr/bin/zsh".into()),
                        wsl_distro: None,
                        environment_variables,
                        qterm_version: Some(env!("CARGO_PKG_VERSION").into()),
                        preexec: Some(false),
                        osc_lock: Some(false),
                        alias: Some(include_str!("../../test/alias.txt").into()),
                    }),
                })),
            })),
        }
    }

    #[test]
    fn compression_ratio() {
        let message = mock_message();

        let mut encoder = Base64LineCodec::new();
        let mut dst = BytesMut::new();
        encoder.encode(message.clone(), &mut dst).unwrap();
        let uncompressed_size = dst.len();

        let mut encoder = Base64LineCodec::new().compressed();
        let mut dst = BytesMut::new();
        encoder.encode(message, &mut dst).unwrap();
        let compressed_size = dst.len();

        let ratio = compressed_size as f64 / uncompressed_size as f64 * 100.0;
        println!("Compression ratio: {ratio:.2}%");
        println!("Size: {uncompressed_size} -> {compressed_size}");
    }
}
