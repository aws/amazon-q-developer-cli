use std::borrow::Cow;

use aws_config::Region;
use serde_json::Value;
use tracing::error;

use crate::consts::{
    PROD_CODEWHISPERER_ENDPOINT_REGION,
    PROD_CODEWHISPERER_ENDPOINT_URL,
    PROD_CODEWHISPERER_FRA_ENDPOINT_REGION,
    PROD_CODEWHISPERER_FRA_ENDPOINT_URL,
    PROD_Q_ENDPOINT_REGION,
    PROD_Q_ENDPOINT_URL,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub url: Cow<'static, str>,
    pub region: Region,
}

impl Endpoint {
    pub const CODEWHISPERER_ENDPOINTS: [Self; 2] = [Self::DEFAULT_ENDPOINT, Self {
        url: Cow::Borrowed(PROD_CODEWHISPERER_FRA_ENDPOINT_URL),
        region: PROD_CODEWHISPERER_FRA_ENDPOINT_REGION,
    }];
    pub const DEFAULT_ENDPOINT: Self = Self {
        url: Cow::Borrowed(PROD_CODEWHISPERER_ENDPOINT_URL),
        region: PROD_CODEWHISPERER_ENDPOINT_REGION,
    };
    pub const PROD_Q: Self = Self {
        url: Cow::Borrowed(PROD_Q_ENDPOINT_URL),
        region: PROD_Q_ENDPOINT_REGION,
    };

    pub fn load_codewhisperer() -> Self {
        let (endpoint, region) =
            if let Ok(Some(Value::Object(o))) = fig_settings::settings::get_value("api.codewhisperer.service") {
                // The following branch is evaluated in case the user has set their own endpoint.
                (
                    o.get("endpoint").and_then(|v| v.as_str()).map(|v| v.to_owned()),
                    o.get("region").and_then(|v| v.as_str()).map(|v| v.to_owned()),
                )
            } else if let Ok(Some(Value::Object(o))) = fig_settings::state::get_value("api.codewhisperer.profile") {
                // The following branch is evaluated in the case of user profile being set.
                match o.get("arn").and_then(|v| v.as_str()).map(|v| v.to_owned()) {
                    Some(arn) => {
                        let region = arn.split(':').nth(3).unwrap_or_default().to_owned();
                        match Self::CODEWHISPERER_ENDPOINTS
                            .iter()
                            .find(|e| e.region().as_ref() == region)
                        {
                            Some(endpoint) => (Some(endpoint.url().to_owned()), Some(region)),
                            None => {
                                error!("Failed to find endpoint for region: {region}");
                                (None, None)
                            },
                        }
                    },
                    None => (None, None),
                }
            } else {
                (None, None)
            };

        match (endpoint, region) {
            (Some(endpoint), Some(region)) => Self {
                url: endpoint.clone().into(),
                region: Region::new(region.clone()),
            },
            _ => Endpoint::DEFAULT_ENDPOINT,
        }
    }

    pub fn load_q() -> Self {
        match fig_settings::settings::get_value("api.q.service") {
            Ok(Some(Value::Object(o))) => {
                let endpoint = o.get("endpoint").and_then(|v| v.as_str());
                let region = o.get("region").and_then(|v| v.as_str());

                match (endpoint, region) {
                    (Some(endpoint), Some(region)) => Self {
                        url: endpoint.to_owned().into(),
                        region: Region::new(region.to_owned()),
                    },
                    _ => Endpoint::PROD_Q,
                }
            },
            _ => Endpoint::PROD_Q,
        }
    }

    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    pub(crate) fn region(&self) -> &Region {
        &self.region
    }
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::*;

    #[test]
    fn test_endpoints() {
        let _ = Endpoint::load_codewhisperer();
        let _ = Endpoint::load_q();

        let prod = &Endpoint::DEFAULT_ENDPOINT;
        Url::parse(prod.url()).unwrap();
        assert_eq!(prod.region(), &PROD_CODEWHISPERER_ENDPOINT_REGION);

        let custom = Endpoint {
            region: Region::new("us-west-2"),
            url: "https://example.com".into(),
        };
        Url::parse(custom.url()).unwrap();
        assert_eq!(custom.region(), &Region::new("us-west-2"));
    }
}
