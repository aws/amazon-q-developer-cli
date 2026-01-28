use tracing::{
    debug,
    error,
};

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
    region: &str,
) -> Result<Vec<AuthProfile>, ApiClientError> {
    debug!(region = %region, "list_available_profiles called");

    // Check if custom endpoint is configured
    let configured = Endpoint::configured_value(database);
    let is_custom = configured != Endpoint::DEFAULT_ENDPOINT
        && configured != Endpoint::FRA_ENDPOINT
        && configured != Endpoint::GOV_ENDPOINT_EAST
        && configured != Endpoint::GOV_ENDPOINT_WEST;

    let endpoints = if is_custom {
        debug!(endpoint = ?configured, "Using custom endpoint");
        vec![configured]
    } else {
        Endpoint::get_endpoints_from_region(region)
    };

    let mut profiles = vec![];
    for endpoint in endpoints {
        debug!(endpoint = ?endpoint, "Trying endpoint");
        let client = ApiClient::new(env, fs, database, Some(endpoint.clone())).await?;
        match client.list_available_profiles().await {
            Ok(mut p) => {
                debug!(count = p.len(), endpoint = ?endpoint, "Got profiles");
                profiles.append(&mut p);
            },
            Err(e) => {
                error!(endpoint = ?endpoint, error = ?e, "Failed to list profiles");
            },
        }
    }

    debug!(total = profiles.len(), "Total profiles across all endpoints");
    Ok(profiles)
}
