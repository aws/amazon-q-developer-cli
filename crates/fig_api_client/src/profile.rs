use serde::{
    Deserialize,
    Serialize,
};

use crate::Client;
use crate::endpoints::Endpoint;

#[derive(Debug, Deserialize, Serialize)]
pub struct Profile {
    pub arn: String,
    pub profile_name: String,
}

impl From<amzn_codewhisperer_client::types::Profile> for Profile {
    fn from(profile: amzn_codewhisperer_client::types::Profile) -> Self {
        Self {
            arn: profile.arn,
            profile_name: profile.profile_name,
        }
    }
}

pub async fn list_available_profiles() -> Vec<Profile> {
    let mut profiles = vec![];
    for endpoint in Endpoint::CODEWHISPERER_ENDPOINTS {
        let client = Client::new_codewhisperer_client(&endpoint).await;
        match client.list_available_profiles().await {
            Ok(mut p) => profiles.append(&mut p),
            Err(e) => tracing::error!("Failed to list profiles from endpoint {:?}: {:?}", endpoint, e),
        }
    }

    profiles
}
