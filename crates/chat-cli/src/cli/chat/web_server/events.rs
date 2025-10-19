use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::agent_env::events::{
    AgentEnvironmentEvent, AgentLoopEvent, JobCompletionResult, JobEvent, OutputChunk,
    SystemEvent, UserInteractionRequired, WorkerEvent,
};

use super::serialization::{ConversationEntryJson, WorkerMetadataJson};

// Global state for time conversion (initialized at process start)
static PROCESS_START_INSTANT: OnceLock<Instant> = OnceLock::new();
static PROCESS_START_SYSTEM_TIME: OnceLock<SystemTime> = OnceLock::new();

/// Initialize time conversion (call at process start)
///
/// This must be called once at application startup to establish the relationship
/// between Instant (monotonic) and SystemTime (wall clock) for timestamp conversion.
pub fn init_time_conversion() {
    PROCESS_START_INSTANT.get_or_init(Instant::now);
    PROCESS_START_SYSTEM_TIME.get_or_init(SystemTime::now);
}

/// Convert Instant to Unix timestamp
///
/// Note: Instant is monotonic and doesn't have a fixed epoch.
/// We track the relationship between Instant and SystemTime at process start.
pub fn instant_to_unix_timestamp(instant: Instant) -> f64 {
    let start_instant = PROCESS_START_INSTANT
        .get()
        .expect("init_time_conversion() must be called at startup");
    let start_system_time = PROCESS_START_SYSTEM_TIME
        .get()
        .expect("init_time_conversion() must be called at startup");

    let elapsed = instant.duration_since(*start_instant);
    let system_time = *start_system_time + elapsed;

    system_time
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

/// Worker lifecycle state (serializable)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerLifecycleState {
    Idle,
    Busy,
    IdleFailed,
}

impl From<crate::agent_env::events::WorkerLifecycleState> for WorkerLifecycleState {
    fn from(state: crate::agent_env::events::WorkerLifecycleState) -> Self {
        match state {
            crate::agent_env::events::WorkerLifecycleState::Idle => Self::Idle,
            crate::agent_env::events::WorkerLifecycleState::Busy => Self::Busy,
            crate::agent_env::events::WorkerLifecycleState::IdleFailed => Self::IdleFailed,
        }
    }
}

/// Job completion result (serializable)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum JobResult {
    Success {
        #[serde(skip_serializing_if = "Option::is_none")]
        task_metadata: Option<serde_json::Value>,
        user_interaction_required: bool,
    },
    Cancelled,
    Failed {
        error: String,
    },
}

impl From<JobCompletionResult> for JobResult {
    fn from(result: JobCompletionResult) -> Self {
        match result {
            JobCompletionResult::Success {
                task_metadata,
                user_interaction_required,
            } => Self::Success {
                task_metadata: if task_metadata.is_empty() {
                    None
                } else {
                    Some(serde_json::to_value(task_metadata).unwrap_or(serde_json::Value::Null))
                },
                user_interaction_required: matches!(
                    user_interaction_required,
                    UserInteractionRequired::ToolApproval
                ),
            },
            JobCompletionResult::Cancelled => Self::Cancelled,
            JobCompletionResult::Failed { error } => Self::Failed { error },
        }
    }
}

/// Output chunk data (serializable)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "chunk_type", rename_all = "snake_case")]
pub enum OutputChunkData {
    AssistantResponse { text: String },
    ToolUse {
        tool_name: String,
        tool_input: serde_json::Value,
    },
    ToolResult {
        tool_name: String,
        result: String,
    },
}

impl From<OutputChunk> for OutputChunkData {
    fn from(chunk: OutputChunk) -> Self {
        match chunk {
            OutputChunk::AssistantResponse(text) => Self::AssistantResponse { text },
            OutputChunk::ToolUse {
                tool_name,
                tool_input,
            } => Self::ToolUse {
                tool_name,
                tool_input,
            },
            OutputChunk::ToolResult { tool_name, result } => Self::ToolResult { tool_name, result },
        }
    }
}

/// Serializable events for WebUI
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebUIEvent {
    // Worker Events
    WorkerCreated {
        worker_id: String,
        name: String,
        timestamp: f64,
    },
    WorkerDeleted {
        worker_id: String,
        timestamp: f64,
    },
    WorkerStateChanged {
        worker_id: String,
        old_state: WorkerLifecycleState,
        new_state: WorkerLifecycleState,
        timestamp: f64,
    },

    // Job Events
    JobStarted {
        worker_id: String,
        job_id: String,
        task_type: String,
        timestamp: f64,
    },
    JobCompleted {
        worker_id: String,
        job_id: String,
        result: JobResult,
        timestamp: f64,
    },
    OutputChunk {
        worker_id: String,
        job_id: String,
        chunk: OutputChunkData,
        timestamp: f64,
    },

    // AgentLoop Events
    ResponseReceived {
        worker_id: String,
        job_id: String,
        text: String,
        timestamp: f64,
    },
    ToolUseRequested {
        worker_id: String,
        job_id: String,
        tool_name: String,
        tool_input: serde_json::Value,
        timestamp: f64,
    },

    // System Events
    ShutdownInitiated {
        reason: String,
        timestamp: f64,
    },

    // WebUI Events
    PromptReceived {
        worker_id: String,
        text: String,
        timestamp: f64,
    },
    ServerStarted {
        address: String,
        timestamp: f64,
    },
    WebSocketConnected {
        timestamp: f64,
    },

    // Snapshot Events (for initial state sync and queries)
    WorkersSnapshot {
        workers: Vec<WorkerMetadataJson>,
        timestamp: f64,
    },
    ConversationSnapshot {
        worker_id: String,
        entries: Vec<ConversationEntryJson>,
        timestamp: f64,
    },

    // Error Events
    Error {
        command: String,
        message: String,
        timestamp: f64,
    },
}

