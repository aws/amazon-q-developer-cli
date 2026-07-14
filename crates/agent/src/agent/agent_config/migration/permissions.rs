//! Permission conversion: CLI `toolsSettings`/`allowedTools` -> V3 (KAS) `permissions.rules`.
//! The hard case is shell patterns — the CLI allows regex, KAS matches tokenized globs, so some
//! translate only best-effort (flagged with a warning). Pure: warnings accumulate into a
//! caller-supplied `Vec`. `toolsSettings` is read from the crate's typed [`ToolsSettings`] (so key
//! aliases come from the shared definitions); `allowedTools` is read from raw `Value` since it is a
//! free-form selector list, not a typed struct.

use std::collections::BTreeSet;

use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;
use typeshare::typeshare;

use super::regex_to_glob::{
    RegexFidelity,
    RegexToGlobOptions,
    regex_to_glob,
};
use super::tool_table::v3_cap_by_v2_name;
use crate::agent::agent_config::definitions::ToolsSettings;

/// A rule effect — drives the allow/deny label and deny-all status in diagnostics.
#[typeshare]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effect {
    Allow,
    Deny,
    /// Never emitted by the migration, but a live V3 rule effect the wire schema must round-trip.
    Ask,
}

/// A single permission rule in the V3 (KAS) format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V3PermissionRule {
    pub capability: String,
    #[serde(rename = "match", skip_serializing_if = "Option::is_none")]
    pub r#match: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude: Option<Vec<String>>,
    pub effect: Effect,
}

/// The V3 (KAS) permissions block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V3Permissions {
    pub rules: Vec<V3PermissionRule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policies: Option<Vec<String>>,
}

/// The kind of a lossy or ambiguous conversion warning. Spellings are contractual — diagnostics
/// keys on the exact kind string.
#[typeshare]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MigrationWarningKind {
    RegexShellPattern,
    RegexWebPattern,
    UnconvertiblePattern,
    UnmappedAllowedTool,
    DeprecatedAwsTool,
    /// `denyByDefault` + `autoAllowReadonly` can't coexist in V3 — read-only auto-approval dropped.
    DenyByDefaultReadonly,
    FilePrompt,
    /// A hook KAS can't represent (a CLI tool hook, or an unknown trigger) was dropped.
    UnconvertibleHook,
}

/// A warning about a lossy or ambiguous conversion.
#[typeshare]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationWarning {
    pub kind: MigrationWarningKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Source config field, e.g. `toolsSettings.shell.allowedCommands`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribute: Option<String>,
    /// Emitted glob(s) for regex conversions (empty if unconvertible).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub converted: Option<Vec<String>>,
    /// Rule effect — drives the allow/deny label and deny-all status in diagnostics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect: Option<Effect>,
}

/// Convert CLI command/path patterns into V3 `match` globs, classified by [`regex_to_glob`]:
/// lossless -> emitted; lossy -> emitted + `regex-*-pattern` warning; unconvertible -> *not*
/// emitted (a fabricated glob would be dangerously broad) + `unconvertible-pattern` warning.
/// Filesystem paths are already globs and pass through; a regex may fan out to several globs.
fn convert_patterns(
    patterns: &[String],
    capability: &str,
    warnings: &mut Vec<MigrationWarning>,
    effect: Effect,
) -> Vec<String> {
    let is_regex_capability = capability == "shell" || capability == "web_fetch";
    if !is_regex_capability {
        return patterns.to_vec();
    }

    let regex_kind = if capability == "shell" {
        MigrationWarningKind::RegexShellPattern
    } else {
        MigrationWarningKind::RegexWebPattern
    };
    let attribute = if capability == "shell" {
        format!(
            "toolsSettings.shell.{}",
            if effect == Effect::Deny {
                "deniedCommands"
            } else {
                "allowedCommands"
            }
        )
    } else {
        format!(
            "toolsSettings.web_fetch.{}",
            if effect == Effect::Deny { "blocked" } else { "trusted" }
        )
    };

    let mut out: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for p in patterns {
        let result = regex_to_glob(p, RegexToGlobOptions {
            drop_chaining: capability == "shell",
        });
        if result.fidelity == RegexFidelity::Unconvertible {
            if effect == Effect::Deny {
                // Fail closed: a deny we can't translate must never silently vanish (that would
                // re-grant a command the author explicitly blocked). Deny the whole capability
                // instead; the warning tells the user to narrow it.
                if seen.insert("*".to_string()) {
                    out.push("*".to_string());
                }
                warnings.push(MigrationWarning {
                    kind: MigrationWarningKind::UnconvertiblePattern,
                    detail: Some(p.clone()),
                    attribute: Some(attribute.clone()),
                    converted: Some(vec!["*".to_string()]),
                    effect: Some(effect),
                });
            } else {
                warnings.push(MigrationWarning {
                    kind: MigrationWarningKind::UnconvertiblePattern,
                    detail: Some(p.clone()),
                    attribute: Some(attribute.clone()),
                    converted: Some(vec![]),
                    effect: Some(effect),
                });
            }
            continue;
        }
        if result.fidelity == RegexFidelity::Lossy {
            warnings.push(MigrationWarning {
                kind: regex_kind,
                detail: Some(p.clone()),
                attribute: Some(attribute.clone()),
                converted: Some(result.globs.clone()),
                effect: Some(effect),
            });
        }
        for g in result.globs {
            if seen.insert(g.clone()) {
                out.push(g);
            }
        }
    }
    out
}

