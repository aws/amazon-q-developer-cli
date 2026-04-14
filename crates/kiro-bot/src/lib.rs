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

pub mod config;
pub mod engine;
pub mod frontend;
