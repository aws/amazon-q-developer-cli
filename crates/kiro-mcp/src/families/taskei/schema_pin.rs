//! Startup-time schema-pin assertion (plan §305-313, §389-394).
//!
//! At process start, kiro-mcp signs a `tools/list` against the live
//! Taskei gateway, normalizes the response (canonical-JSON SHA-256 of
//! each tool's `inputSchema`, name-keyed map, sorted), and compares
//! it against a checked-in fixture. Mismatch emits one paging-severity
//! audit line per discrepancy on the `taskei_audit` tracing target and
//! forces a non-zero exit. On success, the same validated live catalog
//! is returned to the caller so startup does not fetch `tools/list`
//! twice.
//!
//! Why fail closed at boot rather than silently degrade in user
//! traffic: a gateway-side rename / annotation flip / argument-shape
//! change would otherwise reach Slack threads as either
//! gateway-side 4xx errors or *worse* — silently wrong dispatch (e.g.
//! a write tool that's been renamed gets routed to the read path
//! because our local overlay doesn't recognize it). The fixture is
//! the contract; CI rejects any deploy that drifts from it.
//!
//! Operator break-glass: setting `KIRO_MCP_SKIP_SCHEMA_PIN=1` at
//! launch skips the assertion. Only intended for the rare case where
//! a Taskei gateway change has rolled out *before* we've refreshed
//! the fixture and the bot needs to keep running. Skipping should be
//! tracked and short-lived; the audit log still notes it loudly.

use std::collections::BTreeMap;

use rmcp::model::Tool;
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;
use sha2::{
    Digest,
    Sha256,
};
use tracing::{
    error,
    info,
    warn,
};

use crate::families::taskei::mcp_proxy;
use crate::sts_bridge::ReadOnlyView;

/// Embedded fixture pinned at compile time. Path is relative to this
/// source file. Phase 1d task #7 captures the fixture from the live
/// gateway.
pub const FIXTURE_BYTES: &str = include_str!("fixtures/taskei-tools.json");

/// Env var that opts out of the assertion. Documented break-glass.
pub const SKIP_ENV: &str = "KIRO_MCP_SKIP_SCHEMA_PIN";

/// One row of the pinned fixture. The shape is intentionally lossy —
/// only the load-bearing facts travel: tool name (the map key), the
/// annotation overlay we depend on, and a SHA-256 of the canonical
/// `inputSchema` so the gateway can add a `description` field or a
/// new `default` without breaking us, but a *shape change* (new
/// required field, rename) trips the assertion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnedTool {
    /// What the gateway returns under `annotations.readOnlyHint` for
    /// this tool. `None` literal in the fixture means "gateway
    /// doesn't currently emit one"; if the gateway *starts* emitting
    /// one (or vice versa) the fixture is wrong and the assertion
    /// will trip.
    #[serde(rename = "readOnlyHint")]
    pub read_only_hint: Option<bool>,
    #[serde(rename = "destructiveHint")]
    pub destructive_hint: Option<bool>,
    /// SHA-256 (hex, lowercase) of the canonical-JSON serialization
    /// of the tool's `inputSchema`. Canonical = recursively sorted
    /// object keys, no whitespace, no trailing newline. See
    /// [`canonicalize`].
    #[serde(rename = "inputSchemaSha256")]
    pub input_schema_sha256: String,
}

/// Top-level fixture shape. `BTreeMap` so `serde_json::to_string`
/// produces deterministic output (sorted by key).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnedFixture {
    /// MCP protocol version we negotiated with the gateway when the
    /// fixture was captured. Compared against the live response to
    /// catch silent protocol downgrades.
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub tools: BTreeMap<String, PinnedTool>,
}

impl PinnedFixture {
    /// Parse from the bundled `include_str!` bytes.
    pub fn embedded() -> Result<Self, String> {
        serde_json::from_str(FIXTURE_BYTES).map_err(|e| format!("parse embedded fixture: {e}"))
    }
}

