use std::borrow::Cow;

use aws_config::Region;
use serde_json::Value;
use tracing::error;

use crate::database::Database;
use crate::database::settings::Setting;
use crate::util::{
    US_GOV_EAST,
    US_GOV_WEST,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub url: Cow<'static, str>,
    pub region: Region,
}

impl Endpoint {
    pub const DEFAULT_ENDPOINT: Self = Self {
        url: Cow::Borrowed("https://q.us-east-1.amazonaws.com"),
        region: Region::from_static("us-east-1"),
    };
    pub const FRA_ENDPOINT: Self = Self {
        url: Cow::Borrowed("https://q.eu-central-1.amazonaws.com/"),
        region: Region::from_static("eu-central-1"),
    };
    pub const GOV_ENDPOINT_EAST: Self = Self {
        url: Cow::Borrowed("https://q.us-gov-east-1.amazonaws.com"),
        region: Region::from_static(US_GOV_EAST),
    };
    pub const GOV_ENDPOINT_WEST: Self = Self {
        url: Cow::Borrowed("https://q.us-gov-west-1.amazonaws.com"),
        region: Region::from_static(US_GOV_WEST),
    };

    pub fn get_endpoints_from_region(region: &str) -> Vec<Self> {
        if region == US_GOV_EAST || region == US_GOV_WEST {
            return vec![Self::GOV_ENDPOINT_EAST, Self::GOV_ENDPOINT_WEST];
        }
        vec![Self::DEFAULT_ENDPOINT, Self::FRA_ENDPOINT]
    }

    pub fn configured_value(database: &Database) -> Self {
        let (endpoint, region) = if let Some(Value::Object(o)) = database.settings.get(Setting::ApiCodeWhispererService)
        {
            // The following branch is evaluated in case the user has set their own endpoint.
            (
                o.get("endpoint").and_then(|v| v.as_str()).map(|v| v.to_owned()),
                o.get("region").and_then(|v| v.as_str()).map(|v| v.to_owned()),
            )
        } else if let Ok(Some(profile)) = database.get_auth_profile() {
            // The following branch is evaluated in the case of user profile being set.
            let region = profile.arn.split(':').nth(3).unwrap_or_default().to_owned();
            match Self::get_endpoints_from_region(&region)
                .iter()
                .find(|e| e.region().as_ref() == region)
            {
                Some(endpoint) => (Some(endpoint.url().to_owned()), Some(region)),
                None => {
                    error!("Failed to find endpoint for region: {region}");
                    (None, None)
                },
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

    #[tokio::test]
    async fn test_endpoints() {
        let database = Database::new_default().await.unwrap();
        let _ = Endpoint::configured_value(&database);

        let prod = &Endpoint::DEFAULT_ENDPOINT;
        Url::parse(prod.url()).unwrap();

        let custom = Endpoint {
            region: Region::new("us-west-2"),
            url: "https://example.com".into(),
        };
        Url::parse(custom.url()).unwrap();
        assert_eq!(custom.region(), &Region::new("us-west-2"));
    }
}
