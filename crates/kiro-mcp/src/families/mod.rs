//! Tool-family scaffolding for the bundled `kiro-mcp` shim.
//!
//! Each family lives under `families/<name>/` and splits into a `read/`
//! submodule (read-only tools — `Taskei___list_tasks`, `_get_task`, etc.)
//! and a `write/` submodule (state-mutating tools — `Taskei___create_task`,
//! `_update_task`). The split is *the* code-enforced
//! read-cannot-escalate-to-write boundary the bundled-shim design
//! depends on (plan §62–93, §270): with one process serving both, OS
//! isolation no longer applies, so the boundary moves into the type
//! system.
//!
//! How the boundary works:
//!
//! - `read/` modules take a [`crate::sts_bridge::ReadOnlyView`] (returned by
//!   [`crate::sts_bridge::StsBridge::read_only`]). The view exposes only
//!   `read_credentials_provider()`. There is no public path from `ReadOnlyView` to
//!   [`crate::sts_bridge::StsBridge::assume_write_once`], so a `read/` module *cannot* assume the
//!   write role even if it wanted to — the type doesn't have the method.
//! - `write/` modules take the full [`crate::sts_bridge::StsBridge`] and call `assume_write_once()`
//!   per invocation, dropping the AssumeRoleProvider after a single resolve.
//! - The `tests/compile_fail/` trybuild fixture asserts the boundary at build time: a
//!   `read/`-shaped fixture that imports `assume_write_once` produces a compile error. If a future
//!   refactor silently exposes the helper on `ReadOnlyView`, that test goes green and the boundary
//!   is broken — that's by design, the test is the canary.
//!
//! Phase 1c-bundle lands the scaffolding empty. Phase 1d fills in the
//! `rmcp::ServerHandler` impl on `families/taskei/mod.rs` and adds the
//! per-tool `readOnlyHint` annotations called for in plan §296–306.

pub mod taskei;
