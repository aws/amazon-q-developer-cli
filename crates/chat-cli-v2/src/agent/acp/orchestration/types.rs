//! Shared types for the orchestration system.

use std::time::SystemTime;

use sacp::schema::SessionId;
use serde::{
    Deserialize,
    Serialize,
};

/// A message stored in a session's inbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxMessage {
    pub from_session: SessionId,
    pub from_name: String,
    pub message: String,
    pub timestamp: SystemTime,
    pub read: bool,
    #[serde(default)]
    pub is_escalation: bool,
}

/// Metadata about an orchestrated session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratedSession {
    pub session_id: SessionId,
    pub name: String,
    pub role: Option<String>,
    pub agent_name: String,
    pub task: String,
    pub parent_session: Option<SessionId>,
    pub group: Option<String>,
    pub status: SessionStatus,
    pub created_at: SystemTime,
    pub last_activity: SystemTime,
    /// Whether a human is currently attached to this session
    pub human_attached: bool,
    /// Whether this is a persistent session (knight) or ephemeral (squire)
    pub persistent: bool,
    /// DAG edges: names of stages this session depends on
    pub depends_on: Vec<String>,
    /// Result stored on completion — injected into dependent stages' context regardless of inbox
    /// state
    pub result: Option<String>,
}

/// Status of an orchestrated session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionStatus {
    /// Session is idle, waiting for input
    Idle,
    /// Session is actively processing a turn
    Busy,
    /// Session has been terminated
    Terminated,
}

/// A named group of sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionGroup {
    pub name: String,
    pub series: String,
    pub members: Vec<GroupMembership>,
    /// Stages waiting for their dependencies to complete (crew DAG)
    #[serde(default)]
    pub pending_stages: Vec<PendingStage>,
}

/// A pipeline stage waiting for dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingStage {
    pub name: String,
    pub role: String,
    pub task: String,
    pub depends_on: Vec<String>,
    pub agent_name: String,
}

/// Membership info for a session in a group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMembership {
    pub session_id: SessionId,
    pub name: String,
    pub role: Option<String>,
    pub joined_at: SystemTime,
}
