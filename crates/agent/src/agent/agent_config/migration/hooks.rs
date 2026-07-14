//! Convert CLI object-form `hooks` (a map keyed by trigger) into the KAS array-of-documents form,
//! so a migrated config loads under KAS's `hooks: z.array(hookDocumentSchema)` profile schema
//! (object-form `hooks` fails whole-profile validation and drops the entire agent). Array-form
//! input is already universal and passes through unchanged, so re-migration is a no-op. A hook KAS
//! can't represent — a CLI tool hook (no `command`) or an unknown trigger — is dropped with a
//! warning rather than emitted as an invalid document.

use std::str::FromStr;

use serde_json::{
    Map,
    Value,
};

use super::permissions::{
    MigrationWarning,
    MigrationWarningKind,
};
use crate::agent::agent_config::definitions::HookTrigger;

// CLI hook defaults shared by the V1 and V2 readers. These extras are emitted only when the
// authored value differs, keeping the common case minimal while round-tripping safely (both readers
// restore the same default). `timeout_ms` is handled separately — its default differs per reader.
const DEFAULT_MAX_OUTPUT_SIZE: u64 = 10 * 1024;
const DEFAULT_CACHE_TTL_SECONDS: u64 = 0;
// CLI absent-timeout default (milliseconds). KAS defaults an absent array-form timeout to 60s, so
// an omitted value would run at a different timeout per engine; we emit this explicitly instead.
const DEFAULT_TIMEOUT_MS: u64 = 10 * 1000;

/// Convert a `hooks` value to the universal (KAS array) form. `None` when there is nothing to write
/// (absent or a non-object/array value); an empty object yields an empty array (valid under KAS,
/// unlike `{}`).
pub fn convert_hooks(hooks: &Value, warnings: &mut Vec<MigrationWarning>) -> Option<Value> {
    match hooks {
        // Already universal — pass through verbatim so re-running the migration changes nothing.
        Value::Array(_) => Some(hooks.clone()),
        Value::Object(map) => Some(Value::Array(object_form_to_docs(map, warnings))),
        _ => None,
    }
}

/// Flatten the trigger-keyed map into a flat list of KAS hook documents. Triggers are visited in
/// sorted order for deterministic output.
fn object_form_to_docs(map: &Map<String, Value>, warnings: &mut Vec<MigrationWarning>) -> Vec<Value> {
    let mut trigger_keys: Vec<&String> = map.keys().collect();
    trigger_keys.sort();

    let mut docs: Vec<Value> = Vec::new();
    for key in trigger_keys {
        let Ok(trigger) = HookTrigger::from_str(key) else {
            warnings.push(unconvertible_hook_warning(key));
            continue;
        };
        let canonical = trigger.to_string();
        let Some(entries) = map[key].as_array() else {
            continue;
        };
        for (idx, entry) in entries.iter().enumerate() {
            if let Some(doc) = hook_entry_to_doc(&canonical, idx, entry, warnings) {
                docs.push(doc);
            }
        }
    }
    docs
}

