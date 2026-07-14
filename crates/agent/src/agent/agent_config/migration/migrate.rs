//! Agent config migration orchestration: V2 (CLI) -> V3 (KAS). Tool-name mapping, V2/V3 detection,
//! and field passthrough. Pure and deterministic: same input -> same output and warnings.
//!
//! The overall config is carried as a `serde_json::Value` so untouched fields round-trip
//! byte-for-byte (the universal output must preserve everything the V2 loader still reads). The
//! `toolsSettings` block is deserialized into the crate's typed [`ToolsSettings`] for the
//! permission derivation, so its key aliases come from the shared serde definitions rather than a
//! re-listed set.

use std::collections::BTreeSet;

use serde::{
    Deserialize,
    Serialize,
};
use serde_json::{
    Map,
    Value,
};
use typeshare::typeshare;

use super::hooks::convert_hooks;
use super::permissions::{
    MigrationWarning,
    agent_identifier_str,
    convert_allowed_tools,
    convert_tools_settings,
};
use super::tool_table::{
    aliases_of_v2_name,
    v3_tag_by_v2_name,
};
use crate::agent::agent_config::definitions::ToolsSettings;

/// V2-only tool names with no V3 tag. Used only as a V2-authorship *signal* (see
/// `is_v2_marker_tool_name`); the names themselves pass through `convert_tools` verbatim like any
/// unrecognized selector — KAS ignores tags it doesn't know.
const V2_ONLY_MARKER_TOOLS: &[&str] = &["thinking", "report_issue", "report"];

/// V2-only schema fields — a V2-detection signal, preserved on disk.
const CLI_ONLY_FIELDS: &[&str] = &["keyboardShortcut", "toolsSettings", "allowedTools", "toolAliases"];

/// V3-only trust fields — their presence marks a config KAS already understands.
const V3_TRUST_FIELDS: &[&str] = &["permissions", "includePowers", "excludedTools"];

/// Fields that carry over unchanged from V2 to V3.
const PASSTHROUGH_FIELDS: &[&str] = &[
    "name",
    "description",
    "model",
    "prompt",
    "resources",
    "mcpServers",
    // V3-schema fields, so a hybrid config with V2 markers keeps trust the user already set when
    // migrated.
    "permissions",
    "welcomeMessage",
    "includeMcpJson",
    "includePowers",
    "excludedTools",
];

/// True if `name` is a V2 name that translates to a *different* V3 selector (or a dropped tool) —
/// i.e. a spelling that signals a V2-authored config.
fn is_v2_marker_tool_name(name: &str) -> bool {
    if V2_ONLY_MARKER_TOOLS.contains(&name) {
        return true;
    }
    // A V2 name whose V3 tag differs from the name itself needs translation.
    matches!(v3_tag_by_v2_name(name), Some(tag) if tag != name)
}

/// True if a parsed config carries any V3 trust field.
fn has_v3_trust_field(config: &Value) -> bool {
    config
        .as_object()
        .is_some_and(|obj| V3_TRUST_FIELDS.iter().any(|f| obj.contains_key(*f)))
}

/// True when a config shows a V2-authored signal: a `toolsSettings`/`allowedTools` block, or
/// `tools` entries spelled as CLI tool names that need translation. (Broader than "trust" — a
/// V2-spelled `tools` entry alone qualifies.)
fn has_v2_signal(config: &Value) -> bool {
    let Some(obj) = config.as_object() else {
        return false;
    };
    if obj.contains_key("toolsSettings") || obj.contains_key("allowedTools") {
        return true;
    }
    // Object-form `hooks` is CLI-only: KAS validates `hooks` as an array and drops the whole agent
    // when it's an object, so any object here (even empty) needs converting to array form.
    if obj.get("hooks").is_some_and(Value::is_object) {
        return true;
    }
    if let Some(tools) = obj.get("tools").and_then(Value::as_array) {
        return tools.iter().any(|t| t.as_str().is_some_and(is_v2_marker_tool_name));
    }
    false
}

