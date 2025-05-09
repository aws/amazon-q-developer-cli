//! This lib.rs is only here for testing purposes.
//! `test_mcp_server/test_server.rs` is declared as a separate binary and would need a way to
//! reference types defined inside of this crate, hence the export.
pub mod mcp_client;

pub use mcp_client::*;
