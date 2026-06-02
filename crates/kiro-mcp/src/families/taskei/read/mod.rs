//! Read-only Taskei tools. Phase 1c-bundle scaffolding — Phase 1d will
//! fill in `Taskei___list_tasks`, `_get_task`, `_get_room`,
//! `_list_room_resource`, each annotated `readOnlyHint: true`.
//!
//! **Credential boundary:** this module imports
//! [`crate::sts_bridge::ReadOnlyView`] and *never*
//! [`crate::sts_bridge::StsBridge`] directly. `ReadOnlyView` lacks the
//! `assume_write_once` method, so by construction nothing here can
//! escalate to the write role. A trybuild compile-fail fixture under
//! `tests/compile_fail/read_cannot_assume_write.rs` asserts this at
//! build time — see `crates/kiro-mcp/tests/compile_fail.rs`.
//!
//! Per-call wiring (Phase 1d, sketched here):
//!
//! ```ignore
//! use crate::sigv4_client::SigV4HttpClient;
//! use crate::sts_bridge::ReadOnlyView;
//!
//! pub async fn list_tasks(view: &ReadOnlyView, region: &str) {
//!     let provider = view.read_credentials_provider().expect("provider");
//!     let _client = SigV4HttpClient::with_provider(provider, region.to_string());
//!     // ... rmcp call_tool dispatch ...
//! }
//! ```

#[allow(unused_imports)]
use crate::sts_bridge::ReadOnlyView;
