pub const CLI_BINARY_NAME: &str = "q";
pub const PRODUCT_NAME: &str = "Amazon Q";

/// User agent override
pub const USER_AGENT_ENV_VAR: &str = "AWS_EXECUTION_ENV";
// Constants for setting the user agent in HTTP requests
pub const USER_AGENT_APP_NAME: &str = "AmazonQ-For-CLI";
pub const USER_AGENT_VERSION_KEY: &str = "Version";
pub const USER_AGENT_VERSION_VALUE: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    /// `USER_AGENT_VERSION_VALUE` is forwarded to spawned `aws` CLI subprocesses
    /// via `AWS_EXECUTION_ENV`. The same `env!("CARGO_PKG_VERSION")` is also
    /// used for the ACP `Implementation.version` field in `kiro-bot`. The
    /// `0.0.0-dev` placeholder must never leak into built binaries —
    /// `crates/agent/build.rs::inject_kiro_version()` overrides it.
    /// Don't remove this test without removing that override.
    #[test]
    fn cli_version_is_not_dev_placeholder() {
        assert_ne!(
            super::USER_AGENT_VERSION_VALUE, "0.0.0-dev",
            "build.rs failed to inject a real version — outbound services will see the placeholder"
        );
    }
}

pub mod env_var {
    macro_rules! define_env_vars {
        ($($(#[$meta:meta])* $ident:ident = $name:expr),*) => {
            $(
                $(#[$meta])*
                pub const $ident: &str = $name;
            )*

            pub const ALL: &[&str] = &[$($ident),*];
        }
    }

    define_env_vars! {
        /// Path to the data directory
        ///
        /// Overrides the default data directory location
        CLI_DATA_DIR = "Q_CLI_DATA_DIR",

        /// Flag for running integration tests
        CLI_IS_INTEG_TEST = "Q_CLI_IS_INTEG_TEST",

        /// The session ID of the current Kiro chat session.
        ///
        /// Set on the process environment at session creation so that all child
        /// processes (shell commands, hooks, MCP servers, AWS CLI, etc.) can
        /// detect they are running inside a Kiro agent context and correlate
        /// activity back to a specific session for telemetry purposes.
        KIRO_SESSION_ID = "KIRO_SESSION_ID"
    }

    /// Publish `KIRO_SESSION_ID` into the process environment so that child
    /// processes inherit it.
    ///
    /// Called when the interactive session changes (initial creation, `/chat new`,
    /// `/chat load`). Subagents and delegate child processes inherit the value
    /// from their parent and should not call this.
    ///
    /// # Safety
    ///
    /// `std::env::set_var` is unsafe in Rust 2024 because `setenv(3)` is not
    /// thread-safe on POSIX: it may reallocate the `environ` array while a
    /// concurrent `getenv` reads from it (use-after-free). In practice this is
    /// called early in session init before most background work begins, and the
    /// risk is the same as any other `set_var` call in the ecosystem. We accept
    /// it here rather than threading a session ID through every spawn-point
    /// signature in the codebase.
    pub fn publish_session_id(session_id: &str) {
        // SAFETY: see above.
        unsafe {
            std::env::set_var(KIRO_SESSION_ID, session_id);
        }
    }
}
