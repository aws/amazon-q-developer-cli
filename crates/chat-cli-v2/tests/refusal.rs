mod common;

use std::time::Duration;

use chat_cli_v2::api_client::model::ChatResponseStream;
use chat_cli_v2::api_client::send_message_output::MockStreamItem;
use common::AcpTestHarnessBuilder;
use ntest::timeout;
use serial_test::serial;

/// Integration test: a MetadataEvent carrying a content-policy refusal is
/// surfaced to the client via the `_kiro.dev/metadata` ext notification, with
/// the provider explanation and stop reason intact.
#[tokio::test(flavor = "multi_thread")]
#[timeout(30000)]
#[serial]
async fn refusal_metadata_surfaced_via_notification() {
    let (mut harness, client, session_id, _cwd) = AcpTestHarnessBuilder::new("refusal_notification")
        .with_trust_all(true)
        .build_with_session()
        .await;

    // Response stream ends with a MetadataEvent that reports a content-policy
    // refusal (as the upgraded streaming client now delivers via stop_details).
    harness
        .push_mock_response(
            &session_id.0,
            Some(vec![
                MockStreamItem::Event(ChatResponseStream::AssistantResponseEvent {
                    content: "I'm sorry, but I can't help with that.".to_string(),
                }),
                MockStreamItem::Event(ChatResponseStream::MetadataEvent {
                    total_tokens: None,
                    uncached_input_tokens: None,
                    output_tokens: None,
                    cache_read_input_tokens: None,
                    cache_write_input_tokens: None,
                    stop_reason: Some("CONTENT_FILTERED".to_string()),
                    refusal_category: Some("CYBER".to_string()),
                    refusal_explanation: Some("This request was declined by content policy.".to_string()),
                    refusal_recommended_model: Some("kiro-safe".to_string()),
                }),
            ]),
        )
        .await;
    harness.push_mock_response(&session_id.0, None).await;

    let result = client
        .prompt_text(session_id.clone(), "do something disallowed")
        .await
        .expect("prompt should succeed");
    assert_eq!(result.stop_reason, agent_client_protocol::StopReason::EndTurn);
    assert_eq!(
        result
            .meta
            .as_ref()
            .and_then(|meta| meta.get("kiro"))
            .and_then(|kiro| kiro.get("turnFailureReason"))
            .and_then(|reason| reason.as_str()),
        Some("model_error")
    );

    // The refusal must also be surfaced through the metadata ext notification.
    let saw_refusal = client
        .wait_for_timeout(
            |n| {
                n.ext_notifications.iter().any(|ext| {
                    let params = ext.params.get();
                    params.contains("CONTENT_FILTERED")
                        && params.contains("This request was declined by content policy.")
                        && params.contains("kiro-safe")
                })
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        saw_refusal,
        "expected a metadata notification carrying the refusal explanation and stop reason"
    );
}