/// Detect V3 (KAS) format: no CLI-only fields and no CLI tool names.
fn is_already_v3(config: &Value) -> bool {
    let Some(obj) = config.as_object() else {
        return true;
    };
    if CLI_ONLY_FIELDS.iter().any(|f| obj.contains_key(*f)) {
        return false;
    }
    // Object-form `hooks` is CLI-only; array form is already universal.
    if obj.get("hooks").is_some_and(Value::is_object) {
        return false;
    }
    if let Some(tools) = obj.get("tools").and_then(Value::as_array)
        && tools.iter().any(|t| t.as_str().is_some_and(is_v2_marker_tool_name))
    {
        return false;
    }
    true
}

/// Deserialize a `toolsSettings` object into the typed [`ToolsSettings`], resiliently: a clean
/// whole-object parse is used when it succeeds, else each section is parsed independently (keyed by
/// the crate's own `ALIAS_GROUPS`) so one malformed section (e.g. `allowedCommands` typed as a
/// string) drops only that section's rules rather than every derived permission.
fn parse_tools_settings(value: &Value) -> ToolsSettings {
    if let Ok(ts) = serde_json::from_value::<ToolsSettings>(value.clone()) {
        return ts;
    }
    let Some(obj) = value.as_object() else {
        return ToolsSettings::default();
    };
    // Rebuild an object with only the sections that individually deserialize, under their canonical
    // key, then parse that. Unknown keys and malformed sections are skipped.
    let mut kept = Map::new();
    for (key, sub) in obj {
        // Resolve the section's canonical key. ALIAS_GROUPS covers the aliased fields; grep/glob
        // have no aliases so they are their own canonical name.
        let canonical = ToolsSettings::ALIAS_GROUPS
            .iter()
            .find(|g| g.contains(&key.as_str()))
            .map(|g| g[0])
            .or(match key.as_str() {
                "grep" => Some("grep"),
                "glob" => Some("glob"),
                _ => None,
            });
        let Some(canonical) = canonical else {
            continue;
        };
        // Validate the section in isolation via a single-key ToolsSettings parse.
        let mut probe = Map::new();
        probe.insert(canonical.to_string(), sub.clone());
        if serde_json::from_value::<ToolsSettings>(Value::Object(probe)).is_ok() {
            kept.insert(canonical.to_string(), sub.clone());
        }
    }
    serde_json::from_value(Value::Object(kept)).unwrap_or_default()
}

/// The transformed `tools` value: a KAS tag list or the `*` wildcard.
fn convert_tools(cli_tools: &Value, warnings: &mut Vec<MigrationWarning>) -> Option<Value> {
    if cli_tools.as_str() == Some("*") {
        return Some(Value::String("*".to_string()));
    }
    let arr = cli_tools.as_array()?;

    let mut kas_tools: BTreeSet<String> = BTreeSet::new();
    for tool in arr {
        let Some(tool) = tool.as_str() else {
            continue;
        };
        if let Some(mapped_tag) = v3_tag_by_v2_name(tool) {
            kas_tools.insert(mapped_tag.to_string());
        } else if tool == "aws" || tool == "use_aws" {
            kas_tools.insert(tool.to_string());
            warnings.push(MigrationWarning {
                kind: super::permissions::MigrationWarningKind::DeprecatedAwsTool,
                detail: Some(tool.to_string()),
                attribute: None,
                converted: None,
                effect: None,
            });
        } else {
            // Not a known V2 tool: a V3 tag/id, MCP ref, glob, or custom name. V3 ignores names it
            // doesn't recognize, so keep it verbatim for V2 — we deliberately don't track the
            // open-ended set of valid V3 selectors.
            kas_tools.insert(tool.to_string());
        }
    }
    Some(Value::Array(kas_tools.into_iter().map(Value::String).collect()))
}

