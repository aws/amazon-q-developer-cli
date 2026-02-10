//! Tool permission utilities.

mod pattern_matcher;

pub use pattern_matcher::{
    PatternMode,
    Rule,
    RuleAction,
    match_rules,
};
