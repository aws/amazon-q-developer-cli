use std::sync::LazyLock;

use regex::Regex;

use crate::{
    EventClass,
    MetricRecord,
};

const REDACTOR_NAME: &str = "default";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldClass {
    Prompt,
    Context,
    ToolOutput,
    FileContent,
    HttpHeader,
    Other,
}

impl FieldClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Prompt => "prompt",
            Self::Context => "context",
            Self::ToolOutput => "tool_output",
            Self::FileContent => "file_content",
            Self::HttpHeader => "http_header",
            Self::Other => "other",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PiiType {
    Email,
    AwsAccessKey,
    AwsSecret,
    Arn,
    Ipv4,
    HomePath,
    Phone,
    Jwt,
    CreditCard,
}

impl PiiType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::AwsAccessKey => "aws_access_key",
            Self::AwsSecret => "aws_secret",
            Self::Arn => "arn",
            Self::Ipv4 => "ipv4",
            Self::HomePath => "home_path",
            Self::Phone => "phone",
            Self::Jwt => "jwt",
            Self::CreditCard => "credit_card",
        }
    }

    const fn placeholder(self) -> &'static str {
        match self {
            Self::Email => "[REDACTED:email]",
            Self::AwsAccessKey => "[REDACTED:aws_access_key]",
            Self::AwsSecret => "[REDACTED:aws_secret]",
            Self::Arn => "[REDACTED:arn]",
            Self::Ipv4 => "[REDACTED:ipv4]",
            Self::HomePath => "[REDACTED:home_path]",
            Self::Phone => "[REDACTED:phone]",
            Self::Jwt => "[REDACTED:jwt]",
            Self::CreditCard => "[REDACTED:credit_card]",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedactionResult {
    Scrubbed,
    Passthrough,
}

impl RedactionResult {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Scrubbed => "scrubbed",
            Self::Passthrough => "passthrough",
        }
    }
}

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
    pub fn metric_records(&self, event_class: EventClass, channel: &str) -> Vec<MetricRecord> {
        let mut records = Vec::with_capacity(self.findings.len() + 1);
        records.push(
            MetricRecord::counter("pii_redaction_runs_total", 1)
                .with_attribute("redactor", REDACTOR_NAME)
                .with_attribute("event_class", event_class.as_str())
                .with_attribute("channel", channel)
                .with_attribute("redaction_result", self.result.as_str()),
        );

        records.extend(self.findings.iter().map(|finding| {
            MetricRecord::counter("pii_redaction_matches_total", finding.count)
                .with_attribute("pii_type", finding.pii_type.as_str())
                .with_attribute("field_class", finding.field_class.as_str())
        }));
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

        let records = outcome.metric_records(EventClass::Log, "otel");
        assert_eq!(records[0].name, "pii_redaction_runs_total");
        assert!(
            records[0]
                .attributes
                .iter()
                .any(|attr| attr.key == "redaction_result" && attr.value == "scrubbed")
        );
        assert!(records.iter().any(|record| {
            record.name == "pii_redaction_matches_total"
                && record
                    .attributes
                    .iter()
                    .any(|attr| attr.key == "pii_type" && attr.value == "aws_access_key")
        }));
        assert!(
            records
                .iter()
                .filter(|record| record.name == "pii_redaction_matches_total")
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

        let records = outcome.metric_records(EventClass::Metric, "otel");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].name, "pii_redaction_runs_total");
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