/// Project one CLI hook object into a KAS hook document. `None` (with a warning) when the hook has
/// no `command` — KAS's action is a discriminated union with no equivalent for a CLI tool hook.
fn hook_entry_to_doc(trigger: &str, idx: usize, entry: &Value, warnings: &mut Vec<MigrationWarning>) -> Option<Value> {
    let obj = entry.as_object()?;
    let Some(command) = obj.get("command").and_then(Value::as_str) else {
        warnings.push(unconvertible_hook_warning(trigger));
        return None;
    };

    let mut doc = Map::new();
    // KAS requires a non-empty `name`; synthesize a stable one from trigger + position.
    doc.insert("name".to_string(), Value::String(format!("{trigger}-{idx}")));
    doc.insert("trigger".to_string(), Value::String(trigger.to_string()));
    if let Some(matcher) = obj.get("matcher").and_then(Value::as_str) {
        doc.insert("matcher".to_string(), Value::String(matcher.to_string()));
    }
    let mut action = Map::new();
    action.insert("type".to_string(), Value::String("command".to_string()));
    action.insert("command".to_string(), Value::String(command.to_string()));
    doc.insert("action".to_string(), Value::Object(action));

    // CLI stores milliseconds; KAS's `timeout` is seconds. Always emitted (using the CLI default
    // when the source omitted it): the CLI and KAS readers apply *different* absent-timeout defaults,
    // so an omitted value would run at a different timeout depending on which engine reads it.
    // Clamp to a floor of 1s for any authored value: KAS's timeout is an integer >= 1s (0 means
    // "disabled", not "instant"), so an authored sub-second — or literal 0 — maps to the tightest
    // bound KAS can express rather than inverting the user's intent into no-timeout.
    let timeout = match obj.get("timeout_ms").and_then(Value::as_u64) {
        Some(ms) => ms.div_ceil(1000).max(1),
        None => DEFAULT_TIMEOUT_MS / 1000,
    };
    doc.insert("timeout".to_string(), Value::from(timeout));
    // CLI-only extras; KAS silently ignores unknown keys, and the CLI reader restores them. Both
    // readers share one default for these, so omitting at the default round-trips safely.
    if let Some(size) = non_default_u64(obj, "max_output_size", DEFAULT_MAX_OUTPUT_SIZE) {
        doc.insert("maxOutputSize".to_string(), Value::from(size));
    }
    if let Some(ttl) = non_default_u64(obj, "cache_ttl_seconds", DEFAULT_CACHE_TTL_SECONDS) {
        doc.insert("cacheTtlSeconds".to_string(), Value::from(ttl));
    }
    Some(Value::Object(doc))
}

/// A numeric field read only when present and set to a non-default value.
fn non_default_u64(obj: &Map<String, Value>, key: &str, default: u64) -> Option<u64> {
    obj.get(key).and_then(Value::as_u64).filter(|v| *v != default)
}

