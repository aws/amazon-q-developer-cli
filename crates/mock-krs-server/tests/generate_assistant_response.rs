//! End-to-end tests for the fake KRS.
//!
//! These speak raw HTTP and decode the response frames themselves, rather than
//! going through a generated client. That is deliberate: the bytes are the
//! contract KAS's TypeScript client consumes, so asserting on them proves the
//! mock is wire-correct instead of proving that one codegen output agrees with
//! another.
//!
//! The request side mirrors what real KAS sends: awsJson1_0, so `POST /` with
//! `x-amz-target: KiroRuntimeService.GenerateAssistantResponse`, a bearer token
//! and the whole input as a JSON document.

use aws_smithy_eventstream::frame::read_message_from;
use bytes::Bytes;
use mock_krs_server::scenario::Turn;
use mock_krs_server::{
    Config,
    GENERATE_ASSISTANT_RESPONSE_TARGET,
    MockKrsServer,
};
use serde_json::{
    Value,
    json,
};

const API_KEY: &str = "test-api-key";

async fn server() -> MockKrsServer {
    MockKrsServer::start(Config {
        api_key: Some(API_KEY.to_string()),
        ..Default::default()
    })
    .await
    .expect("mock KRS starts")
}

fn turns(value: Value) -> Vec<Turn> {
    serde_json::from_value(value).expect("scenario parses")
}

/// One decoded event-stream frame: its smithy member name and JSON payload.
#[derive(Debug)]
struct Frame {
    message_type: String,
    member: String,
    payload: Value,
}

/// A completed call: either a stream of frames, or a failed HTTP response.
#[derive(Debug)]
struct Answer {
    frames: Vec<Frame>,
}

#[derive(Debug)]
struct Failure {
    status: u16,
    body: String,
}

impl Answer {
    /// All `assistantResponseEvent` content, concatenated as a consumer would.
    fn text(&self) -> String {
        self.events("assistantResponseEvent")
            .filter_map(|payload| payload["content"].as_str())
            .collect()
    }

    fn events<'a>(&'a self, member: &'a str) -> impl Iterator<Item = &'a Value> {
        self.frames
            .iter()
            .filter(move |frame| frame.message_type == "event" && frame.member == member)
            .map(|frame| &frame.payload)
    }

    fn exceptions(&self) -> impl Iterator<Item = &Frame> {
        self.frames.iter().filter(|frame| frame.message_type == "exception")
    }

    fn stop_reasons(&self) -> Vec<String> {
        self.events("metadataEvent")
            .filter_map(|payload| payload["stopReason"].as_str())
            .map(str::to_string)
            .collect()
    }
}

/// Sends one user message the way KAS's client would.
async fn ask(server: &MockKrsServer, prompt: &str) -> Result<Answer, Failure> {
    ask_as(server, prompt, API_KEY, None).await
}

async fn ask_as(
    server: &MockKrsServer,
    prompt: &str,
    token: &str,
    agent_mode: Option<&str>,
) -> Result<Answer, Failure> {
    let mut body = json!({
        "conversationState": {
            "chatTriggerType": "MANUAL",
            "currentMessage": {"userInputMessage": {"content": prompt}},
        }
    });
    if let Some(mode) = agent_mode {
        // awsJson1_0 ignores HTTP bindings, so `agentMode` rides in the document
        // rather than the header the model nominally binds it to.
        body["agentMode"] = json!(mode);
    }

    let response = reqwest::Client::new()
        .post(server.endpoint())
        .bearer_auth(token)
        .header("x-amz-target", GENERATE_ASSISTANT_RESPONSE_TARGET)
        .header("content-type", "application/x-amz-json-1.0")
        .json(&body)
        .send()
        .await
        .expect("request reaches the mock");

    let status = response.status().as_u16();
    let raw = response.bytes().await.expect("response body");

    if status != 200 {
        return Err(Failure {
            status,
            body: String::from_utf8_lossy(&raw).into_owned(),
        });
    }

    Ok(Answer {
        frames: decode_frames(raw),
    })
}

