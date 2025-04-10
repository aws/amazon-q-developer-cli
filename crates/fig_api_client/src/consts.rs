use aws_config::Region;

// Endpoint constants
pub const PROD_CODEWHISPERER_ENDPOINT_URL: &str = "https://codewhisperer.us-east-1.amazonaws.com";
pub const PROD_CODEWHISPERER_ENDPOINT_REGION: Region = Region::from_static("us-east-1");

pub const PROD_Q_ENDPOINT_URL: &str = "https://q.us-east-1.amazonaws.com";
pub const PROD_Q_ENDPOINT_REGION: Region = Region::from_static("us-east-1");

// Opt out constants
pub const SHARE_CODEWHISPERER_CONTENT_SETTINGS_KEY: &str = "codeWhisperer.shareCodeWhispererContentWithAWS";
pub const X_AMZN_CODEWHISPERER_OPT_OUT_HEADER: &str = "x-amzn-codewhisperer-optout";

// Session ID constants
pub const X_AMZN_SESSIONID_HEADER: &str = "x-amzn-sessionid";
