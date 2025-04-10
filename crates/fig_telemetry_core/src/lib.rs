use std::any::Any;
use std::sync::OnceLock;
use std::time::{
    Duration,
    SystemTime,
};

pub use amzn_toolkit_telemetry::types::MetricDatum;
use aws_toolkit_telemetry_definitions::IntoMetricDatum;
use aws_toolkit_telemetry_definitions::metrics::{
    AmazonqEndChat,
    AmazonqStartChat,
    CodewhispererterminalAddChatMessage,
    CodewhispererterminalCliSubcommandExecuted,
    CodewhispererterminalCompletionInserted,
    CodewhispererterminalDashboardPageViewed,
    CodewhispererterminalDoctorCheckFailed,
    CodewhispererterminalFigUserMigrated,
    CodewhispererterminalInlineShellActioned,
    CodewhispererterminalMenuBarActioned,
    CodewhispererterminalMigrateOldClientId,
    CodewhispererterminalRefreshCredentials,
    CodewhispererterminalToolUseSuggested,
    CodewhispererterminalTranslationActioned,
    CodewhispererterminalUserLoggedIn,
};
use aws_toolkit_telemetry_definitions::types::{
    CodewhispererterminalInCloudshell,
    CodewhispererterminalIsToolValid,
    CodewhispererterminalToolName,
    CodewhispererterminalToolUseId,
    CodewhispererterminalToolUseIsSuccess,
    CodewhispererterminalUserInputId,
    CodewhispererterminalUtteranceId,
};
use strum::{
    Display,
    EnumString,
};

type GlobalTelemetryEmitter = dyn TelemetryEmitter + Send + Sync + 'static;

/// Global telemetry emitter for the current process.
static EMITTER: OnceLock<Box<GlobalTelemetryEmitter>> = OnceLock::new();

pub fn init_global_telemetry_emitter<T>(telemetry_emitter: T)
where
    T: TelemetryEmitter + Send + Sync + 'static,
{
    match EMITTER.set(Box::new(telemetry_emitter)) {
        Ok(_) => (),
        Err(_) => panic!("The global telemetry emitter can only be initialized once"),
    }
}

/// Sends the telemetry event through the global [TelemetryEmitter] as set by
/// [init_global_telemetry_emitter], returning [None] if no telemetry emitter was set.
pub async fn send_event(event: Event) -> Option<()> {
    if let Some(emitter) = EMITTER.get() {
        emitter.send(event).await;
        Some(())
    } else {
        None
    }
}

/// Trait to handle sending telemetry events. This is intended to be used globally within the
/// application, and can be set using [init_global_telemetry_emitter]. Only one global
/// [TelemetryEmitter] impl should exist.
///
/// TODO: Update all telemetry calls to go through the global [TelemetryEmitter] impl instead.
#[async_trait::async_trait]
pub trait TelemetryEmitter {
    async fn send(&self, event: Event);

    fn as_any(&self) -> &dyn Any;
}

/// A serializable telemetry event that can be sent or queued.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub created_time: Option<SystemTime>,
    pub credential_start_url: Option<String>,
    #[serde(flatten)]
    pub ty: EventType,
}

impl Event {
    pub fn new(ty: EventType) -> Self {
        Self {
            ty,
            created_time: Some(SystemTime::now()),
            credential_start_url: None,
        }
    }

    pub fn with_credential_start_url(mut self, credential_start_url: String) -> Self {
        self.credential_start_url = Some(credential_start_url);
        self
    }

