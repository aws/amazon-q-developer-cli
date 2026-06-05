//! Bundled MCP stdio shim for kiro-bot.
//!
//! Phase 1d wires up rmcp's `ServerHandler` so kiro-help can dispatch
//! Taskei tools over a single bundled process. Phase 1c-bundle laid down
//! the `families/` scaffolding the handler hangs off of; Phase 1d fills
//! it in with a SigV4-signed JSON-RPC proxy to the IAD Taskei gateway,
//! per-tool `readOnlyHint` annotations, and a startup `tools/list`
//! schema-pin assertion that fails closed on gateway drift.
//!
//! Module layout:
//!
//! - [`sigv4_client`] — Phase 1b's SigV4-signing HTTP client.
//! - [`sts_bridge`] — Phase 1c's STS AssumeRole bridge (cached read role, per-call write role,
//!   no-op when both ARNs are unset).
//! - [`families`] — tool-family scaffolding. Currently houses `taskei` only; knowledge / github
//!   migrate in via a follow-up consolidation plan (§5 open question 6). Each family splits into
//!   `read/` and `write/` submodules: `read/` modules hold a [`sts_bridge::ReadOnlyView`] and
//!   cannot import `sts_bridge::StsBridge::assume_write_once`. The compile-fail trybuild fixture
//!   under `tests/compile_fail/` asserts that boundary.
//! - [`cli`] — the top-level CLI entry point shared by both `[[bin]]` targets (canonical `kiro-mcp`
//!   and the deprecated `kiro-taskei-mcp` alias).
//! - [`dumper`] — dev/operator probe used to refresh the Taskei `tools/list` schema-pin fixture
//!   from the live gateway. Not part of the runtime path.

pub mod cli;
pub mod dumper;
pub mod families;
pub mod sigv4_client;
pub mod sts_bridge;
