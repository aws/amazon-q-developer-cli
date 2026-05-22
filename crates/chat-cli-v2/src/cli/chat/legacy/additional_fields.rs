//! Generic additional model request fields — schema + overrides.
//!
//! `AdditionalModelFields` holds the raw JSON Schema from the API and the user's current
//! overrides. It provides methods to validate, set overrides, flatten for display,
//! and convert to the Smithy `Document` needed by the API.

use aws_smithy_types::Document;
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;

/// Manages additional model request fields: schema validation and user overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdditionalModelFields {
    /// The raw JSON Schema from the API (describes available fields and their enums).
    schema: Value,
    /// User overrides as a nested JSON object (e.g. `{"output_config": {"effort": "low"}}`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    overrides: Option<Value>,
}

impl AdditionalModelFields {
    /// Create from a Smithy Document (as returned by `additional_model_request_fields_schema()`).
    pub fn from_document(doc: &Document) -> Self {
        Self {
            schema: document_to_value(doc),
            overrides: None,
        }
    }

    /// Flatten the schema into dotted paths → allowed enum values for display.
    ///
    /// Example output: `[("output_config.effort", ["low", "medium", "high"])]`
    /// Note: Only includes fields with enum constraints. Integer/range fields are excluded.
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
            let values: Vec<String> = enums.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
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

    /// Set an override at a dotted path (validates first).
    pub fn set(&mut self, path: &str, value: &str) -> Result<(), String> {
        self.set_typed(path, Value::String(value.to_string()))
    }

    /// Set a typed override at a dotted path, validating against the schema type.
    pub fn set_typed(&mut self, path: &str, value: Value) -> Result<(), String> {
        self.validate_value(path, &value)?;
        self.set_raw(path, value);
        Ok(())
    }

    /// Validate a typed value against the schema at the given dotted path.
    fn validate_value(&self, path: &str, value: &Value) -> Result<(), String> {
        let schema_node = self.resolve_schema_node(path)?;

        let schema_type = schema_node.get("type").and_then(|t| t.as_str()).unwrap_or("string");

        match schema_type {
            "string" => {
                let s = value
                    .as_str()
                    .ok_or_else(|| format!("expected string for '{}'", path))?;
                if let Some(enums) = schema_node.get("enum").and_then(|v| v.as_array()) {
                    let allowed: Vec<&str> = enums.iter().filter_map(|v| v.as_str()).collect();
                    if !allowed.contains(&s) {
                        return Err(format!(
                            "invalid value '{}' for '{}', must be one of: {}",
                            s,
                            path,
                            allowed.join(", ")
                        ));
                    }
                }
                Ok(())
            },
            "integer" | "number" => {
                let n = match value {
                    Value::Number(n) => n.as_f64().ok_or_else(|| format!("invalid number for '{}'", path))?,
                    Value::String(s) => s
                        .parse::<f64>()
                        .map_err(|e| format!("expected number for '{}', got '{}': {}", path, s, e))?,
                    _ => return Err(format!("expected number for '{}'", path)),
                };
                if let Some(min) = schema_node.get("minimum").and_then(|v| v.as_f64())
                    && n < min
                {
                    return Err(format!("value {} for '{}' is below minimum {}", n, path, min));
                }
                if let Some(max) = schema_node.get("maximum").and_then(|v| v.as_f64())
                    && n > max
                {
                    return Err(format!("value {} for '{}' is above maximum {}", n, path, max));
                }
                Ok(())
            },
            "boolean" => match value {
                Value::Bool(_) => Ok(()),
                Value::String(s) if s == "true" || s == "false" => Ok(()),
                _ => Err(format!("expected boolean for '{}'", path)),
            },
            _ => Ok(()),
        }
    }

    /// Resolve the schema node for a dotted path.
    fn resolve_schema_node(&self, path: &str) -> Result<&Value, String> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = &self.schema;
        for part in &parts {
            current = current.get("properties").and_then(|p| p.get(*part)).ok_or_else(|| {
                let fields = self.flatten_schema();
                let available: Vec<&str> = fields.iter().map(|(p, _)| p.as_str()).collect();
                format!("unknown field '{}', available fields: {}", path, available.join(", "))
            })?;
        }
        Ok(current)
    }

    /// Set a raw value at a dotted path without validation.
    fn set_raw(&mut self, path: &str, value: Value) {
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
            current[*last] = value;
        }
    }

    /// Clear all overrides.
    pub fn clear(&mut self) {
        self.overrides = None;
    }

    /// Apply overrides from a nested JSON object, validating each leaf against the schema.
    /// Returns any validation errors (invalid paths or values are skipped).
    pub fn apply_overrides(&mut self, overrides: &Value) -> Vec<String> {
        let mut errors = Vec::new();
        if let Some(obj) = overrides.as_object() {
            for (path, value) in Self::collect_leaves(obj, String::new()) {
                if let Err(e) = self.set_typed(&path, value) {
                    errors.push(e);
                }
            }
        }
        errors
    }

    fn collect_leaves(obj: &serde_json::Map<String, Value>, prefix: String) -> Vec<(String, Value)> {
        let mut out = Vec::new();
        for (key, value) in obj {
            let path = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{}.{}", prefix, key)
            };
            match value {
                Value::Object(nested) => out.extend(Self::collect_leaves(nested, path)),
                Value::Null | Value::Array(_) => {},
                other => out.push((path, other.clone())),
            }
        }
        out
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
            aws_smithy_types::Number::Float(f) => serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number),
        },
        Document::String(s) => Value::String(s.clone()),
        Document::Array(arr) => Value::Array(arr.iter().map(document_to_value).collect()),
        Document::Object(map) => Value::Object(map.iter().map(|(k, v)| (k.clone(), document_to_value(v))).collect()),
    }
}

