//! Epoch-stamped telemetry identity.
//!
//! Identity is resolved at *export*, never carried on an event. Every event is
//! stamped with the current epoch when it is enqueued; the epoch is resolved to
//! a pseudonym when the record is exported. Two invariants make misattribution
//! unrepresentable rather than merely prevented by ordering discipline:
//!
//! 1. an event never changes epoch after it is stamped, and
//! 2. an epoch's binding is written once and never changes.
//!
//! The only reachable failure mode is an *unbound* epoch (e.g. one pruned from
//! the bounded window), which resolves to anonymous — never to a wrong identity.
//!
//! This replaces the previous design's four hand-synchronised identity copies
//! (the persisted row, the host's write-once slot, the V2 coordinator's
//! generation/revision state, and the TUI provider Resource) and the FIFO
//! control message + acknowledgement that ordered them.

use std::collections::BTreeMap;
use std::sync::atomic::{
    AtomicU64,
    Ordering,
};
use std::sync::{
    Arc,
    Mutex,
    RwLock,
};

use crate::pseudonymous_user_id;

/// Longest raw user id we accept before treating the value as malformed.
const MAX_RAW_USER_ID_BYTES: usize = 512;

/// Number of trailing epochs whose bindings are retained. A CLI process makes a
/// handful of identity transitions across its whole lifetime (startup discovery,
/// the occasional account switch or logout), so this window is far larger than
/// any realistic in-flight event backlog while keeping the map bounded for a
/// pathological or long-lived process.
const RETAINED_EPOCHS: u64 = 128;

/// Outcome of an [`IdentityEpochs::identify`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentifyOutcome {
    /// The observed epoch was no longer current: the discovery result was stale
    /// (a newer transition already happened) and nothing changed.
    Stale,
    /// The identity was validated, persisted, and bound to a fresh epoch.
    Identified,
}

/// Error from an [`IdentityEpochs::identify`] call.
#[derive(Debug, PartialEq, Eq)]
pub enum IdentifyError<E> {
    /// The raw user id failed validation (blank after trim, overlong, or
    /// containing control characters). No epoch was bound.
    Invalid,
    /// The persistence callback failed. No epoch was bound and nothing changed.
    Persist(E),
}

/// A monotonic epoch counter plus a write-once map of `epoch -> Option<pseudonym>`.
///
/// Cheap to share: hold it behind an [`Arc`] and clone the handle into the event
/// enqueue path (which calls [`current`](Self::current)), the export path (which
/// calls [`resolve`](Self::resolve)), and the auth/discovery call sites (which
/// call [`identify`](Self::identify) / [`clear`](Self::clear)).
#[derive(Debug)]
pub struct IdentityEpochs {
    current: AtomicU64,
    bindings: RwLock<BTreeMap<u64, Option<Arc<str>>>>,
    /// Serialises identity *transitions* so each `validate -> persist -> bind`
    /// sequence is atomic with respect to every other transition — without ever
    /// holding the `bindings` lock across the (synchronous) persist callback.
    /// `current()` and `resolve()` never take this lock, so a slow persist can
    /// never block event stamping or export resolution, and a persist callback
    /// that itself reads identity cannot deadlock.
    transition: Mutex<()>,
}

impl IdentityEpochs {
    /// Seed epoch 0 from the persisted raw id (if any) at construction, before
    /// the telemetry worker accepts any event. An invalid persisted value binds
    /// anonymous, so a corrupt row can never identify one emitter while another
    /// stays anonymous.
    pub fn from_persisted(persisted: Option<&str>) -> Self {
        let binding = persisted
            .filter(|raw| is_valid_raw_user_id(raw))
            .map(|raw| Arc::from(pseudonymous_user_id(raw)));
        let mut bindings = BTreeMap::new();
        bindings.insert(0, binding);
        Self {
            current: AtomicU64::new(0),
            bindings: RwLock::new(bindings),
            transition: Mutex::new(()),
        }
    }

    /// The epoch to stamp onto an event at enqueue time. One atomic load.
    pub fn current(&self) -> u64 {
        self.current.load(Ordering::Acquire)
    }

