use fig_proto::fig::server_originated_message::Submessage as ServerOriginatedSubMessage;
use fig_proto::fig::{
    GetSettingsPropertyRequest,
    GetSettingsPropertyResponse,
    UpdateSettingsPropertyRequest,
};
use fig_settings::{
    JsonStore,
    OldSettings,
    settings,
};

use super::{
    RequestResult,
    RequestResultImpl,
};

pub async fn get(request: GetSettingsPropertyRequest) -> RequestResult {
    let res = match request.key {
        Some(key) => serde_json::to_string(
            &settings::get_value(&key)
                .map_err(|err| format!("Failed getting settings value for {key}: {err}"))?
                .ok_or_else(|| format!("No value for key '{key}'"))?,
        ),
        None => OldSettings::load()
            .map(|s| serde_json::to_string(&*s.map()))
            .map_err(|err| format!("Failed getting settings: {err}"))?,
    };

    let json_blob = res.map_err(|err| format!("Could not convert value for key to JSON: {err}"))?;

    let response = ServerOriginatedSubMessage::GetSettingsPropertyResponse(GetSettingsPropertyResponse {
        json_blob: Some(json_blob),
        is_default: None,
    });

    Ok(response.into())
}

pub async fn update(request: UpdateSettingsPropertyRequest) -> RequestResult {
    match (&request.key, request.value) {
        (Some(key), Some(value)) => {
            let value = serde_json::from_str(&value).unwrap_or(serde_json::Value::String(value));
            fig_settings::settings::set_value(key, value).map_err(|err| format!("Failed setting {key}: {err}"))?;
        },
        (Some(key), None) => {
            fig_settings::settings::remove_value(key).map_err(|err| format!("Failed removing {key}: {err}"))?;
        },
        (None, _) => {
            return RequestResult::error("No key provided with request");
        },
    }

    RequestResult::success()
}
