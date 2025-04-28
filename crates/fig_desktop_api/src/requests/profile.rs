use fig_api_client::profile::Profile;
use fig_proto::fig::{
    ListAvailableProfilesRequest,
    ListAvailableProfilesResponse,
    SetProfileRequest,
};
use fig_telemetry_core::{
    QProfileSwitchIntent,
    TelemetryResult,
};

use super::{
    RequestResult,
    RequestResultImpl,
    ServerOriginatedSubMessage,
};

pub async fn set_profile(request: SetProfileRequest) -> RequestResult {
    let Some(profile) = request.profile else {
        fig_telemetry::send_did_select_profile(
            QProfileSwitchIntent::Auth,
            "not-set".to_string(),
            TelemetryResult::Failed,
            fig_settings::state::get_string("auth.idc.region").ok().flatten(),
            None,
        )
        .await;
        return RequestResult::error("Profile was not provided.");
    };

    let profile = Profile {
        arn: profile.arn,
        profile_name: profile.profile_name,
    };

    let profile_str = match serde_json::to_string(&profile) {
        Ok(profile) => profile,
        Err(err) => return RequestResult::error(err.to_string()),
    };

    if let Err(err) = fig_settings::state::set_value("api.codewhisperer.profile", profile_str) {
        return RequestResult::error(err.to_string());
    }

    let _ = fig_settings::state::remove_value("api.selectedCustomization");

    if let Some(profile_region) = profile.arn.split(':').nth(3) {
        fig_telemetry::send_did_select_profile(
            QProfileSwitchIntent::Auth,
            profile_region.to_string(),
            TelemetryResult::Succeeded,
            fig_settings::state::get_string("auth.idc.region").ok().flatten(),
            None,
        )
        .await;
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
