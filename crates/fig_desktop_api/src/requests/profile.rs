use fig_api_client::profile::Profile;
use fig_proto::fig::{
    ListAvailableProfilesRequest,
    ListAvailableProfilesResponse,
    SetProfileRequest,
};

use super::{
    RequestResult,
    RequestResultImpl,
    ServerOriginatedSubMessage,
};

pub fn set_profile(request: SetProfileRequest) -> RequestResult {
    let Some(profile) = request.profile else {
        return RequestResult::error("Profile was not provided.");
    };

    let profile = Profile {
        arn: profile.arn,
        profile_name: profile.profile_name,
    };

    let profile = match serde_json::to_string(&profile) {
        Ok(profile) => profile,
        Err(err) => return RequestResult::error(err.to_string()),
    };

    if let Err(err) = fig_settings::state::set_value("api.codewhisperer.profile", profile) {
        return RequestResult::error(err.to_string());
    }

    RequestResult::success()
}

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
