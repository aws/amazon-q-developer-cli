use crate::api_client::endpoints::Endpoint;
use crate::api_client::{
    ApiClient,
    ApiClientError,
};
use crate::database::{
    AuthProfile,
    Database,
};
use crate::os::{
    Env,
    Fs,
};

pub async fn list_available_profiles(
    env: &Env,
    fs: &Fs,
    database: &mut Database,
) -> Result<Vec<AuthProfile>, ApiClientError> {
    let mut profiles = vec![];
    for endpoint in Endpoint::CODEWHISPERER_ENDPOINTS {
        let client = ApiClient::new(env, fs, database, Some(endpoint.clone())).await?;
        match client.list_available_profiles().await {
            Ok(mut p) => profiles.append(&mut p),
            Err(e) => tracing::error!("Failed to list profiles from endpoint {:?}: {:?}", endpoint, e),
        }
    }

    Ok(profiles)
}
