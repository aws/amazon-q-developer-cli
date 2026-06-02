//! trybuild fixture for `crates/kiro-mcp/tests/compile_fail.rs`.
//!
//! Pretends to be a read-side family module: it has a `ReadOnlyView`
//! and tries to call `assume_write_once` on it. The boundary
//! guarantees this is a compile error — `ReadOnlyView` is the type
//! `families/<x>/read/` modules will take in Phase 1d, and it
//! deliberately lacks the write helper. If this fixture starts
//! compiling, the type-system half of the read/write boundary has
//! regressed.

use kiro_mcp::sts_bridge::ReadOnlyView;

async fn forbidden(view: &ReadOnlyView) {
    // This MUST fail to compile. `assume_write_once` is a method on
    // `StsBridge`, not on `ReadOnlyView`.
    let _ = view.assume_write_once().await;
}

fn main() {}
