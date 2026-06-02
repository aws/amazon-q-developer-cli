//! SigV4-signing HTTP client for the Taskei MCP gateway.
//!
//! Shape: a thin wrapper around [`reqwest::Client`] that, on each call, takes
//! a fresh credential snapshot from a [`SharedCredentialsProvider`] (the
//! default provider chain from `aws-config`), signs the outbound request with
//! SigV4 (`service=execute-api`), and forwards it. One JSON line per call is
//! emitted on the `taskei_audit` tracing target with request id, method,
//! path, status, and latency.
//!
//! Phase 1b is **fail-closed** on credential resolution: if the provider
//! returns an error or expired creds, [`SigV4HttpClient::sign_and_send`]
//! returns [`SigV4Error::CredentialFailure`]. The caller (today: `main.rs`,
//! Phase 1d: the rmcp transport bridge) is expected to terminate the process
//! with a paging-severity log line — see the `taskei_audit` emission for the
//! shape downstream alarms should match on.

use std::time::{Duration, Instant, SystemTime};

use aws_credential_types::Credentials;
use aws_credential_types::provider::{ProvideCredentials, SharedCredentialsProvider};
use aws_sigv4::http_request::{
    PayloadChecksumKind, SignableBody, SignableRequest, SigningSettings, sign,
};
use aws_sigv4::sign::v4;
use http::HeaderValue;
use reqwest::{Method, Response};
use serde::Serialize;
use thiserror::Error;
use tracing::{Level, debug, error, event};
use uuid::Uuid;

/// Service name for the API Gateway-fronted Taskei MCP endpoint.
const SERVICE: &str = "execute-api";

/// Refresh creds when they expire within this window. Picked to match common
/// STS session minimums; aws-config's default chain typically returns 60min
/// sessions for assume-role and longer for ECS task-role creds, so the bot
/// will refresh well before expiry under normal operation.
const REFRESH_MARGIN: Duration = Duration::from_secs(300); // 5min

#[derive(Debug, Error)]
pub enum SigV4Error {
    /// Credential provider returned an error or an expired snapshot. Caller
    /// MUST treat this as fail-closed: terminate the process. We deliberately
    /// do not auto-retry here — IAM/STS issues are operator-fixable, and a
    /// silent retry burns Bedrock tokens against a problem the user can't
    /// see.
    #[error("credential resolution failed: {0}")]
    CredentialFailure(String),

    /// SigV4 signing itself failed. Not retriable — indicates a bug or
    /// unsupported header set.
    #[error("sigv4 signing failed: {0}")]
    SigningFailure(String),

    /// `reqwest` send returned an error (network / TLS / DNS). Retriable in
    /// principle but Phase 1b leaves retry to the caller; Phase 1d's rmcp
    /// transport will add a bounded backoff.
    #[error("http send failed: {0}")]
    HttpFailure(#[from] reqwest::Error),

    /// Misuse: bad URL, bad header value, etc.
    #[error("invalid request: {0}")]
    InvalidRequest(String),
}

/// One JSON line per request, on the `taskei_audit` tracing target.
///
/// Schema is **stable**: alarm queries will key on these field names. Adding
/// fields is fine; renaming or removing is breaking.
#[derive(Serialize)]
struct AuditLine<'a> {
    request_id: &'a str,
    method: &'a str,
    /// Request path (the URL's path-and-query). The host is stable per
    /// deployment so we do not log it; bytes-on-the-wire savings add up over
    /// long-lived runs.
    path: &'a str,
    region: &'a str,
    /// HTTP status code. Absent means the request never made it onto the
    /// wire (signing or cred failure); see `outcome`.
    status: Option<u16>,
    latency_ms: u128,
    outcome: &'static str,
    /// Set on failure paths so log-search can filter without parsing the
    /// outcome string.
    error: Option<&'a str>,
}

/// SigV4-signing HTTP client targeted at a single AWS region.
///
/// Cheap to clone — the inner `reqwest::Client` and `SharedCredentialsProvider`
/// are both `Arc`-backed.
#[derive(Clone)]
pub struct SigV4HttpClient {
    inner: reqwest::Client,
    creds: SharedCredentialsProvider,
    region: String,
}

