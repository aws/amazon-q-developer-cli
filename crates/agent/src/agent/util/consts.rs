pub const CLI_BINARY_NAME: &str = "q";
pub const PRODUCT_NAME: &str = "Amazon Q";

/// User agent override
pub const USER_AGENT_ENV_VAR: &str = "AWS_EXECUTION_ENV";
// Constants for setting the user agent in HTTP requests
pub const USER_AGENT_APP_NAME: &str = "AmazonQ-For-CLI";
pub const USER_AGENT_VERSION_KEY: &str = "Version";
pub const USER_AGENT_VERSION_VALUE: &str = env!("CARGO_PKG_VERSION");

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
        CLI_IS_INTEG_TEST = "Q_CLI_IS_INTEG_TEST"
    }
}
