//! Taskei tool family — exposes `Taskei___*` tools to upstream MCP
//! clients. Phase 1c-bundle ships the module skeleton plus the read /
//! write split that carries the credential boundary; Phase 1d wires
//! the rmcp `ServerHandler` impl + per-tool `readOnlyHint` annotations
//! + startup schema-pin assertion (plan §288–337).
//!
//! Submodules:
//!
//! - [`read`] — Phase 1d will house `Taskei___list_tasks`, `_get_task`, `_get_room`,
//!   `_list_room_resource`. These take a [`crate::sts_bridge::ReadOnlyView`] and never see
//!   `assume_write_once`.
//! - [`write`] — Phase 1d will house `Taskei___create_task` and `_update_task` (the latter also
//!   handles `addComment`). These take the full [`crate::sts_bridge::StsBridge`] so they can call
//!   `assume_write_once()` per invocation.

pub mod read;
pub mod write;
