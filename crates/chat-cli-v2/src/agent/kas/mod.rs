//! KAS session archive utilities.
//!
//! Wrap KAS session directories (`~/.kiro/sessions/{workspaceHash}/{sessionId}/`)
//! in zip archives for export, and extract them back into the local
//! sessions directory on import. This module only reads and writes
//! files; wiring imported sessions into a running agent (via ACP
//! `session/load`) is the caller's responsibility.

mod export;
pub mod file_detection;
mod import;
pub mod persist;
pub mod schema;
pub mod session_id;
mod shared;
#[cfg(test)]
mod test_support;
pub mod v2_to_kas;
pub mod workspace_hash;
mod zip_util;

// The chat_cli_v2 BINARY is a separate compilation unit from the
// library, and does not consume these `pub use` re-exports - only
// external crates like `chat_cli` do. Without `#[allow]` the binary
// flags the re-exports as unused. Suppress at module scope; the
// items stay reachable via `chat_cli_v2::agent::kas::*` for library
// consumers.
#[allow(unused_imports)]
pub use export::{
    ExportSessionOptions,
    export_session,
};
#[allow(unused_imports)]
pub use import::{
    ImportResult,
    ImportSessionOptions,
    ImportTarget,
    import_session,
};
#[allow(unused_imports)]
pub use persist::{
    WriteKasSessionError,
    WriteKasSessionOptions,
    write_kas_session_dir,
};
#[allow(unused_imports)]
pub use shared::{
    SessionArchiveError,
    default_kas_sessions_root,
};
