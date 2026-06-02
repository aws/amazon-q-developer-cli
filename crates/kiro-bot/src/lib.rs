//! # kiro-bot
//!
//! Bot runtime for Kiro CLI. Manages ACP-backed agent instances as daemons
//! with pluggable frontends — interactive Slack bots, scheduled headless jobs,
//! or one-shot CLI tasks.
//!
//! ## Modules
//!
//! - [`config`] — TOML configuration and secrets loading
//! - [`engine`] — Bot runtime core (ACP pool, dispatch, authz, policies)
//! - [`frontend`] — I/O adapters (Slack, CLI, cron)

pub mod agents;
pub mod cli;
pub mod config;
pub mod engine;
pub mod frontend;

#[cfg(test)]
mod version_test {
    /// `env!("CARGO_PKG_VERSION")` is sent to KAS in the ACP
    /// `Implementation.version` field (see `engine::acp`). The `0.0.0-dev`
    /// placeholder shipped in `Cargo.toml` must never leak into built
    /// binaries — `crates/kiro-bot/build.rs::inject_kiro_version()` overrides it.
    /// Don't remove this test without removing that override.
    #[test]
    fn cli_version_is_not_dev_placeholder() {
        assert_ne!(
            env!("CARGO_PKG_VERSION"),
            "0.0.0-dev",
            "build.rs failed to inject a real version — KAS will see the placeholder in ACP init"
        );
    }
}