impl WebUIEvent {
    /// Convert AgentEnvironmentEvent to WebUIEvent
    pub fn from_agent_event(event: AgentEnvironmentEvent) -> Self {
        match event {
            AgentEnvironmentEvent::Worker(worker_event) => Self::from_worker_event(worker_event),
            AgentEnvironmentEvent::Job(job_event) => Self::from_job_event(job_event),
            AgentEnvironmentEvent::AgentLoop(agent_loop_event) => {
                Self::from_agent_loop_event(agent_loop_event)
            }
            AgentEnvironmentEvent::System(system_event) => {
                Self::from_system_event(system_event)
            }
            AgentEnvironmentEvent::WebUI(webui_event) => {
                Self::from_webui_event(webui_event)
            }
        }
    }

    fn from_worker_event(event: WorkerEvent) -> Self {
        match event {
            WorkerEvent::Created {
                worker_id,
                name,
                timestamp,
            } => WebUIEvent::WorkerCreated {
                worker_id: worker_id.to_string(),
                name,
                timestamp: instant_to_unix_timestamp(timestamp),
            },
            WorkerEvent::Deleted {
                worker_id,
                timestamp,
            } => WebUIEvent::WorkerDeleted {
                worker_id: worker_id.to_string(),
                timestamp: instant_to_unix_timestamp(timestamp),
            },
            WorkerEvent::LifecycleStateChanged {
                worker_id,
                old_state,
                new_state,
                timestamp,
            } => WebUIEvent::WorkerStateChanged {
                worker_id: worker_id.to_string(),
                old_state: old_state.into(),
                new_state: new_state.into(),
                timestamp: instant_to_unix_timestamp(timestamp),
            },
        }
    }

    fn from_job_event(event: JobEvent) -> Self {
        match event {
            JobEvent::Started {
                worker_id,
                job_id,
                task_type,
                timestamp,
            } => WebUIEvent::JobStarted {
                worker_id: worker_id.to_string(),
                job_id: job_id.to_string(),
                task_type,
                timestamp: instant_to_unix_timestamp(timestamp),
            },
            JobEvent::Completed {
                worker_id,
                job_id,
                result,
                timestamp,
            } => WebUIEvent::JobCompleted {
                worker_id: worker_id.to_string(),
                job_id: job_id.to_string(),
                result: result.into(),
                timestamp: instant_to_unix_timestamp(timestamp),
            },
            JobEvent::OutputChunk {
                worker_id,
                job_id,
                chunk,
                timestamp,
            } => WebUIEvent::OutputChunk {
                worker_id: worker_id.to_string(),
                job_id: job_id.to_string(),
                chunk: chunk.into(),
                timestamp: instant_to_unix_timestamp(timestamp),
            },
        }
    }

    fn from_agent_loop_event(event: AgentLoopEvent) -> Self {
        match event {
            AgentLoopEvent::ResponseReceived {
                worker_id,
                job_id,
                text,
                timestamp,
            } => WebUIEvent::ResponseReceived {
                worker_id: worker_id.to_string(),
                job_id: job_id.to_string(),
                text,
                timestamp: instant_to_unix_timestamp(timestamp),
            },
            AgentLoopEvent::ToolUseRequestReceived {
                worker_id,
                job_id,
                tool_name,
                tool_input,
                timestamp,
            } => WebUIEvent::ToolUseRequested {
                worker_id: worker_id.to_string(),
                job_id: job_id.to_string(),
                tool_name,
                tool_input,
                timestamp: instant_to_unix_timestamp(timestamp),
            },
        }
    }

    fn from_system_event(event: SystemEvent) -> Self {
        match event {
            SystemEvent::ShutdownInitiated { reason, timestamp } => {
                WebUIEvent::ShutdownInitiated {
                    reason,
                    timestamp: instant_to_unix_timestamp(timestamp),
                }
            }
        }
    }

    fn from_webui_event(event: crate::agent_env::events::WebUIEvent) -> Self {
        match event {
            crate::agent_env::events::WebUIEvent::PromptReceived {
                worker_id,
                text,
                timestamp,
            } => WebUIEvent::PromptReceived {
                worker_id: worker_id.to_string(),
                text,
                timestamp: instant_to_unix_timestamp(timestamp),
            },
            crate::agent_env::events::WebUIEvent::ServerStarted {
                address,
                timestamp,
            } => WebUIEvent::ServerStarted {
                address,
                timestamp: instant_to_unix_timestamp(timestamp),
            },
            crate::agent_env::events::WebUIEvent::WebSocketConnected {
                timestamp,
            } => WebUIEvent::WebSocketConnected {
                timestamp: instant_to_unix_timestamp(timestamp),
            },
        }
    }

