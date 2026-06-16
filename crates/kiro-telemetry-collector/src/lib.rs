//! kiro-telemetry-collector: lifecycle for a local `otelcol-contrib` binary.
//!
//! This crate manages a per-user singleton OpenTelemetry Collector that
//! kiro-cli's SDK exports OTLP/HTTP to over loopback. The collector runs as a
//! detached child process (orphaned via `setsid` on unix) so it lives across
//! kiro-cli invocations; coordination is done via a state directory under
//! `$HOME/.kiro/telemetry/collector`:
//!
//! - `collector.lock` — fs4 advisory exclusive lock taken by every public entry point so concurrent
//!   kiro-cli invocations serialize through here.
//! - `collector.pid.json` — `PidRecord` written atomically after spawn.
//! - `collector.yaml` + `collector.fingerprint` — last rendered config and its SHA-256. A change in
//!   the fingerprint forces a respawn rather than reuse of a stale collector that was started
//!   against a different config.
//! - `queue/` — file-storage extension's persistent OTLP send queue.
//! - `crashes/` — drop dir for crash signature JSON; tailed by the filelog receiver so crashes
//!   survive the kiro-cli process death.
//!
//! Public surface:
//!
//! - [`ensure_running`] — idempotent: reuse a healthy collector with matching config or spawn a new
//!   one.
//! - [`status`] — diagnostic snapshot.
//! - [`stop`] — cooperative shutdown (SIGTERM → SIGKILL).
//! - [`StateDir`] — encapsulates state-dir paths.
//! - [`CollectorHandle`] / [`CollectorStatus`] — return types.
//! - [`CollectorError`] — error taxonomy.

mod binary;
mod config;
mod error;
mod health;
mod lifecycle;
mod state;

pub use error::CollectorError;
pub use lifecycle::{
    CollectorHandle,
    CollectorStatus,
    ensure_running,
    status,
    stop,
};
pub use state::{
    PidRecord,
    StateDir,
};

/// Default loopback OTLP endpoint that kiro-cli's SDK targets and the local
/// collector binds. Uses 14318 instead of 4318 to avoid conflicts with other
/// agents (Datadog, Honeycomb, dev environments) on user machines.
pub const DEFAULT_LOOPBACK_OTLP_ENDPOINT: &str = lifecycle::DEFAULT_LOOPBACK_OTLP_ENDPOINT;

/// Default health-check endpoint for the spawned collector.
pub const DEFAULT_HEALTH_ENDPOINT: &str = lifecycle::DEFAULT_HEALTH_ENDPOINT;
