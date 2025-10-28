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
        KIRO_PARENT = "KIRO_PARENT",

        /// Set the [`KIRO_PARENT`] parent socket to connect to
        KIRO_SET_PARENT = "KIRO_SET_PARENT",

        /// Guard for the [`KIRO_SET_PARENT`] check
        KIRO_SET_PARENT_CHECK = "KIRO_SET_PARENT_CHECK",

        /// Set if qterm is running, contains the version
        KIRO_TERM = "KIRO_TERM",

        /// Sets the current log level
        KIRO_LOG_LEVEL = "KIRO_LOG_LEVEL",

        /// Overrides the ZDOTDIR environment variable
        KIRO_ZDOTDIR = "KIRO_ZDOTDIR",

        /// Indicates a process was launched by Kiro
        PROCESS_LAUNCHED_BY_KIRO = "PROCESS_LAUNCHED_BY_KIRO",

        /// The shell to use in qterm
        KIRO_SHELL = "KIRO_SHELL",

        /// Indicates the user is debugging the shell
        KIRO_DEBUG_SHELL = "KIRO_DEBUG_SHELL",

        /// Indicates the user is using zsh autosuggestions which disables Inline
        KIRO_USING_ZSH_AUTOSUGGESTIONS = "KIRO_USING_ZSH_AUTOSUGGESTIONS",

        /// Overrides the path to the bundle metadata released with certain desktop builds.
        KIRO_BUNDLE_METADATA_PATH = "KIRO_BUNDLE_METADATA_PATH",

        /// Identifier for the client application or service using the chat-cli
        KIRO_CLI_CLIENT_APPLICATION = "KIRO_CLI_CLIENT_APPLICATION",

        /// Shows continuation IDs in chat output for debugging/development
        KIRO_SHOW_CONTINUATION_IDS = "KIRO_SHOW_CONTINUATION_IDS"
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
