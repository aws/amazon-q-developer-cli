//! KAS session archive utilities.
//!
//! Wrap KAS session directories (`~/.kiro/sessions/{workspaceHash}/{sessionId}/`)
//! in zip archives for export, and extract them back into the local
//! sessions directory on import. This module only reads and writes
//! files; wiring imported sessions into a running agent (via ACP
//! `session/load`) is the caller's responsibility.

mod export;
mod import;
mod shared;
#[cfg(test)]
mod test_support;
pub(crate) mod workspace_hash;
mod zip_util;

// `pub use` is part of the public crate API, but the chat_cli_v2
// BINARY (a separate compilation unit from the library) does not
// consume these re-exports - only external crates like `chat_cli` do.
// Without `#[allow]`, the binary build flags the re-exports as
// unused. Suppress on the binary target's view; the items stay
// reachable via `chat_cli_v2::agent::kas::*` for library consumers.
#[allow(unused_imports)]
pub use export::{
    ExportSessionOptions,
    export_session,
};
#[allow(unused_imports)]
pub use import::{
    ImportSessionOptions,
    import_session,
};
#[allow(unused_imports)]
pub use shared::{
    SessionArchiveError,
    default_kas_sessions_root,
};
