use aws_config::Region;

// Endpoint constants
pub const PROD_CODEWHISPERER_ENDPOINT_URL: &str = "https://codewhisperer.us-east-1.amazonaws.com";
pub const PROD_CODEWHISPERER_ENDPOINT_REGION: Region = Region::from_static("us-east-1");

pub const PROD_Q_ENDPOINT_URL: &str = "https://q.us-east-1.amazonaws.com";
pub const PROD_Q_ENDPOINT_REGION: Region = Region::from_static("us-east-1");

// FRA endpoint constants
pub const PROD_CODEWHISPERER_FRA_ENDPOINT_URL: &str = "https://q.eu-central-1.amazonaws.com/";
pub const PROD_CODEWHISPERER_FRA_ENDPOINT_REGION: Region = Region::from_static("eu-central-1");

// Opt out constants
pub const X_AMZN_CODEWHISPERER_OPT_OUT_HEADER: &str = "x-amzn-codewhisperer-optout";
