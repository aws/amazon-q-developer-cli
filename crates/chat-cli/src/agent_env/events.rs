//! Event types for the Agent Environment system.
//!
//! This module defines all event types that flow through the EventBus.
//! Events are organized in a nested enum structure for clear categorization:
//! - Worker events: lifecycle and state changes
//! - Job events: execution lifecycle and output
//! - AgentLoop events: task-specific events
//! - System events: system-level notifications

use std::collections::HashMap;
use std::time::Instant;
use uuid::Uuid;

/// Worker lifecycle states (managed by Session)
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum WorkerLifecycleState {
    /// Worker is idle and ready to accept new jobs
    #[default]
    Idle,
    /// Worker is currently executing a job
    Busy,
    /// Worker completed a job with failure
    IdleFailed,
}

/// Indicates if a completed job requires user interaction to continue
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserInteractionRequired {
    /// No user interaction required - job completed cleanly
    None,
    /// Job is waiting for tool approval from user
    ToolApproval,
}

/// Job completion results
#[derive(Debug, Clone)]
pub enum JobCompletionResult {
    /// Job completed successfully
    Success {
        task_metadata: HashMap<String, serde_json::Value>,
        user_interaction_required: UserInteractionRequired,
    },
    /// Job was cancelled
    Cancelled,
    /// Job failed with an error
    Failed {
        error: String,
    },
}

/// Output chunk types
#[derive(Debug, Clone)]
pub enum OutputChunk {
    /// Text response from the assistant
    AssistantResponse(String),
    /// Tool use request from the assistant
    ToolUse {
        tool_name: String,
        tool_input: serde_json::Value,
    },
    /// Result from tool execution
    ToolResult {
        tool_name: String,
        result: String,
    },
}

/// Worker lifecycle and state events
#[derive(Debug, Clone)]
pub enum WorkerEvent {
    /// Worker was created
    Created {
        worker_id: Uuid,
        name: String,
        timestamp: Instant,
    },
    /// Worker was deleted
    Deleted {
        worker_id: Uuid,
        timestamp: Instant,
    },
    /// Worker lifecycle state changed
    LifecycleStateChanged {
        worker_id: Uuid,
        old_state: WorkerLifecycleState,
        new_state: WorkerLifecycleState,
        timestamp: Instant,
    },
}

/// Job execution events
#[derive(Debug, Clone)]
pub enum JobEvent {
    /// Job started execution
    Started {
        worker_id: Uuid,
        job_id: Uuid,
        task_type: String,
        timestamp: Instant,
    },
    /// Job completed execution
    Completed {
        worker_id: Uuid,
        job_id: Uuid,
        result: JobCompletionResult,
        timestamp: Instant,
    },
    /// Job produced an output chunk
    OutputChunk {
        worker_id: Uuid,
        job_id: Uuid,
        chunk: OutputChunk,
        timestamp: Instant,
    },
}

/// AgentLoop-specific events
#[derive(Debug, Clone)]
pub enum AgentLoopEvent {
    /// AgentLoop received a complete response from the LLM
    ResponseReceived {
        worker_id: Uuid,
        job_id: Uuid,
        text: String,
        timestamp: Instant,
    },
    /// AgentLoop received a tool use request from the LLM
    ToolUseRequestReceived {
        worker_id: Uuid,
        job_id: Uuid,
        tool_name: String,
        tool_input: serde_json::Value,
        timestamp: Instant,
    },
}

/// System-level events
#[derive(Debug, Clone)]
pub enum SystemEvent {
    /// System shutdown was initiated
    ShutdownInitiated {
        reason: String,
        timestamp: Instant,
    },
}

/// WebUI-specific events
#[derive(Debug, Clone)]
pub enum WebUIEvent {
    /// User sent a prompt through WebUI
    PromptReceived {
        worker_id: Uuid,
        text: String,
        timestamp: Instant,
    },
    /// Web server started
    ServerStarted {
        address: String,
        timestamp: Instant,
    },
    /// Client connected to WebSocket
    WebSocketConnected {
        timestamp: Instant,
    },
}

/// Top-level event envelope
#[derive(Debug, Clone)]
pub enum AgentEnvironmentEvent {
    /// Worker-related event
    Worker(WorkerEvent),
    /// Job-related event
    Job(JobEvent),
    /// AgentLoop-specific event
    AgentLoop(AgentLoopEvent),
    /// System-level event
    System(SystemEvent),
    /// WebUI-specific event
    WebUI(WebUIEvent),
}

impl AgentEnvironmentEvent {
    /// Extract worker_id from any event that has one
    pub fn worker_id(&self) -> Option<Uuid> {
        match self {
            Self::Worker(WorkerEvent::Created { worker_id, .. }) => Some(*worker_id),
            Self::Worker(WorkerEvent::Deleted { worker_id, .. }) => Some(*worker_id),
            Self::Worker(WorkerEvent::LifecycleStateChanged { worker_id, .. }) => Some(*worker_id),
            Self::Job(JobEvent::Started { worker_id, .. }) => Some(*worker_id),
            Self::Job(JobEvent::Completed { worker_id, .. }) => Some(*worker_id),
            Self::Job(JobEvent::OutputChunk { worker_id, .. }) => Some(*worker_id),
            Self::AgentLoop(AgentLoopEvent::ResponseReceived { worker_id, .. }) => Some(*worker_id),
            Self::AgentLoop(AgentLoopEvent::ToolUseRequestReceived { worker_id, .. }) => Some(*worker_id),
            Self::WebUI(WebUIEvent::PromptReceived { worker_id, .. }) => Some(*worker_id),
            Self::WebUI(WebUIEvent::ServerStarted { .. }) => None,
            Self::WebUI(WebUIEvent::WebSocketConnected { .. }) => None,
            Self::System(_) => None,
        }
    }