fn unconvertible_hook_warning(detail: &str) -> MigrationWarning {
    MigrationWarning {
        kind: MigrationWarningKind::UnconvertibleHook,
        detail: Some(detail.to_string()),
        attribute: Some("hooks".to_string()),
        converted: None,
        effect: None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn convert(hooks: Value) -> (Value, Vec<String>) {
        let mut warnings = Vec::new();
        let converted = convert_hooks(&hooks, &mut warnings).expect("some");
        let kinds = warnings
            .iter()
            .map(|w| serde_json::to_value(w.kind).unwrap().as_str().unwrap().to_string())
            .collect();
        (converted, kinds)
    }

    #[test]
    fn object_form_becomes_kas_array() {
        let (out, warnings) = convert(json!({
            "agentSpawn": [ { "command": "git status" } ]
        }));
        assert_eq!(
            out,
            json!([
                { "name": "agentSpawn-0", "trigger": "agentSpawn",
                  "action": { "type": "command", "command": "git status" }, "timeout": 10 }
            ])
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn non_default_fields_and_matcher_carry_over_with_unit_conversion() {
        let (out, _) = convert(json!({
            "preToolUse": [ {
                "command": "fmt",
                "matcher": "fs_write",
                "timeout_ms": 5000,
                "max_output_size": 2048,
                "cache_ttl_seconds": 60
            } ]
        }));
        assert_eq!(
            out,
            json!([
                { "name": "preToolUse-0", "trigger": "preToolUse", "matcher": "fs_write",
                  "action": { "type": "command", "command": "fmt" },
                  "timeout": 5, "maxOutputSize": 2048, "cacheTtlSeconds": 60 }
            ])
        );
    }

    #[test]
    fn default_valued_extras_are_omitted() {
        // Extras at their (reader-shared) default are dropped; they round-trip back to the default.
        let (out, _) = convert(json!({
            "stop": [ { "command": "x", "max_output_size": 10240, "cache_ttl_seconds": 0 } ]
        }));
        let doc = &out.as_array().unwrap()[0];
        assert!(doc.get("maxOutputSize").is_none());
        assert!(doc.get("cacheTtlSeconds").is_none());
    }

    #[test]
    fn timeout_always_emitted_even_at_a_default() {
        // The CLI and KAS readers default an absent `timeout` differently, so the value must always
        // be emitted — otherwise the round-tripped timeout depends on which engine reads it.
        let (out, _) = convert(json!({
            "stop": [ { "command": "x", "timeout_ms": 10000 } ]
        }));
        assert_eq!(out.as_array().unwrap()[0].get("timeout"), Some(&Value::from(10)));
    }

    #[test]
    fn absent_timeout_emits_cli_default() {
        // A hook that omits `timeout_ms` still gets an explicit `timeout` (the CLI default, 10s), so
        // KAS doesn't silently apply its own larger default and diverge from the CLI.
        let (out, _) = convert(json!({ "stop": [ { "command": "x" } ] }));
        assert_eq!(out.as_array().unwrap()[0].get("timeout"), Some(&Value::from(10)));
    }

    #[test]
    fn authored_timeout_clamps_up_to_at_least_one_second() {
        // Ceil with a 1s floor: an authored sub-second (or literal 0) maps to KAS's tightest
        // expressible bound (1s), never to 0 — which KAS reads as "disabled", inverting the user's
        // intent. A partial second rounds up so the migrated cap is never below the authored one.
        for (ms, secs) in [
            (0u64, 1u64),
            (1, 1),
            (500, 1),
            (999, 1),
            (1000, 1),
            (1001, 2),
            (2500, 3),
        ] {
            let (out, _) = convert(json!({ "stop": [ { "command": "x", "timeout_ms": ms } ] }));
            assert_eq!(
                out.as_array().unwrap()[0].get("timeout"),
                Some(&Value::from(secs)),
                "{ms}ms should map to {secs}s",
            );
        }
    }

    #[test]
    fn empty_object_becomes_empty_array() {
        // `hooks: {}` is rejected by KAS; `[]` is a valid empty array.
        let (out, warnings) = convert(json!({}));
        assert_eq!(out, json!([]));
        assert!(warnings.is_empty());
    }

    #[test]
    fn array_form_passes_through_unchanged() {
        let already = json!([
            { "name": "h", "trigger": "agentSpawn", "action": { "type": "command", "command": "x" } }
        ]);
        let (out, warnings) = convert(already.clone());
        assert_eq!(out, already);
        assert!(warnings.is_empty());
    }

    #[test]
    fn tool_hook_without_command_is_dropped_with_warning() {
        let (out, warnings) = convert(json!({
            "postToolUse": [ { "tool_name": "my_tool", "args": {} } ]
        }));
        assert_eq!(out, json!([]));
        assert_eq!(warnings, vec!["unconvertible-hook"]);
    }

    #[test]
    fn unknown_trigger_is_dropped_with_warning() {
        let (out, warnings) = convert(json!({ "postFileSave": [ { "command": "x" } ] }));
        assert_eq!(out, json!([]));
        assert_eq!(warnings, vec!["unconvertible-hook"]);
    }

    #[test]
    fn multiple_hooks_under_one_trigger_get_indexed_names() {
        let (out, _) = convert(json!({
            "agentSpawn": [ { "command": "a" }, { "command": "b" } ]
        }));
        let names: Vec<&str> = out
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d.get("name").and_then(Value::as_str).unwrap())
            .collect();
        assert_eq!(names, vec!["agentSpawn-0", "agentSpawn-1"]);
    }

    #[test]
    fn absent_hooks_value_yields_none() {
        let mut warnings = Vec::new();
        assert!(convert_hooks(&Value::Null, &mut warnings).is_none());
    }
}
