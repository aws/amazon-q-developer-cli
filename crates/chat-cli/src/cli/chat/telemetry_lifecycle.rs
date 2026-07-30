use super::ChatSession;
use crate::os::Os;

#[derive(Clone, Debug, PartialEq, Eq)]
struct LegacyChatTelemetrySession {
    conversation_id: String,
    model: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct LegacyChatTelemetryTransition {
    previous: Option<LegacyChatTelemetrySession>,
    current: LegacyChatTelemetrySession,
}

#[derive(Default)]
pub(super) struct LegacyChatTelemetryLifecycle {
    active: Option<LegacyChatTelemetrySession>,
}

impl LegacyChatTelemetryLifecycle {
    fn start(&self, session: LegacyChatTelemetrySession) -> Option<LegacyChatTelemetrySession> {
        if self.active.is_some() {
            return None;
        }
        Some(session)
    }

    fn commit_start(&mut self, session: LegacyChatTelemetrySession) {
        self.active = Some(session);
    }

    fn transition(&self, session: LegacyChatTelemetrySession) -> Option<LegacyChatTelemetryTransition> {
        if self.active.as_ref() == Some(&session) {
            return None;
        }
        Some(LegacyChatTelemetryTransition {
            previous: self.active.clone(),
            current: session,
        })
    }

    fn commit_transition(&mut self, session: LegacyChatTelemetrySession) {
        self.active = Some(session);
    }

    fn finish(&self) -> Option<LegacyChatTelemetrySession> {
        self.active.clone()
    }

    fn commit_finish(&mut self, session: &LegacyChatTelemetrySession) {
        if self.active.as_ref() == Some(session) {
            self.active = None;
        }
    }
}

impl ChatSession {
    fn legacy_chat_telemetry_session(&self) -> LegacyChatTelemetrySession {
        LegacyChatTelemetrySession {
            conversation_id: self.conversation.conversation_id().to_string(),
            model: self
                .conversation
                .model_info
                .as_ref()
                .map(|model| model.model_id.clone()),
        }
    }

    pub(super) async fn start_legacy_chat_telemetry(&mut self, os: &Os) {
        let session = self.legacy_chat_telemetry_session();
        let Some(session) = self.legacy_chat_telemetry.start(session) else {
            return;
        };
        if os
            .telemetry
            .send_chat_start(&os.database, session.conversation_id.clone(), session.model.clone())
            .await
            .is_ok()
        {
            self.legacy_chat_telemetry.commit_start(session);
        }
    }

    pub(super) async fn transition_legacy_chat_telemetry(&mut self, os: &Os) {
        let session = self.legacy_chat_telemetry_session();
        let Some(transition) = self.legacy_chat_telemetry.transition(session) else {
            return;
        };
        let result = os
            .telemetry
            .send_chat_transition(
                &os.database,
                transition
                    .previous
                    .map(|previous| (previous.conversation_id, previous.model)),
                transition.current.conversation_id.clone(),
                transition.current.model.clone(),
            )
            .await;
        if result.is_ok() {
            self.legacy_chat_telemetry.commit_transition(transition.current);
        }
    }

    pub(super) async fn finish_legacy_chat_telemetry(&mut self, os: &Os) {
        let Some(session) = self.legacy_chat_telemetry.finish() else {
            return;
        };
        if os
            .telemetry
            .send_chat_end(&os.database, session.conversation_id.clone(), session.model.clone())
            .await
            .is_ok()
        {
            self.legacy_chat_telemetry.commit_finish(&session);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(conversation_id: &str, model: Option<&str>) -> LegacyChatTelemetrySession {
        LegacyChatTelemetrySession {
            conversation_id: conversation_id.to_string(),
            model: model.map(str::to_string),
        }
    }

    #[test]
    fn completion_keeps_the_started_telemetry_identity() {
        let mut lifecycle = LegacyChatTelemetryLifecycle::default();
        let started = session("original", Some("model"));

        assert_eq!(lifecycle.start(started.clone()), Some(started.clone()));
        lifecycle.commit_start(started.clone());
        assert_eq!(lifecycle.finish(), Some(started));
        lifecycle.commit_finish(&session("original", Some("model")));
        assert_eq!(lifecycle.finish(), None);
    }

    #[test]
    fn conversation_transition_is_stateful_and_idempotent() {
        let mut lifecycle = LegacyChatTelemetryLifecycle::default();
        let original = session("original", Some("model-a"));
        let resumed = session("resumed", Some("model-b"));
        lifecycle.commit_start(original.clone());

        let transition = LegacyChatTelemetryTransition {
            previous: Some(original.clone()),
            current: resumed.clone(),
        };
        assert_eq!(lifecycle.transition(resumed.clone()), Some(transition));
        assert_eq!(lifecycle.finish(), Some(original));
        lifecycle.commit_transition(resumed.clone());
        assert_eq!(lifecycle.transition(resumed.clone()), None);
        assert_eq!(lifecycle.finish(), Some(resumed));
    }

    #[test]
    fn uncommitted_start_does_not_create_an_active_session() {
        let lifecycle = LegacyChatTelemetryLifecycle::default();

        assert!(lifecycle.start(session("new", None)).is_some());
        assert_eq!(lifecycle.finish(), None);
    }
}