/// Reduce a live `Vec<Tool>` to the same lossy normalized form as the
/// fixture. Used both at runtime to compare and (in task #6's dumper
/// binary) to *write* the fixture.
pub fn normalize(tools: &[Tool], protocol_version: impl Into<String>) -> PinnedFixture {
    let mut map = BTreeMap::new();
    for tool in tools {
        let schema_json: Value = serde_json::Value::Object((*tool.input_schema).clone());
        let canonical = canonicalize(&schema_json);
        let mut hasher = Sha256::new();
        hasher.update(canonical.as_bytes());
        let digest = hex::encode(hasher.finalize());

        let (read_only, destructive) = match tool.annotations.as_ref() {
            Some(ann) => (ann.read_only_hint, ann.destructive_hint),
            None => (None, None),
        };
        map.insert(tool.name.to_string(), PinnedTool {
            read_only_hint: read_only,
            destructive_hint: destructive,
            input_schema_sha256: digest,
        });
    }
    PinnedFixture {
        protocol_version: protocol_version.into(),
        tools: map,
    }
}

/// Canonicalize a JSON value: recursively sorted object keys, no
/// whitespace. The output is what we hash for the schema-pin
/// fingerprint. We can't rely on `serde_json::to_string(value)`
/// because `serde_json::Map` preserves insertion order and the
/// gateway can reorder keys across calls.
pub fn canonicalize(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(&mut out, value);
    out
}

fn write_canonical(out: &mut String, value: &Value) {
    use std::fmt::Write;
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            // `serde_json::Number` already round-trips losslessly via
            // its Display impl. Two snapshots of the same numeric
            // value will produce the same canonical form.
            let _ = write!(out, "{}", n);
        },
        Value::String(s) => {
            // Re-serialize via serde_json so escapes / unicode are
            // treated the same way both sides.
            let serialized = serde_json::to_string(s).expect("string is always serializable");
            out.push_str(&serialized);
        },
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(out, item);
            }
            out.push(']');
        },
        Value::Object(map) => {
            // Sort keys so two semantically-equal objects with
            // different insertion orders hash identically.
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                let key_serialized = serde_json::to_string(*k).expect("string is always serializable");
                out.push_str(&key_serialized);
                out.push(':');
                write_canonical(out, map.get(*k).expect("key from same map"));
            }
            out.push('}');
        },
    }
}

/// One discrepancy between fixture and live response. Worded for
/// human + log-search reading.
#[derive(Debug)]
pub enum Discrepancy {
    ProtocolVersion {
        expected: String,
        actual: String,
    },
    UnexpectedTool {
        name: String,
    },
    MissingTool {
        name: String,
    },
    ReadOnlyHint {
        name: String,
        expected: Option<bool>,
        actual: Option<bool>,
    },
    DestructiveHint {
        name: String,
        expected: Option<bool>,
        actual: Option<bool>,
    },
    InputSchema {
        name: String,
        expected: String,
        actual: String,
    },
}

impl std::fmt::Display for Discrepancy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Discrepancy::ProtocolVersion { expected, actual } => {
                write!(f, "protocolVersion: expected {expected}, gateway returned {actual}")
            },
            Discrepancy::UnexpectedTool { name } => {
                write!(f, "gateway exposed tool `{name}` not in pinned fixture")
            },
            Discrepancy::MissingTool { name } => {
                write!(f, "gateway no longer exposes tool `{name}` from pinned fixture")
            },
            Discrepancy::ReadOnlyHint { name, expected, actual } => write!(
                f,
                "tool `{name}` readOnlyHint: expected {expected:?}, gateway returned {actual:?}"
            ),
            Discrepancy::DestructiveHint { name, expected, actual } => write!(
                f,
                "tool `{name}` destructiveHint: expected {expected:?}, gateway returned {actual:?}"
            ),
            Discrepancy::InputSchema { name, expected, actual } => write!(
                f,
                "tool `{name}` inputSchema sha256: expected {expected}, gateway returned {actual}"
            ),
        }
    }
}

