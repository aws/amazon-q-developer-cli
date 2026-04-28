use tracing::{
    debug,
    error,
    info,
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

/// Discover the correct endpoint for API key users by calling GetProfile against
/// all known endpoints. Returns the first endpoint that responds successfully.
/// This is analogous to how IDC users discover their region via ListAvailableProfiles.
pub async fn discover_endpoint_for_api_key(env: &Env, fs: &Fs, database: &mut Database) -> Option<Endpoint> {
    // If a custom endpoint is configured, respect it.
    let configured = Endpoint::configured_value(database);
    if Endpoint::is_custom(&configured) {
        debug!(endpoint = ?configured, "Using custom endpoint for API key");
        return Some(configured);
    }

    let endpoints = vec![Endpoint::DEFAULT_ENDPOINT, Endpoint::FRA_ENDPOINT];
    for endpoint in endpoints {
        debug!(endpoint = ?endpoint, "Trying GetProfile for API key");
        let client = match ApiClient::new(env, fs, database, Some(endpoint.clone())).await {
            Ok(c) => c,
            Err(e) => {
                error!(endpoint = ?endpoint, error = ?e, "Failed to create ApiClient");
                continue;
            },
        };
        match client.get_profile_for_api_key().await {
            Ok(_) => {
                info!(endpoint = ?endpoint, "GetProfile succeeded for API key");
                return Some(endpoint);
            },
            Err(e) => {
                debug!(endpoint = ?endpoint, error = ?e, "GetProfile failed for API key");
            },
        }
    }

    error!("GetProfile failed on all endpoints for API key");
    None
}

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

#[cfg(test)]
mod tests {
    use aws_config::Region;

    use super::*;

    #[tokio::test]
    async fn discover_endpoint_returns_custom_when_configured() {
        let env = Env::new();
        let fs = Fs::new();
        let mut database = Database::new_default().await.unwrap();

        let custom = Endpoint {
            url: "https://custom.example.com".into(),
            region: Region::new("us-west-2"),
        };
        database
            .settings
            .set(
                crate::database::settings::Setting::ApiCodeWhispererService,
                serde_json::json!({
                    "endpoint": custom.url().to_string(),
                    "region": "us-west-2",
                }),
                None,
            )
            .await
            .unwrap();

        let result = discover_endpoint_for_api_key(&env, &fs, &mut database).await;
        assert!(result.is_some());
        let ep = result.unwrap();
        assert_eq!(ep.url(), "https://custom.example.com");
        assert_eq!(ep.region(), &Region::new("us-west-2"));
    }

    #[tokio::test]
    async fn discover_endpoint_returns_none_without_api_key() {
        // Without KIRO_API_KEY set, GetProfile will fail on both endpoints (no auth).
        // The function should return None since neither endpoint succeeds.
        let env = Env::new();
        let fs = Fs::new();
        let mut database = Database::new_default().await.unwrap();

        let result = discover_endpoint_for_api_key(&env, &fs, &mut database).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn get_profile_for_api_key_callable() {
        // Verify the method exists and is callable (will fail without real auth, but shouldn't panic)
        let env = Env::new();
        let fs = Fs::new();
        let mut database = Database::new_default().await.unwrap();
        let client = ApiClient::new(&env, &fs, &mut database, None).await.unwrap();
        // Should return an error (no real API key), not panic
        let _ = client.get_profile_for_api_key().await;
    }
}
