use std::sync::{
    Mutex,
    OnceLock,
};

use fig_ipc::SendMessage as _;
use fig_proto::figterm::figterm_request_message::Request as FigtermRequest;
use fig_proto::figterm::{
    FigtermRequestMessage,
    TelemetryRequest,
};
use fig_util::env_var::QTERM_SESSION_ID;
use tracing::error;

use crate::event::AppTelemetryEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchMode {
    Off,
    On,
}

static DISPATCH_MODE: Mutex<DispatchMode> = Mutex::new(DispatchMode::Off);

pub fn dispatch_mode() -> DispatchMode {
    *DISPATCH_MODE.lock().expect("Failed to lock dispatch mode")
}

pub fn set_dispatch_mode(mode: DispatchMode) {
    *DISPATCH_MODE.lock().expect("Failed to lock dispatch mode") = mode;
}

fn q_term_session_id() -> &'static Option<String> {
    static SESSION_ID: OnceLock<Option<String>> = OnceLock::new();
    SESSION_ID.get_or_init(|| std::env::var(QTERM_SESSION_ID).ok())
}

pub(crate) enum DispatchStatus {
    Failed,
    Succeeded,
    NotEnabled,
}

impl DispatchStatus {
    pub fn should_fallback(self) -> bool {
        !matches!(self, Self::Succeeded)
    }
}

pub(crate) async fn dispatch(event: &AppTelemetryEvent) -> DispatchStatus {
    if dispatch_mode() == DispatchMode::Off {
        return DispatchStatus::NotEnabled;
    };

    let Some(session_id) = q_term_session_id() else {
        return DispatchStatus::Failed;
    };

    let event_blob = match serde_json::to_string(event) {
        Ok(event_blob) => event_blob,
        Err(err) => {
            error!(%err, "Failed to serialize event");
            return DispatchStatus::Failed;
        },
    };

    let socket_path = match fig_util::directories::figterm_socket_path(session_id) {
        Ok(socket_path) => socket_path,
        Err(err) => {
            error!(%err, "Failed to get figterm socket path");
            return DispatchStatus::Failed;
        },
    };

    let mut socket = match fig_ipc::socket_connect(socket_path).await {
        Ok(socket) => socket,
        Err(err) => {
            error!(%err, "Failed to connect to figterm socket");
            return DispatchStatus::Failed;
        },
    };

    match socket
        .send_message(FigtermRequestMessage {
            request: Some(FigtermRequest::Telemtety(TelemetryRequest { event_blob })),
        })
        .await
    {
        Ok(_) => DispatchStatus::Succeeded,
        Err(err) => {
            error!(%err, "Failed to send telemetry event");
            DispatchStatus::Failed
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dispatch_mode() {
        assert_eq!(dispatch_mode(), DispatchMode::Off);
        set_dispatch_mode(DispatchMode::On);
        assert_eq!(dispatch_mode(), DispatchMode::On);
        set_dispatch_mode(DispatchMode::Off);
        assert_eq!(dispatch_mode(), DispatchMode::Off);
    }

    #[test]
    fn test_q_term_session_id() {
        q_term_session_id();
    }
}