    /// Resolve an event's stamped epoch to its pseudonym at export time. An
    /// absent (pruned) epoch or one bound to anonymous resolves to `None`.
    pub fn resolve(&self, epoch: u64) -> Option<Arc<str>> {
        self.bindings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&epoch)
            .cloned()
            .flatten()
    }

    /// Record that identity became known.
    ///
    /// `observed` is the epoch captured with [`current`](Self::current) *before*
    /// the discovery network call. If it is no longer current the result is
    /// stale (a newer transition intervened) and this is a no-op — this single
    /// check is the entire stale-discovery guard, and it works identically for
    /// V1 and V2 discovery callers.
    ///
    /// Persist-then-bind: the synchronous `persist` callback runs first; on
    /// failure nothing is bound and the caller gets [`IdentifyError::Persist`],
    /// so the live and persisted identity can never diverge.
    pub fn identify<E>(
        &self,
        observed: u64,
        raw_user_id: &str,
        persist: impl FnOnce(&str) -> Result<(), E>,
    ) -> Result<IdentifyOutcome, IdentifyError<E>> {
        let _txn = self.transition.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if observed != self.current.load(Ordering::Acquire) {
            return Ok(IdentifyOutcome::Stale);
        }
        if !is_valid_raw_user_id(raw_user_id) {
            return Err(IdentifyError::Invalid);
        }
        persist(raw_user_id).map_err(IdentifyError::Persist)?;
        let pseudonym: Arc<str> = Arc::from(pseudonymous_user_id(raw_user_id));
        self.bind(Some(pseudonym));
        Ok(IdentifyOutcome::Identified)
    }

    /// Logout / pre-switch boundary. Binds anonymous and advances the epoch
    /// UNCONDITIONALLY first, then attempts to clear the persisted value. A
    /// broken DB therefore degrades a logged-out user to anonymous, never leaves
    /// their identity live. Returns the persist result so the caller can log or
    /// retry the durable clear.
    pub fn clear<E>(&self, persist_clear: impl FnOnce() -> Result<(), E>) -> Result<(), E> {
        let _txn = self.transition.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        self.bind(None);
        persist_clear()
    }

    /// Bind the next epoch and advance the counter. The caller MUST hold
    /// `transition`, so `current` is stable across the read/insert/store here.
    fn bind(&self, value: Option<Arc<str>>) {
        let mut bindings = self.bindings.write().unwrap_or_else(|poisoned| poisoned.into_inner());
        let next = self
            .current
            .load(Ordering::Acquire)
            .checked_add(1)
            .expect("telemetry identity epoch counter exhausted");
        bindings.insert(next, value);
        // Bound the map to the trailing window. Any event still holding an older
        // epoch resolves to anonymous (never a wrong identity).
        if next >= RETAINED_EPOCHS {
            let retained = bindings.split_off(&(next - RETAINED_EPOCHS + 1));
            *bindings = retained;
        }
        self.current.store(next, Ordering::Release);
    }
}

