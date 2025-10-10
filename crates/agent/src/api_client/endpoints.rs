use std::borrow::Cow;

use aws_config::Region;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub url: Cow<'static, str>,
    pub region: Region,
}

impl Endpoint {
    pub const CODEWHISPERER_ENDPOINTS: [Self; 2] = [Self::DEFAULT_ENDPOINT, Self::FRA_ENDPOINT];
    pub const DEFAULT_ENDPOINT: Self = Self {
        url: Cow::Borrowed("https://q.us-east-1.amazonaws.com"),
        region: Region::from_static("us-east-1"),
    };
    pub const FRA_ENDPOINT: Self = Self {
        url: Cow::Borrowed("https://q.eu-central-1.amazonaws.com/"),
        region: Region::from_static("eu-central-1"),
    };

    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    pub(crate) fn region(&self) -> &Region {
        &self.region
    }
}
