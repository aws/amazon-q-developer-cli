use aws_types::region::Region;
use time::Duration;

pub(crate) use crate::constants::CLIENT_NAME;

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

pub(crate) const CLIENT_TYPE: &str = "public";

pub const SOCIAL_AUTH_SERVICE_ENDPOINT: &str = "https://prod.us-east-1.auth.desktop.kiro.dev";

// The start URL for public builder ID users
pub const START_URL: &str = "https://view.awsapps.com/start";

// The start URL for internal amzn users
pub const AMZN_START_URL: &str = "https://amzn.awsapps.com/start";

pub(crate) const DEVICE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";
pub(crate) const REFRESH_GRANT_TYPE: &str = "refresh_token";

/// Pre-expiry buffer for OIDC tokens. Tokens within this window of their
/// `expires_at` are treated as expired and refreshed.
///
/// 3.5 minutes: 3-min KAS rejection buffer (per kiro-agent's
/// `acp-callback-auth-provider.ts`) plus a 30s cushion for the OIDC
/// round-trip to land before KAS would reject. Also covers in-process
/// callers that initiate streaming chat calls which can outlive a smaller
/// buffer.
pub(crate) const REFRESH_PRE_EXPIRY_BUFFER: Duration = Duration::seconds(210);
