//! Schema validation for metric and log records.
//!
//! The CLI no longer enforces a runtime cardinality budget — high-cardinality
//! enforcement (per-series caps, "_other_" overflow bucketing) is handled by
//! the OTel collector via `transform`/`filter` processors. What remains here
//! is purely structural schema validation: the metric must be registered, the
//! kind must match, every attribute must be declared on the metric, no
//! duplicates, and closed-enum dimensions must hold one of the registered
//! values (or use the `_other_` overflow bucket if the schema permits one).

use indexmap::IndexSet;
use kiro_telemetry_schema::{
    AttributeSpec,
    MetricKind,
    MetricSpec,
    Registry,
    registry,
};

use crate::record::{
    Attribute,
    MetricRecord,
    TelemetryLogRecord,
};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LimitError {
    #[error("unknown metric `{0}`")]
    UnknownMetric(String),
    #[error("metric `{metric}` used unregistered attribute `{attribute}`")]
    UnknownAttribute { metric: String, attribute: String },
    #[error("metric `{metric}` does not allow attribute `{attribute}`")]
    UnexpectedAttribute { metric: String, attribute: String },
    #[error("metric `{metric}` sent duplicate attribute `{attribute}`")]
    DuplicateAttribute { metric: String, attribute: String },
    #[error("metric `{metric}` used forbidden metric attribute `{attribute}`")]
    ForbiddenMetricAttribute { metric: String, attribute: String },
    #[error("metric `{metric}` sent invalid value `{value}` for attribute `{attribute}`")]
    InvalidAttributeValue {
        metric: String,
        attribute: String,
        value: String,
    },
    #[error("metric `{metric}` expected value kind `{expected:?}` but received `{actual:?}`")]
    WrongMetricKind {
        metric: String,
        expected: MetricKind,
        actual: MetricKind,
    },
}

/// Validate a metric record against the registered schema.
///
/// Checks: metric exists, value kind matches, no duplicate attribute keys,
/// every attribute is registered and declared on this metric, and closed-enum
/// dimensions use a valid value (or have an `_other_` overflow bucket).
pub fn validate_metric_record(record: &MetricRecord) -> Result<(), LimitError> {
    let registry = registry();
    let metric = registry
        .metric(&record.name)
        .ok_or_else(|| LimitError::UnknownMetric(record.name.clone()))?;

    let actual_kind = record.value.metric_kind();
    if metric.kind != actual_kind {
        return Err(LimitError::WrongMetricKind {
            metric: record.name.clone(),
            expected: metric.kind,
            actual: actual_kind,
        });
    }

    validate_attributes(registry, metric, &record.name, &record.attributes, true)
}

/// Validate a log record against the registered schema.
///
/// Same shape as [`validate_metric_record`] except the `metric_allowed` flag
/// is not enforced (log events accept attributes such as
/// `anonymous_client_id` that are forbidden on metrics).
pub fn validate_log_record(record: &TelemetryLogRecord) -> Result<(), LimitError> {
    let registry = registry();
    let metric = registry
        .metric(&record.name)
        .ok_or_else(|| LimitError::UnknownMetric(record.name.clone()))?;

    if metric.kind != MetricKind::LogEvent {
        return Err(LimitError::WrongMetricKind {
            metric: record.name.clone(),
            expected: metric.kind,
            actual: MetricKind::LogEvent,
        });
    }

    validate_attributes(registry, metric, &record.name, &record.attributes, false)
}

fn validate_attributes(
    registry: &Registry,
    metric: &MetricSpec,
    record_name: &str,
    attributes: &[Attribute],
    enforce_metric_allowed: bool,
) -> Result<(), LimitError> {
    let mut seen_keys: IndexSet<&str> = IndexSet::new();

    for attribute in attributes {
        if !seen_keys.insert(attribute.key.as_str()) {
            return Err(LimitError::DuplicateAttribute {
                metric: record_name.to_string(),
                attribute: attribute.key.clone(),
            });
        }

        let spec = registry
            .attribute(&attribute.key)
            .ok_or_else(|| LimitError::UnknownAttribute {
                metric: record_name.to_string(),
                attribute: attribute.key.clone(),
            })?;

        if enforce_metric_allowed && !spec.metric_allowed {
            return Err(LimitError::ForbiddenMetricAttribute {
                metric: record_name.to_string(),
                attribute: attribute.key.clone(),
            });
        }

        if !metric.attributes.iter().any(|allowed| allowed == &attribute.key) {
            return Err(LimitError::UnexpectedAttribute {
                metric: record_name.to_string(),
                attribute: attribute.key.clone(),
            });
        }

        validate_closed_value(record_name, attribute, spec)?;
    }

    Ok(())
}

