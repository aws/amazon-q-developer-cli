// Core modules
pub mod events;
pub mod event_bus;
pub mod commands;
pub mod agent_environment;
pub mod worker_job_continuations;
pub mod model_providers;
pub mod context_container;
pub mod context_builder;
pub mod worker_builder;
pub mod worker;
pub mod worker_task;
pub mod worker_job;
pub mod session;

// Task implementations
pub mod worker_tasks;

// Re-exports for convenience
pub use events::*;
pub use event_bus::EventBus;
pub use commands::*;
pub use agent_environment::{AgentEnvironment, UserInterface, HeadlessInterface};
pub use worker_job_continuations::{Continuations, WorkerJobCompletionType};
pub use model_providers::{
    ModelProvider, ModelRequest, ModelResponse, ModelResponseChunk,
};
pub use context_container::{ContextContainer, ConversationHistory, ConversationEntry};
pub use context_builder::ContextBuilder;
pub use worker_builder::WorkerBuilder;
pub use worker::{Worker, WorkerStates};
pub use worker_task::WorkerTask;
pub use session::Session;
