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
    line_delimited: AnyDelimiterCodec,
    compressed: bool,
    _phantom: PhantomData<T>,
}

impl<T: Message> Default for Base64LineCodec<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Message> Base64LineCodec<T> {
    pub fn new() -> Base64LineCodec<T> {
        Base64LineCodec {
            line_delimited: AnyDelimiterCodec::new(b"\r\n".into(), b"\n".into()),
            compressed: false,
            _phantom: PhantomData,
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
        let line = match self.line_delimited.decode(src) {
            Ok(Some(line)) => line,
            Ok(None) => return Ok(None),
            Err(AnyDelimiterCodecError::Io(io)) => return Err(io),
            Err(err @ AnyDelimiterCodecError::MaxChunkLengthExceeded) => {
                return Err(Error::new(std::io::ErrorKind::Other, err.to_string()));
            },
        };
        let base64_decoded = BASE64_STANDARD
            .decode(line)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;
        let message = T::decode(&*base64_decoded)?;
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
        match self.line_delimited.encode(&base64_encoded, dst) {
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
    use std::collections::HashMap;

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

    // We load real env vars/alias from cloudshell to test with so the
    // compression ratio test is accurate
    const ENV: &str = include_str!("../../test/data/env.txt");
    const ALIAS: &str = include_str!("../../test/data/alias.txt");

    fn mock_message() -> Hostbound {
        let environment_variables = ENV
            .lines()
            .filter(|line| !line.starts_with('#'))
            .map(|line| {
                let (key, value) = line.split_once('=').unwrap();
                (key.into(), if value.is_empty() { None } else { Some(value.into()) })
            })
            .collect::<HashMap<String, Option<String>>>();
        let get_var = |key: &str| environment_variables.get(key).cloned().unwrap_or(None);

        Hostbound {
            packet: Some(hostbound::Packet::Request(hostbound::Request {
                nonce: None,
                request: Some(hostbound::request::Request::Prompt(PromptHook {
                    context: Some(ShellContext {
                        pid: Some(123456),
                        ttys: get_var("TTY"),
                        process_name: Some("zsh".into()),
                        current_working_directory: get_var("PWD"),
                        session_id: Some(uuid::Uuid::new_v4().to_string()),
                        terminal: None,
                        hostname: Some("cloudshell-user@127.0.0.1.ec2.internal".into()),
                        shell_path: get_var("SHELL"),
                        wsl_distro: None,
                        environment_variables: environment_variables
                            .into_iter()
                            .map(|(key, value)| EnvironmentVariable { key, value })
                            .collect(),
                        qterm_version: Some(env!("CARGO_PKG_VERSION").into()),
                        preexec: Some(false),
                        osc_lock: Some(false),
                        alias: Some(ALIAS.into()),
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

        let ratio = compressed_size as f64 / uncompressed_size as f64;
        println!("Compression ratio: {:.2}%", ratio * 100.0);
        println!("Size: {uncompressed_size} -> {compressed_size}");

        // Just make sure the size is at least somewhat smaller, the real value is closer to 0.5
        assert!(ratio < 0.9);
    }

    #[test]
    fn round_trip() {
        let message = mock_message();

        let mut encoder = Base64LineCodec::new();
        let mut dst = BytesMut::new();
        encoder.encode(message.clone(), &mut dst).unwrap();

        let mut decoder = Base64LineCodec::new();
        let mut src = BytesMut::from(dst.as_ref());
        let decoded_message = decoder.decode(&mut src).unwrap().unwrap();

        assert_eq!(message, decoded_message);
    }
}
