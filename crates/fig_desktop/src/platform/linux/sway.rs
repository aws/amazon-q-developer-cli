use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

use anyhow::{
    Error,
    anyhow,
};
use bytes::{
    Buf,
    BufMut,
    Bytes,
    BytesMut,
};
use fig_util::Terminal;
use flume::Receiver;
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::{
    Value,
    json,
};
use tao::dpi::{
    LogicalPosition,
    LogicalSize,
};
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use tracing::{
    error,
    info,
    trace,
    warn,
};

use super::integrations::GSE_ALLOWLIST;
use crate::EventLoopProxy;
use crate::utils::Rect;

#[derive(Debug, Serialize)]
pub struct SwayState {
    pub active_window_rect: Mutex<Option<Rect>>,
    pub active_terminal: Mutex<Option<Terminal>>,
    #[serde(skip)]
    pub sway_tx: flume::Sender<SwayCommand>,
}

pub enum SwayCommand {
    PositionWindow { x: i64, y: i64 },
}

impl SwayCommand {
    #[allow(dead_code)]
    fn to_request(&self) -> String {
        match self {
            SwayCommand::PositionWindow { x, y } => {
                format!("for_window [window_role=\"autocomplete\"] move absolute position {x} px {y} px")
            },
        }
    }
}

#[derive(Debug)]
enum Payload {
    Json(Value),
    #[allow(dead_code)]
    Bytes(Bytes),
}

#[derive(Debug)]
struct I3Ipc {
    payload_type: u32,
    payload: Payload,
}

enum ParseResult {
    Ok { i3ipc: I3Ipc, size: usize },
    Incomplete,
    Error(Error),
}

impl I3Ipc {
    pub fn parse(src: &mut Cursor<&[u8]>) -> ParseResult {
        if src.remaining() < 14 {
            return ParseResult::Incomplete;
        }

        let mut magic_string = [0; 6];
        src.copy_to_slice(&mut magic_string);
        if &magic_string != b"i3-ipc" {
            return ParseResult::Error(anyhow!("header is not `i3-ipc`"));
        }

        let mut payload_length_buf = [0; 4];
        src.copy_to_slice(&mut payload_length_buf);
        let payload_length = u32::from_ne_bytes(payload_length_buf);

        let mut payload_type_buf = [0; 4];
        src.copy_to_slice(&mut payload_type_buf);
        let payload_type = u32::from_ne_bytes(payload_type_buf);

        if src.remaining() < payload_length as usize {
            return ParseResult::Incomplete;
        }

        let mut payload_buf = vec![0; payload_length as usize];
        src.copy_to_slice(&mut payload_buf);

        let payload = Payload::Json(match serde_json::from_slice(&payload_buf) {
            Ok(payload) => payload,
            Err(err) => return ParseResult::Error(err.into()),
        });

        ParseResult::Ok {
            i3ipc: Self { payload_type, payload },
            size: 14 + payload_length as usize,
        }
    }

    pub fn serialize(&self) -> Bytes {
        let payload: Bytes = match &self.payload {
            Payload::Json(value) => serde_json::to_vec(&value).unwrap().into(),
            Payload::Bytes(bytes) => bytes.clone(),
        };

        let mut bytes = BytesMut::with_capacity(14);
        bytes.extend_from_slice(b"i3-ipc");
        bytes.put_u32_le(payload.len() as u32);
        bytes.put_u32_le(self.payload_type);
        bytes.extend_from_slice(&payload);

        bytes.freeze()
    }
}

pub async fn handle_sway(
    _proxy: EventLoopProxy,
    sway_state: Arc<SwayState>,
    socket: impl AsRef<Path>,
    _sway_rx: Receiver<SwayCommand>,
) {
    use tokio::io::AsyncReadExt;

    let mut conn = UnixStream::connect(socket).await.unwrap();

    let message = I3Ipc {
        payload_type: 2,
        payload: Payload::Json(json!(["window"])),
    };

    conn.write_all(&message.serialize()).await.unwrap();

    let mut buf = BytesMut::new();
    loop {
        tokio::select! {
            res = conn.read_buf(&mut buf) => {
                res.unwrap();
                handle_incoming(&mut conn, &mut buf, &sway_state).await;
            }
            // command = sway_rx.recv_async() => {
            //     command.unwrap().to_request().as_bytes()).await.unwrap();
            //     let message = I3Ipc {
            //         payload_type: 0,
            //         payload: Payload::Bytes(Bytes::from_static(b"exit")),
            //     };
            //     conn.write_all(&message.serialize()).await.unwrap();
            // }
        }
    }
}

pub async fn handle_incoming(conn: &mut UnixStream, buf: &mut BytesMut, sway_state: &SwayState) {
    use tokio::io::AsyncReadExt;

    loop {
        match I3Ipc::parse(&mut Cursor::new(buf.as_ref())) {
            ParseResult::Ok {
                i3ipc: I3Ipc { payload, payload_type },
                size,
            } => {
                let payload = match payload {
                    Payload::Json(value) => value,
                    Payload::Bytes(_) => panic!("unimplemented"),
                };

                trace!(%payload_type, %payload, "Received event");
                buf.advance(size);

                // Handle the message
                match payload_type {
                    2 => match payload.get("success") {
                        Some(Value::Bool(true)) => info!("Successfully subscribed to sway events"),
                        _ => warn!(%payload, "Failed to subscribe to sway events"),
                    },
                    0x80000003 => match payload.get("change") {
                        Some(Value::String(event)) if event == "focus" => {
                            if let Some(Value::Object(container)) = payload.get("container") {
                                let app_id = container.get("app_id").and_then(|x| x.as_str());

                                let geometey = match container.get("rect") {
                                    Some(Value::Object(geometry)) => {
                                        let x = geometry.get("x").and_then(|x| x.as_i64()).unwrap_or(0) as f64;
                                        let y = geometry.get("y").and_then(|y| y.as_i64()).unwrap_or(0) as f64;
                                        let width = geometry.get("width").and_then(|w| w.as_i64()).unwrap_or(0) as f64;
                                        let height =
                                            geometry.get("height").and_then(|h| h.as_i64()).unwrap_or(0) as f64;

                                        Rect {
                                            position: LogicalPosition { x, y }.into(),
                                            size: LogicalSize { width, height }.into(),
                                        }
                                    },
                                    _ => {
                                        tracing::warn!(?app_id, "Failed to get window geometey");
                                        continue;
                                    },
                                };

                                tracing::trace!(?geometey, ?app_id, "Received focus change");

                                if let Some(app_id) = app_id {
                                    let terminal = GSE_ALLOWLIST.get(app_id);
                                    tracing::debug!(?terminal, "TERMINAL");

                                    if let Some(terminal) = terminal {
                                        *sway_state.active_window_rect.lock() = Some(geometey);
                                        *sway_state.active_terminal.lock() = Some(terminal.clone());
                                    }
                                }
                            }
                        },
                        Some(Value::String(event)) => trace!("Unknown event: {event}"),
                        Some(event) => trace!(%event, "Unknown event"),
                        None => trace!(event = "None", "Unknown event"),
                    },
                    _ => trace!(%payload_type, %payload, "Unknown payload type"),
                }
                break;
            },
            ParseResult::Incomplete => {
                conn.read_buf(buf).await.unwrap();
                continue;
            },
            ParseResult::Error(err) => {
                error!(%err, "Failed to parse sway message");
                break;
            },
        }
    }
}
