pub trait IntoMetricDatum: Send {
    fn into_metric_datum(self) -> amzn_toolkit_telemetry::types::MetricDatum;
}

include!(concat!(env!("OUT_DIR"), "/mod.rs"));

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::*;
    use crate::metrics::CodewhispererterminalCompletionInserted;

    #[test]
    fn test_serde() {
        let metric_datum_init =
            Metric::CodewhispererterminalCompletionInserted(CodewhispererterminalCompletionInserted {
                create_time: Some(SystemTime::now()),
                value: None,
                credential_start_url: Some("https://example.com".to_owned().into()),
                codewhispererterminal_terminal: Some("vscode".to_owned().into()),
                codewhispererterminal_terminal_version: Some("1.2.3".to_owned().into()),
                codewhispererterminal_shell: Some("zsh".to_owned().into()),
                codewhispererterminal_shell_version: Some("4.5.6".to_owned().into()),
                codewhispererterminal_command: Some("git".to_owned().into()),
                codewhispererterminal_duration: Some(123.into()),
                codewhispererterminal_in_cloudshell: Some(false.into()),
            });

        let s = serde_json::to_string_pretty(&metric_datum_init).unwrap();
        println!("{s}");

        let metric_datum_out: Metric = serde_json::from_str(&s).unwrap();
        println!("{metric_datum_out:#?}");

        assert_eq!(metric_datum_init, metric_datum_out);
    }
}
