#![cfg(test)]

//! Utilities for ACP actor implementation

/// Ignore errors from a Result, useful for oneshot sends where receiver may be dropped
pub fn ignore_error<E>(result: Result<(), E>) {
    let _ = result;
}
