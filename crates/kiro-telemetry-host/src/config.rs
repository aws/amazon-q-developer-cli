//! Host-side telemetry configuration.
//!
//! `HostConfig` is the data-first input to [`crate::thread::TelemetryThread::new`].
//! All values are pre-resolved by the caller (V1/V2's `Os::new`, V3's observer, etc.);
//! the host crate performs no env/database reads of its own. The single optional
//! [`LegacySink`] trait abstracts the V1/V2 Toolkit/CodeWhisperer post paths so
//! the host crate stays free of AWS-SDK dependencies.

use std::sync::Arc;

use futures::future::BoxFuture;
use kiro_telemetry::metric::{
    ClientApplication,
    Engine,
};
use kiro_telemetry::{
    MetricRecord,
    TelemetryConfig,
};
use uuid::{
    Uuid,
    uuid,
};

use crate::event::Event;
use crate::process::ProcessIdentity;

/// Sentinel client id used when telemetry is disabled.
const TELEMETRY_DISABLED_CLIENT_ID: Uuid = uuid!("ffffffff-ffff-ffff-ffff-ffffffffffff");

/// V3 / kiro-bot will introduce additional variants in PR L.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HostRole {
    #[default]
    UserCli,
}

/// Optional legacy sink consumed by [`crate::thread::TelemetryThread`].
///
/// V1 and V2 implement this over their Toolkit + CodeWhisperer post paths.
/// V3 / kiro-bot / tests pass `None`.
pub trait LegacySink: Send + Sync + std::fmt::Debug {
    fn send_event(&self, event: Event) -> BoxFuture<'_, ()>;
    fn send_event_govcloud(&self, event: Event, partition: &'static str) -> BoxFuture<'_, ()>;
}

/// Translates a host-level [`Event`] into OTel records. V1 and V2 provide
/// implementations backed by `kiro-telemetry-legacy`; V3 and lite harnesses
/// can leave [`HostConfig::otel_translator`] as `None` to skip OTel emission.
pub trait OtelEventTranslator: Send + Sync + std::fmt::Debug {
    fn metric_records(&self, event: &Event) -> Vec<MetricRecord>;
}

/// Async closure that enriches an outbound [`Event`] with caller-supplied
/// session/auth metadata (start URL, SSO region, client application, ...).
///
/// V2 builds one of these capturing `Arc<Database>`; V3 / kiro-bot / tests
/// pass `None`. The HRTB lifetime lets the closure body hold `&mut Event`
/// across `.await` without forcing the caller to clone the event.
pub type EventEnricher = Arc<dyn for<'a> Fn(&'a mut Event) -> BoxFuture<'a, ()> + Send + Sync>;

/// Pre-resolved configuration handed to `TelemetryThread::new`.
#[derive(Clone)]
pub struct HostConfig {
    /// Resolved at startup: env > database > new uuid.
    pub client_id: Uuid,
    /// Already-resolved telemetry-enabled gate.
    pub telemetry_enabled: bool,
    /// OTel config — caller builds via `TelemetryConfig::from_env() + .with_machine_id(...)`.
    pub otel_config: TelemetryConfig,
    /// Optional legacy sink — `Some(_)` for V1/V2, `None` for V3 / kiro-bot / tests.
    pub legacy_sink: Option<Arc<dyn LegacySink>>,
    /// Optional OTel translator. `Some(_)` enables OTel emission; `None` skips it.
    pub otel_translator: Option<Arc<dyn OtelEventTranslator>>,
    /// Optional per-event metadata enrichment closure. `None` skips enrichment.
    pub metadata_enricher: Option<EventEnricher>,
    /// Forwarded into per-event enrichment by the caller-side observer (PR E).
    pub client_application: Option<ClientApplication>,
    /// Canonical architecture applied to events that do not already carry one.
    pub engine: Option<Engine>,
    /// Reserved for PR L; defaults to `UserCli`.
    pub host_role: HostRole,
    /// Physical process identity for the shared process sampler.
    pub process_identity: Option<ProcessIdentity>,
    /// `Some("aws-us-gov")` disables the legacy-sink branch.
    pub govcloud_partition: Option<&'static str>,
}

impl std::fmt::Debug for HostConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostConfig")
            .field("client_id", &self.client_id)
            .field("telemetry_enabled", &self.telemetry_enabled)
            .field("otel_config", &self.otel_config)
            .field("legacy_sink", &self.legacy_sink)
            .field("otel_translator", &self.otel_translator)
            .field(
                "metadata_enricher",
                &self.metadata_enricher.as_ref().map(|_| "<closure>"),
            )
            .field("client_application", &self.client_application)
            .field("engine", &self.engine)
            .field("host_role", &self.host_role)
            .field("process_identity", &self.process_identity)
            .field("govcloud_partition", &self.govcloud_partition)
            .finish()
    }
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            client_id: TELEMETRY_DISABLED_CLIENT_ID,
            telemetry_enabled: false,
            otel_config: TelemetryConfig::new(false, kiro_telemetry::OtelMode::Off, None, std::env::temp_dir()),
            legacy_sink: None,
            otel_translator: None,
            metadata_enricher: None,
            client_application: None,
            engine: None,
            host_role: HostRole::UserCli,
            process_identity: None,
            govcloud_partition: None,
        }
    }
}

/// US GovCloud partition string.
pub const US_GOV_PARTITION: &str = "aws-us-gov";

/// Returns `Some("aws-us-gov")` for known GovCloud regions, `None` otherwise.
pub fn govcloud_partition(region: &str) -> Option<&'static str> {
    match region {
        "us-gov-east-1" | "us-gov-west-1" => Some(US_GOV_PARTITION),
        _ => None,
    }
}
