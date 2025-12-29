use glob::Pattern;
use rand::prelude::IndexedRandom;
use regex::Regex;
use serde::Deserialize;

use crate::cli::agent::PermissionEvalResult;

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum ResourceType {
    Url,
    Command,
    FilePath,
}

#[derive(Debug, Default, Deserialize)]
pub struct ResourceSettings {
    #[serde(default)]
    pub trusted: Vec<String>,
    #[serde(default)]
    pub blocked: Vec<String>,
}

/// Result of evaluating a pattern against a value
struct EvaluatedPattern {
    pattern: String,
    matched: bool,
    invalid: bool,
}

/// Result of permission denial with evaluated patterns
struct DenialInfo {
    patterns: Vec<EvaluatedPattern>,
}

impl DenialInfo {
    /// Format for legacy Vec<String> output (sampled to 100)
    fn to_legacy_format(&self) -> Vec<String> {
        let matched: Vec<_> = self.patterns.iter().filter(|p| p.matched).collect();
        let unmatched: Vec<_> = self.patterns.iter().filter(|p| !p.matched).collect();

        let mut result = matched;
        let remaining_slots = 100_usize.saturating_sub(result.len());

        if unmatched.len() <= remaining_slots {
            result.extend(unmatched);
        } else {
            let mut rng = rand::rng();
            let sampled: Vec<_> = unmatched
                .as_slice()
                .choose_multiple(&mut rng, remaining_slots)
                .copied()
                .collect();
            result.extend(sampled);
        }

        result
            .iter()
            .map(|p| {
                let mut s = String::new();
                if p.matched {
                    s.push_str("[MATCHED] ");
                }
                s.push_str(&p.pattern);
                if p.invalid {
                    s.push_str(" [INVALID]");
                }
                s
            })
            .collect()
    }
}

enum CompiledMatcher {
    Regex(Regex),
    Glob(Pattern),
}

struct ResourcePattern {
    pattern: String,
    matcher: Option<CompiledMatcher>,
}

impl ResourcePattern {
    fn compile(pattern: &str, resource_type: ResourceType) -> Self {
        let matcher = match resource_type {
            ResourceType::Url | ResourceType::Command => {
                // Only add anchors if not already present
                let anchored = match (pattern.starts_with('^'), pattern.ends_with('$')) {
                    (true, true) => pattern.to_string(),
                    (true, false) => format!("{pattern}$"),
                    (false, true) => format!("^{pattern}"),
                    (false, false) => format!("^{pattern}$"),
                };
                Regex::new(&anchored).map(CompiledMatcher::Regex).ok()
            },
            ResourceType::FilePath => Pattern::new(pattern).map(CompiledMatcher::Glob).ok(),
        };
        Self {
            pattern: pattern.to_string(),
            matcher,
        }
    }

    fn is_match(&self, value: &str) -> bool {
        match &self.matcher {
            Some(CompiledMatcher::Regex(r)) => r.is_match(value),
            Some(CompiledMatcher::Glob(g)) => g.matches(value),
            None => true, // invalid = blocks all
        }
    }

    fn is_invalid(&self) -> bool {
        self.matcher.is_none()
    }

    fn evaluate(&self, value: &str) -> EvaluatedPattern {
        EvaluatedPattern {
            pattern: self.pattern.clone(),
            matched: self.is_match(value),
            invalid: self.is_invalid(),
        }
    }
}

/// Evaluate permission for a resource value against settings
/// `is_tool_allowed` indicates if the tool is in allowedTools list
pub fn eval_permission(
    settings: &ResourceSettings,
    value: &str,
    resource_type: ResourceType,
    is_tool_allowed: bool,
) -> PermissionEvalResult {
    // 1. Check blocked - highest priority
    let blocked: Vec<ResourcePattern> = settings
        .blocked
        .iter()
        .map(|p| ResourcePattern::compile(p, resource_type))
        .collect();
    if blocked.iter().any(|p| p.is_match(value)) {
        let evaluated: Vec<EvaluatedPattern> = blocked.iter().map(|p| p.evaluate(value)).collect();
        let denial = DenialInfo { patterns: evaluated };
        return PermissionEvalResult::Deny(denial.to_legacy_format());
    }

    // 2. Check trusted (skip invalid patterns)
    let trusted: Vec<ResourcePattern> = settings
        .trusted
        .iter()
        .map(|p| ResourcePattern::compile(p, resource_type))
        .collect();
    if trusted.iter().any(|p| !p.is_invalid() && p.is_match(value)) {
        return PermissionEvalResult::Allow;
    }

    // 3. Default: depends on whether tool is in allowedTools
    if is_tool_allowed {
        PermissionEvalResult::Allow
    } else {
        PermissionEvalResult::Ask
    }
}