fn validate_closed_value(metric_name: &str, attribute: &Attribute, spec: &AttributeSpec) -> Result<(), LimitError> {
    if spec.is_closed_enum() && !spec.accepts_value(&attribute.value) && !spec.has_other_bucket() {
        Err(LimitError::InvalidAttributeValue {
            metric: metric_name.to_string(),
            attribute: attribute.key.clone(),
            value: attribute.value.clone(),
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        MetricRecord,
        TelemetryLogRecord,
    };

    // Raw records are intentional here: these tests exercise the limiter paths
    // that run after construction and cover malformed or overflow-bound inputs.

    #[test]
    fn accepts_known_metric_with_valid_attribute() {
        validate_metric_record(
            &MetricRecord::counter("kiro_cli_model_invocations_total", 1).with_attribute("model", "claude-sonnet-4"),
        )
        .expect("valid record");
    }

    #[test]
    fn rejects_unknown_metric() {
        let err = validate_metric_record(&MetricRecord::counter("not_registered_total", 1))
            .expect_err("unknown metric should fail");
        assert!(matches!(err, LimitError::UnknownMetric(_)));
    }

    #[test]
    fn rejects_forbidden_metric_attributes() {
        let err = validate_metric_record(
            &MetricRecord::counter("kiro_cli_model_invocations_total", 1).with_attribute("anonymous_client_id", "abc"),
        )
        .expect_err("high-cardinality user ids are metric-forbidden");

        assert_eq!(err, LimitError::ForbiddenMetricAttribute {
            metric: "kiro_cli_model_invocations_total".to_string(),
            attribute: "anonymous_client_id".to_string()
        });
    }

    #[test]
    fn accepts_log_only_attributes_on_log_events() {
        validate_log_record(
            &TelemetryLogRecord::new("kiro_cli_user_turn_completed")
                .with_attribute("anonymous_client_id", "abc")
                .with_attribute("conversation_id", "conversation-1")
                .with_attribute("client_application", "chat_cli"),
        )
        .expect("log-only attributes are allowed on log events");
    }

    #[test]
    fn rejects_attributes_not_declared_on_metric() {
        let err = validate_metric_record(
            &MetricRecord::counter("kiro_cli_model_invocations_total", 1)
                .with_attribute("model", "claude-sonnet-4")
                .with_attribute("version_full", "1.2.3"),
        )
        .expect_err("metric-specific attributes are enforced");

        assert_eq!(err, LimitError::UnexpectedAttribute {
            metric: "kiro_cli_model_invocations_total".to_string(),
            attribute: "version_full".to_string()
        });
    }

    #[test]
    fn rejects_wrong_metric_value_kind() {
        let err = validate_metric_record(
            &MetricRecord::histogram("kiro_cli_model_invocations_total", 1.0)
                .with_attribute("model", "claude-sonnet-4"),
        )
        .expect_err("metric kind is enforced");

        assert_eq!(err, LimitError::WrongMetricKind {
            metric: "kiro_cli_model_invocations_total".to_string(),
            expected: MetricKind::Counter,
            actual: MetricKind::Histogram,
        });
    }

    #[test]
    fn rejects_duplicate_attribute_keys() {
        let err = validate_metric_record(
            &MetricRecord::counter("kiro_cli_model_invocations_total", 1)
                .with_attribute("model", "claude-sonnet-4")
                .with_attribute("model", "claude-haiku"),
        )
        .expect_err("duplicate keys are ambiguous");

        assert_eq!(err, LimitError::DuplicateAttribute {
            metric: "kiro_cli_model_invocations_total".to_string(),
            attribute: "model".to_string()
        });
    }

    #[test]
    fn rejects_closed_enum_without_other_bucket() {
        let err = validate_metric_record(
            &MetricRecord::gauge("active_users_daily", 1.0)
                .with_attribute("install_method", "brew")
                .with_attribute("client_application", "chat_cli")
                .with_attribute("is_internal_amazon", "maybe"),
        )
        .expect_err("boolean-like enums do not silently bucket");

        assert_eq!(err, LimitError::InvalidAttributeValue {
            metric: "active_users_daily".to_string(),
            attribute: "is_internal_amazon".to_string(),
            value: "maybe".to_string()
        });
    }
}
