//! Generic additional model request fields — schema + overrides.
//!
//! `AdditionalFields` holds the raw JSON Schema from the API and the user's current
//! overrides. It provides methods to validate, set overrides, flatten for display,
//! and convert to the Smithy `Document` needed by the API.

use aws_smithy_types::Document;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Manages additional model request fields: schema validation and user overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdditionalFields {
    /// The raw JSON Schema from the API (describes available fields and their enums).
    schema: Value,
    /// User overrides as a nested JSON object (e.g. `{"output_config": {"effort": "low"}}`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    overrides: Option<Value>,
}

impl AdditionalFields {
    /// Create from a Smithy Document (as returned by `additional_model_request_fields_schema()`).
    pub fn from_document(doc: &Document) -> Self {
        Self {
            schema: document_to_value(doc),
            overrides: None,
        }
    }

    /// Create from a raw JSON schema value.
    pub fn from_schema(schema: Value) -> Self {
        Self { schema, overrides: None }
    }

    /// Flatten the schema into dotted paths → allowed enum values for display.
    ///
    /// Example output: `[("output_config.effort", ["low", "medium", "high"])]`
    pub fn flatten_schema(&self) -> Vec<(String, Vec<String>)> {
        let mut result = Vec::new();
        if let Some(properties) = self.schema.get("properties").and_then(|v| v.as_object()) {
            for (name, obj) in properties {
                Self::flatten_recursive(obj, name, &mut result);
            }
        }
        result
    }

    fn flatten_recursive(obj: &Value, prefix: &str, result: &mut Vec<(String, Vec<String>)>) {
        // If this field has an enum, it's a leaf
        if let Some(enums) = obj.get("enum").and_then(|v| v.as_array()) {
            let values: Vec<String> = enums
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            if !values.is_empty() {
                result.push((prefix.to_string(), values));
            }
            return;
        }
        // Otherwise recurse into nested properties
        if let Some(properties) = obj.get("properties").and_then(|v| v.as_object()) {
            for (name, child) in properties {
                Self::flatten_recursive(child, &format!("{}.{}", prefix, name), result);
            }
        }
    }

    /// Validate that a dotted path exists and the value is allowed.
    pub fn validate(&self, path: &str, value: &str) -> Result<(), String> {
        let fields = self.flatten_schema();
        match fields.iter().find(|(p, _)| p == path) {
            Some((_, allowed)) => {
                if allowed.contains(&value.to_string()) {
                    Ok(())
                } else {
                    Err(format!(
                        "invalid value '{}' for '{}', must be one of: {}",
                        value, path, allowed.join(", ")
                    ))
                }
            }
            None => {
                let available: Vec<&str> = fields.iter().map(|(p, _)| p.as_str()).collect();
                Err(format!(
                    "unknown field '{}', available fields: {}",
                    path, available.join(", ")
                ))
            }
        }
    }

    /// Set an override at a dotted path (validates first).
    pub fn set(&mut self, path: &str, value: &str) -> Result<(), String> {
        self.validate(path, value)?;
        let overrides = self.overrides.get_or_insert_with(|| Value::Object(Default::default()));
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = &mut *overrides;
        for part in &parts[..parts.len() - 1] {
            if !current.get(*part).is_some_and(|v| v.is_object()) {
                current[*part] = Value::Object(Default::default());
            }
            current = current.get_mut(*part).unwrap();
        }
        if let Some(last) = parts.last() {
            current[*last] = Value::String(value.to_string());
        }
        Ok(())
    }

    /// Clear all overrides.
    pub fn clear(&mut self) {
        self.overrides = None;
    }

    /// Get the current overrides as a reference.
    pub fn overrides(&self) -> Option<&Value> {
        self.overrides.as_ref()
    }

    /// Build the overrides as a `serde_json::Value` to pass in the API request.
    /// Returns `None` if no overrides are set.
    pub fn to_value(&self) -> Option<&Value> {
        self.overrides.as_ref()
    }
}

/// Convert `aws_smithy_types::Document` → `serde_json::Value`.
fn document_to_value(doc: &Document) -> Value {
    match doc {
        Document::Null => Value::Null,
        Document::Bool(b) => Value::Bool(*b),
        Document::Number(n) => match *n {
            aws_smithy_types::Number::PosInt(i) => Value::Number(i.into()),
            aws_smithy_types::Number::NegInt(i) => Value::Number(i.into()),
            aws_smithy_types::Number::Float(f) => {
                serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number)
            }
        },
        Document::String(s) => Value::String(s.clone()),
        Document::Array(arr) => Value::Array(arr.iter().map(document_to_value).collect()),
        Document::Object(map) => {
            Value::Object(map.iter().map(|(k, v)| (k.clone(), document_to_value(v))).collect())
        }
    }
}

