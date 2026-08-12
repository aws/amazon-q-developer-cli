//! Tests for the control plane — the surface a non-Rust harness uses, so it is
//! exercised over HTTP rather than through the in-process helpers.

use mock_krs_server::{
    Config,
    MockKrsServer,
};
use serde_json::{
    Value,
    json,
};

async fn server() -> MockKrsServer {
    MockKrsServer::start(Config::default()).await.expect("mock KRS starts")
}

async fn post(url: String, body: Value) -> (u16, Value) {
    let response = reqwest::Client::new()
        .post(url)
        .json(&body)
        .send()
        .await
        .expect("request");
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

async fn get(url: String) -> (u16, Value) {
    let response = reqwest::Client::new().get(url).send().await.expect("request");
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

#[tokio::test]
async fn health_reports_the_queue_depth() {
    let server = server().await;
    let (status, body) = get(format!("{}/__control/health", server.endpoint())).await;
    assert_eq!(status, 200);
    assert_eq!(body["ok"], true);
    assert_eq!(body["queuedTurns"], 0);
    server.shutdown().await;
}

#[tokio::test]
async fn scenarios_can_be_injected_over_http_and_read_back() {
    let server = server().await;
    let (status, body) = post(
        format!("{}/__control/scenarios", server.endpoint()),
        json!({"turns": [
            {"name": "one", "respond": {"events": [{"type": "text", "content": "hi"}]}},
            {"name": "two", "respond": {"events": [{"type": "text", "content": "there"}]}},
        ]}),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["queued"], 2);

    let (_, state) = get(format!("{}/__control/state", server.endpoint())).await;
    assert_eq!(state["queuedTurns"], json!(["one", "two"]));

    server.shutdown().await;
}

#[tokio::test]
async fn a_malformed_scenario_is_rejected_with_the_offending_field() {
    // Injection time is the only place a scripting typo can be reported
    // usefully; later it is just a turn that never gets answered.
    let server = server().await;
    let (status, body) = post(
        format!("{}/__control/scenarios", server.endpoint()),
        json!([{"respond": {"evnets": []}}]),
    )
    .await;
    assert_eq!(status, 400);
    assert!(
        body["error"].as_str().unwrap_or_default().contains("evnets"),
        "error should name the bad field: {body}"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn requests_are_captured_with_their_body_and_safe_headers() {
    let server = server().await;
    post(
        format!("{}/__control/scenarios", server.endpoint()),
        json!([{"respond": {"events": [{"type": "text", "content": "hi"}]}}]),
    )
    .await;

    let response = reqwest::Client::new()
        .post(server.endpoint())
        .header("authorization", "Bearer any-token")
        .header("x-amz-target", mock_krs_server::GENERATE_ASSISTANT_RESPONSE_TARGET)
        .header("content-type", "application/x-amz-json-1.0")
        .body(
            json!({"conversationState": {"currentMessage": {"userInputMessage": {"content": "captured?"}}}})
                .to_string(),
        )
        .send()
        .await
        .expect("request");
    assert_eq!(response.status().as_u16(), 200);

    let (_, body) = get(format!("{}/__control/requests", server.endpoint())).await;
    let captured = &body["requests"][0];
    // awsJson1_0 posts every operation to the root; the target names the
    // operation, so it is worth capturing.
    assert_eq!(captured["path"], "/");
    let headers = captured["headers"].to_string();
    assert!(
        headers.contains(mock_krs_server::GENERATE_ASSISTANT_RESPONSE_TARGET),
        "{headers}"
    );
    assert_eq!(
        captured["body"]["conversationState"]["currentMessage"]["userInputMessage"]["content"],
        "captured?"
    );
    // The bearer value must never be persisted in the capture log.
    assert!(headers.contains("<redacted>"), "{headers}");
    assert!(!headers.contains("any-token"), "{headers}");

    server.shutdown().await;
}

#[tokio::test]
async fn the_modeled_operation_answers_with_an_event_stream() {
    let server = server().await;
    post(
        format!("{}/__control/scenarios", server.endpoint()),
        json!([{"respond": {"events": [{"type": "text", "content": "hi"}]}}]),
    )
    .await;

    let response = reqwest::Client::new()
        .post(server.endpoint())
        .header("authorization", "Bearer any-token")
        .header("x-amz-target", mock_krs_server::GENERATE_ASSISTANT_RESPONSE_TARGET)
        .header("content-type", "application/x-amz-json-1.0")
        .body(
            json!({"conversationState": {"currentMessage": {"userInputMessage": {"content": "via target"}}}})
                .to_string(),
        )
        .send()
        .await
        .expect("request");

    assert_eq!(response.status().as_u16(), 200);
    // The content type is the SDK's: awsJson1_0 labels the response with the
    // protocol's own media type even though the body is event-stream frames.
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("application/x-amz-json-1.0")
    );

    server.shutdown().await;
}
