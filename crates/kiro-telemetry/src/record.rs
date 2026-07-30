use std::time::{
    SystemTime,
    UNIX_EPOCH,
};

use kiro_telemetry_schema::MetricKind;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MetricRecord {
    pub name: String,
    pub value: MetricValue,
    pub timestamp_unix_millis: u64,
    pub attributes: Vec<Attribute>,
    pub resource_attributes: Vec<Attribute>,
}

impl MetricRecord {
    pub fn counter(name: impl Into<String>, value: u64) -> Self {
        Self {
            name: name.into(),
            value: MetricValue::Counter(value),
            timestamp_unix_millis: now_unix_millis(),
            attributes: Vec::new(),
            resource_attributes: Vec::new(),
        }
    }

    pub fn counter_f64(name: impl Into<String>, value: f64) -> Self {
        Self {
            name: name.into(),
            value: MetricValue::FloatCounter(value),
            timestamp_unix_millis: now_unix_millis(),
            attributes: Vec::new(),
            resource_attributes: Vec::new(),
        }
    }

    pub fn histogram(name: impl Into<String>, value: f64) -> Self {
        Self {
            name: name.into(),
            value: MetricValue::Histogram(value),
            timestamp_unix_millis: now_unix_millis(),
            attributes: Vec::new(),
            resource_attributes: Vec::new(),
        }
    }

    pub fn gauge(name: impl Into<String>, value: f64) -> Self {
        Self {
            name: name.into(),
            value: MetricValue::Gauge(value),
            timestamp_unix_millis: now_unix_millis(),
            attributes: Vec::new(),
            resource_attributes: Vec::new(),
        }
    }

    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.push(Attribute {
            key: key.into(),
            value: value.into(),
        });
        self
    }

    pub fn with_resource_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.resource_attributes.push(Attribute {
            key: key.into(),
            value: value.into(),
        });
        self
    }

    pub fn mark_replayed(mut self) -> Self {
        self.resource_attributes.push(Attribute {
            key: "replayed".to_string(),
            value: "true".to_string(),
        });
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MetricValue {
    Counter(u64),
    FloatCounter(f64),
    Histogram(f64),
    Gauge(f64),
}

impl MetricValue {
    pub fn metric_kind(&self) -> MetricKind {
        match self {
            Self::Counter(_) | Self::FloatCounter(_) => MetricKind::Counter,
            Self::Histogram(_) => MetricKind::Histogram,
            Self::Gauge(_) => MetricKind::ObservableGauge,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attribute {
    pub key: String,
    pub value: String,
}

fn now_unix_millis() -> u64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    millis.try_into().unwrap_or(u64::MAX)
}
