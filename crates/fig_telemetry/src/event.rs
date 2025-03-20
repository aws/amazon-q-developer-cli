use std::time::SystemTime;

use fig_telemetry_core::{
    Event,
    EventType,
    MetricDatum,
};

/// Wrapper around the default telemetry [Event]. Used to initialize other metadata fields
/// within the global telemetry emitter implementation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AppTelemetryEvent(Event);

impl std::ops::Deref for AppTelemetryEvent {
    type Target = Event;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AppTelemetryEvent {
    pub async fn new(ty: EventType) -> Self {
        Self(Event {
            ty,
            credential_start_url: fig_auth::builder_id_token()
                .await
                .ok()
                .flatten()
                .and_then(|t| t.start_url),
            created_time: Some(SystemTime::now()),
        })
    }

    pub async fn from_event(event: Event) -> Self {
        let credential_start_url = match event.credential_start_url {
            Some(v) => Some(v),
            None => fig_auth::builder_id_token()
                .await
                .ok()
                .flatten()
                .and_then(|t| t.start_url),
        };
        Self(Event {
            ty: event.ty,
            credential_start_url,
            created_time: event.created_time.or_else(|| Some(SystemTime::now())),
        })
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn into_metric_datum(self) -> Option<MetricDatum> {
        self.0.into_metric_datum()
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineShellCompletionActionedOptions {}

#[cfg(test)]
pub(crate) mod tests {
    use std::time::Duration;

    use fig_telemetry_core::{
        SuggestionState,
        TelemetryResult,
    };

    use super::*;

    async fn user_logged_in() -> AppTelemetryEvent {
        AppTelemetryEvent::new(EventType::UserLoggedIn {}).await
    }

    async fn refresh_credentials() -> AppTelemetryEvent {
        AppTelemetryEvent::new(EventType::RefreshCredentials {
            request_id: "request_id".into(),
            result: TelemetryResult::Failed,
            reason: Some("some failure".into()),
            oauth_flow: "pkce".into(),
        })
        .await
    }

    async fn completion_inserted() -> AppTelemetryEvent {
        AppTelemetryEvent::new(EventType::CompletionInserted {
            command: "test".into(),
            terminal: Some("vscode".into()),
            shell: Some("bash".into()),
        })
        .await
    }

    async fn inline_shell_actioned() -> AppTelemetryEvent {
        AppTelemetryEvent::new(EventType::InlineShellCompletionActioned {
            session_id: "XXX".into(),
            request_id: "XXX".into(),
            suggestion_state: SuggestionState::Accept,
            edit_buffer_len: Some(123),
            suggested_chars_len: 42,
            number_of_recommendations: 3,
            latency: Duration::from_millis(500),
            terminal: Some("vscode".into()),
            terminal_version: Some("1.0".into()),
            shell: Some("bash".into()),
            shell_version: Some("4.4".into()),
        })
        .await
    }
    async fn translation_actioned() -> AppTelemetryEvent {
        AppTelemetryEvent::new(EventType::TranslationActioned {
            latency: Duration::from_millis(500),
            suggestion_state: SuggestionState::Accept,
            terminal: Some("vscode".into()),
            terminal_version: Some("1.0".into()),
            shell: Some("bash".into()),
            shell_version: Some("4.4".into()),
        })
        .await
    }

    async fn cli_subcommand_executed() -> AppTelemetryEvent {
        AppTelemetryEvent::new(EventType::CliSubcommandExecuted {
            subcommand: "test".into(),
            terminal: Some("vscode".into()),
            terminal_version: Some("1.0".into()),
            shell: Some("bash".into()),
            shell_version: Some("4.4".into()),
        })
        .await
    }

    async fn doctor_check_failed() -> AppTelemetryEvent {
        AppTelemetryEvent::new(EventType::DoctorCheckFailed {
            doctor_check: "test".into(),
            terminal: Some("vscode".into()),
            terminal_version: Some("1.0".into()),
            shell: Some("bash".into()),
            shell_version: Some("4.4".into()),
        })
        .await
    }

    async fn dashboard_page_viewed() -> AppTelemetryEvent {
        AppTelemetryEvent::new(EventType::DashboardPageViewed { route: "test".into() }).await
    }

    async fn menu_bar_actioned() -> AppTelemetryEvent {
        AppTelemetryEvent::new(EventType::MenuBarActioned {
            menu_bar_item: Some("test".into()),
        })
        .await
    }

    async fn fig_user_migrated() -> AppTelemetryEvent {
        AppTelemetryEvent::new(EventType::FigUserMigrated {}).await
    }

    async fn chat_start() -> AppTelemetryEvent {
        AppTelemetryEvent::new(EventType::ChatStart {
            conversation_id: "XXX".into(),
        })
        .await
    }

    async fn chat_end() -> AppTelemetryEvent {
        AppTelemetryEvent::new(EventType::ChatEnd {
            conversation_id: "XXX".into(),
        })
        .await
    }

    async fn chat_added_message() -> AppTelemetryEvent {
        AppTelemetryEvent::new(EventType::ChatAddedMessage {
            conversation_id: "XXX".into(),
            message_id: "YYY".into(),
            context_file_length: Some(5),
        })
        .await
    }

    async fn migrate_client_id_message() -> AppTelemetryEvent {
        AppTelemetryEvent::new(EventType::MigrateClientId {
            old_client_id: "XXX".into(),
        })
        .await
    }

    pub(crate) async fn all_events() -> Vec<AppTelemetryEvent> {
        vec![
            user_logged_in().await,
            refresh_credentials().await,
            completion_inserted().await,
            inline_shell_actioned().await,
            translation_actioned().await,
            cli_subcommand_executed().await,
            doctor_check_failed().await,
            dashboard_page_viewed().await,
            menu_bar_actioned().await,
            fig_user_migrated().await,
            chat_start().await,
            chat_end().await,
            chat_added_message().await,
            migrate_client_id_message().await,
        ]
    }

    #[tokio::test]
    async fn from_event_test() {
        let event = Event {
            ty: EventType::UserLoggedIn {},
            credential_start_url: Some("https://example.com".into()),
            created_time: None,
        };
        let app_event = AppTelemetryEvent::from_event(event).await;
        assert_eq!(app_event.ty, EventType::UserLoggedIn {});
        assert_eq!(app_event.credential_start_url, Some("https://example.com".into()));
        assert!(app_event.created_time.is_some());
    }

    #[tokio::test]
    async fn test_event_ser() {
        for event in all_events().await {
            let json = serde_json::to_string_pretty(&event).unwrap();
            println!("\n{json}\n");
            let deser = AppTelemetryEvent::from_json(&json).unwrap();
            assert_eq!(event, deser);
        }
    }

    #[tokio::test]
    async fn test_into_metric_datum() {
        for event in all_events().await {
            let metric_datum = event.into_metric_datum();
            if let Some(metric_datum) = metric_datum {
                println!("\n{}: {metric_datum:?}\n", metric_datum.metric_name());
            }
        }
    }
}
