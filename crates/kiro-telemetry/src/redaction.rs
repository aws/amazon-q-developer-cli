use std::sync::LazyLock;

use regex::Regex;

use crate::MetricRecord;
use crate::metric::{
    self,
    EventClass,
    FieldClass,
    PiiType,
    RedactionResult,
    Redactor,
    TelemetryChannel,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedactionFinding {
    pub pii_type: PiiType,
    pub field_class: FieldClass,
    pub count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedactionOutcome {
    pub text: String,
    pub result: RedactionResult,
    pub findings: Vec<RedactionFinding>,
}

impl RedactionOutcome {
    pub fn metric_records(&self, event_class: EventClass, channel: TelemetryChannel) -> Vec<MetricRecord> {
        let mut records = Vec::with_capacity(self.findings.len() + 1);
        records.push(metric::pii_redaction_run(
            Redactor::Default,
            event_class,
            channel,
            self.result,
        ));

        records.extend(
            self.findings
                .iter()
                .map(|finding| metric::pii_redaction_match(finding.count, finding.pii_type, finding.field_class)),
        );
        records
    }
}

#[derive(Clone, Debug, Default)]
pub struct PiiRedactor;

impl PiiRedactor {
    pub fn redact(&self, field_class: FieldClass, text: &str) -> RedactionOutcome {
        let mut redacted = text.to_string();
        let mut findings = Vec::new();

        for pattern in PATTERNS.iter() {
            let count = pattern.regex.find_iter(&redacted).count() as u64;
            if count == 0 {
                continue;
            }
            findings.push(RedactionFinding {
                pii_type: pattern.pii_type,
                field_class,
                count,
            });
            redacted = pattern
                .regex
                .replace_all(&redacted, pattern.pii_type.placeholder())
                .into_owned();
        }

        let result = if findings.is_empty() {
            RedactionResult::Passthrough
        } else {
            RedactionResult::Scrubbed
        };

        RedactionOutcome {
            text: redacted,
            result,
            findings,
        }
    }
}

struct Pattern {
    pii_type: PiiType,
    regex: Regex,
}

static PATTERNS: LazyLock<Vec<Pattern>> = LazyLock::new(|| {
    vec![
        Pattern {
            pii_type: PiiType::Jwt,
            regex: Regex::new(r"\beyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b").unwrap(),
        },
        Pattern {
            pii_type: PiiType::Email,
            regex: Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b").unwrap(),
        },
        Pattern {
            pii_type: PiiType::AwsAccessKey,
            regex: Regex::new(r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b").unwrap(),
        },
        Pattern {
            pii_type: PiiType::AwsSecret,
            regex: Regex::new(
                r#"(?i)\b(?:aws_)?(?:secret|secret_access_key|aws_secret_access_key)\s*[:=]\s*["']?[A-Za-z0-9/+=]{40}["']?"#,
            )
            .unwrap(),
        },
        Pattern {
            pii_type: PiiType::Arn,
            regex: Regex::new(r#"\barn:(?:aws|aws-us-gov|aws-cn):[A-Za-z0-9-]+:[A-Za-z0-9-]*:\d{12}:[^\s,"')]+"#)
                .unwrap(),
        },
        Pattern {
            pii_type: PiiType::HomePath,
            regex: Regex::new(r#"(?:/Users|/home)/[A-Za-z0-9._-]+(?:/[^\s,'"()]*)?|[A-Za-z]:\\Users\\[A-Za-z0-9._-]+(?:\\[^\s,'"()]*)?"#)
                .unwrap(),
        },
        Pattern {
            pii_type: PiiType::CreditCard,
            regex: Regex::new(r"\b(?:\d[ -]*?){13,19}\b").unwrap(),
        },
        Pattern {
            pii_type: PiiType::Phone,
            regex: Regex::new(r"\+?\d[\d .()-]{8,}\d").unwrap(),
        },
        Pattern {
            pii_type: PiiType::Ipv4,
            regex: Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").unwrap(),
        },
    ]
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrubs_known_pii_and_reports_match_metrics() {
        let outcome = PiiRedactor.redact(
            FieldClass::Prompt,
            "email dev@example.com key AKIA1234567890ABCDEF arn arn:aws:iam::123456789012:user/test",
        );

        assert_eq!(outcome.result, RedactionResult::Scrubbed);
        assert!(!outcome.text.contains("dev@example.com"));
        assert!(!outcome.text.contains("AKIA1234567890ABCDEF"));
        assert!(!outcome.text.contains("arn:aws:iam::123456789012:user/test"));
        assert!(outcome.text.contains("[REDACTED:email]"));
        assert!(outcome.text.contains("[REDACTED:aws_access_key]"));
        assert!(outcome.text.contains("[REDACTED:arn]"));

        let records = outcome.metric_records(EventClass::Log, TelemetryChannel::Otel);
        assert_eq!(records[0].name, "kiro_cli_pii_redaction_runs_total");
        assert!(
            records[0]
                .attributes
                .iter()
                .any(|attr| attr.key == "redaction_result" && attr.value == "scrubbed")
        );
        assert!(records.iter().any(|record| {
            record.name == "kiro_cli_pii_redaction_matches_total"
                && record
                    .attributes
                    .iter()
                    .any(|attr| attr.key == "pii_type" && attr.value == "aws_access_key")
        }));
        assert!(
            records
                .iter()
                .filter(|record| record.name == "kiro_cli_pii_redaction_matches_total")
                .all(|record| record
                    .attributes
                    .iter()
                    .any(|attr| attr.key == "field_class" && attr.value == "prompt"))
        );
    }

    #[test]
    fn passthrough_still_counts_redaction_run() {
        let outcome = PiiRedactor.redact(FieldClass::Context, "ordinary bounded telemetry value");

        assert_eq!(outcome.result, RedactionResult::Passthrough);
        assert_eq!(outcome.text, "ordinary bounded telemetry value");
        assert!(outcome.findings.is_empty());

        let records = outcome.metric_records(EventClass::Metric, TelemetryChannel::Otel);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].name, "kiro_cli_pii_redaction_runs_total");
        assert!(
            records[0]
                .attributes
                .iter()
                .any(|attr| attr.key == "redaction_result" && attr.value == "passthrough")
        );
        assert!(
            records[0]
                .attributes
                .iter()
                .any(|attr| attr.key == "event_class" && attr.value == "metric")
        );
    }
}