fn decode_frames(body: Bytes) -> Vec<Frame> {
    let mut remaining = body;
    let mut frames = Vec::new();
    while !remaining.is_empty() {
        let message = read_message_from(&mut remaining).expect("frame decodes");
        let header = |name: &str| -> Option<String> {
            message
                .headers()
                .iter()
                .find(|header| header.name().as_str() == name)
                .and_then(|header| header.value().as_string().ok())
                .map(|value| value.as_str().to_string())
        };
        let message_type = header(":message-type").expect(":message-type header");
        let member = header(":event-type")
            .or_else(|| header(":exception-type"))
            .unwrap_or_default();
        frames.push(Frame {
            message_type,
            member,
            payload: serde_json::from_slice(&message.payload()[..]).expect("payload is JSON"),
        });
    }
    frames
}

#[tokio::test]
async fn scripted_text_reaches_the_wire_as_modeled_events() {
    let server = server().await;
    server
        .enqueue(turns(json!([{
            "name": "greeting",
            "respond": {"events": [
                {"type": "text", "content": "Hello "},
                {"type": "text", "content": "world"},
            ]},
        }])))
        .await;

    let answer = ask(&server, "hi").await.expect("turn succeeds");
    assert_eq!(answer.text(), "Hello world");

    // The terminal stop reason is supplied even though the scenario omitted it —
    // without it KAS would retry the turn and double the invocation count.
    assert_eq!(answer.stop_reasons(), vec!["END_TURN".to_string()]);

    server.shutdown().await;
}

