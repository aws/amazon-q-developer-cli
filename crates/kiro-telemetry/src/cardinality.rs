//! Schema validation for metric records.
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
    use crate::MetricRecord;

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
    fn rejects_unregistered_metric_attributes() {
        let err = validate_metric_record(
            &MetricRecord::counter("kiro_cli_model_invocations_total", 1).with_attribute("anonymous_client_id", "abc"),
        )
        .expect_err("unregistered attributes must be rejected");

        assert_eq!(err, LimitError::UnknownAttribute {
            metric: "kiro_cli_model_invocations_total".to_string(),
            attribute: "anonymous_client_id".to_string()
        });
    }

    #[test]
    fn rejects_attributes_not_declared_on_metric() {
        let err = validate_metric_record(
            &MetricRecord::counter("kiro_cli_model_invocations_total", 1)
                .with_attribute("model", "claude-sonnet-4")
                .with_attribute("auth_method", "builder_id"),
        )
        .expect_err("metric-specific attributes are enforced");

        assert_eq!(err, LimitError::UnexpectedAttribute {
            metric: "kiro_cli_model_invocations_total".to_string(),
            attribute: "auth_method".to_string()
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
            &MetricRecord::counter("kiro_cli_chat_session_started_total", 1)
                .with_attribute("session_interface", "browser")
                .with_attribute("agent_mode", "default")
                .with_attribute("agent_engine", "v2"),
        )
        .expect_err("closed enums do not silently bucket");

        assert_eq!(err, LimitError::InvalidAttributeValue {
            metric: "kiro_cli_chat_session_started_total".to_string(),
            attribute: "session_interface".to_string(),
            value: "browser".to_string()
        });
    }
}