/// Result of migrating a single agent config.
#[derive(Debug, Clone)]
struct MigrationResult {
    config: Value,
    warnings: Vec<MigrationWarning>,
}

/// Migrate a parsed V2 config into V3. Pure; already-V3 input echoes back.
fn migrate_agent_config(config: &Value) -> MigrationResult {
    if is_already_v3(config) {
        return MigrationResult {
            config: config.clone(),
            warnings: Vec::new(),
        };
    }

    let mut warnings: Vec<MigrationWarning> = Vec::new();
    let mut result: Map<String, Value> = Map::new();
    let empty = Map::new();
    let obj = config.as_object().unwrap_or(&empty);

    for field in PASSTHROUGH_FIELDS {
        if let Some(v) = obj.get(*field) {
            result.insert((*field).to_string(), v.clone());
        }
    }

    // V3 allows only relative file:// prompts — flag absolute/home-relative ones.
    if let Some(prompt) = obj.get("prompt").and_then(Value::as_str)
        && let Some(p) = prompt.strip_prefix("file://")
        && (p.starts_with('/') || p.starts_with('~'))
    {
        warnings.push(MigrationWarning {
            kind: super::permissions::MigrationWarningKind::FilePrompt,
            detail: Some(prompt.to_string()),
            attribute: None,
            converted: None,
            effect: None,
        });
    }

    // useLegacyMcpJson -> includeMcpJson propagation for V3 compatibility.
    if obj.contains_key("useLegacyMcpJson")
        && !obj.contains_key("includeMcpJson")
        && let Some(v) = obj.get("useLegacyMcpJson")
    {
        result.insert("includeMcpJson".to_string(), v.clone());
    }

    if let Some(cli_tools) = obj.get("tools")
        && let Some(converted) = convert_tools(cli_tools, &mut warnings)
    {
        result.insert("tools".to_string(), converted);
    }

    // Object-form `hooks` is rewritten to KAS array form (array form passes through); other shapes
    // were already dropped from the passthrough set, so an absent/invalid value carries nothing.
    if let Some(hooks) = obj.get("hooks")
        && let Some(converted) = convert_hooks(hooks, &mut warnings)
    {
        result.insert("hooks".to_string(), converted);
    }

    // Deserialize `toolsSettings` into the crate's typed model; its serde aliases resolve the CLI
    // key spellings (`execute_bash`/`shell`, `subagent`/`crew`, `read`/`fs_read`, …).
    let tools_settings: Option<ToolsSettings> = obj
        .get("toolsSettings")
        .filter(|v| v.is_object())
        .map(parse_tools_settings);

    // V2's `availableAgents` scopes invocable subagents; V3 expresses that via per-subagent tags,
    // so swap the broad `subagent` tag for `subagent/<name>`.
    if let Some(ts) = &tools_settings
        && !ts.crew.available_agents.is_empty()
        && let Some(tools) = result.get("tools").and_then(Value::as_array)
    {
        let mut tags: BTreeSet<String> = tools.iter().filter_map(Value::as_str).map(str::to_string).collect();
        if tags.remove("subagent") {
            tags.extend(
                ts.crew
                    .available_agents
                    .iter()
                    .map(|a| format!("subagent/{}", agent_identifier_str(a))),
            );
            result.insert(
                "tools".to_string(),
                Value::Array(tags.into_iter().map(Value::String).collect()),
            );
        }
    }

    let mut generated_rules: Vec<Value> = Vec::new();
    let mut generated_policies: Vec<String> = Vec::new();

    if let Some(ts) = &tools_settings
        && let Some(permissions) = convert_tools_settings(ts, &mut warnings)
    {
        for rule in &permissions.rules {
            generated_rules.push(serde_json::to_value(rule).expect("rule serializes"));
        }
        if let Some(policies) = permissions.policies {
            generated_policies.extend(policies);
        }
    }

    // allowedTools (V2 trusted-tools list) -> capability-level allow rules.
    let allowed_tools = obj.get("allowedTools").cloned().unwrap_or(Value::Null);
    for rule in convert_allowed_tools(&allowed_tools, &mut warnings) {
        generated_rules.push(serde_json::to_value(&rule).expect("rule serializes"));
    }

    if !generated_rules.is_empty() || !generated_policies.is_empty() {
        merge_permissions(&mut result, generated_rules, generated_policies);
    }

    MigrationResult {
        config: Value::Object(result),
        warnings,
    }
}

