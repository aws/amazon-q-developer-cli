//! Internal helpers for the `kiro-taskei-mcp` binary. Exposed as a `lib`
//! target so integration tests under `tests/` can drive the SigV4 client
//! against a localhost mock without re-implementing it.

pub mod sigv4_client;
