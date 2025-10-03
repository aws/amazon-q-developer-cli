#![cfg(test)]

/// Enables tracing to stderr. Useful when debugging a particular test.
#[allow(dead_code)]
pub fn enable_tracing() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_ansi(false)
        .with_writer(std::io::stderr)
        .try_init()
        .ok();
}