/// Canonical validation for a raw telemetry user id, shared by seeding and live
/// updates: reject blank-after-trim, overlong, or control-character values. This
/// is the single source of truth so the TUI and the Rust host can never disagree
/// about whether a persisted row is usable.
pub fn is_valid_raw_user_id(raw_user_id: &str) -> bool {
    !raw_user_id.trim().is_empty()
        && raw_user_id.len() <= MAX_RAW_USER_ID_BYTES
        && !raw_user_id.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        Barrier,
        Mutex,
    };
    use std::thread;

    use super::*;

    fn tail(pseudonym: &str) -> &str {
        pseudonym.strip_prefix("v1:").expect("versioned pseudonym prefix")
    }

    #[test]
    fn t1_misattribution_is_impossible() {
        let epochs = IdentityEpochs::from_persisted(Some("original-user"));
        let stamped = epochs.current();
        let original = epochs.resolve(stamped);

        epochs.identify::<()>(stamped, "next-user", |_| Ok(())).unwrap();
        epochs.clear::<()>(|| Ok(())).unwrap();

        // The event stamped with `stamped` still resolves to its original binding.
        assert_eq!(epochs.resolve(stamped), original);
        assert_eq!(
            epochs.resolve(stamped).as_deref(),
            Some(pseudonymous_user_id("original-user").as_str())
        );
    }

    #[test]
    fn t2_stale_guard_is_a_no_op() {
        let epochs = IdentityEpochs::from_persisted(Some("original-user"));
        let observed = epochs.current();
        epochs.identify::<()>(observed, "intervening-user", |_| Ok(())).unwrap();
        let current = epochs.current();
        let binding = epochs.resolve(current);

        let outcome = epochs
            .identify::<()>(observed, "stale-user", |_| panic!("stale persist must not run"))
            .unwrap();

        assert_eq!(outcome, IdentifyOutcome::Stale);
        assert_eq!(epochs.current(), current);
        assert_eq!(epochs.resolve(current), binding);
    }

    #[test]
    fn t3_identify_persists_before_publish() {
        let epochs = IdentityEpochs::from_persisted(Some("original-user"));
        let before = epochs.current();
        let binding = epochs.resolve(before);

        let result = epochs.identify(before, "next-user", |_| Err("disk full"));

        assert_eq!(result, Err(IdentifyError::Persist("disk full")));
        assert_eq!(epochs.current(), before);
        assert_eq!(epochs.resolve(before), binding);
    }

    #[test]
    fn t4_clear_degrades_to_anonymous_on_persist_failure() {
        let epochs = IdentityEpochs::from_persisted(Some("original-user"));
        let result = epochs.clear(|| Err("disk full"));
        assert_eq!(result, Err("disk full"));
        assert_eq!(epochs.resolve(epochs.current()), None);
    }

    #[test]
    fn t5_malformed_persisted_values_bind_anonymous() {
        let too_long = "x".repeat(MAX_RAW_USER_ID_BYTES + 1);
        for malformed in ["   ", too_long.as_str(), "user\u{0007}id"] {
            let epochs = IdentityEpochs::from_persisted(Some(malformed));
            assert_eq!(epochs.resolve(epochs.current()), None);
        }
    }

    #[test]
    fn t6_pseudonym_format_is_stable_and_distinguishing() {
        let epochs = IdentityEpochs::from_persisted(None);
        epochs
            .identify::<()>(epochs.current(), "private-user-id", |_| Ok(()))
            .unwrap();
        let p = epochs.resolve(epochs.current()).unwrap();

        assert_eq!(tail(&p).len(), 43);
        assert!(
            tail(&p)
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        );
        assert_eq!(p.as_ref(), pseudonymous_user_id("private-user-id"));
        assert_ne!(p.as_ref(), pseudonymous_user_id("different-user-id"));
    }

    #[test]
    fn t7_bounded_map_prunes_but_keeps_recent() {
        let epochs = IdentityEpochs::from_persisted(None);
        for i in 0..(RETAINED_EPOCHS + 50) {
            let observed = epochs.current();
            let raw = format!("user-{i}");
            epochs.identify::<()>(observed, &raw, |_| Ok(())).unwrap();
        }
        let current = epochs.current();
        // The map is bounded...
        assert!(epochs.bindings.read().unwrap().len() as u64 <= RETAINED_EPOCHS);
        // ...but the most recent epoch still resolves, and an ancient one does not.
        assert!(epochs.resolve(current).is_some());
        assert_eq!(epochs.resolve(0), None);
    }

    #[test]
    fn t8_concurrent_stamps_resolve_consistently() {
        const THREADS: usize = 8;
        const READS: usize = 2_000;
        let epochs = Arc::new(IdentityEpochs::from_persisted(Some("initial-user")));
        let start = Arc::new(Barrier::new(THREADS + 1));
        let recorded = Arc::new(Mutex::new(Vec::with_capacity(THREADS * READS)));
        let mut handles = Vec::new();

        for _ in 0..THREADS {
            let epochs = Arc::clone(&epochs);
            let start = Arc::clone(&start);
            let recorded = Arc::clone(&recorded);
            handles.push(thread::spawn(move || {
                start.wait();
                let mut local = Vec::with_capacity(READS);
                for _ in 0..READS {
                    local.push(epochs.current());
                    thread::yield_now();
                }
                recorded.lock().unwrap().extend(local);
            }));
        }

        start.wait();
        for i in 0..40 {
            if i % 3 == 0 {
                epochs.clear::<()>(|| Ok(())).unwrap();
            } else {
                let observed = epochs.current();
                epochs
                    .identify::<()>(observed, &format!("user-{i}"), |_| Ok(()))
                    .unwrap();
            }
            thread::yield_now();
        }
        for handle in handles {
            handle.join().unwrap();
        }

        // Every stamped epoch resolves to whatever the map holds for it — no torn
        // state, no panic. (Bindings are only ever added, so a live epoch that is
        // still within the retained window resolves; nothing resolves to a value
        // that was never bound to it.)
        let final_epoch = epochs.current();
        for &epoch in recorded.lock().unwrap().iter() {
            assert!(epoch <= final_epoch);
            let _ = epochs.resolve(epoch);
        }
    }
}
