use std::borrow::Cow;

use aws_config::Region;
use serde_json::Value;

use crate::consts::{
    PROD_CODEWHISPERER_ENDPOINT_REGION,
    PROD_CODEWHISPERER_ENDPOINT_URL,
    PROD_Q_ENDPOINT_REGION,
    PROD_Q_ENDPOINT_URL,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    url: Cow<'static, str>,
    region: Region,
}

impl Endpoint {
    const PROD_CODEWHISPERER: Self = Self {
        url: Cow::Borrowed(PROD_CODEWHISPERER_ENDPOINT_URL),
        region: PROD_CODEWHISPERER_ENDPOINT_REGION,
    };
    const PROD_Q: Self = Self {
        url: Cow::Borrowed(PROD_Q_ENDPOINT_URL),
        region: PROD_Q_ENDPOINT_REGION,
    };

    pub fn load_codewhisperer() -> Self {
        match fig_settings::settings::get_value("api.codewhisperer.service") {
            Ok(Some(Value::Object(o))) => {
                let endpoint = o.get("endpoint").and_then(|v| v.as_str());
                let region = o.get("region").and_then(|v| v.as_str());

                match (endpoint, region) {
                    (Some(endpoint), Some(region)) => Self {
                        url: endpoint.to_owned().into(),
                        region: Region::new(region.to_owned()),
                    },
                    _ => Endpoint::PROD_CODEWHISPERER,
                }
            },
            _ => Endpoint::PROD_CODEWHISPERER,
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

        let prod = Endpoint::PROD_CODEWHISPERER;
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
