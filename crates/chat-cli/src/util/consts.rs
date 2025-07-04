/// TODO(brandonskiser): revert back to "qchat" for prompting login after standalone releases.
pub const CLI_BINARY_NAME: &str = "q";
pub const CHAT_BINARY_NAME: &str = "qchat";

pub const PRODUCT_NAME: &str = "Amazon Q";

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

        /// Indicates a process was launched by Amazon Q
        PROCESS_LAUNCHED_BY_Q = "PROCESS_LAUNCHED_BY_Q",

        /// The shell to use in qterm
        Q_SHELL = "Q_SHELL",

        /// Indicates the user is debugging the shell
        Q_DEBUG_SHELL = "Q_DEBUG_SHELL",

        /// Indicates the user is using zsh autosuggestions which disables Inline
        Q_USING_ZSH_AUTOSUGGESTIONS = "Q_USING_ZSH_AUTOSUGGESTIONS",

        /// Overrides the path to the bundle metadata released with certain desktop builds.
        Q_BUNDLE_METADATA_PATH = "Q_BUNDLE_METADATA_PATH"
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
