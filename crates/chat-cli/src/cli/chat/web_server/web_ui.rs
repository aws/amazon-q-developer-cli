use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::agent_env::{
    agent_environment::HeadlessInterface, events::AgentEnvironmentEvent, session::Session,
};

use super::events::WebUIEvent;

/// WebUI component for broadcasting events to WebSocket clients
pub struct WebUI {
    session: Arc<Session>,
    event_tx: broadcast::Sender<WebUIEvent>,
}

impl WebUI {
    /// Create new WebUI
    pub fn new(session: Arc<Session>) -> Self {
        // Large buffer to handle bursts of events
        let (event_tx, _) = broadcast::channel(10000);

        Self { session, event_tx }
    }

    /// Subscribe to WebUI events (for WebSocket handlers)
    pub fn subscribe(&self) -> broadcast::Receiver<WebUIEvent> {
        self.event_tx.subscribe()
    }

    /// Get session reference
    pub fn session(&self) -> &Arc<Session> {
        &self.session
    }
}

#[async_trait]
impl HeadlessInterface for WebUI {
    async fn handle_event(&self, event: AgentEnvironmentEvent) {
        // Convert to WebUIEvent
        let web_event = WebUIEvent::from_agent_event(event);

        // Broadcast to all subscribers (WebSocket handlers)
        // Ignore send errors (no subscribers is OK)
        let _ = self.event_tx.send(web_event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_env::{
        event_bus::EventBus,
        events::{SystemEvent, WorkerEvent},
    };
    use std::time::Instant;
    use uuid::Uuid;

    fn create_test_session() -> Arc<Session> {
        let event_bus = EventBus::default();
        Arc::new(Session::new(event_bus, vec![]))
    }

    #[test]
    fn test_web_ui_new() {
        let session = create_test_session();
        let web_ui = WebUI::new(session.clone());

        assert!(Arc::ptr_eq(&web_ui.session, &session));
    }

    #[test]
    fn test_web_ui_subscribe() {
        let session = create_test_session();
        let web_ui = WebUI::new(session);

        let _rx1 = web_ui.subscribe();
        let _rx2 = web_ui.subscribe();

        // Multiple subscriptions should work
    }

    #[test]
    fn test_web_ui_session_access() {
        let session = create_test_session();
        let web_ui = WebUI::new(session.clone());

        let session_ref = web_ui.session();
        assert!(Arc::ptr_eq(session_ref, &session));
    }

    #[tokio::test]
    async fn test_web_ui_handle_event_broadcasts() {
        // Initialize time conversion for tests
        super::super::events::init_time_conversion();

        let session = create_test_session();
        let web_ui = WebUI::new(session);

        let mut rx = web_ui.subscribe();

        let event = AgentEnvironmentEvent::Worker(WorkerEvent::Created {
            worker_id: Uuid::new_v4(),
            name: "test".to_string(),
            timestamp: Instant::now(),
        });

        web_ui.handle_event(event).await;

        let received = rx.recv().await.unwrap();
        match received {
            WebUIEvent::WorkerCreated { name, .. } => {
                assert_eq!(name, "test");
            }
            _ => panic!("Expected WorkerCreated event"),
        }
    }

    #[tokio::test]
    async fn test_web_ui_multiple_subscribers() {
        // Initialize time conversion for tests
        super::super::events::init_time_conversion();

        let session = create_test_session();
        let web_ui = WebUI::new(session);

        let mut rx1 = web_ui.subscribe();
        let mut rx2 = web_ui.subscribe();

        let event = AgentEnvironmentEvent::System(SystemEvent::ShutdownInitiated {
            reason: "test".to_string(),
            timestamp: Instant::now(),
        });

        web_ui.handle_event(event).await;

        // Both subscribers should receive the event
        let received1 = rx1.recv().await.unwrap();
        let received2 = rx2.recv().await.unwrap();

        match (received1, received2) {
            (
                WebUIEvent::ShutdownInitiated { reason: r1, .. },
                WebUIEvent::ShutdownInitiated { reason: r2, .. },
            ) => {
                assert_eq!(r1, "test");
                assert_eq!(r2, "test");
            }
            _ => panic!("Expected ShutdownInitiated events"),
        }
    }

    #[tokio::test]
    async fn test_web_ui_no_subscribers_ok() {
        // Initialize time conversion for tests
        super::super::events::init_time_conversion();

        let session = create_test_session();
        let web_ui = WebUI::new(session);

        // No subscribers, should not panic
        let event = AgentEnvironmentEvent::System(SystemEvent::ShutdownInitiated {
            reason: "test".to_string(),
            timestamp: Instant::now(),
        });

        web_ui.handle_event(event).await;
        // Success if no panic
    }
}
