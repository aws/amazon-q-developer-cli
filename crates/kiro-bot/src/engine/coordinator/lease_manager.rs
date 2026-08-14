use std::collections::HashMap;
use std::sync::{
    Arc,
    Mutex,
    Weak,
};

use super::{
    Coordinator,
    LeaseAcquisition,
    LeaseToken,
};

#[derive(Clone)]
pub struct LeaseManager {
    inner: Arc<LeaseManagerInner>,
}

struct LeaseManagerInner {
    coordinator: Arc<dyn Coordinator>,
    slots: Mutex<HashMap<String, Weak<LeaseSlot>>>,
}

struct LeaseSlot {
    active: tokio::sync::Mutex<Option<Weak<LeaseGuardInner>>>,
    manager: Weak<LeaseManagerInner>,
    conversation_id: String,
}

impl Drop for LeaseSlot {
    fn drop(&mut self) {
        let Some(manager) = self.manager.upgrade() else {
            return;
        };
        let mut slots = manager.slots.lock().expect("lease slots poisoned");
        if slots
            .get(&self.conversation_id)
            .is_some_and(|registered| registered.as_ptr() == std::ptr::from_ref(self))
        {
            slots.remove(&self.conversation_id);
        }
    }
}

pub(crate) enum ManagedLeaseAcquisition {
    Acquired(LeaseGuard),
    Held { peer: String },
    Unavailable,
}

impl LeaseManager {
    pub fn new(coordinator: Arc<dyn Coordinator>) -> Self {
        Self {
            inner: Arc::new(LeaseManagerInner {
                coordinator,
                slots: Mutex::new(HashMap::new()),
            }),
        }
    }

    fn slot(&self, conversation_id: &str) -> Arc<LeaseSlot> {
        let mut slots = self.inner.slots.lock().expect("lease slots poisoned");
        if let Some(slot) = slots.get(conversation_id).and_then(Weak::upgrade) {
            return slot;
        }
        let slot = Arc::new(LeaseSlot {
            active: tokio::sync::Mutex::new(None),
            manager: Arc::downgrade(&self.inner),
            conversation_id: conversation_id.to_string(),
        });
        slots.insert(conversation_id.to_string(), Arc::downgrade(&slot));
        slot
    }

    pub(crate) async fn acquire(&self, conversation_id: &str, request_id: &str) -> ManagedLeaseAcquisition {
        let slot = self.slot(conversation_id);
        self.acquire_in_slot(slot, request_id).await
    }

    async fn acquire_in_slot(&self, slot: Arc<LeaseSlot>, request_id: &str) -> ManagedLeaseAcquisition {
        let conversation_id = &slot.conversation_id;
        let mut active = slot.active.lock().await;
        if let Some(inner) = active.as_ref().and_then(Weak::upgrade) {
            tracing::debug!(
                %request_id,
                %conversation_id,
                "sharing locally-owned coordinator lease"
            );
            return ManagedLeaseAcquisition::Acquired(LeaseGuard { inner });
        }

        match self.inner.coordinator.acquire_lease(conversation_id).await {
            LeaseAcquisition::Acquired { token } => {
                let guard = LeaseGuard::start(
                    self.inner.coordinator.clone(),
                    slot.clone(),
                    conversation_id.to_string(),
                    request_id.to_string(),
                    token,
                );
                *active = Some(Arc::downgrade(&guard.inner));
                ManagedLeaseAcquisition::Acquired(guard)
            },
            LeaseAcquisition::Held { peer } => ManagedLeaseAcquisition::Held { peer },
            LeaseAcquisition::Unavailable => ManagedLeaseAcquisition::Unavailable,
        }
    }
}

pub(crate) struct LeaseGuard {
    inner: Arc<LeaseGuardInner>,
}

struct LeaseGuardInner {
    coordinator: Arc<dyn Coordinator>,
    conversation_id: String,
    request_id: String,
    ownership_token: LeaseToken,
    heartbeat: tokio::task::JoinHandle<()>,
    loss_rx: tokio::sync::watch::Receiver<Option<String>>,
    _slot: Arc<LeaseSlot>,
}

