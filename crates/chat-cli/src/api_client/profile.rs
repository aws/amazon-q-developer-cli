use crate::api_client::Client;
use crate::api_client::endpoints::Endpoint;
use crate::database::Database;
use crate::database::state::AuthProfile;

pub async fn list_available_profiles(database: &mut Database) -> Vec<AuthProfile> {
    let mut profiles = vec![];
    for endpoint in Endpoint::CODEWHISPERER_ENDPOINTS {
        let client = Client::new(database, Some(endpoint.clone())).await;
        match client.list_available_profiles().await {
            Ok(mut p) => profiles.append(&mut p),
            Err(e) => tracing::error!("Failed to list profiles from endpoint {:?}: {:?}", endpoint, e),
        }
    }

    profiles
}