    /// Extract worker_id from events that have one
    pub fn worker_id(&self) -> Option<&str> {
        match self {
            Self::WorkerCreated { worker_id, .. }
            | Self::WorkerDeleted { worker_id, .. }
            | Self::WorkerStateChanged { worker_id, .. }
            | Self::JobStarted { worker_id, .. }
            | Self::JobCompleted { worker_id, .. }
            | Self::OutputChunk { worker_id, .. }
            | Self::ResponseReceived { worker_id, .. }
            | Self::ToolUseRequested { worker_id, .. }
            | Self::PromptReceived { worker_id, .. }
            | Self::ConversationSnapshot { worker_id, .. } => Some(worker_id),
            Self::ShutdownInitiated { .. }
            | Self::ServerStarted { .. }
            | Self::WebSocketConnected { .. }
            | Self::WorkersSnapshot { .. }
            | Self::Error { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_time_conversion_initialization() {
        init_time_conversion();
        assert!(PROCESS_START_INSTANT.get().is_some());
        assert!(PROCESS_START_SYSTEM_TIME.get().is_some());
    }

    #[test]
    fn test_instant_to_unix_timestamp() {
        init_time_conversion();
        let now = Instant::now();
        let timestamp = instant_to_unix_timestamp(now);

        // Timestamp should be reasonable (not negative, not far future)
        assert!(timestamp > 1_600_000_000.0); // After 2020
        assert!(timestamp < 2_000_000_000.0); // Before 2033
    }

    #[test]
    fn test_worker_lifecycle_state_conversion() {
        let idle: WorkerLifecycleState =
            crate::agent_env::events::WorkerLifecycleState::Idle.into();
        assert!(matches!(idle, WorkerLifecycleState::Idle));

        let busy: WorkerLifecycleState =
            crate::agent_env::events::WorkerLifecycleState::Busy.into();
        assert!(matches!(busy, WorkerLifecycleState::Busy));

        let failed: WorkerLifecycleState =
            crate::agent_env::events::WorkerLifecycleState::IdleFailed.into();
        assert!(matches!(failed, WorkerLifecycleState::IdleFailed));
    }

    #[test]
    fn test_worker_created_event_conversion() {
        init_time_conversion();

        let worker_id = Uuid::new_v4();
        let timestamp = Instant::now();

        let internal_event = AgentEnvironmentEvent::Worker(WorkerEvent::Created {
            worker_id,
            name: "test-worker".to_string(),
            timestamp,
        });

        let web_event = WebUIEvent::from_agent_event(internal_event);

        match web_event {
            WebUIEvent::WorkerCreated {
                worker_id: id,
                name,
                timestamp: ts,
            } => {
                assert_eq!(id, worker_id.to_string());
                assert_eq!(name, "test-worker");
                assert!(ts > 0.0);
            }
            _ => panic!("Expected WorkerCreated event"),
        }
    }

    #[test]
    fn test_job_started_event_conversion() {
        init_time_conversion();

        let worker_id = Uuid::new_v4();
        let job_id = Uuid::new_v4();
        let timestamp = Instant::now();

        let internal_event = AgentEnvironmentEvent::Job(JobEvent::Started {
            worker_id,
            job_id,
            task_type: "agent_loop".to_string(),
            timestamp,
        });

        let web_event = WebUIEvent::from_agent_event(internal_event);

        match web_event {
            WebUIEvent::JobStarted {
                worker_id: wid,
                job_id: jid,
                task_type,
                ..
            } => {
                assert_eq!(wid, worker_id.to_string());
                assert_eq!(jid, job_id.to_string());
                assert_eq!(task_type, "agent_loop");
            }
            _ => panic!("Expected JobStarted event"),
        }
    }

    #[test]
    fn test_output_chunk_conversion() {
        let chunk = OutputChunk::AssistantResponse("Hello!".to_string());
        let data: OutputChunkData = chunk.into();

        match data {
            OutputChunkData::AssistantResponse { text } => {
                assert_eq!(text, "Hello!");
            }
            _ => panic!("Expected AssistantResponse"),
        }
    }

    #[test]
    fn test_worker_id_extraction() {
        init_time_conversion();

        let worker_id = Uuid::new_v4();
        let timestamp = Instant::now();

        let event = WebUIEvent::from_agent_event(AgentEnvironmentEvent::Worker(
            WorkerEvent::Created {
                worker_id,
                name: "test".to_string(),
                timestamp,
            },
        ));

        assert_eq!(event.worker_id(), Some(worker_id.to_string().as_str()));

        let event = WebUIEvent::ShutdownInitiated {
            reason: "test".to_string(),
            timestamp: 0.0,
        };
        assert_eq!(event.worker_id(), None);
    }
}
