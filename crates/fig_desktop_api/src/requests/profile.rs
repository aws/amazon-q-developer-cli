use fig_proto::fig::{
    ListAvailableProfilesRequest,
    ListAvailableProfilesResponse,
};

use super::{
    RequestResult,
    ServerOriginatedSubMessage,
};

pub async fn list_available_profiles(_request: ListAvailableProfilesRequest) -> RequestResult {
    Ok(
        ServerOriginatedSubMessage::ListAvailableProfilesResponse(ListAvailableProfilesResponse {
            profiles: fig_api_client::profile::list_available_profiles()
                .await
                .iter()
                .map(|profile| fig_proto::fig::Profile {
                    arn: profile.arn.clone(),
                    profile_name: profile.profile_name.clone(),
                })
                .collect(),
        })
        .into(),
    )
}
