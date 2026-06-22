pub const CLI_BINARY_NAME: &str = "kiro-cli";
pub const CHAT_BINARY_NAME: &str = "kiro-cli-chat";

pub const MCP_SERVER_TOOL_DELIMITER: &str = "/";
pub const BUILTIN_TOOLS_PREFIX: &str = "@builtin";

/// Build time env vars
pub mod build {
    /// A git full sha hash of the current build
    pub const HASH: Option<&str> = option_env!("AMAZON_Q_BUILD_HASH");

    /// The datetime in rfc3339 format of the current build
    pub const DATETIME: Option<&str> = option_env!("AMAZON_Q_BUILD_DATETIME");
}

pub const US_GOV_EAST: &str = "us-gov-east-1";
pub const US_GOV_WEST: &str = "us-gov-west-1";
pub const US_ISO_DCA: &str = "us-iso-east-1";
pub const US_ISO_LCK: &str = "us-isob-east-1";
pub const US_ISO_ALE: &str = "us-isof-south-1";
pub const US_ISO_LTW: &str = "us-isof-east-1";

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
        /// The UUID of the current parent qterm instance
        QTERM_SESSION_ID = "QTERM_SESSION_ID",

        /// The current parent socket to connect to
        Q_PARENT = "Q_PARENT",
        KIRO_PARENT = "KIRO_PARENT",

        /// Set the parent socket to connect to
        Q_SET_PARENT = "Q_SET_PARENT",
        KIRO_SET_PARENT = "KIRO_SET_PARENT",

        /// Guard for the set parent check
        Q_SET_PARENT_CHECK = "Q_SET_PARENT_CHECK",
        KIRO_SET_PARENT_CHECK = "KIRO_SET_PARENT_CHECK",

        /// Set if qterm is running, contains the version
        Q_TERM = "Q_TERM",
        KIRO_TERM = "KIRO_TERM",

        /// Sets the current log level
        Q_LOG_LEVEL = "Q_LOG_LEVEL",
        KIRO_LOG_LEVEL = "KIRO_LOG_LEVEL",

        /// Overrides the ZDOTDIR environment variable
        Q_ZDOTDIR = "Q_ZDOTDIR",
        KIRO_ZDOTDIR = "KIRO_ZDOTDIR",

        /// Indicates a process was launched by Kiro
        PROCESS_LAUNCHED_BY_Q = "PROCESS_LAUNCHED_BY_Q",
        PROCESS_LAUNCHED_BY_KIRO = "PROCESS_LAUNCHED_BY_KIRO",

        /// The shell to use in qterm
        Q_SHELL = "Q_SHELL",
        KIRO_SHELL = "KIRO_SHELL",

        /// Indicates the user is debugging the shell
        Q_DEBUG_SHELL = "Q_DEBUG_SHELL",
        KIRO_DEBUG_SHELL = "KIRO_DEBUG_SHELL",

        /// Indicates the user is using zsh autosuggestions which disables Inline
        Q_USING_ZSH_AUTOSUGGESTIONS = "Q_USING_ZSH_AUTOSUGGESTIONS",
        KIRO_USING_ZSH_AUTOSUGGESTIONS = "KIRO_USING_ZSH_AUTOSUGGESTIONS",

        /// Overrides the path to the bundle metadata released with certain desktop builds.
        Q_BUNDLE_METADATA_PATH = "Q_BUNDLE_METADATA_PATH",
        KIRO_BUNDLE_METADATA_PATH = "KIRO_BUNDLE_METADATA_PATH",

        /// Identifier for the client application or service using the chat-cli
        Q_CLI_CLIENT_APPLICATION = "Q_CLI_CLIENT_APPLICATION",
        KIRO_CLI_CLIENT_APPLICATION = "KIRO_CLI_CLIENT_APPLICATION",

        /// Shows continuation IDs in chat output for debugging/development
        Q_SHOW_CONTINUATION_IDS = "Q_SHOW_CONTINUATION_IDS",
        KIRO_SHOW_CONTINUATION_IDS = "KIRO_SHOW_CONTINUATION_IDS",

        /// Mock chat response for testing
        Q_MOCK_CHAT_RESPONSE = "Q_MOCK_CHAT_RESPONSE",
        KIRO_MOCK_CHAT_RESPONSE = "KIRO_MOCK_CHAT_RESPONSE",

        /// Disable truecolor output
        Q_DISABLE_TRUECOLOR = "Q_DISABLE_TRUECOLOR",
        KIRO_DISABLE_TRUECOLOR = "KIRO_DISABLE_TRUECOLOR",

        /// Log to stdout
        Q_LOG_STDOUT = "Q_LOG_STDOUT",
        KIRO_LOG_STDOUT = "KIRO_LOG_STDOUT",

        /// Disable colored log output
        KIRO_LOG_NO_COLOR = "KIRO_LOG_NO_COLOR",

        /// Log file path override
        KIRO_CHAT_LOG_FILE = "KIRO_CHAT_LOG_FILE",

        /// Disable telemetry
        Q_DISABLE_TELEMETRY = "Q_DISABLE_TELEMETRY",
        KIRO_DISABLE_TELEMETRY = "KIRO_DISABLE_TELEMETRY",

        /// Fake remote environment
        Q_FAKE_IS_REMOTE = "Q_FAKE_IS_REMOTE",
        KIRO_FAKE_IS_REMOTE = "KIRO_FAKE_IS_REMOTE",

        /// Codespaces environment
        Q_CODESPACES = "Q_CODESPACES",
        KIRO_CODESPACES = "KIRO_CODESPACES",

        /// CI environment
        Q_CI = "Q_CI",
        KIRO_CI = "KIRO_CI",

        /// Telemetry client ID
        Q_TELEMETRY_CLIENT_ID = "Q_TELEMETRY_CLIENT_ID",
        KIRO_TELEMETRY_CLIENT_ID = "KIRO_TELEMETRY_CLIENT_ID",

        /// OpenTelemetry telemetry mode: 0/off, 1/dual-write, or 2/OTel-only
        KIRO_TELEMETRY_OTEL = "KIRO_TELEMETRY_OTEL",

        /// OpenTelemetry OTLP endpoint override for team-owned ADOT or KUTS dogfood
        KIRO_TELEMETRY_OTLP_ENDPOINT = "KIRO_TELEMETRY_OTLP_ENDPOINT",

        /// OpenTelemetry export interval override for local development
        KIRO_TELEMETRY_EXPORT_INTERVAL_MS = "KIRO_TELEMETRY_EXPORT_INTERVAL_MS",

        /// OpenTelemetry logs export switch for KUTS metrics-only testing
        KIRO_TELEMETRY_OTLP_LOGS_ENABLED = "KIRO_TELEMETRY_OTLP_LOGS_ENABLED",

        /// Chat shell for Unix systems
        KIRO_CHAT_SHELL = "KIRO_CHAT_SHELL",

        /// Flag for running integration tests
        CLI_IS_INTEG_TEST = "Q_CLI_IS_INTEG_TEST",

        /// Amazon Q chat shell
        AMAZON_Q_CHAT_SHELL = "AMAZON_Q_CHAT_SHELL",

        /// Editor environment variable
        EDITOR = "EDITOR",

        /// Visual editor environment variable (preferred over EDITOR)
        VISUAL = "VISUAL",

        /// Terminal type
        TERM = "TERM",

        /// AWS region
        AWS_REGION = "AWS_REGION",

        /// GitHub Codespaces environment
        CODESPACES = "CODESPACES",

        /// CI environment
        CI = "CI",

        /// Chat UI mode override: "legacy" or "tui"
        KIRO_CHAT_UI = "KIRO_CHAT_UI",

        /// Path to the Node.js binary used to run the KAS agent engine.
        /// Set in two scenarios:
        ///   * The launcher always sets this on the TUI child to forward
        ///     the resolved node binary (extracted embedded node, or a
        ///     user-supplied override).
        ///   * Users can set it directly to override the embedded Node
        ///     runtime that ships with release builds and point KAS at a
        ///     specific Node.js install. The chosen Node must be
        ///     compatible with the bundled KAS server (see `NODE_VERSION`
        ///     in `scripts/const.py`).
        ///
        /// Read by the TUI's KAS spawn (`acp-client.ts`) and by
        /// `chat_cli`'s `resolve_kas_paths` for KAS-side subcommands.
        KIRO_KAS_NODE_PATH = "KIRO_KAS_NODE_PATH",

        /// Path to the chat_cli (`kiro-cli-chat`) binary. Set by the kiro-cli
        /// launcher when spawning the TUI. Consumed by:
        ///   * the TUI's V1/V2 ACP child spawn (`packages/tui/src/index.tsx`)
        ///   * shell-outs that need to invoke chat_cli (auth callback,
        ///     session listing, etc. - see `packages/tui/src/utils/chat-cli-bin.ts`)
        KIRO_CHAT_CLI_BIN = "KIRO_CHAT_CLI_BIN",

        /// Overrides the version reported by the TUI bundle. Set by the
        /// kiro-cli launcher when spawning the TUI so the JS side
        /// (`getCliVersion()` in `packages/tui/src/utils/version.ts`) reports
        /// the binary's real compile-time version instead of the bundled
        /// `0.0.0-dev` placeholder / `99.99.99-dev` dev fallback. Users can
        /// also set it directly to test version-gated features.
        KIRO_VERSION_OVERRIDE = "KIRO_VERSION_OVERRIDE",

        /// Path to the KAS ACP server JS entrypoint (`acp-server.js`).
        /// Set in two scenarios:
        ///   * The launcher always sets this on the TUI child to forward
        ///     the resolved server path (extracted embedded server.js, or
        ///     a user-supplied override).
        ///   * Users can set it directly to point KAS at a checkout of
        ///     the `kiro-agent` repo, taking precedence over the bundled
        ///     server extracted from the binary.
        ///
        /// Read by the TUI's KAS spawn (`acp-client.ts`) and by
        /// `chat_cli`'s `resolve_kas_paths` for KAS-side subcommands.
        KIRO_KAS_SERVER_PATH = "KIRO_KAS_SERVER_PATH",

        /// Used for E2E tests
        KIRO_TEST_TUI_JS_PATH = "KIRO_TEST_TUI_JS_PATH",

        /// Test-only injection of KAS-side session entries for the
        /// merged listing (`chat --list-sessions`). When set, the value
        /// MUST parse as a JSON array of `SessionInfoEntry` and is used
        /// in place of spawning a real KAS child. Malformed values panic
        /// so the test setup failure surfaces loudly. Production callers
        /// MUST NOT set this.
        KIRO_TEST_MOCK_KAS_SESSIONS = "KIRO_TEST_MOCK_KAS_SESSIONS",

        /// Overrides the SQLite database path for the V1 (classic)
        /// conversation store. Tests point this at a sandbox file so
        /// the binary reads/writes V1 conversations there instead of
        /// the developer's real `data.sqlite3`. Mirrors the env var
        /// honored in `chat-cli-v2`.
        KIRO_TEST_DB_PATH = "KIRO_TEST_DB_PATH",

        /// Overrides the directory used for user-level Kiro config data.
        /// When set, this replaces `$HOME/.kiro` as the root for global
        /// Kiro paths (agents, prompts, settings, steering, sessions, etc.).
        KIRO_HOME = "KIRO_HOME",

        /// Overrides the data directory for runtime assets (bun, tui.js,
        /// node, feed.json). Used by enterprise IT to redirect extracted
        /// executables to AppLocker-whitelisted paths.
        KIRO_DATA_DIR = "KIRO_DATA_DIR",

        /// API key for headless/non-interactive authentication
        KIRO_API_KEY = "KIRO_API_KEY",

        /// Disable automatic update check on startup
        KIRO_NO_AUTO_UPDATE = "KIRO_NO_AUTO_UPDATE",

        /// Override the update release URL at runtime
        KIRO_DESKTOP_RELEASE_URL = "KIRO_DESKTOP_RELEASE_URL",

        /// Comma-separated MCP server names (matching entries in mcp.json) that must
        /// always be loaded and their tools always available, regardless of agent profile.
        ASBX_KIRO_MANDATORY_MCPS = "ASBX_KIRO_MANDATORY_MCPS",

        /// Text displayed to the user at session start (display-only, never reaches model).
        ASBX_KIRO_TERMINAL_BANNER = "ASBX_KIRO_TERMINAL_BANNER",

        /// The session ID of the current Kiro chat session.
        ///
        /// Set on the process environment at session creation so that all child
        /// processes can detect they are running inside a Kiro agent context and
        /// correlate activity back to a specific session for telemetry purposes.
        KIRO_SESSION_ID = "KIRO_SESSION_ID"
    }

    /// Default update manifest URL. Set this to the production URL when ready.
    // TODO: Update this to the production manifest URL once the update infrastructure is deployed.
    pub const DEFAULT_UPDATE_MANIFEST_URL: &str =
        "https://desktop-release.q.us-east-1.amazonaws.com/latest/manifest.json";
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;
    use time::format_description::well_known::Rfc3339;

    use super::*;

    #[test]
    fn test_build_envs() {
        if let Some(build_hash) = build::HASH {
            println!("build_hash: {build_hash}");
            assert!(!build_hash.is_empty());
        }

        if let Some(build_datetime) = build::DATETIME {
            println!("build_datetime: {build_datetime}");
            println!("{}", OffsetDateTime::parse(build_datetime, &Rfc3339).unwrap());
        }
    }
}