/// Convert `serde_json::Value` → `aws_smithy_types::Document`.
pub fn value_to_document(value: &Value) -> Document {
    match value {
        Value::Null => Document::Null,
        Value::Bool(b) => Document::Bool(*b),
        Value::Number(n) => {
            if let Some(u) = n.as_u64() {
                Document::Number(aws_smithy_types::Number::PosInt(u))
            } else if let Some(i) = n.as_i64() {
                Document::Number(aws_smithy_types::Number::NegInt(i))
            } else if let Some(f) = n.as_f64() {
                Document::Number(aws_smithy_types::Number::Float(f))
            } else {
                Document::Null
            }
        },
        Value::String(s) => Document::String(s.clone()),
        Value::Array(arr) => Document::Array(arr.iter().map(value_to_document).collect()),
        Value::Object(map) => Document::Object(map.iter().map(|(k, v)| (k.clone(), value_to_document(v))).collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> AdditionalModelFields {
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
        AdditionalModelFields {
            schema,
            overrides: None,
        }
    }

    #[test]
    fn test_flatten_schema() {
        let af = sample();
        let flat = af.flatten_schema();
        assert!(flat.contains(&("output_config.effort".into(), vec![
            "low".into(),
            "medium".into(),
            "high".into(),
            "xhigh".into(),
            "max".into()
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
        let af = AdditionalModelFields {
            schema,
            overrides: None,
        };
        let flat = af.flatten_schema();
        assert_eq!(flat, vec![("thinking.budget.mode".into(), vec![
            "auto".into(),
            "fixed".into()
        ])]);
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
        let mut af = AdditionalModelFields {
            schema,
            overrides: None,
        };
        af.set("thinking.budget.mode", "fixed").unwrap();
        assert_eq!(af.overrides.as_ref().unwrap()["thinking"]["budget"]["mode"], "fixed");
    }

    #[test]
    fn test_apply_overrides_integer_below_minimum() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "max_tokens": {
                    "type": "integer",
                    "minimum": 1024
                }
            }
        });
        let mut af = AdditionalModelFields {
            schema,
            overrides: None,
        };
        let overrides = serde_json::json!({"max_tokens": 512});
        let errors = af.apply_overrides(&overrides);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("below minimum"));
    }

    #[test]
    fn test_apply_overrides_invalid_value_returns_error() {
        let mut af = sample();
        let overrides = serde_json::json!({"output_config": {"effort": "ultra"}});
        let errors = af.apply_overrides(&overrides);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("invalid value 'ultra'"));
    }

    /// End-to-end: Smithy Document schema → AdditionalModelFields → apply typed overrides → Smithy
    /// Document output. Verifies string, integer, and boolean fields preserve their types
    /// through the full pipeline.
    #[test]
    fn test_e2e_smithy_roundtrip_all_types() {
        use aws_smithy_types::Number;

        // 1. Start with a Smithy Document (as the API would return)
        let schema_doc = Document::Object(
            [
                ("type".to_string(), Document::String("object".to_string())),
                (
                    "properties".to_string(),
                    Document::Object(
                        [
                            (
                                "output_config".to_string(),
                                Document::Object(
                                    [
                                        ("type".to_string(), Document::String("object".to_string())),
                                        (
                                            "properties".to_string(),
                                            Document::Object(
                                                [(
                                                    "effort".to_string(),
                                                    Document::Object(
                                                        [
                                                            (
                                                                "type".to_string(),
                                                                Document::String("string".to_string()),
                                                            ),
                                                            (
                                                                "enum".to_string(),
                                                                Document::Array(vec![
                                                                    Document::String("low".to_string()),
                                                                    Document::String("high".to_string()),
                                                                ]),
                                                            ),
                                                        ]
                                                        .into_iter()
                                                        .collect(),
                                                    ),
                                                )]
                                                .into_iter()
                                                .collect(),
                                            ),
                                        ),
                                    ]
                                    .into_iter()
                                    .collect(),
                                ),
                            ),
                            (
                                "max_tokens".to_string(),
                                Document::Object(
                                    [
                                        ("type".to_string(), Document::String("integer".to_string())),
                                        ("minimum".to_string(), Document::Number(Number::PosInt(1024))),
                                    ]
                                    .into_iter()
                                    .collect(),
                                ),
                            ),
                            (
                                "verbose".to_string(),
                                Document::Object(
                                    [("type".to_string(), Document::String("boolean".to_string()))]
                                        .into_iter()
                                        .collect(),
                                ),
                            ),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        );

        // 2. Convert to AdditionalModelFields (as done when model info is received)
        let mut af = AdditionalModelFields::from_document(&schema_doc);

        // 3. Apply overrides from a nested JSON object (as read from settings file)
        let settings_overrides = serde_json::json!({
            "output_config": { "effort": "low" },
            "max_tokens": 4096,
            "verbose": true
        });
        let errors = af.apply_overrides(&settings_overrides);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);

        // 4. Convert overrides to Smithy Document (as sent to the API)
        let output_value = af.to_value().unwrap();
        let output_doc = value_to_document(output_value);

        // 5. Verify types are preserved in the final Document
        let doc_map = match &output_doc {
            Document::Object(m) => m,
            _ => panic!("expected object"),
        };

        // String field
        let effort = doc_map["output_config"].as_object().unwrap().get("effort").unwrap();
        assert_eq!(effort, &Document::String("low".to_string()));

        // Integer field — stored as number, not string
        let max_tokens = doc_map.get("max_tokens").unwrap();
        assert_eq!(max_tokens, &Document::Number(Number::PosInt(4096)));

        // Boolean field
        let verbose = doc_map.get("verbose").unwrap();
        assert_eq!(verbose, &Document::Bool(true));
    }
}
