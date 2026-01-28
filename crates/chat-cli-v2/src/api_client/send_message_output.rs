use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{
    Mutex,
    OnceLock,
};

use aws_types::request_id::RequestId;
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::mpsc;
use typeshare::typeshare;

use crate::api_client::ApiClientError;
use crate::api_client::error::ConverseStreamError;
use crate::api_client::model::ChatResponseStream;

/// Global file handle for recording API responses (lazy initialized).
/// Mutex is required because `File` isn't `Sync`, even though in practice
/// `recv()` is called sequentially from a single async task.
static RECORD_FILE: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();

/// A mock stream item for testing. Supports 3 scenarios:
/// - `Event`: Normal streaming event, yielded from `recv()` as `Ok(Some(event))`
/// - `StreamError`: Mid-stream error, yielded from `recv()` as `Err(error)`
/// - `SendError`: Initial error, causes `send_message()` to return `Err` immediately
#[typeshare]
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "camelCase")]
pub enum MockStreamItem {
    Event(ChatResponseStream),
    StreamError(ConverseStreamError),
    SendError(ConverseStreamError),
}

impl From<ChatResponseStream> for MockStreamItem {
    fn from(event: ChatResponseStream) -> Self {
        MockStreamItem::Event(event)
    }
}

impl From<ConverseStreamError> for MockStreamItem {
    fn from(err: ConverseStreamError) -> Self {
        MockStreamItem::SendError(err)
    }
}

fn get_record_file() -> &'static Option<Mutex<std::fs::File>> {
    RECORD_FILE.get_or_init(|| {
        std::env::var("KIRO_RECORD_API_RESPONSES_PATH").ok().and_then(|path| {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .ok()
                .map(Mutex::new)
        })
    })
}

fn record_event(event: Option<&ChatResponseStream>) {
    if let Some(file) = get_record_file()
        && let Ok(mut f) = file.lock()
    {
        match event {
            Some(e) => {
                if let Ok(json) = serde_json::to_string(e) {
                    let _ = writeln!(f, "{}", json);
                }
            },
            None => {
                let _ = writeln!(f);
            },
        }
    }
}

pub fn record_send_error(err: ConverseStreamError) {
    if let Some(file) = get_record_file()
        && let Ok(mut f) = file.lock()
        && let Ok(json) = serde_json::to_string(&MockStreamItem::SendError(err))
    {
        let _ = writeln!(f, "{}", json);
        let _ = writeln!(f);
    }
}

pub fn record_mid_stream_error(err_msg: String) {
    if let Some(file) = get_record_file()
        && let Ok(mut f) = file.lock()
    {
        let _ = writeln!(f, "{}", err_msg);
        let _ = writeln!(f);
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum SendMessageOutput {
    Codewhisperer(
        amzn_codewhisperer_streaming_client::operation::generate_assistant_response::GenerateAssistantResponseOutput,
    ),
    Mock(Vec<ChatResponseStream>),
    IpcMock(mpsc::Receiver<MockStreamItem>),
}

impl SendMessageOutput {
    pub fn request_id(&self) -> Option<&str> {
        match self {
            SendMessageOutput::Codewhisperer(output) => output.request_id(),
            SendMessageOutput::Mock(_) => None,
            SendMessageOutput::IpcMock(_) => None,
        }
    }

    pub async fn recv(&mut self) -> Result<Option<ChatResponseStream>, ApiClientError> {
        match self {
            SendMessageOutput::Codewhisperer(output) => {
                let event = output.generate_assistant_response_response.recv().await;
                match event {
                    Ok(ev) => {
                        let ev = ev.map(|v| v.into());
                        record_event(ev.as_ref());
                        Ok(ev)
                    },
                    Err(err) => {
                        record_mid_stream_error(err.to_string());
                        Err(err.into())
                    },
                }
            },
            SendMessageOutput::Mock(vec) => Ok(vec.pop()),
            SendMessageOutput::IpcMock(rx) => match rx.recv().await {
                Some(MockStreamItem::Event(event)) => Ok(Some(event)),
                Some(MockStreamItem::StreamError(err)) => Err(ApiClientError::ConverseStream(err)),
                Some(MockStreamItem::SendError(_)) => {
                    unreachable!("SendError should be handled in send_message, not recv")
                },
                None => Ok(None),
            },
        }
    }
}

impl RequestId for SendMessageOutput {
    fn request_id(&self) -> Option<&str> {
        match self {
            SendMessageOutput::Codewhisperer(output) => output.request_id(),
            SendMessageOutput::Mock(_) => Some("<mock-request-id>"),
            SendMessageOutput::IpcMock(_) => Some("<ipc-mock-request-id>"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_client::error::{
        ConverseStreamError,
        ConverseStreamErrorKind,
    };

    #[test]
    fn test_serialize_stream_error() {
        let err = ConverseStreamError {
            request_id: Some("overflow-request".to_string()),
            status_code: Some(400),
            kind: ConverseStreamErrorKind::ContextWindowOverflow,
            source: None,
        };
        let item = MockStreamItem::StreamError(err);
        println!("{}", serde_json::to_string_pretty(&item).unwrap());
    }
}