/// Merge derived rules/policies into `result["permissions"]`, appending to an existing
/// `permissions.rules` array when present and otherwise creating the block.
fn merge_permissions(result: &mut Map<String, Value>, rules: Vec<Value>, policies: Vec<String>) {
    let has_rules_array = matches!(
        result.get("permissions"),
        Some(Value::Object(o)) if o.get("rules").is_some_and(Value::is_array)
    );
    if !has_rules_array {
        let mut perms = Map::new();
        perms.insert("rules".to_string(), Value::Array(Vec::new()));
        result.insert("permissions".to_string(), Value::Object(perms));
    }

    let Some(Value::Object(perms)) = result.get_mut("permissions") else {
        return;
    };
    if let Some(Value::Array(existing)) = perms.get_mut("rules") {
        existing.extend(rules);
    }
    if !policies.is_empty() {
        let mut merged = perms
            .get("policies")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        merged.extend(policies.into_iter().map(Value::String));
        perms.insert("policies".to_string(), Value::Array(merged));
    }
}

// -- Universal-config upgrade --

/// Upgrade-flow classification. `universal-in-sync` is also the no-trust fallback; `v2-only` and
/// `universal-out-of-sync` are the actionable states.
#[typeshare]
// clap::ValueEnum defaults to kebab-case, matching the serde spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum AgentClassification {
    V2Only,
    UniversalOutOfSync,
    UniversalInSync,
    V3Only,
}

/// Result of upgrading a single agent config.
#[derive(Debug, Clone)]
pub struct UpgradeResult {
    /// V2 fields preserved verbatim, V3 fields added/updated.
    pub config: Value,
    pub classification: AgentClassification,
    pub warnings: Vec<MigrationWarning>,
    /// True when `config` differs from the input — the caller should write.
    pub changed: bool,
}

/// Drop V3 trust fields so a re-derivation depends only on V2 inputs.
fn strip_v3_trust_fields(config: &Value) -> Value {
    let Some(obj) = config.as_object() else {
        return config.clone();
    };
    let stripped: Map<String, Value> = obj
        .iter()
        .filter(|(k, _)| !V3_TRUST_FIELDS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    Value::Object(stripped)
}

/// Stable, recursive JSON serialization with sorted object keys (array order preserved).
fn canonical_value(value: &Value) -> Value {
    match value {
        Value::Array(a) => Value::Array(a.iter().map(canonical_value).collect()),
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut sorted = Map::new();
            for k in keys {
                sorted.insert(k.clone(), canonical_value(&o[k]));
            }
            Value::Object(sorted)
        },
        other => other.clone(),
    }
}

/// Stable, recursive JSON serialization with sorted keys.
fn canonical_json(value: &Value) -> String {
    canonical_value(value).to_string()
}

/// Order-independent structural equality of two JSON values.
fn deep_equal_json(a: &Value, b: &Value) -> bool {
    canonical_json(a) == canonical_json(b)
}

