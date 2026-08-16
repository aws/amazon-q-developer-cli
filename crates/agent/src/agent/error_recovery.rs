//! Agent-layer retry policy for transient backend failures.
//!
//! The transport SDK already makes a few attempts of its own, so a failure that
//! reaches the agent layer is normally treated as terminal. Throttling, 5xx, and
//! transport-level drops are in fact recoverable, so this adds one last bounded,
//! backoff-spaced retry budget on top of the SDK's. The budget is deliberately
//! small: a struggling backend must not be hammered, and a server-provided
//! `Retry-After` is always honored.
//!
//! The concrete error enums differ per engine, so each engine maps its own error
//! into [`TransientErrorClass`]; the classification predicates, backoff schedule,
//! and retry cap here are shared so the recovery behavior can not drift apart.

use std::error::Error;
use std::time::Duration;

use rand::RngExt as _;

/// Why a backend failure is transient (safe to retry at the agent layer).
///
/// Only constructed once a caller has ruled out terminal conditions (auth,
/// validation, quota, overflow), so the presence of a value already
/// means "retry this".
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransientErrorClass {
    /// HTTP 429 / `ThrottlingException`.
    Throttle,
    /// HTTP 5xx (`InternalServerError` / `ServiceUnavailableError`).
    ServerError,
    /// Transport-level failure that produced no usable response: connection
    /// reset, unexpected EOF, TLS peer-close, broken pipe, or DNS failure.
    Network,
}

impl TransientErrorClass {
    /// Stable label for telemetry attributes.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Throttle => "throttle",
            Self::ServerError => "server_error",
            Self::Network => "network",
        }
    }
}

/// Maximum agent-layer retries for a transient failure within a single request.
///
/// Deliberately as small as the stream-timeout continuation bound
/// ([`super::consts::MAX_STREAM_TIMEOUT_RETRIES`]) but an independent budget — this is a last
/// gentle backstop after the SDK's own attempts, not an aggressive retry storm.
/// Raising it requires widening the telemetry `attempt_number_bucket` range
/// (`AttemptNumberBucket`); a guard test in `kiro-telemetry-observer` ties the two.
pub const MAX_TRANSIENT_RETRIES: usize = 2;

/// Ceiling applied to any single backoff wait, including a server-provided
/// `Retry-After`, so a bogus or hostile header can not park a turn indefinitely.
const MAX_TRANSIENT_BACKOFF: Duration = Duration::from_secs(120);

/// Base unit of the exponential backoff schedule.
const TRANSIENT_BACKOFF_BASE: Duration = Duration::from_secs(2);

/// How long to wait before the next agent-layer retry.
///
/// A server-provided `retry_after` always wins (clamped to the ceiling); otherwise
/// an exponential schedule `base * 2^attempt` is used, jittered by up to 25% to
/// avoid synchronized retries across clients, and clamped to the same ceiling.
/// `attempt` is zero-based (0 for the first retry).
pub fn transient_backoff(attempt: usize, retry_after: Option<Duration>) -> Duration {
    if let Some(retry_after) = retry_after {
        return retry_after.min(MAX_TRANSIENT_BACKOFF);
    }
    let base_ms = TRANSIENT_BACKOFF_BASE.as_millis() as u64;
    let scaled = base_ms.saturating_mul(1u64 << attempt.min(10));
    let jitter = (scaled as f64 * 0.25 * rand::rng().random::<f64>()) as u64;
    Duration::from_millis(scaled.saturating_add(jitter)).min(MAX_TRANSIENT_BACKOFF)
}

/// Parses a `Retry-After` header value in the delay-seconds form (e.g. `"120"`).
///
/// The HTTP-date form is intentionally not supported (returns `None`): the backend
/// emits delay-seconds, and falling back to the computed schedule on an unparseable
/// value is safe.
pub fn parse_retry_after(value: &str) -> Option<Duration> {
    value.trim().parse::<u64>().ok().map(Duration::from_secs)
}

