use fig_proto::fig::{
    TelemetryPageRequest,
    TelemetryTrackRequest,
};
use serde_json::{
    Map,
    Value,
};

use super::{
    RequestResult,
    RequestResultImpl,
};

pub async fn handle_track_request(request: TelemetryTrackRequest) -> RequestResult {
    let event: String = request.event.ok_or_else(|| "Empty track event".to_string())?;

    #[allow(deprecated)]
    let mut properties: Map<String, Value> = request
        .properties
        .iter()
        .map(|p| (p.key.as_str().into(), p.value.as_str().into()))
        .collect();

    if let Some(ref json_blob) = request.json_blob {
        match serde_json::from_str::<serde_json::Map<String, Value>>(json_blob) {
            Ok(props) => {
                properties.extend(props);
            },
            Err(err) => return Err(format!("Failed to decode json blob: {err}").into()),
        }
    }

    // TODO(chay): send directly from autocomplete
    if event == "autocomplete-insert" {
        if let Some(root_command) = properties.get("rootCommand").and_then(|r| r.as_str()) {
            let terminal = properties
                .get("terminal")
                .and_then(|t| t.as_str().map(ToString::to_string));
            let shell = properties
                .get("shell")
                .and_then(|t| t.as_str().map(ToString::to_string));

            fig_telemetry::send_completion_inserted(root_command.to_owned(), terminal, shell).await;
        }
    }

    RequestResult::success()
}

pub async fn handle_page_request(request: TelemetryPageRequest) -> RequestResult {
    let properties = if let Some(ref json_blob) = request.json_blob {
        match serde_json::from_str::<serde_json::Map<String, Value>>(json_blob) {
            Ok(props) => props,
            Err(err) => return Err(format!("Failed to decode json blob: {err}").into()),
        }
    } else {
        Map::new()
    };

    if let Some(pathname) = properties.get("pathname").and_then(|s| s.as_str()) {
        fig_telemetry::send_dashboard_page_viewed(pathname).await;
    }

    RequestResult::success()
}