/// Compare a normalized live response to a fixture. Returns the empty
/// vec on match, or a list of discrepancies otherwise (sorted for
/// stable log ordering).
pub fn compare(fixture: &PinnedFixture, live: &PinnedFixture) -> Vec<Discrepancy> {
    let mut discrepancies = Vec::new();
    if fixture.protocol_version != live.protocol_version {
        discrepancies.push(Discrepancy::ProtocolVersion {
            expected: fixture.protocol_version.clone(),
            actual: live.protocol_version.clone(),
        });
    }

    // Names in fixture but absent from live.
    for name in fixture.tools.keys() {
        if !live.tools.contains_key(name) {
            discrepancies.push(Discrepancy::MissingTool { name: name.clone() });
        }
    }
    // Names in live but absent from fixture.
    for name in live.tools.keys() {
        if !fixture.tools.contains_key(name) {
            discrepancies.push(Discrepancy::UnexpectedTool { name: name.clone() });
        }
    }
    // For tools present on both sides, compare hint and schema hash.
    for (name, fixture_row) in &fixture.tools {
        if let Some(live_row) = live.tools.get(name) {
            if fixture_row.read_only_hint != live_row.read_only_hint {
                discrepancies.push(Discrepancy::ReadOnlyHint {
                    name: name.clone(),
                    expected: fixture_row.read_only_hint,
                    actual: live_row.read_only_hint,
                });
            }
            if fixture_row.destructive_hint != live_row.destructive_hint {
                discrepancies.push(Discrepancy::DestructiveHint {
                    name: name.clone(),
                    expected: fixture_row.destructive_hint,
                    actual: live_row.destructive_hint,
                });
            }
            if fixture_row.input_schema_sha256 != live_row.input_schema_sha256 {
                discrepancies.push(Discrepancy::InputSchema {
                    name: name.clone(),
                    expected: fixture_row.input_schema_sha256.clone(),
                    actual: live_row.input_schema_sha256.clone(),
                });
            }
        }
    }
    discrepancies
}

