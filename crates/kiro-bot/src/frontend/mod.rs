//! Frontend implementations for kiro-bot.
//!
//! - [`slack`] — Slack Socket Mode frontend (production, long-running)
//! - [`cli`] — Interactive CLI frontend (local testing)
//! - [`cron`] — Headless single-prompt frontend (cron jobs)

pub mod cli;
pub mod cron;
pub mod slack;
