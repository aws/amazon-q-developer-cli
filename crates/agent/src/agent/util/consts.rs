pub const CLI_BINARY_NAME: &str = "q";
pub const PRODUCT_NAME: &str = "Amazon Q";

/// User agent override
pub const USER_AGENT_ENV_VAR: &str = "AWS_EXECUTION_ENV";
// Constants for setting the user agent in HTTP requests
pub const USER_AGENT_APP_NAME: &str = "AmazonQ-For-CLI";
pub const USER_AGENT_VERSION_KEY: &str = "Version";
pub const USER_AGENT_VERSION_VALUE: &str = env!("CARGO_PKG_VERSION");

/// User-agent token label prefixing the ACP client name in `AWS_EXECUTION_ENV`.
pub const ACP_CLIENT_UA_TOKEN: &str = "acp-client";

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
        KIRO_SESSION_ID = "KIRO_SESSION_ID",

        /// Env var carrying the name of the driving ACP client (e.g. "MeshClaw").
        ///
        /// Stamped once at ACP `Initialize` and read only by the AWS CLI env builders
        /// so spawned `aws` subprocesses can attribute calls to a specific client.
        /// This is intentionally decoupled from `KIRO_CLI_CLIENT_APPLICATION`, telemetry,
        /// and the SDK user-agent interceptor.
        ///
        /// It is set on the *real* process environment (`set_var`), so like
        /// `KIRO_SESSION_ID` it is inherited by ALL child processes spawned for the
        /// remainder of the process lifetime, not just `aws` invocations. The write is
        /// process-global and last-writer-wins, so this assumes a single driving ACP
        /// client per process; a second `Initialize` with a different name overwrites
        /// the first.
        ///
        /// Note: this is distinct from the value-constant
        /// `kiro_telemetry_observer::context::KIRO_ACP_CLIENT_NAME` (= `"kiro-tui"`),
        /// which is a fixed client *name*, not this env-var *key*.
        ACP_CLIENT_NAME_ENV_VAR = "KIRO_CLI_ACP_CLIENT_NAME"
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
