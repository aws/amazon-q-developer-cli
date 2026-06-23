use std::borrow::Cow;

use aws_config::Region;
use serde_json::Value;
use tracing::error;

use crate::database::Database;
use crate::database::settings::Setting;
use crate::util::{
    US_GOV_EAST,
    US_GOV_WEST,
    US_ISO_ALE,
    US_ISO_DCA,
    US_ISO_LCK,
    US_ISO_LTW,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub url: Cow<'static, str>,
    pub region: Region,
}

impl Endpoint {
    pub const ALE_ENDPOINT: Self = Self {
        url: Cow::Borrowed("https://q.us-isof-south-1.csp.hci.ic.gov"),
        region: Region::from_static(US_ISO_ALE),
    };
    pub const CPS_EU_CENTRAL_1: Self = Self {
        url: Cow::Borrowed("https://management.eu-central-1.kiro.dev"),
        region: Region::from_static("eu-central-1"),
    };
    pub const CPS_US_EAST_1: Self = Self {
        url: Cow::Borrowed("https://management.us-east-1.kiro.dev"),
        region: Region::from_static("us-east-1"),
    };
    pub const CPS_US_GOV_EAST_1: Self = Self {
        url: Cow::Borrowed("https://management.us-gov-east-1.kiro.dev"),
        region: Region::from_static(US_GOV_EAST),
    };
    pub const CPS_US_GOV_WEST_1: Self = Self {
        url: Cow::Borrowed("https://management.us-gov-west-1.kiro.dev"),
        region: Region::from_static(US_GOV_WEST),
    };
    pub const CPS_US_ISOB_EAST_1: Self = Self {
        url: Cow::Borrowed("https://kiro-management.us-isob-east-1.sc2s.sgov.gov"),
        region: Region::from_static(US_ISO_LCK),
    };
    pub const CPS_US_ISOF_EAST_1: Self = Self {
        url: Cow::Borrowed("https://kiro-management.us-isof-east-1.csp.hci.ic.gov"),
        region: Region::from_static(US_ISO_LTW),
    };
    pub const CPS_US_ISOF_SOUTH_1: Self = Self {
        url: Cow::Borrowed("https://kiro-management.us-isof-south-1.csp.hci.ic.gov"),
        region: Region::from_static(US_ISO_ALE),
    };
    pub const CPS_US_ISO_EAST_1: Self = Self {
        url: Cow::Borrowed("https://kiro-management.us-iso-east-1.c2s.ic.gov"),
        region: Region::from_static(US_ISO_DCA),
    };
    pub const DCA_ENDPOINT: Self = Self {
        url: Cow::Borrowed("https://q.us-iso-east-1.c2s.ic.gov"),
        region: Region::from_static(US_ISO_DCA),
    };
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
    const KNOWN_ENDPOINTS: &'static [Self] = &[
        Self::DEFAULT_ENDPOINT,
        Self::FRA_ENDPOINT,
        Self::GOV_ENDPOINT_EAST,
        Self::GOV_ENDPOINT_WEST,
        Self::ALE_ENDPOINT,
        Self::LCK_ENDPOINT,
        Self::LTW_ENDPOINT,
        Self::DCA_ENDPOINT,
    ];
    pub const KRS_EU_CENTRAL_1: Self = Self {
        url: Cow::Borrowed("https://runtime.eu-central-1.kiro.dev"),
        region: Region::from_static("eu-central-1"),
    };
    pub const KRS_US_EAST_1: Self = Self {
        url: Cow::Borrowed("https://runtime.us-east-1.kiro.dev"),
        region: Region::from_static("us-east-1"),
    };
    pub const KRS_US_GOV_EAST_1: Self = Self {
        url: Cow::Borrowed("https://runtime.us-gov-east-1.kiro.dev"),
        region: Region::from_static(US_GOV_EAST),
    };
    pub const KRS_US_GOV_WEST_1: Self = Self {
        url: Cow::Borrowed("https://runtime.us-gov-west-1.kiro.dev"),
        region: Region::from_static(US_GOV_WEST),
    };
    pub const KRS_US_ISOB_EAST_1: Self = Self {
        url: Cow::Borrowed("https://kiro-runtime.us-isob-east-1.sc2s.sgov.gov"),
        region: Region::from_static(US_ISO_LCK),
    };
    pub const KRS_US_ISOF_EAST_1: Self = Self {
        url: Cow::Borrowed("https://kiro-runtime.us-isof-east-1.csp.hci.ic.gov"),
        region: Region::from_static(US_ISO_LTW),
    };
    pub const KRS_US_ISOF_SOUTH_1: Self = Self {
        url: Cow::Borrowed("https://kiro-runtime.us-isof-south-1.csp.hci.ic.gov"),
        region: Region::from_static(US_ISO_ALE),
    };
    pub const KRS_US_ISO_EAST_1: Self = Self {
        url: Cow::Borrowed("https://kiro-runtime.us-iso-east-1.c2s.ic.gov"),
        region: Region::from_static(US_ISO_DCA),
    };
    pub const LCK_ENDPOINT: Self = Self {
        url: Cow::Borrowed("https://q.us-isob-east-1.sc2s.sgov.gov"),
        region: Region::from_static(US_ISO_LCK),
    };
    pub const LTW_ENDPOINT: Self = Self {
        url: Cow::Borrowed("https://q.us-isof-east-1.csp.hci.ic.gov"),
        region: Region::from_static(US_ISO_LTW),
    };

    pub fn all() -> Vec<Self> {
        Self::KNOWN_ENDPOINTS.to_vec()
    }

    pub fn is_custom(endpoint: &Self) -> bool {
        !Self::KNOWN_ENDPOINTS.contains(endpoint)
    }

    pub fn get_endpoints_from_region(region: &str) -> Vec<Self> {
        if region == US_GOV_EAST || region == US_GOV_WEST {
            return vec![Self::GOV_ENDPOINT_EAST, Self::GOV_ENDPOINT_WEST];
        }
        if region == US_ISO_DCA {
            return vec![Self::DCA_ENDPOINT];
        }
        if region == US_ISO_LCK {
            return vec![Self::LCK_ENDPOINT];
        }
        if region == US_ISO_ALE {
            return vec![Self::ALE_ENDPOINT];
        }
        if region == US_ISO_LTW {
            return vec![Self::LTW_ENDPOINT];
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

    pub(crate) fn krs_for_region(region: &str) -> Self {
        match region {
            "us-east-1" => Self::KRS_US_EAST_1,
            "eu-central-1" => Self::KRS_EU_CENTRAL_1,
            US_GOV_EAST => Self::KRS_US_GOV_EAST_1,
            US_GOV_WEST => Self::KRS_US_GOV_WEST_1,
            US_ISO_DCA => Self::KRS_US_ISO_EAST_1,
            US_ISO_LCK => Self::KRS_US_ISOB_EAST_1,
            US_ISO_ALE => Self::KRS_US_ISOF_SOUTH_1,
            US_ISO_LTW => Self::KRS_US_ISOF_EAST_1,
            _ => Self::get_endpoints_from_region(region)
                .into_iter()
                .find(|e| e.region().as_ref() == region)
                .unwrap_or(Self::DEFAULT_ENDPOINT),
        }
    }

    pub(crate) fn cps_for_region(region: &str) -> Self {
        match region {
            "us-east-1" => Self::CPS_US_EAST_1,
            "eu-central-1" => Self::CPS_EU_CENTRAL_1,
            US_GOV_EAST => Self::CPS_US_GOV_EAST_1,
            US_GOV_WEST => Self::CPS_US_GOV_WEST_1,
            US_ISO_DCA => Self::CPS_US_ISO_EAST_1,
            US_ISO_LCK => Self::CPS_US_ISOB_EAST_1,
            US_ISO_ALE => Self::CPS_US_ISOF_SOUTH_1,
            US_ISO_LTW => Self::CPS_US_ISOF_EAST_1,
            _ => Self::get_endpoints_from_region(region)
                .into_iter()
                .find(|e| e.region().as_ref() == region)
                .unwrap_or(Self::DEFAULT_ENDPOINT),
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

    #[test]
    fn test_get_endpoints_from_region_iso_dca() {
        assert_eq!(Endpoint::get_endpoints_from_region(US_ISO_DCA), vec![
            Endpoint::DCA_ENDPOINT
        ]);
    }

    #[test]
    fn test_get_endpoints_from_region_iso_lck() {
        assert_eq!(Endpoint::get_endpoints_from_region(US_ISO_LCK), vec![
            Endpoint::LCK_ENDPOINT
        ]);
    }

    #[test]
    fn test_get_endpoints_from_region_iso_ale() {
        assert_eq!(Endpoint::get_endpoints_from_region(US_ISO_ALE), vec![
            Endpoint::ALE_ENDPOINT
        ]);
    }

    #[test]
    fn test_get_endpoints_from_region_iso_ltw() {
        assert_eq!(Endpoint::get_endpoints_from_region(US_ISO_LTW), vec![
            Endpoint::LTW_ENDPOINT
        ]);
    }

    #[test]
    fn test_krs_for_region() {
        assert_eq!(Endpoint::krs_for_region("us-east-1"), Endpoint::KRS_US_EAST_1);
        assert_eq!(Endpoint::krs_for_region("eu-central-1"), Endpoint::KRS_EU_CENTRAL_1);
        assert_eq!(Endpoint::krs_for_region(US_GOV_EAST), Endpoint::KRS_US_GOV_EAST_1);
        assert_eq!(Endpoint::krs_for_region(US_GOV_WEST), Endpoint::KRS_US_GOV_WEST_1);
        assert_eq!(Endpoint::krs_for_region(US_ISO_DCA), Endpoint::KRS_US_ISO_EAST_1);
        assert_eq!(Endpoint::krs_for_region(US_ISO_LCK), Endpoint::KRS_US_ISOB_EAST_1);
        assert_eq!(Endpoint::krs_for_region(US_ISO_ALE), Endpoint::KRS_US_ISOF_SOUTH_1);
        assert_eq!(Endpoint::krs_for_region(US_ISO_LTW), Endpoint::KRS_US_ISOF_EAST_1);
    }

    #[test]
    fn test_cps_for_region() {
        assert_eq!(Endpoint::cps_for_region("us-east-1"), Endpoint::CPS_US_EAST_1);
        assert_eq!(Endpoint::cps_for_region("eu-central-1"), Endpoint::CPS_EU_CENTRAL_1);
        assert_eq!(Endpoint::cps_for_region(US_GOV_EAST), Endpoint::CPS_US_GOV_EAST_1);
        assert_eq!(Endpoint::cps_for_region(US_GOV_WEST), Endpoint::CPS_US_GOV_WEST_1);
        assert_eq!(Endpoint::cps_for_region(US_ISO_DCA), Endpoint::CPS_US_ISO_EAST_1);
        assert_eq!(Endpoint::cps_for_region(US_ISO_LCK), Endpoint::CPS_US_ISOB_EAST_1);
        assert_eq!(Endpoint::cps_for_region(US_ISO_ALE), Endpoint::CPS_US_ISOF_SOUTH_1);
        assert_eq!(Endpoint::cps_for_region(US_ISO_LTW), Endpoint::CPS_US_ISOF_EAST_1);
    }
}