impl LeaseGuard {
    fn start(
        coordinator: Arc<dyn Coordinator>,
        slot: Arc<LeaseSlot>,
        conversation_id: String,
        request_id: String,
        ownership_token: LeaseToken,
    ) -> Self {
        let interval = coordinator.lease_heartbeat_interval();
        let ttl = coordinator.lease_ttl();
        let heartbeat_coordinator = coordinator.clone();
        let heartbeat_conversation = conversation_id.clone();
        let heartbeat_request = request_id.clone();
        let heartbeat_token = ownership_token.clone();
        let (loss_tx, loss_rx) = tokio::sync::watch::channel(None);
        let heartbeat = tokio::spawn(async move {
            let start = tokio::time::Instant::now() + interval;
            let mut ticks = tokio::time::interval_at(start, interval);
            let mut expires_at = tokio::time::Instant::now() + ttl;
            loop {
                ticks.tick().await;
                let failure = match tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    heartbeat_coordinator.renew(&heartbeat_conversation, &heartbeat_token),
                )
                .await
                {
                    Ok(Ok(())) => {
                        expires_at = tokio::time::Instant::now() + ttl;
                        tracing::debug!(
                            request_id = %heartbeat_request,
                            conversation_id = %heartbeat_conversation,
                            "coordinator lease renewed"
                        );
                        continue;
                    },
                    Ok(Err(error)) => {
                        tracing::warn!(
                            request_id = %heartbeat_request,
                            conversation_id = %heartbeat_conversation,
                            %error,
                            "coordinator lease renewal failed"
                        );
                        error.to_string()
                    },
                    Err(_) => {
                        tracing::warn!(
                            request_id = %heartbeat_request,
                            conversation_id = %heartbeat_conversation,
                            "coordinator lease renewal timed out"
                        );
                        "lease renewal timed out".to_string()
                    },
                };
                let ownership_lost = failure.contains("no longer owned") || failure.contains("not owned");
                if ownership_lost || tokio::time::Instant::now() + interval >= expires_at {
                    tracing::error!(
                        request_id = %heartbeat_request,
                        conversation_id = %heartbeat_conversation,
                        %failure,
                        "coordinator lease lost; cancelling active dispatch"
                    );
                    loss_tx.send_replace(Some(failure));
                    break;
                }
            }
        });
        let inner = Arc::new(LeaseGuardInner {
            coordinator,
            conversation_id,
            request_id,
            ownership_token,
            heartbeat,
            loss_rx,
            _slot: slot,
        });
        Self { inner }
    }

    pub(crate) fn subscribe_loss(&self) -> tokio::sync::watch::Receiver<Option<String>> {
        self.inner.loss_rx.clone()
    }
}