/// Build the universal `tools` array: start from the V3-derived tags, then add back each original
/// V2 name UNLESS one of its aliases is already present. This keeps distinct V2 tools V3 folds into
/// a tag (e.g. `grep`/`web_fetch`, whose name isn't their tag, so V2 needs them listed) while
/// dropping redundant alias spellings (`fs_read`/`execute_bash` when `read`/`shell` are already
/// there). Scoped subagent discovery drops the broad subagent aliases; `'*'` stays `'*'`.
fn merge_tools_for_universal_config(original: &Value, derived: &Value) -> Value {
    if original.as_str() == Some("*") || derived.as_str() == Some("*") {
        return Value::String("*".to_string());
    }
    let original_names: Vec<&str> = original
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let derived_tags: Vec<&str> = derived
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    let mut result: BTreeSet<String> = derived_tags.iter().map(|s| (*s).to_string()).collect();
    for name in original_names {
        if !aliases_of_v2_name(name).iter().any(|a| result.contains(a.as_str())) {
            result.insert(name.to_string());
        }
    }

    // Scoped subagents present -> drop the broad subagent aliases so V3 stays scoped.
    if result.iter().any(|t| t.starts_with("subagent/")) {
        for a in aliases_of_v2_name("subagent") {
            result.remove(a.as_str());
        }
    }

    Value::Array(result.into_iter().map(Value::String).collect())
}

