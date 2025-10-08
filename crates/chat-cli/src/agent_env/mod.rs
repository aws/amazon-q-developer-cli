// Core modules
pub mod worker_job_continuations;
pub mod model_providers;
pub mod context_container;
pub mod worker;
pub mod worker_task;
pub mod worker_job;
pub mod worker_interface;
pub mod session;

// Task implementations
pub mod worker_tasks;

// Demo module
pub mod demo;

// Re-exports for convenience
pub use worker_job_continuations::{Continuations, WorkerJobCompletionType};
pub use model_providers::{
    ModelProvider, ModelRequest, ModelResponse, ModelResponseChunk,
};
pub use context_container::{ContextContainer, ConversationHistory, ConversationEntry};
pub use worker::{Worker, WorkerStates};
pub use worker_task::WorkerTask;
pub use worker_interface::WorkerToHostInterface;
pub use session::Session;