impl SigV4HttpClient {
    /// Build from the AWS default provider chain. In ECS this resolves to
    /// the task role via `AWS_CONTAINER_CREDENTIALS_RELATIVE_URI`; locally it
    /// resolves whatever `AWS_PROFILE` / env / IMDS provides. Returns an
    /// error only on `reqwest::Client` build failure; credential resolution
    /// happens lazily per-call.
    pub async fn from_default_chain(region: String) -> anyhow::Result<Self> {
        let cfg = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(region.clone()))
            .load()
            .await;
        let creds = cfg.credentials_provider().ok_or_else(|| {
            anyhow::anyhow!("aws-config returned no credentials provider for region {region}")
        })?;
        let inner = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            inner,
            creds,
            region,
        })
    }

    /// Construct directly. Used by tests (in-crate and integration) to
    /// inject a static [`SharedCredentialsProvider`] and a `reqwest::Client`
    /// pointed at a localhost mock. Marked `#[doc(hidden)]` because
    /// production callers must always go through
    /// [`SigV4HttpClient::from_default_chain`] so the default provider
    /// chain's caching + refresh behaviour applies.
    #[doc(hidden)]
    pub fn for_test(
        inner: reqwest::Client,
        creds: SharedCredentialsProvider,
        region: String,
    ) -> Self {
        Self {
            inner,
            creds,
            region,
        }
    }

    /// POST a JSON body to `url`, signing the request with the current
    /// credential snapshot. Emits one `taskei_audit` line on success or
    /// failure.
    pub async fn post_json(
        &self,
        url: &str,
        body: serde_json::Value,
    ) -> Result<Response, SigV4Error> {
        let payload = serde_json::to_vec(&body)
            .map_err(|e| SigV4Error::InvalidRequest(format!("body serialize: {e}")))?;
        self.sign_and_send(Method::POST, url, payload, "application/json")
            .await
    }

    async fn sign_and_send(
        &self,
        method: Method,
        url: &str,
        body: Vec<u8>,
        content_type: &'static str,
    ) -> Result<Response, SigV4Error> {
        let request_id = Uuid::new_v4().to_string();
        let started = Instant::now();
        let parsed = url::Url::parse(url)
            .map_err(|e| SigV4Error::InvalidRequest(format!("parse url: {e}")))?;
        let path = parsed.path().to_string();

        let creds = self.fresh_creds(&request_id, method.as_str(), &path).await?;

        // Build a stub http::Request to sign, using only the fields aws-sigv4
        // needs. We then transfer the signing instructions onto the
        // reqwest::RequestBuilder before send.
        let mut to_sign = http::Request::builder()
            .method(method.clone())
            .uri(url)
            .header(http::header::CONTENT_TYPE, content_type)
            .header(http::header::HOST, host_header(&parsed)?)
            .body(body.clone())
            .map_err(|e| SigV4Error::InvalidRequest(format!("build request: {e}")))?;

        let mut settings = SigningSettings::default();
        settings.payload_checksum_kind = PayloadChecksumKind::XAmzSha256;

        let identity = creds.into();
        let params = v4::SigningParams::builder()
            .identity(&identity)
            .region(&self.region)
            .name(SERVICE)
            .time(SystemTime::now())
            .settings(settings)
            .build()
            .map_err(|e| SigV4Error::SigningFailure(e.to_string()))?
            .into();

        let signable = SignableRequest::new(
            to_sign.method().as_str(),
            to_sign.uri().to_string(),
            to_sign.headers().iter().map(|(k, v)| {
                (k.as_str(), std::str::from_utf8(v.as_bytes()).unwrap_or(""))
            }),
            SignableBody::Bytes(&body),
        )
        .map_err(|e| SigV4Error::SigningFailure(e.to_string()))?;

        let (instructions, _signature) = sign(signable, &params)
            .map_err(|e| SigV4Error::SigningFailure(e.to_string()))?
            .into_parts();
        instructions.apply_to_request_http1x(&mut to_sign);

        // Translate the signed http::Request headers onto a reqwest builder.
        // We deliberately do NOT pre-set Content-Type here — every header
        // that needs to land on the wire (including Content-Type and the
        // SigV4 set) is already on `to_sign` and gets copied below. Setting
        // it twice produces the canonical string `content-type:application/
        // json,application/json` on the gateway side, which doesn't match
        // the single value we signed → 403 with "signature does not match".
        let mut builder = self.inner.request(method.clone(), url).body(body);
        for (k, v) in to_sign.headers() {
            // reqwest sets Host itself.
            if k == http::header::HOST {
                continue;
            }
            builder = builder.header(k.as_str(), v);
        }

        debug!(target: "kiro_taskei_mcp", %request_id, %method, %path, "sending signed request");

        let send_res = builder.send().await;
        let latency_ms = started.elapsed().as_millis();
        match send_res {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let outcome = if resp.status().is_success() {
                    "ok"
                } else {
                    "http_error"
                };
                emit_audit(AuditLine {
                    request_id: &request_id,
                    method: method.as_str(),
                    path: &path,
                    region: &self.region,
                    status: Some(status),
                    latency_ms,
                    outcome,
                    error: None,
                });
                Ok(resp)
            }
            Err(e) => {
                let msg = e.to_string();
                emit_audit(AuditLine {
                    request_id: &request_id,
                    method: method.as_str(),
                    path: &path,
                    region: &self.region,
                    status: None,
                    latency_ms,
                    outcome: "send_error",
                    error: Some(&msg),
                });
                Err(SigV4Error::HttpFailure(e))
            }
        }
    }

    /// Resolve credentials for a single call. Provider implementations are
    /// expected to cache and refresh internally — `aws-config`'s
    /// `LazyCachingCredentialsProvider` (the default) refreshes when the
    /// remaining lifetime drops below its margin.
    ///
    /// We additionally re-check the expiry against [`REFRESH_MARGIN`]: if
    /// the snapshot returned by the provider is *already* about to expire,
    /// we treat that as a refresh failure (provider is unhealthy) and fail
    /// closed rather than send a request that may 403 on a stale signature.
    async fn fresh_creds(
        &self,
        request_id: &str,
        method: &str,
        path: &str,
    ) -> Result<Credentials, SigV4Error> {
        let snap = self.creds.provide_credentials().await.map_err(|e| {
            let msg = e.to_string();
            error!(target: "taskei_audit", request_id, method, path, error = %msg, "credential resolution failed; fail-closed");
            emit_audit(AuditLine {
                request_id,
                method,
                path,
                region: &self.region,
                status: None,
                latency_ms: 0,
                outcome: "cred_failure",
                error: Some(&msg),
            });
            SigV4Error::CredentialFailure(msg)
        })?;

        if let Some(expiry) = snap.expiry() {
            let remaining = expiry
                .duration_since(SystemTime::now())
                .unwrap_or(Duration::ZERO);
            if remaining < REFRESH_MARGIN {
                let msg = format!(
                    "credential expiry within refresh margin ({}s remaining < {}s margin); provider did not refresh",
                    remaining.as_secs(),
                    REFRESH_MARGIN.as_secs()
                );
                error!(target: "taskei_audit", request_id, method, path, error = %msg, "credential snapshot stale; fail-closed");
                emit_audit(AuditLine {
                    request_id,
                    method,
                    path,
                    region: &self.region,
                    status: None,
                    latency_ms: 0,
                    outcome: "cred_stale",
                    error: Some(&msg),
                });
                return Err(SigV4Error::CredentialFailure(msg));
            }
        }
        Ok(snap)
    }
}