/// `exclude` list for a `denyByDefault` catch-all shell deny: each allow glob, plus the bare
/// `<cmd>` form of a `<cmd> *` glob (KAS expands allow matches but not excludes, so without it the
/// catch-all would still block the bare command).
fn shell_deny_exclude_for(allow_globs: &[String]) -> Vec<String> {
    let mut exclude: BTreeSet<String> = BTreeSet::new();
    for g in allow_globs {
        exclude.insert(g.clone());
        if let Some(bare) = g.strip_suffix(" *")
            && !bare.is_empty()
            && !bare.contains('*')
            && !bare.contains('?')
        {
            exclude.insert(bare.to_string());
        }
    }
    exclude.into_iter().collect()
}

/// Expand a bare AWS service name (`s3`) to `s3:*`, matching the runtime `use_aws` matcher; entries
/// already carrying a `:` operator or a `*` wildcard pass through unchanged.
fn normalize_aws_services(services: &[String]) -> Vec<String> {
    services
        .iter()
        .map(|s| {
            if s.contains(':') || s.contains('*') {
                s.clone()
            } else {
                format!("{s}:*")
            }
        })
        .collect()
}

/// Convert `patterns` for `capability`/`effect` and, if any glob survives, push one rule. No-op on
/// an empty input or an all-unconvertible allow list (which yields no globs).
fn push_pattern_rule(
    rules: &mut Vec<V3PermissionRule>,
    capability: &str,
    patterns: &[String],
    effect: Effect,
    warnings: &mut Vec<MigrationWarning>,
) {
    if patterns.is_empty() {
        return;
    }
    let m = convert_patterns(patterns, capability, warnings, effect);
    if m.is_empty() {
        return;
    }
    rules.push(V3PermissionRule {
        capability: capability.to_string(),
        r#match: Some(m),
        exclude: None,
        effect,
    });
}

