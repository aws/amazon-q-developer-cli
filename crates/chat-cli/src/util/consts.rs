/// TODO(brandonskiser): revert back to "qchat" for prompting login after standalone releases.
pub const CLI_BINARY_NAME: &str = "kiro-cli";
pub const CHAT_BINARY_NAME: &str = "kiro-cli-chat";

pub const GITHUB_REPO_NAME: &str = "aws/amazon-q-developer-cli";

pub const MCP_SERVER_TOOL_DELIMITER: &str = "/";

pub const GOV_REGIONS: &[&str] = &["us-gov-east-1", "us-gov-west-1"];

/// Build time env vars
pub mod build {
    /// A git full sha hash of the current build
    pub const HASH: Option<&str> = option_env!("AMAZON_Q_BUILD_HASH");

    /// The datetime in rfc3339 format of the current build
    pub const DATETIME: Option<&str> = option_env!("AMAZON_Q_BUILD_DATETIME");
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
        /// The UUID of the current parent qterm instance
        QTERM_SESSION_ID = "QTERM_SESSION_ID",

        /// The current parent socket to connect to
        Q_PARENT = "Q_PARENT",

        /// Set the [`Q_PARENT`] parent socket to connect to
        Q_SET_PARENT = "Q_SET_PARENT",

        /// Guard for the [`Q_SET_PARENT`] check
        Q_SET_PARENT_CHECK = "Q_SET_PARENT_CHECK",

        /// Set if qterm is running, contains the version
        Q_TERM = "Q_TERM",

        /// Sets the current log level
        Q_LOG_LEVEL = "Q_LOG_LEVEL",

        /// Overrides the ZDOTDIR environment variable
        Q_ZDOTDIR = "Q_ZDOTDIR",

        /// Indicates a process was launched by Kiro
        PROCESS_LAUNCHED_BY_Q = "PROCESS_LAUNCHED_BY_Q",

        /// The shell to use in qterm
        Q_SHELL = "Q_SHELL",

        /// Indicates the user is debugging the shell
        Q_DEBUG_SHELL = "Q_DEBUG_SHELL",

        /// Indicates the user is using zsh autosuggestions which disables Inline
        Q_USING_ZSH_AUTOSUGGESTIONS = "Q_USING_ZSH_AUTOSUGGESTIONS",

        /// Overrides the path to the bundle metadata released with certain desktop builds.
        Q_BUNDLE_METADATA_PATH = "Q_BUNDLE_METADATA_PATH",

        /// Identifier for the client application or service using the chat-cli
        Q_CLI_CLIENT_APPLICATION = "Q_CLI_CLIENT_APPLICATION",

        /// Shows continuation IDs in chat output for debugging/development
        Q_SHOW_CONTINUATION_IDS = "Q_SHOW_CONTINUATION_IDS",

        /// Flag for running integration tests
        CLI_IS_INTEG_TEST = "Q_CLI_IS_INTEG_TEST",

        /// Enable logging to stdout
        Q_LOG_STDOUT = "Q_LOG_STDOUT",

        /// Disable telemetry collection
        Q_DISABLE_TELEMETRY = "Q_DISABLE_TELEMETRY",

        /// Mock chat response for testing
        Q_MOCK_CHAT_RESPONSE = "Q_MOCK_CHAT_RESPONSE",

        /// Disable truecolor terminal support
        Q_DISABLE_TRUECOLOR = "Q_DISABLE_TRUECOLOR",

        /// Fake remote environment for testing
        Q_FAKE_IS_REMOTE = "Q_FAKE_IS_REMOTE",

        /// Codespaces environment indicator
        Q_CODESPACES = "Q_CODESPACES",

        /// CI environment indicator
        Q_CI = "Q_CI",

        /// Telemetry client ID
        Q_TELEMETRY_CLIENT_ID = "Q_TELEMETRY_CLIENT_ID",

        /// Amazon Q SigV4 authentication
        AMAZON_Q_SIGV4 = "AMAZON_Q_SIGV4",

        /// Amazon Q chat shell
        AMAZON_Q_CHAT_SHELL = "AMAZON_Q_CHAT_SHELL",

        /// Editor environment variable
        EDITOR = "EDITOR",

        /// Terminal type
        TERM = "TERM",

        /// AWS region
        AWS_REGION = "AWS_REGION",

        /// GitHub Codespaces environment
        CODESPACES = "CODESPACES",

        /// CI environment
        CI = "CI"
    }
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