/// Top-level entry point — runs the full assertion at startup and
/// returns the validated live tool catalog.
///
/// Returns `Ok(Vec<Tool>)` on match, and `Err` describing the first
/// reason to abort otherwise. Caller logs the error and exits
/// non-zero. The individual discrepancies are also emitted as their
/// own `taskei_audit` lines so log-search alarms can fire on the
/// right granularity.
pub async fn assert_pinned_schema(view: &ReadOnlyView, endpoint: &str, region: &str) -> Result<Vec<Tool>, String> {
    if std::env::var(SKIP_ENV).map(|v| v == "1").unwrap_or(false) {
        warn!(
            target: "taskei_audit",
            env_var = SKIP_ENV,
            "schema-pin assertion SKIPPED via break-glass env var; refresh the fixture and remove the override"
        );
        return mcp_proxy::list_tools_remote(view, endpoint, region)
            .await
            .map_err(|e| format!("live tools/list: {}", e.message));
    }

    let fixture = PinnedFixture::embedded().map_err(|e| format!("embedded fixture: {e}"))?;

    let protocol_version = mcp_proxy::initialize_remote(view, endpoint, region)
        .await
        .map_err(|e| format!("live initialize: {}", e.message))?;
    let live_tools = mcp_proxy::list_tools_remote(view, endpoint, region)
        .await
        .map_err(|e| format!("live tools/list: {}", e.message))?;
    let live = normalize(&live_tools, protocol_version);

    let discrepancies = compare(&fixture, &live);
    if discrepancies.is_empty() {
        info!(
            target: "taskei_audit",
            tool_count = fixture.tools.len(),
            protocol_version = %fixture.protocol_version,
            "schema-pin OK — gateway tools/list matches checked-in fixture"
        );
        return Ok(live_tools);
    }

    for d in &discrepancies {
        // One audit line per discrepancy so log-search can route on the
        // exact field. The summary error returned at the bottom of this
        // function is what the caller exits on.
        error!(
            target: "taskei_audit",
            discrepancy = %d,
            "schema-pin discrepancy"
        );
    }

    Err(format!(
        "schema-pin failed against fixture: {} discrepancy/discrepancies (see taskei_audit)",
        discrepancies.len()
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rmcp::model::{
        Tool,
        ToolAnnotations,
    };
    use serde_json::json;

    use super::*;

    fn tool_with(name: &str, schema: Value, read_only: Option<bool>, destructive: Option<bool>) -> Tool {
        let annotations = match (read_only, destructive) {
            (None, None) => None,
            (r, d) => Some(ToolAnnotations::from_raw(None, r, d, None, None)),
        };
        let mut tool = Tool::new_with_raw(
            name.to_string(),
            None,
            Arc::new(serde_json::from_value(schema).unwrap()),
        );
        tool.annotations = annotations;
        tool
    }

    #[test]
    fn canonicalize_sorts_object_keys() {
        let a = json!({ "b": 2, "a": 1 });
        let b = json!({ "a": 1, "b": 2 });
        assert_eq!(canonicalize(&a), canonicalize(&b));
        assert_eq!(canonicalize(&a), r#"{"a":1,"b":2}"#);
    }

    #[test]
    fn canonicalize_handles_nested_objects() {
        let a = json!({ "outer": { "z": 1, "a": 2 } });
        assert_eq!(canonicalize(&a), r#"{"outer":{"a":2,"z":1}}"#);
    }

    #[test]
    fn normalize_produces_stable_hashes_for_equal_schemas() {
        let t1 = tool_with("X", json!({ "type": "object", "x": 1 }), Some(true), None);
        let t2 = tool_with("X", json!({ "x": 1, "type": "object" }), Some(true), None);
        let n1 = normalize(&[t1], "p");
        let n2 = normalize(&[t2], "p");
        assert_eq!(
            n1.tools["X"].input_schema_sha256, n2.tools["X"].input_schema_sha256,
            "key reordering must not change the hash"
        );
    }

    #[test]
    fn compare_detects_added_tool() {
        let fixture = PinnedFixture {
            protocol_version: "p".into(),
            tools: BTreeMap::from([("A".into(), PinnedTool {
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                input_schema_sha256: "0".into(),
            })]),
        };
        let mut live = fixture.clone();
        live.tools.insert("B".into(), PinnedTool {
            read_only_hint: None,
            destructive_hint: None,
            input_schema_sha256: "0".into(),
        });
        let d = compare(&fixture, &live);
        assert!(matches!(d.as_slice(), [Discrepancy::UnexpectedTool { name }] if name == "B"));
    }

    #[test]
    fn compare_detects_removed_tool() {
        let fixture = PinnedFixture {
            protocol_version: "p".into(),
            tools: BTreeMap::from([("A".into(), PinnedTool {
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                input_schema_sha256: "0".into(),
            })]),
        };
        let mut live = fixture.clone();
        live.tools.clear();
        let d = compare(&fixture, &live);
        assert!(matches!(d.as_slice(), [Discrepancy::MissingTool { name }] if name == "A"));
    }

    #[test]
    fn compare_detects_read_only_hint_flip() {
        let fixture = PinnedFixture {
            protocol_version: "p".into(),
            tools: BTreeMap::from([("A".into(), PinnedTool {
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                input_schema_sha256: "0".into(),
            })]),
        };
        let mut live = fixture.clone();
        live.tools.get_mut("A").unwrap().read_only_hint = Some(false);
        let d = compare(&fixture, &live);
        assert!(matches!(d.as_slice(), [Discrepancy::ReadOnlyHint { name, .. }] if name == "A"));
    }

    #[test]
    fn compare_detects_input_schema_change() {
        let fixture = PinnedFixture {
            protocol_version: "p".into(),
            tools: BTreeMap::from([("A".into(), PinnedTool {
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                input_schema_sha256: "old".into(),
            })]),
        };
        let mut live = fixture.clone();
        live.tools.get_mut("A").unwrap().input_schema_sha256 = "new".into();
        let d = compare(&fixture, &live);
        assert!(matches!(d.as_slice(), [Discrepancy::InputSchema { name, .. }] if name == "A"));
    }

    #[test]
    fn compare_detects_protocol_version_mismatch() {
        let fixture = PinnedFixture {
            protocol_version: "2025-06-18".into(),
            tools: BTreeMap::new(),
        };
        let live = PinnedFixture {
            protocol_version: "2024-11-05".into(),
            tools: BTreeMap::new(),
        };
        let d = compare(&fixture, &live);
        assert!(matches!(d.as_slice(), [Discrepancy::ProtocolVersion { .. }]));
    }

    #[test]
    fn compare_returns_empty_on_exact_match() {
        let fixture = PinnedFixture {
            protocol_version: "p".into(),
            tools: BTreeMap::from([("A".into(), PinnedTool {
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                input_schema_sha256: "h".into(),
            })]),
        };
        let live = fixture.clone();
        assert!(compare(&fixture, &live).is_empty());
    }
}
