//! State-mutating Taskei tools. Phase 1c-bundle scaffolding — Phase 1d
//! will fill in `Taskei___create_task` and `_update_task` (the latter
//! also serves `addComment`), each annotated `readOnlyHint: false`.
//!
//! **Credential boundary:** this module is the *only* place inside
//! `families/taskei/` that imports [`crate::sts_bridge::StsBridge`]
//! directly. Per-call write semantics are achieved by calling
//! [`crate::sts_bridge::StsBridge::assume_write_once`] inside each
//! tool dispatch and dropping the returned
//! [`crate::sts_bridge::WriteCredentials`] before the next call. The
//! `read/` submodule cannot assume the write role because it holds a
//! [`crate::sts_bridge::ReadOnlyView`] which lacks the method (see
//! sibling read/mod.rs and `tests/compile_fail.rs`).
//!
//! Per-call wiring (Phase 1d, sketched here):
//!
//! ```ignore
//! use crate::sigv4_client::SigV4HttpClient;
//! use crate::sts_bridge::{StsBridge, one_shot_provider};
//!
//! pub async fn create_task(bridge: &StsBridge, region: &str) {
//!     let wc = bridge.assume_write_once().await.expect("assume write");
//!     let _client = SigV4HttpClient::with_provider(
//!         one_shot_provider(wc.creds),
//!         region.to_string(),
//!     );
//!     // ... rmcp call_tool dispatch — single signed call, then return ...
//! }
//! ```

#[allow(unused_imports)]
use crate::sts_bridge::{
    StsBridge,
    WriteCredentials,
    one_shot_provider,
};