    /// Check if this is a worker-related event
    pub fn is_worker_event(&self) -> bool {
        matches!(self, Self::Worker(_))
    }

    /// Check if this is a job-related event
    pub fn is_job_event(&self) -> bool {
        matches!(self, Self::Job(_))
    }

    /// Check if this is an agent loop event
    pub fn is_agent_loop_event(&self) -> bool {
        matches!(self, Self::AgentLoop(_))
    }

    /// Check if this is a system event
    pub fn is_system_event(&self) -> bool {
        matches!(self, Self::System(_))
    }

    /// Check if this is a WebUI event
    pub fn is_webui_event(&self) -> bool {
        matches!(self, Self::WebUI(_))
    }

    /// Get timestamp from any event
    pub fn timestamp(&self) -> Instant {
        match self {
            Self::Worker(WorkerEvent::Created { timestamp, .. }) => *timestamp,
            Self::Worker(WorkerEvent::Deleted { timestamp, .. }) => *timestamp,
            Self::Worker(WorkerEvent::LifecycleStateChanged { timestamp, .. }) => *timestamp,
            Self::Job(JobEvent::Started { timestamp, .. }) => *timestamp,
            Self::Job(JobEvent::Completed { timestamp, .. }) => *timestamp,
            Self::Job(JobEvent::OutputChunk { timestamp, .. }) => *timestamp,
            Self::AgentLoop(AgentLoopEvent::ResponseReceived { timestamp, .. }) => *timestamp,
            Self::AgentLoop(AgentLoopEvent::ToolUseRequestReceived { timestamp, .. }) => *timestamp,
            Self::System(SystemEvent::ShutdownInitiated { timestamp, .. }) => *timestamp,
            Self::WebUI(WebUIEvent::PromptReceived { timestamp, .. }) => *timestamp,
            Self::WebUI(WebUIEvent::ServerStarted { timestamp, .. }) => *timestamp,
            Self::WebUI(WebUIEvent::WebSocketConnected { timestamp, .. }) => *timestamp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_id_extraction() {
        let worker_id = Uuid::new_v4();
        let timestamp = Instant::now();

        let event = AgentEnvironmentEvent::Worker(WorkerEvent::Created {
            worker_id,
            name: "test".to_string(),
            timestamp,
        });
        assert_eq!(event.worker_id(), Some(worker_id));

        let event = AgentEnvironmentEvent::Job(JobEvent::Started {
            worker_id,
            job_id: Uuid::new_v4(),
            task_type: "test".to_string(),
            timestamp,
        });
        assert_eq!(event.worker_id(), Some(worker_id));

        let event = AgentEnvironmentEvent::System(SystemEvent::ShutdownInitiated {
            reason: "test".to_string(),
            timestamp,
        });
        assert_eq!(event.worker_id(), None);
    }

    #[test]
    fn test_event_type_checking() {
        let timestamp = Instant::now();

        let event = AgentEnvironmentEvent::Worker(WorkerEvent::Created {
            worker_id: Uuid::new_v4(),
            name: "test".to_string(),
            timestamp,
        });
        assert!(event.is_worker_event());
        assert!(!event.is_job_event());
        assert!(!event.is_agent_loop_event());
        assert!(!event.is_system_event());

        let event = AgentEnvironmentEvent::Job(JobEvent::Started {
            worker_id: Uuid::new_v4(),
            job_id: Uuid::new_v4(),
            task_type: "test".to_string(),
            timestamp,
        });
        assert!(!event.is_worker_event());
        assert!(event.is_job_event());

        let event = AgentEnvironmentEvent::System(SystemEvent::ShutdownInitiated {
            reason: "test".to_string(),
            timestamp,
        });
        assert!(event.is_system_event());
    }

    #[test]
    fn test_timestamp_extraction() {
        let timestamp = Instant::now();

        let event = AgentEnvironmentEvent::Worker(WorkerEvent::Created {
            worker_id: Uuid::new_v4(),
            name: "test".to_string(),
            timestamp,
        });
        assert_eq!(event.timestamp(), timestamp);

        let event = AgentEnvironmentEvent::Job(JobEvent::Completed {
            worker_id: Uuid::new_v4(),
            job_id: Uuid::new_v4(),
            result: JobCompletionResult::Success {
                task_metadata: std::collections::HashMap::new(),
                user_interaction_required: UserInteractionRequired::None,
            },
            timestamp,
        });
        assert_eq!(event.timestamp(), timestamp);
    }

    #[test]
    fn test_user_interaction_required_enum() {
        // Test enum variants can be constructed
        let none = UserInteractionRequired::None;
        let tool_approval = UserInteractionRequired::ToolApproval;

        // Test PartialEq implementation
        assert_eq!(none, UserInteractionRequired::None);
        assert_eq!(tool_approval, UserInteractionRequired::ToolApproval);
        assert_ne!(none, tool_approval);

        // Test Copy trait
        let none_copy = none;
        assert_eq!(none, none_copy);
    }
}
