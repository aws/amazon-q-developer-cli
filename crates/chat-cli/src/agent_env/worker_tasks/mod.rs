pub mod agent_loop;

pub use agent_loop::{AgentLoop, AgentLoopInput};

// Stub for Phase 10
#[derive(Debug, Clone)]
pub struct CompactInput {
    pub instruction: Option<String>,
}