    pub fn into_metric_datum(self) -> Option<MetricDatum> {
        match self.ty {
            EventType::UserLoggedIn {} => Some(
                CodewhispererterminalUserLoggedIn {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    codewhispererterminal_in_cloudshell: in_cloudshell(),
                }
                .into_metric_datum(),
            ),
            EventType::RefreshCredentials {
                request_id,
                result,
                reason,
                oauth_flow,
            } => Some(
                CodewhispererterminalRefreshCredentials {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    request_id: Some(request_id.into()),
                    result: Some(result.to_string().into()),
                    reason: reason.map(Into::into),
                    oauth_flow: Some(oauth_flow.into()),
                    codewhispererterminal_in_cloudshell: in_cloudshell(),
                }
                .into_metric_datum(),
            ),
            EventType::CompletionInserted {
                command,
                terminal,
                shell,
            } => Some(
                CodewhispererterminalCompletionInserted {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    codewhispererterminal_terminal: terminal.map(Into::into),
                    codewhispererterminal_terminal_version: None,
                    codewhispererterminal_shell: shell.map(Into::into),
                    codewhispererterminal_shell_version: None,
                    codewhispererterminal_command: Some(command.into()),
                    codewhispererterminal_duration: None,
                    codewhispererterminal_in_cloudshell: in_cloudshell(),
                }
                .into_metric_datum(),
            ),
            EventType::InlineShellCompletionActioned {
                terminal,
                terminal_version,
                shell,
                shell_version,
                suggestion_state,
                edit_buffer_len,
                suggested_chars_len,
                ..
            } => Some(
                CodewhispererterminalInlineShellActioned {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    codewhispererterminal_duration: None,
                    codewhispererterminal_accepted: Some(suggestion_state.is_accepted().into()),
                    codewhispererterminal_typed_count: edit_buffer_len.map(Into::into),
                    codewhispererterminal_suggested_count: Some(Into::into(suggested_chars_len as i64)),
                    codewhispererterminal_terminal: terminal.map(Into::into),
                    codewhispererterminal_terminal_version: terminal_version.map(Into::into),
                    codewhispererterminal_shell: shell.map(Into::into),
                    codewhispererterminal_shell_version: shell_version.map(Into::into),
                    codewhispererterminal_suggestion_state: Some(suggestion_state.as_str().to_owned().into()),
                    codewhispererterminal_in_cloudshell: in_cloudshell(),
                }
                .into_metric_datum(),
            ),
            EventType::TranslationActioned {
                latency: _,
                suggestion_state,
                terminal,
                terminal_version,
                shell,
                shell_version,
            } => Some(
                CodewhispererterminalTranslationActioned {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    codewhispererterminal_terminal: terminal.map(Into::into),
                    codewhispererterminal_terminal_version: terminal_version.map(Into::into),
                    codewhispererterminal_shell: shell.map(Into::into),
                    codewhispererterminal_shell_version: shell_version.map(Into::into),
                    codewhispererterminal_duration: None,
                    codewhispererterminal_time_to_suggestion: None,
                    codewhispererterminal_accepted: Some(suggestion_state.is_accepted().into()),
                    codewhispererterminal_in_cloudshell: in_cloudshell(),
                }
                .into_metric_datum(),
            ),
            EventType::CliSubcommandExecuted {
                subcommand,
                terminal,
                terminal_version,
                shell,
                shell_version,
            } => Some(
                CodewhispererterminalCliSubcommandExecuted {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    codewhispererterminal_terminal: terminal.map(Into::into),
                    codewhispererterminal_terminal_version: terminal_version.map(Into::into),
                    codewhispererterminal_shell: shell.map(Into::into),
                    codewhispererterminal_shell_version: shell_version.map(Into::into),
                    codewhispererterminal_subcommand: Some(subcommand.into()),
                    codewhispererterminal_in_cloudshell: in_cloudshell(),
                }
                .into_metric_datum(),
            ),
            EventType::DoctorCheckFailed {
                doctor_check,
                terminal,
                terminal_version,
                shell,
                shell_version,
            } => Some(
                CodewhispererterminalDoctorCheckFailed {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    codewhispererterminal_terminal: terminal.map(Into::into),
                    codewhispererterminal_terminal_version: terminal_version.map(Into::into),
                    codewhispererterminal_shell: shell.map(Into::into),
                    codewhispererterminal_shell_version: shell_version.map(Into::into),
                    codewhispererterminal_doctor_check: Some(doctor_check.into()),
                    codewhispererterminal_in_cloudshell: in_cloudshell(),
                }
                .into_metric_datum(),
            ),
            EventType::DashboardPageViewed { route } => Some(
                CodewhispererterminalDashboardPageViewed {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    codewhispererterminal_route: Some(route.into()),
                }
                .into_metric_datum(),
            ),
            EventType::MenuBarActioned { menu_bar_item } => Some(
                CodewhispererterminalMenuBarActioned {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    codewhispererterminal_menu_bar_item: menu_bar_item.map(|item| item.into()),
                }
                .into_metric_datum(),
            ),
            EventType::FigUserMigrated {} => Some(
                CodewhispererterminalFigUserMigrated {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                }
                .into_metric_datum(),
            ),
            EventType::ChatStart { conversation_id } => Some(
                AmazonqStartChat {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    amazonq_conversation_id: Some(conversation_id.into()),
                    codewhispererterminal_in_cloudshell: in_cloudshell(),
                }
                .into_metric_datum(),
            ),
            EventType::ChatEnd { conversation_id } => Some(
                AmazonqEndChat {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    amazonq_conversation_id: Some(conversation_id.into()),
                    codewhispererterminal_in_cloudshell: in_cloudshell(),
                }
                .into_metric_datum(),
            ),
            EventType::ChatAddedMessage {
                conversation_id,
                context_file_length,
                ..
            } => Some(
                CodewhispererterminalAddChatMessage {
                    create_time: self.created_time,
                    value: None,
                    amazonq_conversation_id: Some(conversation_id.into()),
                    credential_start_url: self.credential_start_url.map(Into::into),
                    codewhispererterminal_in_cloudshell: in_cloudshell(),
                    codewhispererterminal_context_file_length: context_file_length.map(|l| l as i64).map(Into::into),
                }
                .into_metric_datum(),
            ),
            EventType::MigrateClientId { old_client_id } => Some(
                CodewhispererterminalMigrateOldClientId {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    codewhispererterminal_old_client_id: Some(old_client_id.into()),
                }
                .into_metric_datum(),
            ),
            EventType::ToolUseSuggested {
                conversation_id,
                utterance_id,
                user_input_id,
                tool_use_id,
                tool_name,
                is_accepted,
                is_valid,
                is_success,
            } => Some(
                CodewhispererterminalToolUseSuggested {
                    create_time: self.created_time,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    value: None,
                    amazonq_conversation_id: Some(conversation_id.into()),
                    codewhispererterminal_utterance_id: utterance_id.map(CodewhispererterminalUtteranceId),
                    codewhispererterminal_user_input_id: user_input_id.map(CodewhispererterminalUserInputId),
                    codewhispererterminal_tool_use_id: tool_use_id.map(CodewhispererterminalToolUseId),
                    codewhispererterminal_tool_name: tool_name.map(CodewhispererterminalToolName),
                    codewhispererterminal_is_tool_use_accepted: Some(is_accepted.into()),
                    codewhispererterminal_is_tool_valid: is_valid.map(CodewhispererterminalIsToolValid),
                    codewhispererterminal_tool_use_is_success: is_success.map(CodewhispererterminalToolUseIsSuccess),
                }
                .into_metric_datum(),
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
pub enum EventType {
    UserLoggedIn {},
    RefreshCredentials {
        request_id: String,
        result: TelemetryResult,
        reason: Option<String>,
        oauth_flow: String,
    },
    CompletionInserted {
        command: String,
        terminal: Option<String>,
        shell: Option<String>,
    },
    InlineShellCompletionActioned {
        session_id: String,
        request_id: String,
        suggestion_state: SuggestionState,
        edit_buffer_len: Option<i64>,
        suggested_chars_len: i32,
        number_of_recommendations: i32,
        latency: Duration,
        terminal: Option<String>,
        terminal_version: Option<String>,
        shell: Option<String>,
        shell_version: Option<String>,
    },
    TranslationActioned {
        latency: Duration,
        suggestion_state: SuggestionState,
        terminal: Option<String>,
        terminal_version: Option<String>,
        shell: Option<String>,
        shell_version: Option<String>,
    },
    CliSubcommandExecuted {
        subcommand: String,
        terminal: Option<String>,
        terminal_version: Option<String>,
        shell: Option<String>,
        shell_version: Option<String>,
    },
    DoctorCheckFailed {
        doctor_check: String,
        terminal: Option<String>,
        terminal_version: Option<String>,
        shell: Option<String>,
        shell_version: Option<String>,
    },
    DashboardPageViewed {
        route: String,
    },
    MenuBarActioned {
        menu_bar_item: Option<String>,
    },
    FigUserMigrated {},
    ChatStart {
        conversation_id: String,
    },
    ChatEnd {
        conversation_id: String,
    },
    ChatAddedMessage {
        conversation_id: String,
        message_id: String,
        context_file_length: Option<usize>,
    },
    MigrateClientId {
        old_client_id: String,
    },
    ToolUseSuggested {
        conversation_id: String,
        utterance_id: Option<String>,
        user_input_id: Option<String>,
        tool_use_id: Option<String>,
        tool_name: Option<String>,
        is_accepted: bool,
        is_success: Option<bool>,
        is_valid: Option<bool>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SuggestionState {
    Accept,
    Discard,
    Empty,
    Reject,
}

impl SuggestionState {
    pub fn is_accepted(&self) -> bool {
        matches!(self, SuggestionState::Accept)
    }

    fn as_str(&self) -> &'static str {
        match self {
            SuggestionState::Accept => "ACCEPT",
            SuggestionState::Discard => "DISCARD",
            SuggestionState::Empty => "EMPTY",
            SuggestionState::Reject => "REJECT",
        }
    }
}

impl From<SuggestionState> for amzn_codewhisperer_client::types::SuggestionState {
    fn from(value: SuggestionState) -> Self {
        match value {
            SuggestionState::Accept => amzn_codewhisperer_client::types::SuggestionState::Accept,
            SuggestionState::Discard => amzn_codewhisperer_client::types::SuggestionState::Discard,
            SuggestionState::Empty => amzn_codewhisperer_client::types::SuggestionState::Empty,
            SuggestionState::Reject => amzn_codewhisperer_client::types::SuggestionState::Reject,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, EnumString, Display, serde::Serialize, serde::Deserialize)]
pub enum TelemetryResult {
    Succeeded,
    Failed,
}

fn in_cloudshell() -> Option<CodewhispererterminalInCloudshell> {
    Some(fig_util::system_info::in_cloudshell().into())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Debug, Default)]
    struct DummyEmitter(Mutex<Vec<Event>>);

    #[async_trait::async_trait]
    impl TelemetryEmitter for DummyEmitter {
        async fn send(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    #[tokio::test]
    async fn test_init_global_telemetry_emitter_receives_event() {
        init_global_telemetry_emitter(DummyEmitter::default());
        send_event(Event::new(EventType::UserLoggedIn {})).await;

        let events = EMITTER
            .get()
            .unwrap()
            .as_any()
            .downcast_ref::<DummyEmitter>()
            .unwrap()
            .0
            .lock()
            .unwrap();
        assert!(events.len() == 1);
        assert!(matches!(events.first().unwrap().ty, EventType::UserLoggedIn {}));
    }

    #[ignore = "depends on test_init_global_telemetry_emitter_receives_event not being ran"]
    #[tokio::test]
    async fn test_no_global_telemetry_emitter() {
        assert!(send_event(Event::new(EventType::UserLoggedIn {})).await.is_none());
    }
}
