#[cfg(test)]
mod tests {
    use crate::mcp_client::{
        SamplingMessage, SamplingContent, SamplingRequest, ModelPreferences, ModelHint,
    };

    #[tokio::test]
    async fn test_sampling_request_creation() {
        let request = SamplingRequest {
            messages: vec![
                SamplingMessage {
                    role: "user".to_string(),
                    content: SamplingContent::Text {
                        text: "What is the capital of France?".to_string(),
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
            system_prompt: Some("You are a helpful assistant.".to_string()),
            max_tokens: Some(100),
            include_context: Some("thisServer".to_string()),
            temperature: Some(0.7),
            stop_sequences: Some(vec!["END".to_string()]),
            metadata: Some(serde_json::json!({"test": true})),
        };

        // Verify the request can be serialized
        let json = serde_json::to_string(&request).expect("Failed to serialize sampling request");
        assert!(json.contains("What is the capital of France?"));
        assert!(json.contains("claude-3-sonnet"));
    }

    #[tokio::test]
    async fn test_sampling_content_types() {
        // Test text content
        let text_content = SamplingContent::Text {
            text: "Hello world".to_string(),
        };
        let json = serde_json::to_string(&text_content).expect("Failed to serialize text content");
        assert!(json.contains("text"));
        assert!(json.contains("Hello world"));

        // Test image content
        let image_content = SamplingContent::Image {
            data: "base64-encoded-data".to_string(),
            mime_type: "image/jpeg".to_string(),
        };
        let json = serde_json::to_string(&image_content).expect("Failed to serialize image content");
        assert!(json.contains("image"));
        assert!(json.contains("base64-encoded-data"));
        assert!(json.contains("image/jpeg"));
    }

    #[tokio::test]
    async fn test_model_preferences() {
        let preferences = ModelPreferences {
            hints: Some(vec![
                ModelHint { name: "claude-3-sonnet".to_string() },
                ModelHint { name: "claude".to_string() },
            ]),
            cost_priority: Some(0.3),
            speed_priority: Some(0.8),
            intelligence_priority: Some(0.5),
        };

        let json = serde_json::to_string(&preferences).expect("Failed to serialize preferences");
        assert!(json.contains("claude-3-sonnet"));
        assert!(json.contains("0.3"));
        assert!(json.contains("0.8"));
        assert!(json.contains("0.5"));

        // Test deserialization
        let deserialized: ModelPreferences = serde_json::from_str(&json)
            .expect("Failed to deserialize preferences");
        assert_eq!(deserialized.hints.as_ref().unwrap().len(), 2);
        assert_eq!(deserialized.cost_priority, Some(0.3));
    }

    #[tokio::test]
    async fn test_mcp_specification_fields() {
        // Test complete SamplingRequest with all MCP specification fields
        let request = SamplingRequest {
            messages: vec![
                SamplingMessage {
                    role: "user".to_string(),
                    content: SamplingContent::Text {
                        text: "Test message".to_string(),
                    },
                }
            ],
            model_preferences: Some(ModelPreferences {
                hints: Some(vec![ModelHint { name: "claude-3".to_string() }]),
                cost_priority: Some(0.2),
                speed_priority: Some(0.8),
                intelligence_priority: Some(0.9),
            }),
            system_prompt: Some("You are a test assistant.".to_string()),
            include_context: Some("thisServer".to_string()),
            temperature: Some(0.7),
            max_tokens: Some(200),
            stop_sequences: Some(vec!["STOP".to_string(), "END".to_string()]),
            metadata: Some(serde_json::json!({
                "test_id": "mcp_spec_test",
                "version": "1.0"
            })),
        };

        // Test serialization
        let json = serde_json::to_string(&request).expect("Failed to serialize complete request");
        assert!(json.contains("thisServer"));
        assert!(json.contains("0.7"));
        assert!(json.contains("STOP"));
        assert!(json.contains("test_id"));

        // Test deserialization
        let deserialized: SamplingRequest = serde_json::from_str(&json)
            .expect("Failed to deserialize complete request");

        assert_eq!(deserialized.include_context, Some("thisServer".to_string()));
        assert_eq!(deserialized.temperature, Some(0.7));
        assert_eq!(deserialized.max_tokens, Some(200));
        assert_eq!(deserialized.stop_sequences.as_ref().unwrap().len(), 2);
        assert!(deserialized.metadata.is_some());

        // Verify metadata content
        let metadata = deserialized.metadata.unwrap();
        assert_eq!(metadata["test_id"], "mcp_spec_test");
        assert_eq!(metadata["version"], "1.0");
    }

    #[tokio::test]
    async fn test_include_context_validation() {
        // Test valid includeContext values
        let valid_contexts = vec!["none", "thisServer", "allServers"];

        for context in valid_contexts {
            let request = SamplingRequest {
                messages: vec![
                    SamplingMessage {
                        role: "user".to_string(),
                        content: SamplingContent::Text {
                            text: "Test".to_string(),
                        },
                    }
                ],
                model_preferences: None,
                system_prompt: None,
                include_context: Some(context.to_string()),
                temperature: None,
                max_tokens: None,
                stop_sequences: None,
                metadata: None,
            };

            // Should serialize and deserialize successfully
            let json = serde_json::to_string(&request).expect("Failed to serialize");
            let _: SamplingRequest = serde_json::from_str(&json).expect("Failed to deserialize");
        }
    }

    #[tokio::test]
    async fn test_pending_sampling_request() {
        use crate::mcp_client::sampling_ipc::{PendingSamplingRequest, SamplingApprovalResult};

        // Create a oneshot channel for testing
        let (sender, receiver) = tokio::sync::oneshot::channel();

        let mut pending_request = PendingSamplingRequest::new(
            "test-server".to_string(),
            "What is 2+2?".to_string(),
            Some("You are a math assistant.".to_string()),
            Some(ModelPreferences {
                hints: Some(vec![ModelHint { name: "claude-3".to_string() }]),
                cost_priority: Some(0.5),
                speed_priority: Some(0.5),
                intelligence_priority: Some(0.8),
            }),
            Some(50),
            sender,
        );

        // Test the description generation
        let description = pending_request.get_description();
        assert!(description.contains("test-server"));
        assert!(description.contains("What is 2+2?"));
        assert!(description.contains("You are a math assistant"));
        assert!(description.contains("Max tokens: 50"));

        // Test sending approval result
        pending_request.send_approval_result(SamplingApprovalResult::approved());

        // Verify the result was received
        let result = receiver.await.expect("Failed to receive approval result");
        assert!(result.approved);
        assert!(result.error_message.is_none());
    }

    #[tokio::test]
    async fn test_sampling_approval_result() {
        use crate::mcp_client::sampling_ipc::SamplingApprovalResult;

        // Test approved result
        let approved = SamplingApprovalResult::approved();
        assert!(approved.approved);
        assert!(approved.error_message.is_none());

        // Test rejected result
        let rejected = SamplingApprovalResult::rejected("User declined".to_string());
        assert!(!rejected.approved);
        assert_eq!(rejected.error_message, Some("User declined".to_string()));
    }
}
