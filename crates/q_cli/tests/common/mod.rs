#![allow(unused_imports, dead_code)]

use std::process::Command;

pub use assert_cmd::prelude::*;
use fig_util::CLI_CRATE_NAME;
use predicates::function::FnPredicate;
pub use predicates::prelude::*;

pub type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;

pub fn cli() -> Command {
    Command::cargo_bin(CLI_CRATE_NAME).unwrap()
}

pub fn is_json() -> FnPredicate<impl Fn(&str) -> bool, str> {
    predicates::function::function(|s: &str| serde_json::from_str::<serde_json::Value>(s).is_ok())
}
