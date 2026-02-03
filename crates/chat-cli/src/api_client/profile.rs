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
    list_profiles_from_endpoints(env, fs, database, &Endpoint::get_endpoints_from_region(region)).await
}

/// List profiles from all endpoints (for External IdP where region is unknown)
pub async fn list_all_available_profiles(
    env: &Env,
    fs: &Fs,
    database: &mut Database,
) -> Result<Vec<AuthProfile>, ApiClientError> {
    list_profiles_from_endpoints(env, fs, database, &Endpoint::all()).await
}

async fn list_profiles_from_endpoints(
    env: &Env,
    fs: &Fs,
    database: &mut Database,
    endpoints: &[Endpoint],
) -> Result<Vec<AuthProfile>, ApiClientError> {
    // Check if custom endpoint is configured
    let configured = Endpoint::configured_value(database);
    let endpoints = if Endpoint::is_custom(&configured) {
        debug!(endpoint = ?configured, "Using custom endpoint");
        vec![configured]
    } else {
        endpoints.to_vec()
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
