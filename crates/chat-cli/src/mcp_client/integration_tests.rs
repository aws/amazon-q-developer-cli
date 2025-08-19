#[cfg(test)]
mod integration_tests {
    use crate::mcp_client::{
        SamplingMessage, SamplingContent, SamplingRequest, ModelPreferences, ModelHint,
    };

    #[tokio::test]
    async fn test_sampling_request_serialization() {
        let sampling_request = SamplingRequest {
            messages: vec![
                SamplingMessage {
                    role: "user".to_string(),
                    content: SamplingContent::Text {
                        text: "What is the weather like today?".to_string(),
                    },
                }
            ],
            model_preferences: Some(ModelPreferences {
                hints: Some(vec![
                    ModelHint { name: "claude-3-sonnet".to_string() }
                ]),
                cost_priority: Some(0.3),
                speed_priority: Some(0.8),
                intelligence_priority: Some(0.5),
            }),
            system_prompt: Some("You are a helpful weather assistant.".to_string()),
            max_tokens: Some(150),
            include_context: Some("thisServer".to_string()),
            temperature: Some(0.7),
            stop_sequences: Some(vec!["END".to_string()]),
            metadata: Some(serde_json::json!({
                "request_id": "weather_test_001",
                "priority": "normal"
            })),
        };

        // Test serialization/deserialization
        let json = serde_json::to_string(&sampling_request).expect("Failed to serialize");
        let deserialized: SamplingRequest = serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(deserialized.messages.len(), 1);
        assert_eq!(deserialized.messages[0].role, "user");
        assert!(matches!(deserialized.messages[0].content, SamplingContent::Text { .. }));
        assert_eq!(deserialized.max_tokens, Some(150));
        assert_eq!(deserialized.temperature, Some(0.7));
        assert!(deserialized.metadata.is_some());
    }

    #[tokio::test]
    async fn test_error_handling() {
        // Test JSON parsing errors
        let invalid_json = r#"{"invalid": json}"#;
        let result: Result<SamplingRequest, _> = serde_json::from_str(invalid_json);
        assert!(result.is_err());

        // Test missing required fields
        let incomplete_json = r#"{"messages": []}"#;
        let result: Result<SamplingRequest, _> = serde_json::from_str(incomplete_json);
        // Should handle gracefully (messages can be empty)
        assert!(result.is_ok());

        // Test invalid content types
        let invalid_content = r#"{"type": "invalid", "data": "test"}"#;
        let result: Result<SamplingContent, _> = serde_json::from_str(invalid_content);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_chat_based_sampling_workflow() {
        use crate::mcp_client::sampling_ipc::PendingSamplingRequest;

        // Test the new chat-based sampling workflow components
        let (approval_sender, approval_receiver) = tokio::sync::oneshot::channel();

        // Create a pending sampling request
        let pending_request = PendingSamplingRequest::new(
            "test-server".to_string(),
            "What is the meaning of life?".to_string(),
            Some("You are a philosophical assistant.".to_string()),
            Some(ModelPreferences {
                hints: Some(vec![ModelHint { name: "claude-3-sonnet".to_string() }]),
                cost_priority: Some(0.5),
                speed_priority: Some(0.5),
                intelligence_priority: Some(0.9),
            }),
            Some(200),
            approval_sender,
        );

        // Verify the request structure
        assert_eq!(pending_request.server_name, "test-server");
        assert!(pending_request.prompt_content.contains("meaning of life"));
        assert_eq!(pending_request.max_tokens, Some(200));

        // Test description generation
        let description = pending_request.get_description();
        assert!(description.contains("test-server"));
        assert!(description.contains("meaning of life"));
        assert!(description.contains("philosophical assistant"));
        assert!(description.contains("Max tokens: 200"));

        // Simulate approval workflow
        tokio::spawn(async move {
            // Simulate user approval after a short delay
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            let _ = approval_receiver.await;
        });

        // In a real scenario, the chat session would send the approval result
        // For this test, we just verify the channel works
        assert!(pending_request.response_sender.is_some());
    }

    #[tokio::test]
    async fn test_concurrent_sampling_requests() {
        use crate::mcp_client::sampling_ipc::{PendingSamplingRequest, SamplingApprovalResult};

        // Test handling multiple concurrent sampling requests
        let mut handles = Vec::new();

        for i in 0..3 {
            let handle = tokio::spawn(async move {
                let (sender, receiver) = tokio::sync::oneshot::channel();

                let mut pending_request = PendingSamplingRequest::new(
                    format!("test-server-{}", i),
                    format!("Question number {}", i),
                    Some("You are a test assistant.".to_string()),
                    None,
                    Some(100),
                    sender,
                );

                // Simulate approval
                pending_request.send_approval_result(SamplingApprovalResult::approved());

                // Verify the result
                let result = receiver.await.expect("Failed to receive approval");
                assert!(result.approved);

                i
            });

            handles.push(handle);
        }

        // Wait for all requests to complete
        let results = futures::future::join_all(handles).await;

        // Verify all requests completed successfully
        for (i, result) in results.into_iter().enumerate() {
            assert_eq!(result.unwrap(), i);
        }
    }
}
