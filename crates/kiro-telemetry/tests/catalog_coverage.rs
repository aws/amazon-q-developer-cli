//! Conclusion gate for the OTel/CloudWatch telemetry migration: asserts that
//! every emittable metric in the canonical §5 catalog (`schema/metrics.yaml`)
//! has a typed constructor that is exercised by the shared catalog record set.
//!
//! Derived recording-rule metrics (SLOs, coverage ratios) are computed downstream
//! and intentionally never emitted from the binary, so they are excluded.

use std::collections::BTreeSet;

use kiro_telemetry::testing::{
    catalog_log_records,
    catalog_metric_records,
};
use kiro_telemetry::validate_metric_record;
use kiro_telemetry_schema::{
    MetricKind,
    registry,
};

#[test]
fn every_emittable_catalog_metric_has_a_constructor() {
    let emitted: BTreeSet<String> = catalog_metric_records()
        .iter()
        .map(|record| record.name.clone())
        .chain(catalog_log_records().iter().map(|record| record.name.clone()))
        .collect();

    let missing: Vec<&str> = registry()
        .metrics
        .iter()
        .filter(|metric| metric.kind != MetricKind::Derived)
        .filter(|metric| !emitted.contains(&metric.name))
        .map(|metric| metric.name.as_str())
        .collect();

    assert!(
        missing.is_empty(),
        "these catalog metrics have no emitted constructor in catalog_metric_records/catalog_log_records:\n{}",
        missing.join("\n")
    );
}

#[test]
fn derived_metrics_are_never_emitted() {
    let derived: BTreeSet<&str> = registry()
        .metrics
        .iter()
        .filter(|metric| metric.kind == MetricKind::Derived)
        .map(|metric| metric.name.as_str())
        .collect();

    // Derived metrics (recording rules) must not appear in the emitted record set.
    for record in catalog_metric_records() {
        assert!(
            !derived.contains(record.name.as_str()),
            "derived recording-rule metric `{}` must not be emitted from the binary",
            record.name
        );
    }
}

#[test]
fn every_emitted_metric_record_is_schema_valid() {
    // Each record built from a typed constructor must validate against the registry:
    // correct name, kind, and only allowed attribute keys/values.
    for record in catalog_metric_records() {
        validate_metric_record(&record)
            .unwrap_or_else(|err| panic!("catalog metric `{}` failed schema validation: {err}", record.name));
    }
}

#[test]
fn catalog_records_cover_every_section_kind() {
    // Sanity: the shared record set exercises every metric instrument kind plus logs,
    // so the smoke run proves counters, histograms, gauges, and log events all flow.
    let mut saw_counter = false;
    let mut saw_histogram = false;
    let mut saw_gauge = false;
    for record in catalog_metric_records() {
        match registry().metric(&record.name).map(|spec| spec.kind) {
            Some(MetricKind::Counter) => saw_counter = true,
            Some(MetricKind::Histogram) => saw_histogram = true,
            Some(MetricKind::ObservableGauge) => saw_gauge = true,
            _ => {},
        }
    }
    assert!(
        saw_counter && saw_histogram && saw_gauge,
        "expected counter, histogram, and gauge records"
    );
    assert!(
        !catalog_log_records().is_empty(),
        "expected at least one log_event record"
    );
}