impl Drop for LeaseGuardInner {
    fn drop(&mut self) {
        self.heartbeat.abort();
        let coordinator = self.coordinator.clone();
        let conversation_id = self.conversation_id.clone();
        let request_id = self.request_id.clone();
        let ownership_token = self.ownership_token.clone();

        tokio::spawn(async move {
            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                coordinator.release(&conversation_id, &ownership_token),
            )
            .await
            {
                Ok(Ok(())) => {},
                Ok(Err(error)) => tracing::warn!(
                    request_id = %request_id,
                    conversation_id = %conversation_id,
                    %error,
                    "coordinator lease release failed"
                ),
                Err(_) => tracing::warn!(
                    request_id = %request_id,
                    conversation_id = %conversation_id,
                    "coordinator lease release timed out"
                ),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{
        AtomicUsize,
        Ordering,
    };

    use super::*;
    use crate::engine::coordinator::{
        DedupeOutcome,
        DedupeToken,
        ForwardEvent,
        InMemoryClusterCoordinator,
        Turn,
    };

    struct RenewFailureCoordinator {
        releases: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl Coordinator for RenewFailureCoordinator {
        async fn dedupe_event_outcome(&self, _slack_event_id: &str) -> DedupeOutcome {
            DedupeOutcome::Accepted {
                token: DedupeToken::new("renew-failure"),
            }
        }

        async fn release_dedup(&self, _slack_event_id: &str, _token: &DedupeToken) -> anyhow::Result<()> {
            Ok(())
        }

        async fn acquire_lease(&self, _conversation_id: &str) -> LeaseAcquisition {
            LeaseAcquisition::Acquired {
                token: LeaseToken::new("renew-failure"),
            }
        }

        async fn renew(&self, _conversation_id: &str, _token: &LeaseToken) -> anyhow::Result<()> {
            anyhow::bail!("coordinator unavailable")
        }

        fn lease_heartbeat_interval(&self) -> std::time::Duration {
            std::time::Duration::from_secs(100)
        }

        fn lease_ttl(&self) -> std::time::Duration {
            std::time::Duration::from_secs(300)
        }

        async fn release(&self, _conversation_id: &str, _token: &LeaseToken) -> anyhow::Result<()> {
            self.releases.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        async fn forward(&self, _peer: &str, _payload: ForwardEvent) -> anyhow::Result<()> {
            Ok(())
        }

        async fn append_turn(&self, _conversation_id: &str, _turn: Turn) -> anyhow::Result<()> {
            Ok(())
        }

        async fn load_history(&self, _conversation_id: &str, _limit: usize) -> anyhow::Result<Vec<Turn>> {
            Ok(Vec::new())
        }

        async fn register_approval(
            &self,
            _slack_msg_ts: &str,
            _conversation_id: &str,
            _ttl: std::time::Duration,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn lookup_approval_owner(&self, _slack_msg_ts: &str) -> anyhow::Result<Option<String>> {
            Ok(None)
        }
    }

    async fn acquire_guard(manager: &LeaseManager, conversation_id: &str, request_id: &str) -> LeaseGuard {
        match manager.acquire(conversation_id, request_id).await {
            ManagedLeaseAcquisition::Acquired(guard) => guard,
            _ => panic!("expected managed lease acquisition"),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn lease_guard_keeps_dispatch_ownership_beyond_300_and_600_seconds() {
        let task_a = Arc::new(InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5)));
        let task_b = task_a.sibling("task-B");
        let manager = LeaseManager::new(task_a);
        let guard = acquire_guard(&manager, "convo-1", "event-1").await;
        tokio::task::yield_now().await;
        tokio::time::advance(std::time::Duration::from_secs(301)).await;
        tokio::task::yield_now().await;
        assert!(matches!(
            task_b.acquire_lease("convo-1").await,
            LeaseAcquisition::Held { .. }
        ));

        tokio::time::advance(std::time::Duration::from_secs(300)).await;
        tokio::task::yield_now().await;
        assert!(matches!(
            task_b.acquire_lease("convo-1").await,
            LeaseAcquisition::Held { .. }
        ));

        drop(guard);
        tokio::task::yield_now().await;
        assert!(matches!(
            task_b.acquire_lease("convo-1").await,
            LeaseAcquisition::Acquired { .. }
        ));
    }

    #[tokio::test]
    async fn duplicate_local_guards_release_only_after_last_owner_drops() {
        let task_a = Arc::new(InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5)));
        let task_b = task_a.sibling("task-B");
        let manager = LeaseManager::new(task_a);
        let first = acquire_guard(&manager, "convo-1", "event-1").await;

        let duplicate = acquire_guard(&manager, "convo-1", "event-2").await;
        drop(duplicate);
        tokio::task::yield_now().await;
        assert!(matches!(
            task_b.acquire_lease("convo-1").await,
            LeaseAcquisition::Held { .. }
        ));

        drop(first);
        tokio::task::yield_now().await;
        assert!(matches!(
            task_b.acquire_lease("convo-1").await,
            LeaseAcquisition::Acquired { .. }
        ));
    }

    #[tokio::test]
    async fn concrete_coordinator_clone_shares_one_managed_guard() {
        let concrete = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
        let task_b = concrete.sibling("task-B");
        let manager = LeaseManager::new(Arc::new(concrete.clone()));
        let cloned_manager = manager.clone();
        let first = acquire_guard(&manager, "convo-1", "event-1").await;
        let second = acquire_guard(&cloned_manager, "convo-1", "event-2").await;
        assert!(Arc::ptr_eq(&first.inner, &second.inner));
        drop(first);
        tokio::task::yield_now().await;
        assert!(matches!(
            task_b.acquire_lease("convo-1").await,
            LeaseAcquisition::Held { peer } if peer == "task-A"
        ));

        drop(second);
        tokio::task::yield_now().await;
        assert!(matches!(
            task_b.acquire_lease("convo-1").await,
            LeaseAcquisition::Acquired { .. }
        ));
    }

    #[tokio::test]
    async fn acquire_upgraded_before_drop_keeps_canonical_slot() {
        let concrete = InMemoryClusterCoordinator::new("task-A", chrono::Duration::minutes(5));
        let task_b = concrete.sibling("task-B");
        let manager = LeaseManager::new(Arc::new(concrete.clone()));
        let first = acquire_guard(&manager, "convo-1", "event-1").await;

        let upgraded_before_drop = manager.slot("convo-1");
        assert!(Arc::ptr_eq(&first.inner._slot, &upgraded_before_drop));
        drop(first);

        let replacement = match manager.acquire_in_slot(upgraded_before_drop, "event-2").await {
            ManagedLeaseAcquisition::Acquired(guard) => guard,
            _ => panic!("expected replacement managed lease acquisition"),
        };
        let concurrent = acquire_guard(&manager, "convo-1", "event-3").await;
        assert!(Arc::ptr_eq(&replacement.inner, &concurrent.inner));

        tokio::task::yield_now().await;
        assert!(matches!(
            task_b.acquire_lease("convo-1").await,
            LeaseAcquisition::Held { peer } if peer == "task-A"
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn lease_guard_signals_loss_before_failed_renewals_reach_ttl() {
        let coordinator = Arc::new(RenewFailureCoordinator {
            releases: AtomicUsize::new(0),
        });
        let started = tokio::time::Instant::now();
        let manager = LeaseManager::new(coordinator.clone());
        let guard = acquire_guard(&manager, "convo-1", "event-1").await;
        let loss = guard.subscribe_loss();
        tokio::task::yield_now().await;

        tokio::time::advance(std::time::Duration::from_secs(101)).await;
        tokio::task::yield_now().await;
        assert!(loss.borrow().is_none());

        tokio::time::advance(std::time::Duration::from_secs(100)).await;
        tokio::task::yield_now().await;
        assert!(loss.borrow().is_some());
        assert!(started.elapsed() < std::time::Duration::from_secs(300));

        drop(guard);
        tokio::task::yield_now().await;
        assert_eq!(coordinator.releases.load(Ordering::Relaxed), 1);
    }
}
