use kiro_telemetry::metric;

use super::ChatSession;
use crate::os::Os;

#[derive(Clone, Debug, PartialEq, Eq)]
struct ChatTelemetrySession {
    conversation_id: String,
    model: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct ChatTelemetryTransition {
    previous: Option<ChatTelemetrySession>,
    current: ChatTelemetrySession,
}

#[derive(Default)]
pub(super) struct ChatTelemetryLifecycle {
    active: Option<ChatTelemetrySession>,
}

impl ChatTelemetryLifecycle {
    fn start(&self, session: ChatTelemetrySession) -> Option<ChatTelemetrySession> {
        if self.active.is_some() {
            return None;
        }
        Some(session)
    }

    fn commit_start(&mut self, session: ChatTelemetrySession) {
        self.active = Some(session);
    }

    fn transition(&self, session: ChatTelemetrySession) -> Option<ChatTelemetryTransition> {
        if self.active.as_ref() == Some(&session) {
            return None;
        }
        Some(ChatTelemetryTransition {
            previous: self.active.clone(),
            current: session,
        })
    }

    fn commit_transition(&mut self, session: ChatTelemetrySession) {
        self.active = Some(session);
    }

    fn finish(&self) -> Option<ChatTelemetrySession> {
        self.active.clone()
    }

    fn commit_finish(&mut self, session: &ChatTelemetrySession) {
        if self.active.as_ref() == Some(session) {
            self.active = None;
        }
    }
}

impl ChatSession {
    pub(super) fn telemetry_mode(&self) -> metric::Mode {
        if self.interactive {
            metric::Mode::Interactive
        } else {
            metric::Mode::Oneshot
        }
    }

    fn chat_telemetry_session(&self) -> ChatTelemetrySession {
        ChatTelemetrySession {
            conversation_id: self.conversation.conversation_id().to_string(),
            model: self
                .conversation
                .model_info
                .as_ref()
                .map(|model| model.model_id.clone()),
        }
    }

    pub(super) async fn start_chat_telemetry(&mut self, os: &Os, session_start_kind: metric::SessionStartKind) {
        let mode = self.telemetry_mode();
        let session = self.chat_telemetry_session();
        let Some(session) = self.chat_telemetry.start(session) else {
            return;
        };
        if os
            .telemetry
            .send_chat_start(
                &os.database,
                session.conversation_id.clone(),
                session.model.clone(),
                mode,
                session_start_kind,
            )
            .await
            .is_ok()
        {
            self.chat_telemetry.commit_start(session);
        }
    }

    pub(super) async fn transition_chat_telemetry(&mut self, os: &Os, session_start_kind: metric::SessionStartKind) {
        let mode = self.telemetry_mode();
        let session = self.chat_telemetry_session();
        let Some(transition) = self.chat_telemetry.transition(session) else {
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
                mode,
                session_start_kind,
            )
            .await;
        if result.is_ok() {
            self.chat_telemetry.commit_transition(transition.current);
        }
    }

    pub(super) async fn finish_chat_telemetry(&mut self, os: &Os) {
        let Some(session) = self.chat_telemetry.finish() else {
            return;
        };
        if os
            .telemetry
            .send_chat_end(&os.database, session.conversation_id.clone(), session.model.clone())
            .await
            .is_ok()
        {
            self.chat_telemetry.commit_finish(&session);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(conversation_id: &str, model: Option<&str>) -> ChatTelemetrySession {
        ChatTelemetrySession {
            conversation_id: conversation_id.to_string(),
            model: model.map(str::to_string),
        }
    }

    #[test]
    fn compaction_keeps_the_started_telemetry_identity() {
        let mut lifecycle = ChatTelemetryLifecycle::default();
        let started = session("original", Some("model"));

        assert_eq!(lifecycle.start(started.clone()), Some(started.clone()));
        lifecycle.commit_start(started.clone());
        assert_eq!(lifecycle.finish(), Some(started));
        lifecycle.commit_finish(&session("original", Some("model")));
        assert_eq!(lifecycle.finish(), None);
    }

    #[test]
    fn conversation_transition_is_stateful_and_idempotent() {
        let mut lifecycle = ChatTelemetryLifecycle::default();
        let original = session("original", Some("model-a"));
        let resumed = session("resumed", Some("model-b"));
        lifecycle.commit_start(original.clone());

        let transition = ChatTelemetryTransition {
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
        let lifecycle = ChatTelemetryLifecycle::default();

        assert!(lifecycle.start(session("new", None)).is_some());
        assert_eq!(lifecycle.finish(), None);
    }
}
