use std::collections::BTreeMap;

use amzn_toolkit_telemetry_client::types::AwsProduct;
use kiro_telemetry_schema::{
    CLOUDWATCH_OTEL_NAMESPACE,
    CLOUDWATCH_PRODUCT_DIMENSION,
    CLOUDWATCH_PRODUCT_VALUE,
    LegacyMappings,
    Registry,
};
use serde_json::{
    Value,
    json,
};

#[test]
fn critical_alarm_emf_contract_preserves_legacy_product_dimension() {
    let registry = Registry::parse().expect("schema should load");
    let mappings = LegacyMappings::parse().expect("legacy mappings should load");
    mappings
        .validate_against(&registry)
        .expect("legacy mappings should reference known metrics");

    let legacy_product = legacy_toolkit_product_value();
    assert_eq!(
        legacy_product, CLOUDWATCH_PRODUCT_VALUE,
        "CloudWatch product dimension must come from the legacy sink identity"
    );
    assert_eq!(mappings.critical_alarms.len(), 5);

    for alarm in &mappings.critical_alarms {
        let metric = registry
            .metric(&alarm.otel_metric)
            .unwrap_or_else(|| panic!("{} must reference a known OTel metric", alarm.name));
        let legacy_payload = mock_awsemf_payload(&alarm.otel_metric, &metric.unit, &legacy_product);
        let otel_payload = mock_awsemf_payload(&alarm.otel_metric, &metric.unit, CLOUDWATCH_PRODUCT_VALUE);

        assert_eq!(
            legacy_payload, otel_payload,
            "{} EMF payload drifted for product dimension",
            alarm.name
        );
    }
}

fn legacy_toolkit_product_value() -> String {
    format!("{:?}", AwsProduct::CodewhispererTerminal)
}

fn mock_awsemf_payload(metric_name: &str, unit: &str, product_value: &str) -> Vec<u8> {
    let mut payload = BTreeMap::new();
    payload.insert(
        "_aws".to_string(),
        json!({
            "Timestamp": 1_700_000_000_000i64,
            "CloudWatchMetrics": [{
                "Namespace": CLOUDWATCH_OTEL_NAMESPACE,
                "Dimensions": [[CLOUDWATCH_PRODUCT_DIMENSION]],
                "Metrics": [{
                    "Name": metric_name,
                    "Unit": unit
                }]
            }]
        }),
    );
    payload.insert(CLOUDWATCH_PRODUCT_DIMENSION.to_string(), json!(product_value));
    payload.insert(metric_name.to_string(), json!(1.0));

    serde_json::to_vec(&Value::Object(payload.into_iter().collect())).expect("EMF JSON should serialize")
}
