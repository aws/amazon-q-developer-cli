//! Probe the collector's `:13133/health` endpoint with a retry+timeout loop.
//!
//! The health extension only listens once the rest of the pipeline is ready,
//! so a single GET that succeeds is sufficient evidence the collector is
//! accepting OTLP on `:14318`.

use std::time::{
    Duration,
    Instant,
};

use reqwest::Client;
use tracing::debug;

use crate::error::CollectorError;

/// Poll interval between probe attempts.
const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Probe `endpoint` (typically `http://127.0.0.1:13133/health`) every
/// [`POLL_INTERVAL`] until it returns a 2xx response or `total_timeout`
/// elapses. Returns `Ok(())` on first success.
pub async fn probe(endpoint: &str, total_timeout: Duration) -> Result<(), CollectorError> {
    let client = build_client();
    probe_with_client(&client, endpoint, total_timeout).await
}

/// As [`probe`] but accepts an external `Client`. Exposed so the lifecycle
/// layer can reuse a single client across multiple probe windows in the same
/// `ensure_running` call.
pub async fn probe_with_client(client: &Client, endpoint: &str, total_timeout: Duration) -> Result<(), CollectorError> {
    let start = Instant::now();
    let mut last_status: Option<u16> = None;
    loop {
        match probe_once(client, endpoint).await {
            ProbeOutcome::Healthy => {
                debug!(
                    endpoint,
                    elapsed_ms = start.elapsed().as_millis() as u64,
                    "collector healthy"
                );
                return Ok(());
            },
            ProbeOutcome::BadStatus(code) => {
                last_status = Some(code);
                debug!(endpoint, status = code, "non-2xx, retrying");
            },
            ProbeOutcome::ConnectError => {
                debug!(endpoint, "connect error, retrying");
            },
        }
        if start.elapsed() >= total_timeout {
            if let Some(code) = last_status
                && !(200..300).contains(&code)
            {
                return Err(CollectorError::HealthEndpointBadStatus { status: code });
            }
            return Err(CollectorError::HealthProbeTimeout {
                endpoint: endpoint.to_string(),
                elapsed_ms: start.elapsed().as_millis() as u64,
            });
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Single-shot probe outcome.
#[derive(Debug)]
enum ProbeOutcome {
    Healthy,
    BadStatus(u16),
    ConnectError,
}

async fn probe_once(client: &Client, endpoint: &str) -> ProbeOutcome {
    match client.get(endpoint).send().await {
        Ok(resp) => {
            let s = resp.status().as_u16();
            if (200..300).contains(&s) {
                ProbeOutcome::Healthy
            } else {
                ProbeOutcome::BadStatus(s)
            }
        },
        Err(_) => ProbeOutcome::ConnectError,
    }
}

fn build_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(2))
        .connect_timeout(Duration::from_millis(500))
        .build()
        .unwrap_or_else(|_| Client::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn probe_invalid_url_times_out() {
        // 127.0.0.1 with a port that's vanishingly unlikely to be bound.
        let r = probe("http://127.0.0.1:1/health", Duration::from_millis(300)).await;
        let err = r.expect_err("should fail");
        assert!(matches!(err, CollectorError::HealthProbeTimeout { .. }), "got: {err:?}");
    }
}
