//! Shared types for the orchestration system.

use std::time::SystemTime;

use sacp::schema::SessionId;
use serde::{
    Deserialize,
    Serialize,
};

/// Metadata about an orchestrated session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratedSession {
    pub session_id: SessionId,
    pub name: String,
    pub role: Option<String>,
    pub agent_name: String,
    #[serde(default)]
    pub model: Option<String>,
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
    /// Result stored on completion — injected into dependent stages' context
    pub result: Option<String>,
    /// Loop-back config: when this session completes and output contains trigger, re-run target.
    #[serde(default)]
    pub loop_config: Option<LoopConfig>,
    /// Current loop iteration count for this session.
    #[serde(default)]
    pub loop_iteration: u32,
    /// Whether the subagent explicitly signaled "changes_needed" via the summary tool's resultType.
    #[serde(default)]
    pub changes_needed: bool,
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

/// Loop-back configuration for pipeline stages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopConfig {
    /// Name of the stage to loop back to.
    pub target: String,
    /// Maximum number of loop iterations.
    pub max_iterations: u32,
    /// Text in output that triggers the loop.
    pub trigger: String,
}

/// A pipeline stage waiting for dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingStage {
    pub name: String,
    pub role: String,
    pub task: String,
    pub depends_on: Vec<String>,
    pub agent_name: String,
    #[serde(default)]
    pub model: Option<String>,
    /// Loop-back config: when this stage completes and output contains trigger, re-run target.
    #[serde(default)]
    pub loop_config: Option<LoopConfig>,
    /// Current loop iteration count.
    #[serde(default)]
    pub loop_iteration: u32,
}

/// Membership info for a session in a group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMembership {
    pub session_id: SessionId,
    pub name: String,
    pub role: Option<String>,
    pub joined_at: SystemTime,
}

/// Data extracted from a terminated session whose loop trigger fired.
/// Captures everything needed to re-enqueue stages without holding a borrow
/// on the session map.
#[derive(Debug, Clone)]
pub struct LoopTriggerData {
    pub loop_config: LoopConfig,
    pub iteration: u32,
    pub session_name: String,
    pub result_text: String,
    pub session_task: String,
    pub session_role: String,
    pub agent_name: String,
    pub model: Option<String>,
}