/// Universal-config upgrade: preserve V2 fields, overlay V3-derived `tools` (union) and
/// `permissions`. Pure. Re-deriving overwrites V3, so writers back up first (see
/// `upgrade_agent_file`).
pub fn upgrade_agent_config(config: &Value) -> UpgradeResult {
    let has_v2 = has_v2_signal(config);
    let has_v3 = has_v3_trust_field(config);

    if !has_v2 && has_v3 {
        return UpgradeResult {
            config: config.clone(),
            classification: AgentClassification::V3Only,
            warnings: Vec::new(),
            changed: false,
        };
    }
    if !has_v2 && !has_v3 {
        return UpgradeResult {
            config: config.clone(),
            classification: AgentClassification::UniversalInSync,
            warnings: Vec::new(),
            changed: false,
        };
    }

    let stripped = strip_v3_trust_fields(config);
    let derived = migrate_agent_config(&stripped);

    let mut enriched: Map<String, Value> = config.as_object().cloned().unwrap_or_default();
    let derived_obj = derived.config.as_object();
    if let Some(derived_tools) = derived_obj.and_then(|o| o.get("tools")) {
        let original_tools = config.get("tools").cloned().unwrap_or(Value::Null);
        enriched.insert(
            "tools".to_string(),
            merge_tools_for_universal_config(&original_tools, derived_tools),
        );
    }
    // Object-form hooks became array form during derivation; overlay so the universal config drops
    // the CLI-only object shape KAS rejects. Array-form input derives back to itself (no-op).
    if let Some(derived_hooks) = derived_obj.and_then(|o| o.get("hooks")) {
        enriched.insert("hooks".to_string(), derived_hooks.clone());
    }
    if let Some(derived_permissions) = derived_obj.and_then(|o| o.get("permissions")) {
        enriched.insert("permissions".to_string(), derived_permissions.clone());
    } else if enriched.contains_key("toolsSettings") || enriched.contains_key("allowedTools") {
        // KAS skips a config with CLI-only fields but no `permissions` marker (it reads as
        // CLI-exclusive), so attach an empty one when the derivation produced no rules (e.g.
        // allowedTools: ['aws']).
        let mut perms = Map::new();
        perms.insert("rules".to_string(), Value::Array(Vec::new()));
        enriched.insert("permissions".to_string(), Value::Object(perms));
    }
    // else: nothing was derivable from V2 and there is no CLI-only permission source
    // (`toolsSettings`/`allowedTools`) — so any `permissions` present is hand-authored V3. Preserve
    // it verbatim; deleting it here would silently drop the user's trust (the config was only
    // flagged via tool-name spelling).

    let enriched = Value::Object(enriched);

    if !has_v3 {
        // A config whose only V2 signal is tool-name spelling converges after the union is written
        // once (the V2 name stays alongside its V3 tag, e.g. `web_fetch` + `web`). Re-running then
        // yields an identical config, so derive `changed` structurally: an already-migrated config
        // settles to `universal-in-sync` instead of being re-listed and rewritten (with a fresh
        // .bak) on every run.
        let changed = !deep_equal_json(&enriched, config);
        return UpgradeResult {
            config: enriched,
            classification: if changed {
                AgentClassification::V2Only
            } else {
                AgentClassification::UniversalInSync
            },
            warnings: derived.warnings,
            changed,
        };
    }

    // Both present: in-sync iff the derived tools/permissions/hooks equal the input's. Hooks are
    // included so a V3 config still carrying object-form hooks is rewritten (not skipped in-sync).
    let null = Value::Null;
    let in_sync = deep_equal_json(
        config.get("permissions").unwrap_or(&null),
        enriched.get("permissions").unwrap_or(&null),
    ) && deep_equal_json(
        config.get("tools").unwrap_or(&null),
        enriched.get("tools").unwrap_or(&null),
    ) && deep_equal_json(
        config.get("hooks").unwrap_or(&null),
        enriched.get("hooks").unwrap_or(&null),
    );
    UpgradeResult {
        config: enriched,
        classification: if in_sync {
            AgentClassification::UniversalInSync
        } else {
            AgentClassification::UniversalOutOfSync
        },
        warnings: derived.warnings,
        changed: !in_sync,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// One data-driven case from the shared fixture.
    #[derive(Debug, Deserialize)]
    struct UpgradeCase {
        #[allow(dead_code)]
        description: Option<String>,
        input: Value,
        #[serde(rename = "expectedConfig")]
        expected_config: Value,
        classification: AgentClassification,
        #[serde(default)]
        warnings: Vec<String>,
    }

    const CASES_JSON: &str = include_str!("testdata/upgrade-agent-config-cases.json");

    /// Recursively sort object keys and array elements so comparison is order-independent: `tools`
    /// is a set and permission rules are evaluated by effect precedence (not array position), so
    /// array order carries no meaning here — fixtures may list entries in any order.
    fn sort_deep(value: &Value) -> Value {
        match value {
            Value::Array(a) => {
                let mut sorted: Vec<Value> = a.iter().map(sort_deep).collect();
                sorted.sort_by(|x, y| x.to_string().cmp(&y.to_string()));
                Value::Array(sorted)
            },
            Value::Object(o) => {
                let mut keys: Vec<&String> = o.keys().collect();
                keys.sort();
                let mut out = Map::new();
                for k in keys {
                    out.insert(k.clone(), sort_deep(&o[k]));
                }
                Value::Object(out)
            },
            other => other.clone(),
        }
    }

    /// The kebab warning kind for a `MigrationWarning`, via its serde spelling.
    fn warning_kind_str(w: &MigrationWarning) -> String {
        serde_json::to_value(w.kind)
            .expect("kind serializes")
            .as_str()
            .expect("kind is a string")
            .to_string()
    }

    #[test]
    fn discovers_at_least_one_case() {
        let cases: Vec<UpgradeCase> = serde_json::from_str(CASES_JSON).expect("fixture parses");
        assert!(!cases.is_empty());
    }

    #[test]
    fn fixture_cases_upgrade_as_expected() {
        let cases: Vec<UpgradeCase> = serde_json::from_str(CASES_JSON).expect("fixture parses");
        for case in &cases {
            let label = case.description.as_deref().unwrap_or("case");
            let result = upgrade_agent_config(&case.input);

            assert_eq!(
                sort_deep(&result.config),
                sort_deep(&case.expected_config),
                "config mismatch for case: {label}"
            );
            assert_eq!(
                result.classification, case.classification,
                "classification mismatch for case: {label}"
            );

            let mut actual: Vec<String> = result.warnings.iter().map(warning_kind_str).collect();
            actual.sort();
            let mut expected = case.warnings.clone();
            expected.sort();
            assert_eq!(actual, expected, "warnings mismatch for case: {label}");
        }
    }

    #[test]
    fn second_run_is_a_noop() {
        // Upgrading an already-upgraded config must settle: `changed=false`, `universal-in-sync`.
        // This is the load-bearing idempotency guarantee that stops a fresh `.bak` on every run.
        let cases: Vec<UpgradeCase> = serde_json::from_str(CASES_JSON).expect("fixture parses");
        for case in &cases {
            let first = upgrade_agent_config(&case.input);
            let second = upgrade_agent_config(&first.config);
            assert!(
                !second.changed,
                "second run changed for case: {}",
                case.description.as_deref().unwrap_or("case")
            );
            assert!(
                matches!(
                    second.classification,
                    AgentClassification::UniversalInSync | AgentClassification::V3Only
                ),
                "second run classification {:?} for case: {}",
                second.classification,
                case.description.as_deref().unwrap_or("case")
            );
            // The settled config is a genuine fixed point.
            let third = upgrade_agent_config(&second.config);
            assert!(deep_equal_json(&third.config, &second.config));
        }
    }

    #[test]
    fn object_form_hooks_convert_to_kas_array() {
        // An agent whose only V2 signal is object-form hooks: it upgrades (converting the shape KAS
        // rejects) and the derived `hooks` is the KAS array form.
        let input = json!({
            "name": "h",
            "hooks": { "agentSpawn": [ { "command": "git status" } ] }
        });
        let result = upgrade_agent_config(&input);
        assert_eq!(result.classification, AgentClassification::V2Only);
        assert_eq!(
            result.config.get("hooks").unwrap(),
            &json!([
                { "name": "agentSpawn-0", "trigger": "agentSpawn",
                  "action": { "type": "command", "command": "git status" }, "timeout": 10 }
            ])
        );
    }

    #[test]
    fn object_form_hooks_only_upgrade_is_idempotent() {
        // Converting object-form hooks must settle: the array-form output re-derives to itself, so a
        // second run is a no-op (no endless rewrite / fresh backup).
        let input = json!({
            "name": "h",
            "hooks": { "preToolUse": [ { "command": "fmt", "matcher": "fs_write", "timeout_ms": 5000 } ] }
        });
        let first = upgrade_agent_config(&input);
        assert!(first.changed);
        let second = upgrade_agent_config(&first.config);
        assert!(!second.changed, "array-form hooks re-derive to themselves");
        assert_eq!(second.classification, AgentClassification::UniversalInSync);
    }

    #[test]
    fn v3_config_with_object_form_hooks_is_rewritten_not_skipped() {
        // A config with V3 trust (permissions) but still-CLI object-form hooks must not be treated
        // as in-sync — the hooks shape KAS rejects has to be converted.
        let input = json!({
            "tools": ["read"],
            "permissions": { "rules": [] },
            "hooks": { "stop": [ { "command": "cleanup" } ] }
        });
        let result = upgrade_agent_config(&input);
        assert!(result.changed);
        assert!(result.config.get("hooks").unwrap().is_array());
    }

    #[test]
    fn malformed_tools_settings_section_does_not_drop_others() {
        // One mistyped section (`web_fetch.trusted` as a string, not an array) must not lose the
        // well-formed `shell` rules — resilient per-section parse, not all-or-nothing.
        let input = json!({
            "name": "x",
            "tools": ["execute_bash"],
            "toolsSettings": {
                "execute_bash": { "allowedCommands": ["ls"] },
                "web_fetch": { "trusted": "not-an-array" },
            },
        });
        let result = upgrade_agent_config(&input);
        let perms = result.config.get("permissions").expect("permissions derived");
        let rules = perms.get("rules").and_then(Value::as_array).expect("rules array");
        assert!(
            rules
                .iter()
                .any(|r| r.get("capability").and_then(Value::as_str) == Some("shell")),
            "shell rule survived a malformed sibling section: {rules:?}"
        );
    }
}