/// Whether `err`, or any error in its source chain, is a transport-level failure
/// that produced no response and is therefore safe to retry.
///
/// Mid-stream connection drops and TLS peer-closes surface as nested I/O or
/// transport errors that the engines otherwise collapse to an opaque terminal
/// error; this recovers them into [`TransientErrorClass::Network`].
pub fn is_transient_network_error(err: &(dyn Error + 'static)) -> bool {
    let mut current: Option<&(dyn Error + 'static)> = Some(err);
    while let Some(source) = current {
        if let Some(io) = source.downcast_ref::<std::io::Error>() {
            use std::io::ErrorKind::{
                BrokenPipe,
                ConnectionAborted,
                ConnectionReset,
                TimedOut,
                UnexpectedEof,
            };
            // TimedOut is deliberately transient even though the SDK has already spent
            // its own timeout budget by the time an error reaches this layer: a timed-out
            // request produced no response, and the alternative is failing a long turn
            // outright. The bounded MAX_TRANSIENT_RETRIES budget caps the extra spend.
            if matches!(
                io.kind(),
                ConnectionReset | ConnectionAborted | BrokenPipe | UnexpectedEof | TimedOut
            ) {
                return true;
            }
        }
        if message_has_network_marker(&source.to_string()) {
            return true;
        }
        current = source.source();
    }
    false
}

/// Conservative substring match for transport-drop signatures that do not surface
/// as a typed [`std::io::Error`] (e.g. hyper/rustls wrapping the cause in a string).
///
/// Deliberately limited to connection-drop signatures. Deterministic TLS failures
/// (certificate validation, negotiation alerts) are excluded: retrying them just
/// wastes the budget. An EOF or reset is a transport drop no matter which phase it
/// interrupts — including mid-handshake — so those match regardless of surrounding
/// TLS wording.
fn message_has_network_marker(message: &str) -> bool {
    const MARKERS: &[&str] = &[
        "connection reset",
        "connection aborted",
        "connection closed before message completed",
        "unexpected eof",
        "unexpected end of file",
        "broken pipe",
        "peer closed connection",
    ];
    let lowered = message.to_ascii_lowercase();
    MARKERS.iter().any(|marker| lowered.contains(marker))
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    #[test]
    fn retry_after_wins_and_is_clamped() {
        assert_eq!(
            transient_backoff(0, Some(Duration::from_secs(5))),
            Duration::from_secs(5)
        );
        assert_eq!(
            transient_backoff(0, Some(Duration::from_secs(9999))),
            MAX_TRANSIENT_BACKOFF
        );
    }

    #[test]
    fn computed_backoff_grows_and_stays_within_ceiling() {
        for attempt in 0..MAX_TRANSIENT_RETRIES {
            let delay = transient_backoff(attempt, None);
            assert!(delay >= TRANSIENT_BACKOFF_BASE.saturating_mul(1 << attempt as u32));
            assert!(delay <= MAX_TRANSIENT_BACKOFF);
        }
    }

    #[test]
    fn parses_delay_seconds_only() {
        assert_eq!(parse_retry_after("30"), Some(Duration::from_secs(30)));
        assert_eq!(parse_retry_after("  12 "), Some(Duration::from_secs(12)));
        assert_eq!(parse_retry_after("Wed, 21 Oct 2015 07:28:00 GMT"), None);
        assert_eq!(parse_retry_after(""), None);
    }

    #[test]
    fn classifies_io_connection_reset_as_network() {
        let err = io::Error::new(io::ErrorKind::ConnectionReset, "connection reset by peer");
        assert!(is_transient_network_error(&err));
    }

    /// An EOF interrupting the TLS handshake is a peer connection drop (transient),
    /// not a deterministic TLS failure — the marker matches despite the TLS wording.
    #[test]
    fn classifies_nested_marker_as_network() {
        #[derive(Debug)]
        struct Wrapper(io::Error);
        impl std::fmt::Display for Wrapper {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "request failed")
            }
        }
        impl Error for Wrapper {
            fn source(&self) -> Option<&(dyn Error + 'static)> {
                Some(&self.0)
            }
        }
        let err = Wrapper(io::Error::other("unexpected EOF during TLS handshake"));
        assert!(is_transient_network_error(&err));
    }

    #[test]
    fn unrelated_error_is_not_network() {
        let err = io::Error::new(io::ErrorKind::InvalidData, "bad json in body details");
        assert!(!is_transient_network_error(&err));
    }
}