/// Convert `serde_json::Value` → `aws_smithy_types::Document`.
pub fn value_to_document(value: &Value) -> Document {
    match value {
        Value::Null => Document::Null,
        Value::Bool(b) => Document::Bool(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Document::Number(aws_smithy_types::Number::PosInt(i as u64))
            } else if let Some(f) = n.as_f64() {
                Document::Number(aws_smithy_types::Number::Float(f))
            } else {
                Document::Null
            }
        }
        Value::String(s) => Document::String(s.clone()),
        Value::Array(arr) => Document::Array(arr.iter().map(value_to_document).collect()),
        Value::Object(map) => {
            Document::Object(map.iter().map(|(k, v)| (k.clone(), value_to_document(v))).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> AdditionalFields {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "thinking": {
                    "type": "object",
                    "properties": {
                        "type": { "type": "string", "enum": ["adaptive", "disabled"] },
                        "display": { "type": "string", "enum": ["summarized", "omitted"] }
                    }
                },
                "output_config": {
                    "type": "object",
                    "properties": {
                        "effort": { "type": "string", "enum": ["low", "medium", "high", "xhigh", "max"] }
                    }
                }
            }
        });
        AdditionalFields { schema, overrides: None }
    }

    #[test]
    fn test_flatten_schema() {
        let af = sample();
        let flat = af.flatten_schema();
        assert!(flat.contains(&("output_config.effort".into(), vec![
            "low".into(), "medium".into(), "high".into(), "xhigh".into(), "max".into()
        ])));
        assert!(flat.contains(&("thinking.type".into(), vec!["adaptive".into(), "disabled".into()])));
        assert!(flat.contains(&("thinking.display".into(), vec!["summarized".into(), "omitted".into()])));
    }

    #[test]
    fn test_set_valid() {
        let mut af = sample();
        assert!(af.set("output_config.effort", "low").is_ok());
        assert_eq!(af.overrides.as_ref().unwrap()["output_config"]["effort"], "low");
    }

    #[test]
    fn test_set_invalid_value() {
        let mut af = sample();
        let err = af.set("output_config.effort", "ultra").unwrap_err();
        assert!(err.contains("invalid value 'ultra'"));
    }

    #[test]
    fn test_set_unknown_path() {
        let mut af = sample();
        let err = af.set("foo.bar", "baz").unwrap_err();
        assert!(err.contains("unknown field 'foo.bar'"));
    }

    #[test]
    fn test_set_multiple() {
        let mut af = sample();
        af.set("output_config.effort", "low").unwrap();
        af.set("thinking.type", "adaptive").unwrap();
        let ov = af.overrides.as_ref().unwrap();
        assert_eq!(ov["output_config"]["effort"], "low");
        assert_eq!(ov["thinking"]["type"], "adaptive");
    }

    #[test]
    fn test_clear() {
        let mut af = sample();
        af.set("output_config.effort", "low").unwrap();
        af.clear();
        assert!(af.overrides.is_none());
        assert!(af.to_value().is_none());
    }

    #[test]
    fn test_to_document_roundtrip() {
        let mut af = sample();
        af.set("output_config.effort", "low").unwrap();
        af.set("thinking.type", "adaptive").unwrap();

        let value = af.to_value().unwrap();
        let doc = value_to_document(value);
        let back = document_to_value(&doc);
        assert_eq!(*af.overrides().unwrap(), back);
    }

    #[test]
    fn test_flatten_deep_nesting() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "thinking": {
                    "type": "object",
                    "properties": {
                        "budget": {
                            "type": "object",
                            "properties": {
                                "mode": { "type": "string", "enum": ["auto", "fixed"] }
                            }
                        }
                    }
                }
            }
        });
        let af = AdditionalFields { schema, overrides: None };
        let flat = af.flatten_schema();
        assert_eq!(flat, vec![("thinking.budget.mode".into(), vec!["auto".into(), "fixed".into()])]);
    }

    #[test]
    fn test_set_deep_nesting() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "thinking": {
                    "type": "object",
                    "properties": {
                        "budget": {
                            "type": "object",
                            "properties": {
                                "mode": { "type": "string", "enum": ["auto", "fixed"] }
                            }
                        }
                    }
                }
            }
        });
        let mut af = AdditionalFields { schema, overrides: None };
        af.set("thinking.budget.mode", "fixed").unwrap();
        assert_eq!(af.overrides.as_ref().unwrap()["thinking"]["budget"]["mode"], "fixed");
    }
}
