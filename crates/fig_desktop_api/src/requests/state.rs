use fig_proto::fig::server_originated_message::Submessage as ServerOriginatedSubMessage;
use fig_proto::fig::{
    GetLocalStateRequest,
    GetLocalStateResponse,
    UpdateLocalStateRequest,
};
use fig_settings::state;

use super::{
    RequestResult,
    RequestResultImpl,
};

pub async fn get(request: GetLocalStateRequest) -> RequestResult {
    let res = match request.key {
        Some(key) => serde_json::to_string(
            &state::get_value(&key)
                .map_err(|err| format!("Failed getting state value for {key}: {err}"))?
                .ok_or_else(|| format!("No value for key '{key}'"))?,
        ),
        None => state::all()
            .map(|map| serde_json::to_string(&map))
            .map_err(|err| format!("Failed getting state: {err}"))?,
    };

    let json_blob = res.map_err(|err| format!("Could not convert value for key to JSON: {err}"))?;

    let response = ServerOriginatedSubMessage::GetLocalStateResponse(GetLocalStateResponse {
        json_blob: Some(json_blob),
    });

    Ok(response.into())
}

pub async fn update(request: UpdateLocalStateRequest) -> RequestResult {
    match (&request.key, request.value) {
        (Some(key), Some(value)) => {
            let value = serde_json::from_str(&value).unwrap_or(serde_json::Value::String(value));
            state::set_value(key, value).map_err(|err| format!("Failed setting {key}: {err}"))?;
        },
        (Some(key), None) => state::remove_value(key).map_err(|err| format!("Failed removing {key}: {err}"))?,
        (None, _) => {
            return RequestResult::error("No key provided with request");
        },
    }

    RequestResult::success()
}
