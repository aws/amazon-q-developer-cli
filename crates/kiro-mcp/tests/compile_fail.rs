//! Compile-fail boundary check for the bundled-shim read/write split.
//!
//! What this asserts:
//!
//! Phase 1c-bundle's `families/<x>/read/` modules take a
//! [`kiro_mcp::sts_bridge::ReadOnlyView`] and must NOT be able to
//! invoke [`kiro_mcp::sts_bridge::StsBridge::assume_write_once`]. The
//! view exposes only `read_credentials_provider()`; the write helper
//! is a method on the full `StsBridge` only. The boundary keeps the
//! "read tool can't escalate to write" guarantee that the
//! single-process bundled shim relies on (plan §62–93, §270, §550).
//!
//! How it asserts:
//!
//! Each fixture under `tests/compile_fail/*.rs` deliberately tries
//! something the boundary should forbid. trybuild compiles each
//! fixture and asserts the compiler emits the expected error. If a
//! refactor silently re-exposes the write helper on `ReadOnlyView`,
//! the fixture compiles, the test fails, and CI catches the
//! regression.
//!
//! Why a runtime test isn't enough: at runtime nobody has a
//! `ReadOnlyView` *and* a path to `assume_write_once` to call by
//! mistake. The boundary is a *type-system* claim — proving it
//! requires a compile-time test. Phase 1c shipped this guarantee as
//! an API-shape promise (the view doesn't have the method); 1c-bundle
//! turns it into a build-gated check that survives future edits to
//! the bridge and the family modules.

#[test]
fn read_only_view_cannot_assume_write_role() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/read_view_calls_assume_write.rs");
}