fn host_header(url: &url::Url) -> Result<HeaderValue, SigV4Error> {
    let host = url
        .host_str()
        .ok_or_else(|| SigV4Error::InvalidRequest("url has no host".into()))?;
    let value = match url.port() {
        Some(p) => format!("{host}:{p}"),
        None => host.to_string(),
    };
    HeaderValue::from_str(&value)
        .map_err(|e| SigV4Error::InvalidRequest(format!("host header: {e}")))
}

/// Emit one structured audit line. Goes to the `taskei_audit` tracing
/// target so production deployments can route it to its own log group /
/// retention.
fn emit_audit(line: AuditLine<'_>) {
    let json = match serde_json::to_string(&line) {
        Ok(s) => s,
        Err(_) => return,
    };
    event!(target: "taskei_audit", Level::INFO, "{json}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_credential_types::Credentials;

    #[test]
    fn host_header_includes_port_when_present() {
        let u = url::Url::parse("http://127.0.0.1:8080/mcp").unwrap();
        assert_eq!(host_header(&u).unwrap().to_str().unwrap(), "127.0.0.1:8080");
        let u = url::Url::parse("https://example.com/mcp").unwrap();
        assert_eq!(host_header(&u).unwrap().to_str().unwrap(), "example.com");
    }

    #[tokio::test]
    async fn fresh_creds_fails_closed_on_provider_error() {
        #[derive(Debug)]
        struct Broken;
        impl ProvideCredentials for Broken {
            fn provide_credentials<'a>(
                &'a self,
            ) -> aws_credential_types::provider::future::ProvideCredentials<'a>
            where
                Self: 'a,
            {
                aws_credential_types::provider::future::ProvideCredentials::ready(Err(
                    aws_credential_types::provider::error::CredentialsError::not_loaded(
                        "test failure",
                    ),
                ))
            }
        }
        let client = SigV4HttpClient::for_test(
            reqwest::Client::new(),
            SharedCredentialsProvider::new(Broken),
            "us-east-1".into(),
        );
        let err = client
            .fresh_creds("rid", "POST", "/mcp")
            .await
            .expect_err("expected cred failure");
        assert!(matches!(err, SigV4Error::CredentialFailure(_)));
    }

    #[tokio::test]
    async fn fresh_creds_fails_closed_on_stale_expiry() {
        // Expiry 60s in the past => stale.
        let stale = Credentials::new(
            "AKIDEXAMPLE",
            "secret",
            Some("token".into()),
            Some(SystemTime::now() - Duration::from_secs(60)),
            "test",
        );
        let client = SigV4HttpClient::for_test(
            reqwest::Client::new(),
            SharedCredentialsProvider::new(stale),
            "us-east-1".into(),
        );
        let err = client
            .fresh_creds("rid", "POST", "/mcp")
            .await
            .expect_err("expected stale fail-closed");
        assert!(matches!(err, SigV4Error::CredentialFailure(_)));
    }

    #[tokio::test]
    async fn fresh_creds_accepts_fresh_snapshot() {
        let fresh = Credentials::new(
            "AKIDEXAMPLE",
            "secret",
            Some("token".into()),
            Some(SystemTime::now() + Duration::from_secs(3600)),
            "test",
        );
        let client = SigV4HttpClient::for_test(
            reqwest::Client::new(),
            SharedCredentialsProvider::new(fresh),
            "us-east-1".into(),
        );
        let snap = client
            .fresh_creds("rid", "POST", "/mcp")
            .await
            .expect("should resolve");
        assert_eq!(snap.access_key_id(), "AKIDEXAMPLE");
    }
}
