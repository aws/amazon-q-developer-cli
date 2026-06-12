#![cfg(not(test))]
// TODO: fix all string_slice violations in V1 and remove this allow
#![allow(clippy::string_slice)]
// The CLI's `execute()` async state machine is large enough that its layout
// computation exceeds rustc's default query depth (128). Raise the limit so
// the future's type can be laid out.
#![recursion_limit = "256"]
//! This lib.rs is only here for testing purposes.
//! `test_mcp_server/test_server.rs` is declared as a separate binary and would need a way to
//! reference types defined inside of this crate, hence the export.
pub mod agent;
pub mod api_client;
pub mod auth;
pub mod aws_common;
pub mod cleanup;
pub mod cli;
pub mod constants;
pub mod database;
pub mod embedded_tui;
pub mod feature_flags;
pub mod launch;
pub mod logging;
pub mod mcp_client;
pub mod mcp_registry;
pub mod os;
pub mod request;
pub mod rollout;
pub mod telemetry;
pub mod theme;
pub mod util;

pub use mcp_client::*;