#[tokio::test]
async fn tool_use_events_survive_the_wire() {
    let server = server().await;
    server
        .enqueue(turns(json!([{
            "respond": {"events": [
                {"type": "text", "content": "Reading it."},
                {"type": "toolUse", "toolUseId": "tu-1", "name": "fs_read", "input": "{\"path\":", "stop": false},
                {"type": "toolUse", "toolUseId": "tu-1", "name": "fs_read", "input": "\"/tmp\"}", "stop": true},
                {"type": "metadata", "stopReason": "TOOL_USE"},
            ]},
        }])))
        .await;

    let answer = ask(&server, "read /tmp").await.expect("turn succeeds");
    let tool_events: Vec<&Value> = answer.events("toolUseEvent").collect();

    assert_eq!(tool_events.len(), 2);
    assert_eq!(tool_events[0]["toolUseId"], "tu-1");
    assert_eq!(tool_events[0]["name"], "fs_read");
    assert_eq!(tool_events[0]["stop"], false);
    // Split input reassembles to valid JSON, which is what the consumer parses.
    let input: String = tool_events
        .iter()
        .filter_map(|payload| payload["input"].as_str())
        .collect();
    assert_eq!(input, r#"{"path":"/tmp"}"#);
    assert_eq!(tool_events[1]["stop"], true);

    assert_eq!(answer.stop_reasons(), vec!["TOOL_USE".to_string()]);

    server.shutdown().await;
}

#[tokio::test]
async fn reasoning_metering_and_context_usage_are_serialized_by_the_sdk() {
    let server = server().await;
    server
        .enqueue(turns(json!([{
            "respond": {"events": [
                {"type": "reasoning", "text": "thinking", "signature": "sig"},
                {"type": "metering", "usage": 1.5, "unit": "credit", "unitPlural": "credits"},
                {"type": "contextUsage", "contextUsagePercentage": 12.5},
                {"type": "metadata", "stopReason": "END_TURN", "tokenUsage": {
                    "uncachedInputTokens": 10, "outputTokens": 4, "totalTokens": 14,
                }},
            ]},
        }])))
        .await;

    let answer = ask(&server, "think").await.expect("turn succeeds");

    let reasoning = answer.events("reasoningContentEvent").next().expect("reasoning event");
    assert_eq!(reasoning["text"], "thinking");
    assert_eq!(reasoning["signature"], "sig");

    let metering = answer.events("meteringEvent").next().expect("metering event");
    assert_eq!(metering["usage"], 1.5);
    assert_eq!(metering["unit"], "credit");
    // Member names come from the model, so this is the casing KAS receives.
    assert_eq!(metering["unitPlural"], "credits");

    let context = answer.events("contextUsageEvent").next().expect("context usage event");
    assert_eq!(context["contextUsagePercentage"], 12.5);

    let usage = answer
        .events("metadataEvent")
        .find_map(|payload| payload.get("tokenUsage"))
        .expect("token usage");
    assert_eq!(usage["totalTokens"], 14);
    assert_eq!(usage["uncachedInputTokens"], 10);

    server.shutdown().await;
}

#[tokio::test]
async fn matchers_bind_responses_to_requests_regardless_of_order() {
    let server = server().await;
    server
        .enqueue(turns(json!([
            {"name": "second", "match": {"userInputContains": "goodbye"},
             "respond": {"events": [{"type": "text", "content": "bye"}]}},
            {"name": "first", "match": {"userInputContains": "hello"},
             "respond": {"events": [{"type": "text", "content": "hi"}]}},
        ])))
        .await;

    // Queued in the opposite order to the calls: the matcher decides, not FIFO.
    assert_eq!(ask(&server, "hello there").await.unwrap().text(), "hi");
    assert_eq!(ask(&server, "goodbye now").await.unwrap().text(), "bye");

    let captured = server.requests().await;
    assert_eq!(captured.len(), 2);
    assert_eq!(captured[0].matched_turn.as_deref(), Some("first"));
    assert_eq!(captured[1].matched_turn.as_deref(), Some("second"));
    assert_eq!(captured[0].user_input(), Some("hello there"));

    server.shutdown().await;
}

#[tokio::test]
async fn agent_mode_is_matched_from_the_modeled_member() {
    // Matching the modeled input means the matcher reads `agentMode` wherever the
    // protocol puts it, rather than guessing at a JSON pointer.
    let server = server().await;
    server
        .enqueue(turns(json!([{
            "name": "spec-mode", "match": {"agentMode": "spec"},
            "respond": {"events": [{"type": "text", "content": "speccing"}]},
        }])))
        .await;

    let answer = ask_as(&server, "plan this", API_KEY, Some("spec"))
        .await
        .expect("turn succeeds");
    assert_eq!(answer.text(), "speccing");

    server.shutdown().await;
}

#[tokio::test]
async fn turns_are_consumed_unless_marked_sticky() {
    let server = server().await;
    server
        .enqueue(turns(json!([
            {"name": "sticky", "times": 0, "match": {"userInputContains": "ping"},
             "respond": {"events": [{"type": "text", "content": "pong"}]}},
        ])))
        .await;

    for _ in 0..3 {
        assert_eq!(ask(&server, "ping").await.unwrap().text(), "pong");
    }
    assert_eq!(server.state().await.queued_turns, vec!["sticky".to_string()]);

    server.shutdown().await;
}

#[tokio::test]
async fn an_unscripted_call_fails_loudly() {
    // The whole value of the mock in a version-bump gate is that a mis-scripted
    // test cannot quietly pass.
    let server = server().await;
    let failure = ask(&server, "nothing is queued").await.expect_err("must fail");
    assert_eq!(failure.status, 500);
    assert!(failure.body.contains("no scripted response"), "{failure:?}");

    let state = server.state().await;
    assert_eq!(state.calls, 1);
    assert_eq!(state.unmatched_calls, 1);
    // The unmatched request is still captured, so the test can show what was sent.
    assert_eq!(server.requests().await[0].user_input(), Some("nothing is queued"));

    server.shutdown().await;
}

#[tokio::test]
async fn injected_stream_exception_is_framed_as_the_modeled_member() {
    let server = server().await;
    server
        .enqueue(turns(json!([{
            "respond": {
                "events": [{"type": "text", "content": "partial"}],
                "streamError": {"kind": "throttling", "message": "slow down"},
            },
        }])))
        .await;

    let answer = ask(&server, "hi").await.expect("the response itself is a 200");
    // Partial content is delivered before the failure, which is the shape KAS's
    // stream error handling has to cope with.
    assert_eq!(answer.text(), "partial");

    let exception = answer.exceptions().next().expect("exception frame");
    // The member name is the model's, not this crate's.
    assert_eq!(exception.member, "throttlingError");
    assert_eq!(exception.payload["message"], "slow down");
    assert!(answer.stop_reasons().is_empty(), "a failed stream has no stop reason");

    server.shutdown().await;
}

#[tokio::test]
async fn injected_http_error_uses_the_status_and_code_from_the_model() {
    let server = server().await;
    server
        .enqueue(turns(json!([{
            "respond": {"httpError": {"kind": "throttling", "message": "too many requests"}},
        }])))
        .await;

    let failure = ask(&server, "hi").await.expect_err("call fails");
    // 429 is the model's status for this error; the scenario never names one.
    assert_eq!(failure.status, 429);
    assert!(failure.body.contains("too many requests"), "{failure:?}");

    server.shutdown().await;
}

#[tokio::test]
async fn truncated_stream_ends_without_a_stop_reason() {
    // The opt-in shape that exercises KAS's stream-recovery retry.
    let server = server().await;
    server
        .enqueue(turns(json!([{
            "respond": {"events": [{"type": "text", "content": "cut off"}], "truncate": true},
        }])))
        .await;

    let answer = ask(&server, "hi").await.expect("stream completes");
    assert_eq!(answer.text(), "cut off");
    assert!(
        answer.events("metadataEvent").next().is_none(),
        "truncate must not synthesize a terminal metadata event"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn a_wrong_bearer_token_is_rejected() {
    let server = server().await;
    let failure = ask_as(&server, "hi", "not-the-key", None)
        .await
        .expect_err("auth fails");
    assert_eq!(failure.status, 403);
    assert!(failure.body.contains("api key"), "{failure:?}");

    server.shutdown().await;
}

#[tokio::test]
async fn a_request_without_a_user_message_is_rejected() {
    let server = server().await;
    let response = reqwest::Client::new()
        .post(server.endpoint())
        .bearer_auth(API_KEY)
        .header("x-amz-target", GENERATE_ASSISTANT_RESPONSE_TARGET)
        .header("content-type", "application/x-amz-json-1.0")
        .json(&json!({"conversationState": {"chatTriggerType": "MANUAL"}}))
        .send()
        .await
        .expect("request reaches the mock");

    assert_eq!(response.status().as_u16(), 400);
    let body = response.text().await.expect("body");
    assert!(body.contains("userInputMessage"), "{body}");

    server.shutdown().await;
}

#[tokio::test]
async fn an_unknown_target_gets_the_sdks_own_answer() {
    // Mounting the generated service as the fallback means an operation KRS does
    // not model is refused by the SDK's protocol rules, not by an axum 404 that a
    // client would report as an unknown transport failure.
    let server = server().await;
    let response = reqwest::Client::new()
        .post(server.endpoint())
        .bearer_auth(API_KEY)
        .header("x-amz-target", "KiroRuntimeService.SomethingElse")
        .header("content-type", "application/x-amz-json-1.0")
        .json(&json!({}))
        .send()
        .await
        .expect("request reaches the mock");

    assert_eq!(response.status().as_u16(), 404);

    server.shutdown().await;
}

#[tokio::test]
async fn reset_clears_the_queue_and_the_capture_log() {
    let server = server().await;
    server
        .enqueue(turns(
            json!([{"respond": {"events": [{"type": "text", "content": "hi"}]}}]),
        ))
        .await;
    ask(&server, "hello").await.expect("turn succeeds");

    server.reset().await;

    let state = server.state().await;
    assert!(state.queued_turns.is_empty());
    assert_eq!(state.calls, 0);
    assert!(server.requests().await.is_empty());

    server.shutdown().await;
}
