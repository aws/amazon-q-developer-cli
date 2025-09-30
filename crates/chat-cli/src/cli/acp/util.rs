//! Utilities for ACP actor implementation

/// Ignore errors from a Result, useful for oneshot sends where receiver may be dropped
pub fn ignore_error<E>(result: Result<(), E>) {
    let _ = result;
}

/// Enables tracing to stderr. Useful when debugging a particular test.
#[cfg(test)]
#[allow(dead_code)]
pub fn enable_tracing() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(std::io::stderr)
        .try_init()
        .ok();
}