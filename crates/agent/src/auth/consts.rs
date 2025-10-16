use aws_types::region::Region;

pub(crate) const OIDC_BUILDER_ID_REGION: Region = Region::from_static("us-east-1");

/// The scopes requested for OIDC
///
/// Do not include `sso:account:access`, these permissions are not needed and were
/// previously included
pub(crate) const SCOPES: &[&str] = &[
    "codewhisperer:completions",
    "codewhisperer:analysis",
    "codewhisperer:conversations",
    // "codewhisperer:taskassist",
    // "codewhisperer:transformations",
];

// The start URL for public builder ID users
pub const START_URL: &str = "https://view.awsapps.com/start";

// The start URL for internal amzn users
pub const AMZN_START_URL: &str = "https://amzn.awsapps.com/start";

pub(crate) const REFRESH_GRANT_TYPE: &str = "refresh_token";
