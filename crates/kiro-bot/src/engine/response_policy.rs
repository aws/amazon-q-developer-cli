//! Per-conversation response policies.
//!
//! Controls when the bot responds (trigger) and where it replies (location).
//! Policies are matched by scope using glob patterns: `"dm:*"`, `"channel:C123"`, `"*"`.

use anyhow::bail;
use regex::Regex;
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::json;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    Always,
    MentionOnly,
    DirectedOnly,
    ThreadOnly,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Location {
    Same,
    Thread,
    Dm,
}

fn default_thread_pattern() -> Option<String> {
    Some("🧵|:thread:".into())
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResponsePolicy {
    pub scope: String,
    pub trigger: Trigger,
    pub location: Location,
    #[serde(default = "default_thread_pattern")]
    pub thread_pattern: Option<String>,
}

#[derive(Debug)]
pub struct CompiledPolicy {
    pub scope: String,
    pub trigger: Trigger,
    pub location: Location,
    pub thread_re: Option<Regex>,
}

/// Compiled set of response policies with glob-based scope matching.
#[derive(Debug)]
pub struct ResponsePolicyConfig {
    pub policies: Vec<CompiledPolicy>,
}

fn schema() -> serde_json::Value {
    json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "Response Policy Configuration",
        "type": "object",
        "required": ["response_policies"],
        "additionalProperties": false,
        "properties": {
            "response_policies": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["scope", "trigger", "location"],
                    "additionalProperties": false,
                    "properties": {
                        "scope": { "type": "string", "pattern": "^(dm|global|channel:[A-Z0-9]+)$" },
                        "trigger": { "type": "string", "enum": ["always", "mention_only", "thread_only"] },
                        "location": { "type": "string", "enum": ["same", "thread", "dm"] },
                        "thread_pattern": { "type": "string" }
                    }
                }
            }
        }
    })
}

#[derive(Deserialize)]
struct RawConfig {
    response_policies: Vec<ResponsePolicy>,
}

/// Glob matching: `"dm:*"` matches `"dm"` and `"dm:anything"`, `"*"` matches everything.
fn scope_glob_matches(pattern: &str, scope: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix(":*") {
        scope == prefix || scope.starts_with(&format!("{}:", prefix))
    } else {
        false
    }
}

impl ResponsePolicyConfig {
    /// Default policy: respond to everything in DMs, mention-only in channels.
    pub fn default_policy() -> Self {
        Self::from_policies(vec![
            ResponsePolicy {
                scope: "dm:*".into(),
                trigger: Trigger::Always,
                location: Location::Same,
                thread_pattern: None,
            },
            ResponsePolicy {
                scope: "global".into(),
                trigger: Trigger::MentionOnly,
                location: Location::Same,
                thread_pattern: None,
            },
        ])
        .unwrap()
    }

    /// Compile a list of response policies, validating regex patterns.
    pub fn from_policies(policies: Vec<ResponsePolicy>) -> anyhow::Result<Self> {
        let policies = policies
            .into_iter()
            .map(|p| {
                let thread_re = p
                    .thread_pattern
                    .as_deref()
                    .map(Regex::new)
                    .transpose()
                    .map_err(|e| anyhow::anyhow!("invalid thread_pattern in scope '{}': {e}", p.scope))?;
                Ok(CompiledPolicy {
                    scope: p.scope,
                    trigger: p.trigger,
                    location: p.location,
                    thread_re,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(Self { policies })
    }

    /// Load and validate policies from a JSON file.
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;
        if let Err(e) = jsonschema::draft7::validate(&schema(), &value) {
            bail!("response policy validation failed: {e}");
        }
        let raw: RawConfig = serde_json::from_value(value)?;
        Self::from_policies(raw.response_policies)
    }

    fn find_policy(&self, scope: &str) -> Option<&CompiledPolicy> {
        self.policies
            .iter()
            .find(|p| p.scope == scope)
            .or_else(|| self.policies.iter().find(|p| scope_glob_matches(&p.scope, scope)))
            .or_else(|| self.policies.iter().find(|p| p.scope == "global" || p.scope == "*"))
    }

    /// Determine whether the bot should respond to a message in the given scope.
    pub fn should_respond(&self, scope: &str, directed: bool, is_thread: bool) -> bool {
        match self.find_policy(scope) {
            Some(p) => match p.trigger {
                Trigger::Always => true,
                Trigger::MentionOnly | Trigger::DirectedOnly => directed,
                Trigger::ThreadOnly => is_thread,
            },
            None => directed,
        }
    }

    /// Get the reply location for a given scope.
    pub fn reply_location(&self, scope: &str) -> Location {
        self.find_policy(scope)
            .map(|p| p.location.clone())
            .unwrap_or(Location::Same)
    }

    /// Check if a message should force a thread reply based on pattern matching.
    pub fn force_thread(&self, scope: &str, text: &str) -> bool {
        self.find_policy(scope)
            .and_then(|p| p.thread_re.as_ref())
            .map(|re| re.is_match(text))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(scope: &str, trigger: Trigger, location: Location) -> ResponsePolicy {
        ResponsePolicy {
            scope: scope.into(),
            trigger,
            location,
            thread_pattern: None,
        }
    }

    fn config(policies: Vec<ResponsePolicy>) -> ResponsePolicyConfig {
        ResponsePolicyConfig::from_policies(policies).unwrap()
    }

    #[test]
    fn always_trigger_responds_to_everything() {
        let cfg = config(vec![policy("dm", Trigger::Always, Location::Same)]);
        assert!(cfg.should_respond("dm", false, false));
    }

    #[test]
    fn mention_only_requires_mention() {
        let cfg = config(vec![policy("global", Trigger::MentionOnly, Location::Same)]);
        assert!(!cfg.should_respond("channel:C123", false, false));
        assert!(cfg.should_respond("channel:C123", true, false));
    }

    #[test]
    fn dm_glob_matches_dm_scope() {
        let cfg = config(vec![policy("dm:*", Trigger::Always, Location::Thread)]);
        assert!(cfg.should_respond("dm:U123", false, false));
    }

    #[test]
    fn no_matching_policy_defaults_to_mention_only() {
        let cfg = config(vec![]);
        assert!(!cfg.should_respond("channel:C123", false, false));
        assert!(cfg.should_respond("channel:C123", true, false));
    }

    #[test]
    fn force_thread_matches_pattern() {
        let cfg = ResponsePolicyConfig::from_policies(vec![ResponsePolicy {
            scope: "global".into(),
            trigger: Trigger::Always,
            location: Location::Same,
            thread_pattern: Some("🧵|:thread:".into()),
        }])
        .unwrap();
        assert!(cfg.force_thread("channel:C123", "hello 🧵"));
        assert!(!cfg.force_thread("channel:C123", "normal message"));
    }
}
