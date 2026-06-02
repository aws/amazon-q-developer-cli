//! Bundled MCP stdio shim for kiro-bot.
//!
//! Phase 1c-bundle landed the rename from `kiro-taskei-mcp` to `kiro-mcp`
//! plus the `families/` module scaffolding called for in plan §2 / §317. The
//! library target stays exposed (now as `kiro_mcp`) so integration tests
//! under `tests/` can drive the SigV4 client + STS bridge against
//! localhost mocks without re-implementing them.
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
//!   under `tests/compile_fail/` asserts that boundary; see [`families`] docs for the rationale.

pub mod cli;
pub mod families;
pub mod sigv4_client;
pub mod sts_bridge;