/// Convert a CLI `toolsSettings` block into V3 `permissions.rules`. Takes the crate's typed
/// [`ToolsSettings`] (deserialized by the caller), so key aliases (`execute_bash`/`shell`,
/// `subagent`/`crew`, `read`/`fs_read`, …) are resolved by the shared serde definitions rather than
/// re-listed here.
pub fn convert_tools_settings(ts: &ToolsSettings, warnings: &mut Vec<MigrationWarning>) -> Option<V3Permissions> {
    let mut rules: Vec<V3PermissionRule> = Vec::new();
    let mut policies: Vec<String> = Vec::new();

    let shell = &ts.shell;
    if shell.auto_allow_readonly {
        if shell.deny_by_default {
            // No V3 runtime read-only detection, and a denyByDefault catch-all (deny > allow) would
            // override the read-only-shell preset — so keep deny-by-default (stronger intent), drop
            // auto-approval, and flag it.
            warnings.push(MigrationWarning {
                kind: MigrationWarningKind::DenyByDefaultReadonly,
                detail: None,
                attribute: Some("toolsSettings.shell".to_string()),
                converted: None,
                effect: None,
            });
        } else {
            policies.push("read-only-shell".to_string());
        }
    }

    let mut allowed_shell_globs: Vec<String> = Vec::new();
    if !shell.allowed_commands.is_empty() {
        allowed_shell_globs = convert_patterns(&shell.allowed_commands, "shell", warnings, Effect::Allow);
        if !allowed_shell_globs.is_empty() {
            rules.push(V3PermissionRule {
                capability: "shell".to_string(),
                r#match: Some(allowed_shell_globs.clone()),
                exclude: None,
                effect: Effect::Allow,
            });
        }
    }
    push_pattern_rule(&mut rules, "shell", &shell.denied_commands, Effect::Deny, warnings);
    if shell.deny_by_default {
        // Catch-all deny whose `exclude` carves out the allow list (deny outranks allow, so an
        // un-excluded allow glob would be clobbered).
        let exclude = shell_deny_exclude_for(&allowed_shell_globs);
        rules.push(V3PermissionRule {
            capability: "shell".to_string(),
            r#match: Some(vec!["*".to_string()]),
            exclude: if exclude.is_empty() { None } else { Some(exclude) },
            effect: Effect::Deny,
        });
    }

    // fs_read capability: the dedicated read settings plus grep/glob, which share the `read` tag
    // and are gated by the same `fs_read` capability in V3.
    push_pattern_rule(
        &mut rules,
        "fs_read",
        &ts.fs_read.allowed_paths,
        Effect::Allow,
        warnings,
    );
    push_pattern_rule(&mut rules, "fs_read", &ts.fs_read.denied_paths, Effect::Deny, warnings);
    push_pattern_rule(&mut rules, "fs_read", &ts.grep.allowed_paths, Effect::Allow, warnings);
    push_pattern_rule(&mut rules, "fs_read", &ts.grep.denied_paths, Effect::Deny, warnings);
    push_pattern_rule(&mut rules, "fs_read", &ts.glob.allowed_paths, Effect::Allow, warnings);
    push_pattern_rule(&mut rules, "fs_read", &ts.glob.denied_paths, Effect::Deny, warnings);

    push_pattern_rule(
        &mut rules,
        "fs_write",
        &ts.fs_write.allowed_paths,
        Effect::Allow,
        warnings,
    );
    push_pattern_rule(
        &mut rules,
        "fs_write",
        &ts.fs_write.denied_paths,
        Effect::Deny,
        warnings,
    );

    push_pattern_rule(&mut rules, "web_fetch", &ts.web_fetch.trusted, Effect::Allow, warnings);
    push_pattern_rule(&mut rules, "web_fetch", &ts.web_fetch.blocked, Effect::Deny, warnings);

    // A bare service (`s3`) is matched as `s3:*` at runtime, so migrate it expanded.
    push_pattern_rule(
        &mut rules,
        "use_aws",
        &normalize_aws_services(&ts.use_aws.allowed_services),
        Effect::Allow,
        warnings,
    );
    push_pattern_rule(
        &mut rules,
        "use_aws",
        &normalize_aws_services(&ts.use_aws.denied_services),
        Effect::Deny,
        warnings,
    );

    // `trustedAgents` (prompt-free allowlist) -> a `subagent` allow rule matched by name.
    // `availableAgents` (discovery scoping) is handled via `tools` tags in the orchestrator; this
    // module only emits the `trustedAgents` rule.
    if !ts.crew.trusted_agents.is_empty() {
        let mut trusted_agents: Vec<String> = ts.crew.trusted_agents.iter().map(agent_identifier_str).collect();
        trusted_agents.sort();
        rules.push(V3PermissionRule {
            capability: "subagent".to_string(),
            r#match: Some(trusted_agents),
            exclude: None,
            effect: Effect::Allow,
        });
    }

    if rules.is_empty() && policies.is_empty() {
        return None;
    }
    Some(V3Permissions {
        rules,
        policies: if policies.is_empty() { None } else { Some(policies) },
    })
}

