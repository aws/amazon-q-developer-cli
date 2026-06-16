//! Static per-session telemetry metadata.
//!
//! [`TelemetryContext`] holds the values that are stamped onto every event the
//! observer emits — `app_type`, optional ACP client identity, the subagent
//! flag, and a closure that returns the dynamic model id. The closure pattern
//! decouples the observer from V2's `RtsState`: V2 captures `Arc<RtsState>`
//! at session boot and produces `Arc::new(move || rts_state.model_id())`,
//! while tests pass a closure that returns a fixed string.

use std::sync::Arc;

use kiro_telemetry::metric;
use kiro_telemetry_host::Event;

/// Stable client name used by the built-in Kiro TUI. Mirrors V2's
/// `crate::constants::KIRO_ACP_CLIENT_NAME` so the observer crate doesn't take
/// a dep on V2's constants module.
pub const KIRO_ACP_CLIENT_NAME: &str = "kiro-tui";

/// Application type for telemetry — distinguishes V1, V2 (built-in TUI), and ACP (external).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppType {
    V1,
    V2,
    Acp,
}

impl AppType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::V1 => "V1",
            Self::V2 => "V2",
            Self::Acp => "ACP",
        }
    }
}

/// ACP client identity from `InitializeRequest.client_info`.
#[derive(Debug, Clone)]
pub struct AcpClientInfo {
    pub name: ClientName,
    pub version: ClientVersion,
}

/// Identifies the ACP client connecting to the agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientName {
    /// The built-in Kiro TUI (`kiro-tui`).
    Kiro,
    /// An external ACP client.
    Other(String),
    /// No client info provided (e.g. pre-initialize).
    Unknown,
}

impl ClientName {
    pub fn parse(s: &str) -> Self {
        if s == KIRO_ACP_CLIENT_NAME {
            Self::Kiro
        } else {
            Self::Other(s.to_string())
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Kiro => KIRO_ACP_CLIENT_NAME,
            Self::Other(s) => s,
            Self::Unknown => "Unknown",
        }
    }
}

/// ACP client version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientVersion {
    Known(String),
    Unknown,
}

impl ClientVersion {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Known(v) => v,
            Self::Unknown => "Unknown",
        }
    }
}

impl AcpClientInfo {
    pub fn new(name: String, version: String) -> Self {
        Self {
            name: ClientName::parse(&name),
            version: ClientVersion::Known(version),
        }
    }

    pub fn app_type(&self) -> AppType {
        match self.name {
            ClientName::Kiro => AppType::V2,
            ClientName::Other(_) | ClientName::Unknown => AppType::Acp,
        }
    }
}

/// Closure returning the current model id. V2 captures `Arc<RtsState>` and
/// returns `state.model_id()`; tests use a fixed string closure.
pub type ModelProvider = Arc<dyn Fn() -> Option<String> + Send + Sync>;

/// Static context shared across all events in a session.
#[derive(Clone)]
pub struct TelemetryContext {
    /// Closure that returns the dynamic model id at emit time.
    pub model_provider: ModelProvider,
    pub app_type: AppType,
    pub client_info: Option<AcpClientInfo>,
    pub is_subagent: bool,
}

impl std::fmt::Debug for TelemetryContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelemetryContext")
            .field("model_provider", &"<closure>")
            .field("app_type", &self.app_type)
            .field("client_info", &self.client_info)
            .field("is_subagent", &self.is_subagent)
            .finish()
    }
}

impl TelemetryContext {
    pub fn new(model_provider: ModelProvider, client_info: Option<AcpClientInfo>, is_subagent: bool) -> Self {
        let app_type = client_info.as_ref().map_or(AppType::Acp, |ci| ci.app_type());
        Self {
            model_provider,
            app_type,
            client_info,
            is_subagent,
        }
    }

    pub(crate) fn model(&self) -> Option<String> {
        (self.model_provider)()
    }

    pub(crate) fn client_application(&self) -> metric::ClientApplication {
        match self.app_type {
            AppType::V1 => metric::ClientApplication::ChatCli,
            AppType::V2 => metric::ClientApplication::ChatCliV2,
            AppType::Acp => metric::ClientApplication::AcpExternal,
        }
    }

    pub(crate) fn apply_to(&self, event: &mut Event) {
        event.app_type = Some(self.app_type.as_str().to_string());
        if event.client_application.is_none() {
            event.set_client_application_kind(self.client_application());
        }
        event.is_subagent = self.is_subagent;
        if let Some(ci) = &self.client_info {
            event.acp_client_name = Some(ci.name.as_str().to_string());
            event.acp_client_version = Some(ci.version.as_str().to_string());
        }
    }
}
