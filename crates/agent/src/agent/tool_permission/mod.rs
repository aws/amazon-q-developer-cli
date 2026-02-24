//! Tool permission utilities.

pub mod file_trust;
mod pattern_matcher;

pub use pattern_matcher::{
    PatternMode,
    Rule,
    RuleAction,
    match_rules,
    validate_regex,
};