/// The original string spelling of an `AgentIdentifier` (exact name or glob). `AgentIdentifier`
/// serializes losslessly back to its source string, which is the raw form KAS matches on.
pub(super) fn agent_identifier_str(id: &crate::agent::agent_config::definitions::AgentIdentifier) -> String {
    serde_json::to_value(id)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Tools KAS handles without policy checks — silently skip in allowedTools.
const NO_POLICY_TOOLS: &[&str] = &["knowledge", "todo", "todo_list", "task_list"];

/// Deprecated V2 tools with specific migration messages.
const DEPRECATED_TOOLS: &[&str] = &["aws", "use_aws"];

/// Convert a CLI `allowedTools` list into capability-level `allow` rules (V2 trusts a tool
/// individually; KAS allows its whole capability). `*` -> one `all` allow; `@server[/tool]` -> a
/// merged `mcp` rule; built-ins -> capability rules deduped by capability; NO_POLICY_TOOLS
/// skipped; aws/unmapped flagged.
pub fn convert_allowed_tools(allowed_tools: &Value, warnings: &mut Vec<MigrationWarning>) -> Vec<V3PermissionRule> {
    let is_star = allowed_tools.as_str() == Some("*")
        || allowed_tools
            .as_array()
            .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some("*")));
    if is_star {
        return vec![V3PermissionRule {
            capability: "all".to_string(),
            r#match: None,
            exclude: None,
            effect: Effect::Allow,
        }];
    }

    let arr = match allowed_tools.as_array() {
        Some(a) => a,
        None => return vec![],
    };

    let mut capabilities: BTreeSet<String> = BTreeSet::new();
    let mut mcp_servers: BTreeSet<String> = BTreeSet::new();

    for tool in arr {
        let tool = match tool.as_str() {
            Some(t) => t,
            None => continue,
        };

        // `@server` -> `server/*`, `@server/tool` -> `server/tool` (KAS slash form).
        if let Some(reference) = tool.strip_prefix('@')
            && !reference.is_empty()
        {
            match reference.find('/') {
                Some(idx) if idx > 0 => {
                    mcp_servers.insert(reference.to_string());
                },
                _ => {
                    mcp_servers.insert(format!("{reference}/*"));
                },
            }
            continue;
        }

        if let Some(capability) = v3_cap_by_v2_name(tool) {
            capabilities.insert(capability.to_string());
        } else if NO_POLICY_TOOLS.contains(&tool) {
            // V3 handles these without policy checks — silently skip.
        } else if DEPRECATED_TOOLS.contains(&tool) {
            warnings.push(MigrationWarning {
                kind: MigrationWarningKind::DeprecatedAwsTool,
                detail: Some(tool.to_string()),
                attribute: None,
                converted: None,
                effect: None,
            });
        } else {
            warnings.push(MigrationWarning {
                kind: MigrationWarningKind::UnmappedAllowedTool,
                detail: Some(tool.to_string()),
                attribute: None,
                converted: None,
                effect: None,
            });
        }
    }

    let mut rules: Vec<V3PermissionRule> = capabilities
        .into_iter()
        .map(|capability| V3PermissionRule {
            capability,
            r#match: None,
            exclude: None,
            effect: Effect::Allow,
        })
        .collect();

    if !mcp_servers.is_empty() {
        rules.push(V3PermissionRule {
            capability: "mcp".to_string(),
            r#match: Some(mcp_servers.into_iter().collect()),
            exclude: None,
            effect: Effect::Allow,
        });
    }

    rules
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn kinds(warnings: &[MigrationWarning]) -> Vec<MigrationWarningKind> {
        warnings.iter().map(|w| w.kind).collect()
    }

    // ---- convertPatterns — allow vs deny on unconvertible ----

    #[test]
    fn unconvertible_allow_emits_nothing() {
        let mut warnings = Vec::new();
        let out = convert_patterns(
            &["(?=.*--force).*rm.*".to_string()],
            "shell",
            &mut warnings,
            Effect::Allow,
        );
        assert_eq!(out, Vec::<String>::new());
        assert_eq!(kinds(&warnings), vec![MigrationWarningKind::UnconvertiblePattern]);
    }

    #[test]
    fn unconvertible_deny_fails_closed_to_star() {
        let mut warnings = Vec::new();
        let out = convert_patterns(
            &["(?=.*--force).*rm.*".to_string()],
            "shell",
            &mut warnings,
            Effect::Deny,
        );
        assert_eq!(out, vec!["*".to_string()]);
        assert_eq!(kinds(&warnings), vec![MigrationWarningKind::UnconvertiblePattern]);
    }

    #[test]
    fn deny_with_convertible_and_unconvertible_deny_all_wins() {
        let mut warnings = Vec::new();
        let out = convert_patterns(
            &["git push .*".to_string(), "(?=.*Admin).*".to_string()],
            "shell",
            &mut warnings,
            Effect::Deny,
        );
        assert_eq!(out, vec!["git push *".to_string(), "*".to_string()]);
    }

    // ---- convertToolsSettings — deniedCommands never vanish ----

    #[test]
    fn unconvertible_denied_commands_produce_deny_all_shell_rule() {
        let mut warnings = Vec::new();
        let ts: ToolsSettings =
            serde_json::from_value(json!({ "execute_bash": { "deniedCommands": [".*&&.*rm -rf.*"] } })).unwrap();
        let perms = convert_tools_settings(&ts, &mut warnings);
        let deny_rules: Vec<&V3PermissionRule> = perms
            .as_ref()
            .map(|p| p.rules.iter().filter(|r| r.effect == Effect::Deny).collect())
            .unwrap_or_default();
        assert_eq!(deny_rules, vec![&V3PermissionRule {
            capability: "shell".to_string(),
            r#match: Some(vec!["*".to_string()]),
            exclude: None,
            effect: Effect::Deny,
        }]);
        assert!(
            warnings
                .iter()
                .any(|w| w.kind == MigrationWarningKind::UnconvertiblePattern)
        );
    }

    // ---- grep/glob paths fold into the fs_read capability ----

    #[test]
    fn grep_and_glob_paths_produce_fs_read_rules() {
        let mut warnings = Vec::new();
        let ts: ToolsSettings = serde_json::from_value(json!({
            "grep": { "allowedPaths": ["src/**"] },
            "glob": { "deniedPaths": ["secret/**"] },
        }))
        .unwrap();
        let perms = convert_tools_settings(&ts, &mut warnings).expect("rules");
        assert!(perms.rules.contains(&V3PermissionRule {
            capability: "fs_read".to_string(),
            r#match: Some(vec!["src/**".to_string()]),
            exclude: None,
            effect: Effect::Allow,
        }));
        assert!(perms.rules.contains(&V3PermissionRule {
            capability: "fs_read".to_string(),
            r#match: Some(vec!["secret/**".to_string()]),
            exclude: None,
            effect: Effect::Deny,
        }));
    }

    #[test]
    fn use_aws_services_produce_use_aws_rules() {
        let mut warnings = Vec::new();
        let ts: ToolsSettings =
            serde_json::from_value(json!({ "use_aws": { "allowedServices": ["s3", "ec2"] } })).unwrap();
        let perms = convert_tools_settings(&ts, &mut warnings).expect("rules");
        // Bare services expand to `<svc>:*` to match the runtime use_aws matcher.
        assert!(perms.rules.contains(&V3PermissionRule {
            capability: "use_aws".to_string(),
            r#match: Some(vec!["s3:*".to_string(), "ec2:*".to_string()]),
            exclude: None,
            effect: Effect::Allow,
        }));
    }

    #[test]
    fn use_aws_fine_grained_pattern_passes_through() {
        let mut warnings = Vec::new();
        let ts: ToolsSettings =
            serde_json::from_value(json!({ "use_aws": { "allowedServices": ["s3:get-*", "ec2"] } })).unwrap();
        let perms = convert_tools_settings(&ts, &mut warnings).expect("rules");
        assert!(perms.rules.contains(&V3PermissionRule {
            capability: "use_aws".to_string(),
            r#match: Some(vec!["s3:get-*".to_string(), "ec2:*".to_string()]),
            exclude: None,
            effect: Effect::Allow,
        }));
    }
}
