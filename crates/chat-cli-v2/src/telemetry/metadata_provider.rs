//! V2 factories for the host-level [`EventEnricher`] and observer-level
//! [`ReasonExtractor`] closures.
//!
//! The enricher captures `Arc<Database>` so each call re-reads the current
//! SSO start URL / region before stamping outgoing telemetry events. The
//! reason extractor downcasts a host-level [`StreamError`] to V2's
//! [`ConverseStreamError`] for a richer reason code than the
//! `StreamErrorKind`-only fallback.

use std::sync::Arc;

use agent::agent_loop::types::StreamError;
use futures::future::BoxFuture;
use kiro_telemetry_host::{
    Event,
    EventEnricher,
};
use kiro_telemetry_observer::ReasonExtractor;

use crate::api_client::error::ConverseStreamError;
use crate::auth::builder_id::get_start_url_and_region;
use crate::database::Database;
use crate::telemetry::ReasonCode;
use crate::util::env_var::get_cli_client_application;

/// Build the V2 metadata enricher closure threaded into `HostConfig` and
/// `TelemetryObserver::spawn`. Captures `Arc<Database>` so each call re-reads
/// the current SSO start_url/region before stamping the event.
pub fn build_metadata_enricher(database: Arc<Database>) -> EventEnricher {
    Arc::new(move |event: &mut Event| -> BoxFuture<'_, ()> {
        let database = Arc::clone(&database);
        Box::pin(async move {
            let (start_url, region) = get_start_url_and_region(&database).await;
            if let Some(start_url) = start_url {
                event.set_start_url(start_url);
            }
            if let Some(region) = region {
                event.set_sso_region(region);
            }
            if let Some(client_app) = get_cli_client_application() {
                event.set_client_application(client_app);
            }
        })
    })
}

/// Build the V2 reason-extractor closure passed to `TelemetryObserver::spawn`.
/// Downcasts to `ConverseStreamError` and returns its reason code, falling
/// back to the `StreamErrorKind`-only mapping when the typed source is
/// unavailable.
pub fn build_reason_extractor() -> ReasonExtractor {
    Arc::new(|err: &StreamError| -> Option<(String, String, Option<u16>)> {
        let cse = err.as_concrete_error::<ConverseStreamError>()?;
        Some((cse.reason_code(), err.to_string(), cse.status_code))
    })
}

/// Free helper that mirrors the legacy `set_event_metadata` shape, used by
/// callers that don't yet hold an [`EventEnricher`] (e.g. KAS emit helpers in
/// `acp_agent.rs`).
pub async fn set_event_metadata(database: &Database, event: &mut Event) {
    let (start_url, region) = get_start_url_and_region(database).await;
    if let Some(start_url) = start_url {
        event.set_start_url(start_url);
    }
    if let Some(region) = region {
        event.set_sso_region(region);
    }
    if let Some(client_app) = get_cli_client_application() {
        event.set_client_application(client_app);
    }
}
