#![allow(dead_code)]

// https://github.com/aws/aws-toolkit-common/blob/main/telemetry/telemetryformat.md

pub trait IntoMetricDatum: Send {
    fn into_metric_datum(self) -> amzn_toolkit_telemetry_client::types::MetricDatum;
}

include!(concat!(env!("OUT_DIR"), "/mod.rs"));

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::*;
    use crate::telemetry::core::{
        ChatConversationType,
        MessageMetaTag,
    };
    use crate::telemetry::definitions::metrics::CodewhispererterminalAddChatMessage;

    #[test]
    fn test_serde() {
        let metric_datum_init = Metric::CodewhispererterminalAddChatMessage(CodewhispererterminalAddChatMessage {
            amazonq_conversation_id: None,
            request_id: None,
            codewhispererterminal_context_file_length: None,
            create_time: Some(SystemTime::now()),
            value: None,
            credential_start_url: Some("https://example.com".to_owned().into()),
            sso_region: Some("us-east-1".to_owned().into()),
            codewhispererterminal_in_cloudshell: None,
            codewhispererterminal_utterance_id: Some("message_id".to_owned().into()),
            result: crate::telemetry::definitions::types::Result::new("Succeeded".to_string()),
            reason: None,
            reason_desc: None,
            status_code: None,
            codewhispererterminal_model: None,
            codewhispererterminal_time_to_first_chunks_ms: Some(40.to_string().into()),
            codewhispererterminal_time_between_chunks_ms: Some("1,2,3".to_string().into()),
            codewhispererterminal_chat_conversation_type: Some(ChatConversationType::NotToolUse.into()),
            codewhispererterminal_tool_use_id: None,
            codewhispererterminal_tool_name: None,
            codewhispererterminal_assistant_response_length: Some(20.into()),
            codewhispererterminal_chat_message_meta_tags: Some([MessageMetaTag::Compact.to_string()].join(",").into()),
        });

        let s = serde_json::to_string_pretty(&metric_datum_init).unwrap();
        println!("{s}");

        let metric_datum_out: Metric = serde_json::from_str(&s).unwrap();
        println!("{metric_datum_out:#?}");

        assert_eq!(metric_datum_init, metric_datum_out);
    }
}
