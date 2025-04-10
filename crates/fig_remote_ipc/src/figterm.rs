use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use fig_proto::fig::EnvironmentVariable;
use fig_proto::local::{
    ShellContext,
    TerminalCursorCoordinates,
};
use fig_proto::remote::{
    Clientbound,
    hostbound,
};
use parking_lot::lock_api::MutexGuard;
use parking_lot::{
    FairMutex,
    MappedFairMutexGuard,
    RawFairMutex,
};
use serde::Serialize;
use time::OffsetDateTime;
use tokio::sync::{
    broadcast,
    oneshot,
};
use tokio::time::Instant;
use uuid::Uuid;

#[derive(Clone, Default, Debug)]
pub struct EditBuffer {
    pub text: String,
    pub cursor: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionMetrics {
    pub start_time: OffsetDateTime,
    pub end_time: OffsetDateTime,
    pub num_insertions: i64,
    pub num_popups: i64,
}

impl SessionMetrics {
    pub fn new(start: OffsetDateTime) -> Self {
        Self {
            start_time: start,
            end_time: start,
            num_insertions: 0,
            num_popups: 0,
        }
    }
}

#[derive(Debug, Default, Serialize)]
pub struct InnerFigtermState {
    /// All current sessions of [FigtermSession]'s.
    pub linked_sessions: HashMap<Uuid, FigtermSession>,
    /// The most recent figterm session
    pub most_recent: Option<Uuid>,
}

#[derive(Debug, Default, Serialize)]
pub struct FigtermState {
    #[serde(flatten)]
    pub inner: FairMutex<InnerFigtermState>,
}

impl FigtermState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a new session id
    pub fn insert(&self, session: FigtermSession) {
        let mut figterm_state = self.inner.lock();
        figterm_state.most_recent = Some(session.id);
        figterm_state.linked_sessions.insert(session.id, session);
    }

    /// Gets mutable reference to the given session id and sets the most recent session id
    pub fn with_update<T>(&self, key: Uuid, f: impl FnOnce(&mut FigtermSession) -> T) -> Option<T> {
        let mut guard = self.inner.lock();
        let res = guard
            .linked_sessions
            .get_mut(&key)
            .and_then(|session| match session.dead_since {
                Some(_) => None,
                None => Some(f(session)),
            });

        if res.is_some() {
            guard.most_recent = Some(key);
        }

        res
    }

    pub fn with_most_recent<T>(&self, f: impl FnOnce(&mut FigtermSession) -> T) -> Option<T> {
        let mut guard = self.inner.lock();
        let id = guard.most_recent?;
        guard
            .linked_sessions
            .get_mut(&id)
            .and_then(|session| match session.dead_since {
                Some(_) => None,
                None => Some(f(session)),
            })
    }

    /// Gets mutable reference to the given session id
    pub fn with<T>(&self, session_id: &Uuid, f: impl FnOnce(&mut FigtermSession) -> T) -> Option<T> {
        let mut guard = self.inner.lock();
        guard.linked_sessions.get_mut(session_id).map(f)
    }

