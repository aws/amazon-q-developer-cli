#![cfg(not(test))]
//! This lib.rs is only here for testing purposes.
//! `test_mcp_server/test_server.rs` is declared as a separate binary and would need a way to
//! reference types defined inside of this crate, hence the export.
pub mod api_client;
pub mod auth;
pub mod aws_common;
pub mod cli;
pub mod constants;
pub mod database;
pub mod logging;
pub mod mcp_client;
pub mod mock_llm;
pub mod os;
pub mod request;
pub mod telemetry;
pub mod util;

pub use mcp_client::*;

mod test_util;
