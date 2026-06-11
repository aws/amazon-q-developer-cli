//! Extension trait that emits a `tracing::error!` line on `Err` and
//! returns the result unchanged so it composes with `?`.
//!
//! Use at fallible call sites whose errors would otherwise be visible
//! only at the `?` re-raise site (or, worse, only on stderr after an
//! end-of-process formatter). The chain is logged via `Debug` so
//! `eyre::Report`s render their full cause chain.
//!
//! ```ignore
//! use crate::util::log_on_err::LogOnErr;
//!
//! let db = Database::new()
//!     .await
//!     .log_on_err("Os::new: Database::new failed")?;
//! ```

use std::fmt::Debug;

/// Logs an `error!` line with the given context if the `Result` is `Err`,
/// then returns the `Result` unchanged. Designed to chain in front of `?`.
pub trait LogOnErr<T, E> {
    /// Log on `Err` with a static context string and pass through.
    fn log_on_err(self, context: &'static str) -> Result<T, E>;
}

impl<T, E: Debug> LogOnErr<T, E> for Result<T, E> {
    fn log_on_err(self, context: &'static str) -> Result<T, E> {
        if let Err(ref e) = self {
            tracing::error!(error = ?e, "{context}");
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_is_passed_through_unchanged() {
        let r: Result<i32, &'static str> = Ok(42);
        assert_eq!(r.log_on_err("never logged").unwrap(), 42);
    }

    #[test]
    fn err_is_passed_through_unchanged() {
        let r: Result<i32, &'static str> = Err("boom");
        assert_eq!(r.log_on_err("context").unwrap_err(), "boom");
    }

    #[test]
    fn composes_with_question_mark() {
        fn inner() -> Result<i32, &'static str> {
            let v: i32 = Err::<i32, _>("boom").log_on_err("inner: failed")?;
            Ok(v)
        }
        assert_eq!(inner().unwrap_err(), "boom");
    }
}