    pub fn get(&self, session_id: &Uuid) -> Option<MappedFairMutexGuard<'_, FigtermSession>> {
        MutexGuard::<'_, RawFairMutex, InnerFigtermState>::try_map(
            self.inner.lock(),
            |guard: &mut InnerFigtermState| guard.linked_sessions.get_mut(session_id),
        )
        .ok()
    }

    pub fn most_recent(&self) -> Option<MappedFairMutexGuard<'_, FigtermSession>> {
        MutexGuard::<'_, RawFairMutex, InnerFigtermState>::try_map(
            self.inner.lock(),
            |guard: &mut InnerFigtermState| {
                guard
                    .most_recent
                    .as_mut()
                    .and_then(|id| guard.linked_sessions.get_mut(id))
            },
        )
        .ok()
    }

    pub fn with_maybe_id<T>(&self, session_id: &Option<Uuid>, f: impl FnOnce(&mut FigtermSession) -> T) -> Option<T> {
        match session_id {
            Some(session_id) => self.with(session_id, f),
            None => self.with_most_recent(f),
        }
    }

    pub fn remove_id(&self, session_id: &Uuid) -> Option<FigtermSession> {
        let mut guard = self.inner.lock();
        if guard.most_recent.as_ref() == Some(session_id) {
            guard.most_recent = None;
        }
        guard.linked_sessions.remove(session_id)
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum InterceptMode {
    Locked,
    Unlocked,
}

impl From<bool> for InterceptMode {
    fn from(from: bool) -> Self {
        if from {
            InterceptMode::Locked
        } else {
            InterceptMode::Unlocked
        }
    }
}

impl From<InterceptMode> for bool {
    fn from(from: InterceptMode) -> Self {
        match from {
            InterceptMode::Locked => true,
            InterceptMode::Unlocked => false,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct FigtermSession {
    pub id: Uuid,
    pub secret: String,
    #[serde(skip)]
    pub sender: flume::Sender<FigtermCommand>,
    #[serde(skip)]
    pub writer: Option<flume::Sender<Clientbound>>,
    #[serde(skip)]
    pub dead_since: Option<Instant>, // TODO: prune old sessions
    #[serde(skip)]
    pub edit_buffer: EditBuffer,
    #[serde(skip)]
    pub last_receive: Instant,
    pub context: Option<ShellContext>,
    #[serde(skip)]
    pub terminal_cursor_coordinates: Option<TerminalCursorCoordinates>,
    pub current_session_metrics: Option<SessionMetrics>,
    #[serde(skip)]
    pub response_map: HashMap<u64, oneshot::Sender<hostbound::response::Response>>,
    #[serde(skip)]
    pub nonce_counter: Arc<AtomicU64>,
    #[serde(skip)]
    pub on_close_tx: broadcast::Sender<()>,
    pub intercept: InterceptMode,
    pub intercept_global: InterceptMode,
}

#[derive(Debug)]
pub struct FigtermSessionInfo {
    pub edit_buffer: EditBuffer,
    pub context: Option<ShellContext>,
}

impl FigtermSession {
    #[allow(dead_code)]
    pub fn get_info(&self) -> FigtermSessionInfo {
        FigtermSessionInfo {
            edit_buffer: self.edit_buffer.clone(),
            context: self.context.clone(),
        }
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum FigtermCommand {
    InterceptFigJs {
        intercept_keystrokes: bool,
        intercept_global_keystrokes: bool,
        actions: Vec<fig_proto::figterm::Action>,
        override_actions: bool,
    },
    InterceptFigJSVisible {
        visible: bool,
    },
    InsertText {
        insertion: Option<String>,
        deletion: Option<i64>,
        offset: Option<i64>,
        immediate: Option<bool>,
        insertion_buffer: Option<String>,
        insert_during_command: Option<bool>,
    },
    SetBuffer {
        text: String,
        cursor_position: Option<u64>,
    },
    RunProcess {
        channel: oneshot::Sender<hostbound::response::Response>,
        executable: String,
        arguments: Vec<String>,
        working_directory: Option<String>,
        env: Vec<EnvironmentVariable>,
        timeout: Option<Duration>,
    },
}

macro_rules! field {
    ($fn_name:ident: $enum_name:ident, $($field_name: ident: $field_type: ty),*,) => {
        pub fn $fn_name($($field_name: $field_type),*) -> (Self, oneshot::Receiver<hostbound::response::Response>) {
            let (tx, rx) = oneshot::channel();
            (Self::$enum_name {channel: tx, $($field_name),*}, rx)
        }
    };
}

impl FigtermCommand {
    field!(
        run_process: RunProcess,
        executable: String,
        arguments: Vec<String>,
        working_directory: Option<String>,
        env: Vec<EnvironmentVariable>,
        timeout: Option<Duration>,
    );
}
