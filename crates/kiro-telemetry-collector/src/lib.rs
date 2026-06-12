//! kiro-telemetry-collector: lifecycle for a local otelcol-contrib binary.
//!
//! Subsequent PRs add ensure_running, run/status/stop commands, default_endpoint,
//! lock/PID/config coordination, and platform-specific spawn helpers. The
//! first implementation starts and reuses this collector on demand from the
//! existing Kiro installation rather than registering login services. The
//! kiro-cli SDK exports OTLP/HTTP to localhost:14318 (the kiro-namespaced port)
//! where this collector listens by default.

/// Default loopback OTLP endpoint that kiro-cli's SDK targets and the local
/// collector binds. Uses 14318 instead of 4318 to avoid conflicts with other
/// agents (Datadog, Honeycomb, dev environments) on user machines.
pub const DEFAULT_LOOPBACK_OTLP_ENDPOINT: &str = "http://127.0.0.1:14318";
