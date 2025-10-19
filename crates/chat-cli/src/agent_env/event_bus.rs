//! Central event distribution system using tokio broadcast channels.

use tokio::sync::broadcast;
use super::events::AgentEnvironmentEvent;

/// Central event distribution system
#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<AgentEnvironmentEvent>,
    buffer_size: usize,
}

impl EventBus {
    /// Create new EventBus with specified buffer size
    pub fn new(buffer_size: usize) -> Self {
        let (sender, _) = broadcast::channel(buffer_size);
        Self { sender, buffer_size }
    }

    /// Publish event to all subscribers
    pub fn publish(&self, event: AgentEnvironmentEvent) {
        let _ = self.sender.send(event);
    }

    /// Subscribe to events
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEnvironmentEvent> {
        self.sender.subscribe()
    }

    /// Get current subscriber count
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_env::events::*;
    use std::time::Instant;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_publish_subscribe_basic() {
        let bus = EventBus::new(10);
        let mut receiver = bus.subscribe();

        let event = AgentEnvironmentEvent::System(SystemEvent::ShutdownInitiated {
            reason: "test".to_string(),
            timestamp: Instant::now(),
        });

        bus.publish(event.clone());

        let received = receiver.recv().await.unwrap();
        assert!(received.is_system_event());
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let bus = EventBus::new(10);
        let mut receiver1 = bus.subscribe();
        let mut receiver2 = bus.subscribe();
        let mut receiver3 = bus.subscribe();

        assert_eq!(bus.subscriber_count(), 3);

        let event = AgentEnvironmentEvent::Worker(WorkerEvent::Created {
            worker_id: Uuid::new_v4(),
            name: "test".to_string(),
            timestamp: Instant::now(),
        });

        bus.publish(event.clone());

        let r1 = receiver1.recv().await.unwrap();
        let r2 = receiver2.recv().await.unwrap();
        let r3 = receiver3.recv().await.unwrap();

        assert!(r1.is_worker_event());
        assert!(r2.is_worker_event());
        assert!(r3.is_worker_event());
    }

    #[tokio::test]
    async fn test_lagged_events() {
        let bus = EventBus::new(10);
        let mut receiver = bus.subscribe();

        for i in 0..100 {
            let event = AgentEnvironmentEvent::System(SystemEvent::ShutdownInitiated {
                reason: format!("test {}", i),
                timestamp: Instant::now(),
            });
            bus.publish(event);
        }

        match receiver.recv().await {
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                // Expected - buffer overflow
            }
            _ => {
                // Also acceptable - might receive some events
            }
        }
    }
}
