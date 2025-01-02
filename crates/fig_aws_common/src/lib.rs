mod sdk_error_display;
mod user_agent_override_interceptor;

use std::sync::LazyLock;

use aws_smithy_runtime_api::client::behavior_version::BehaviorVersion;
use aws_types::app_name::AppName;
pub use sdk_error_display::SdkErrorDisplay;
pub use user_agent_override_interceptor::UserAgentOverrideInterceptor;

const APP_NAME_STR: &str = "AmazonQ-For-CLI";

pub fn app_name() -> AppName {
    static APP_NAME: LazyLock<AppName> = LazyLock::new(|| AppName::new(APP_NAME_STR).expect("invalid app name"));
    APP_NAME.clone()
}

pub fn behavior_version() -> BehaviorVersion {
    BehaviorVersion::v2024_03_28()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_name() {
        println!("{}", app_name());
    }

    #[test]
    fn test_behavior_version() {
        assert!(behavior_version() == BehaviorVersion::latest());
    }
}
