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
    use crate::fig_telemetry::definitions::metrics::CodewhispererterminalAddChatMessage;

    #[test]
    fn test_serde() {
        let metric_datum_init = Metric::CodewhispererterminalAddChatMessage(CodewhispererterminalAddChatMessage {
            amazonq_conversation_id: None,
            codewhispererterminal_context_file_length: None,
            create_time: Some(SystemTime::now()),
            value: None,
            credential_start_url: Some("https://example.com".to_owned().into()),
            codewhispererterminal_in_cloudshell: Some(false.into()),
        });

        let s = serde_json::to_string_pretty(&metric_datum_init).unwrap();
        println!("{s}");

        let metric_datum_out: Metric = serde_json::from_str(&s).unwrap();
        println!("{metric_datum_out:#?}");

        assert_eq!(metric_datum_init, metric_datum_out);
    }
}
