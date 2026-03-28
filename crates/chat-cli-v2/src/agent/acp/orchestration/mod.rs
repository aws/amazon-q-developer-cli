//! Agent-to-agent orchestration infrastructure.
//!
//! Provides inbox-based messaging between sessions, session naming,
//! auto-wake logic, and permission enforcement.

pub mod inbox;
pub mod naming;
pub mod permissions;
pub mod types;
