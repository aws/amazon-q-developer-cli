//! STS AssumeRole bridge — Phase 1c.
//!
//! Phase 1b drove every signed call from a single base credential snapshot
//! (the ECS task role in prod, the `kiro-bot` profile locally). Phase 1c
//! re-introduces the option of *role-scoped* creds without forcing the
//! split: when both `--read-role-arn` and `--write-role-arn` are unset (the
//! Phase-0-as-shipped reality, where Taskei accepted the task role
//! directly), this module is a no-op pass-through.
//!
//! When the read role arn is set, the bridge wraps the base provider in an
//! [`AssumeRoleProvider`] **with caching** so the bot's hot read path does
//! not call STS on every tool invocation. When the write role arn is set,
//! [`StsBridge::assume_write_once`] builds a *fresh* [`AssumeRoleProvider`]
//! with no caching, resolves credentials exactly once, and drops the
//! provider before returning. That preserves the plan's "single-call write
//! session" intent while staying inside STS's hard floor of 900s
//! `DurationSeconds` (the plan's "≤60s session" copy is below the API
//! minimum — see the plan-doc note added in this PR).
//!
//! Code-enforced read/write boundary: `assume_write_once` lives only on
//! [`StsBridge`]. Phase 1d's `families/<x>/read/` modules will hold an
//! immutable handle to a "read-only" view (see [`StsBridge::read_only`])
//! that does not expose this method, and a clippy/build-time check will
//! reject any read-side import of the write helper. Until those modules
//! land, the boundary is documented here so future readers know the
//! invariant.

use std::sync::Arc;
use std::time::{
    Duration,
    SystemTime,
};

use aws_config::SdkConfig;
use aws_config::sts::AssumeRoleProvider;
use aws_credential_types::Credentials;
use aws_credential_types::provider::{
    ProvideCredentials,
    SharedCredentialsProvider,
    future as creds_future,
};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{
    debug,
    error,
};

/// Refresh window for the cached read-role provider. When a cached
/// snapshot's remaining lifetime drops below this, the next caller pays
/// the STS hop. Matches the plan's "refresh-on-expiry-5min" knob.
const READ_REFRESH_MARGIN: Duration = Duration::from_secs(300);

/// Hard upper bound on how long a single read-role snapshot is reused,
/// independent of the issuer's stated expiry. Matches the plan's "50-min
/// hard recycle" knob — picks slightly under STS's default 1-hour session
/// so we never cliff-edge on the issuer side.
const READ_HARD_RECYCLE: Duration = Duration::from_secs(50 * 60);

/// Default STS `DurationSeconds` for write-role assumption.
///
/// STS rejects values below 900s (15 minutes); the plan's "≤60s session"
/// language is aspirational and below the API floor. We pick the floor and
/// drop the provider after a single resolve so the *useful* lifetime of the
/// credential — the window in which the bridge will reuse it — is exactly
/// one call. The remaining 900s is service-side TTL we cannot shorten;
/// IAM and the per-call audit log are what bound write-blast-radius, not
/// the duration knob.
const STS_MIN_DURATION_SECS: u64 = 900;

/// Service used to derive a default session name when the operator did not
/// provide one. Surfaces in CloudTrail as `botoSessionName=kiro-mcp-...`
/// so an auditor can attribute writes to this binary without parsing IAM
/// roles. (Phase 1c shipped this as `kiro-taskei-mcp-...`; renamed in
/// 1c-bundle alongside the binary rename. CloudTrail queries pinned to
/// the old prefix should be updated.)
const SESSION_NAME_PREFIX: &str = "kiro-mcp";

/// Errors specific to the STS bridge. SigV4 / signing errors surface from
/// [`crate::sigv4_client::SigV4Error`]; this enum exists because cred
/// resolution against STS fails in a different operational class than a
/// missing base credential.
#[derive(Debug, Error)]
pub enum StsBridgeError {
    /// Building the read-role provider failed at boot. Fail-closed: caller
    /// is expected to abort startup. Once the bridge is up, this variant
    /// can no longer fire.
    #[error("read-role provider build failed: {0}")]
    ReadProviderBuild(String),

    /// Building or resolving the per-call write provider failed. The
    /// caller is the write tool implementation; it MUST surface this as
    /// the tool's outcome rather than retrying silently — IAM/STS issues
    /// are operator-fixable, and silent retries will burn AssumeRole quota
    /// against an issue the user cannot see.
    #[error("write-role assumption failed: {0}")]
    AssumeWriteFailed(String),
}

/// The operational shape of the bridge — set once at boot.
///
/// `Direct` is the Phase-0 reality and the unit-test default. `Split` is
/// kept compiling and tested so re-introducing the role split in a future
/// phase is a config change, not a code change.
#[derive(Clone, Debug)]
enum Mode {
    /// Both ARNs unset — base creds drive both read and write. The bridge
    /// is a thin handle over the [`SdkConfig`]'s credential provider.
    Direct,
    /// Read role only. Read calls go through a cached
    /// [`AssumeRoleProvider`]; write calls fall back to base creds (because
    /// `write_role_arn` is unset).
    ReadOnly { read_provider: SharedCredentialsProvider },
    /// Both roles configured. Reads use a cached provider; writes call
    /// STS per invocation via [`StsBridge::assume_write_once`].
    Split {
        read_provider: SharedCredentialsProvider,
        write_role_arn: Arc<str>,
    },
}

/// Static configuration for the bridge. Populated from clap; serializable
/// purely so test fixtures can build one without going through clap.
#[derive(Clone, Debug, Default)]
pub struct StsBridgeConfig {
    /// IAM role ARN used for read tool invocations. When `None`, base
    /// creds drive read calls.
    pub read_role_arn: Option<String>,
    /// IAM role ARN used for write tool invocations. When `None`, base
    /// creds drive write calls.
    pub write_role_arn: Option<String>,
    /// Optional session-name override. Mostly useful in tests; production
    /// should leave this `None` so each instance gets a unique
    /// auto-generated name.
    pub session_name: Option<String>,
}

impl StsBridgeConfig {
    pub fn new(read_role_arn: Option<String>, write_role_arn: Option<String>) -> Self {
        Self {
            read_role_arn,
            write_role_arn,
            session_name: None,
        }
    }

    /// `true` iff neither role is configured — i.e. the bridge will be a
    /// no-op pass-through and base creds drive everything. Convenience for
    /// startup logging: operators want to see at a glance whether STS is
    /// in the path.
    pub fn is_no_op(&self) -> bool {
        self.read_role_arn.is_none() && self.write_role_arn.is_none()
    }
}

/// Owns the AWS [`SdkConfig`] (base creds, region, etc.) and the optional
/// read-role provider. Cheap to clone — both halves are `Arc`-backed.
#[derive(Clone)]
pub struct StsBridge {
    base: SdkConfig,
    mode: Mode,
    session_name: Arc<str>,
}

impl StsBridge {
    /// Build the bridge from the AWS default provider chain plus the
    /// caller's clap-supplied role ARNs. Must be called once at startup.
    ///
    /// Boot-time STS calls happen *only* for the read role (lazily, on
    /// first credential resolve via the cached provider). Write-role
    /// AssumeRole is deferred to [`Self::assume_write_once`] so a misconfigured
    /// write role does not crash boot for a kiro-bot agent that only ever
    /// calls read tools.
    pub async fn from_default_chain(region: String, cfg: StsBridgeConfig) -> Result<Self, StsBridgeError> {
        let base = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(region))
            .load()
            .await;
        Self::from_base(base, cfg).await
    }

    /// Build from a pre-loaded [`SdkConfig`]. Tests use this entrypoint to
    /// inject mocks; production callers go through
    /// [`Self::from_default_chain`].
    pub async fn from_base(base: SdkConfig, cfg: StsBridgeConfig) -> Result<Self, StsBridgeError> {
        let session_name = cfg.session_name.clone().unwrap_or_else(default_session_name);
        let session_name: Arc<str> = Arc::from(session_name);

        let read_provider = match &cfg.read_role_arn {
            Some(arn) => {
                debug!(target: "kiro_taskei_mcp", role_arn = %arn, "building read-role AssumeRoleProvider");
                // Read-role default session length is left at the
                // SDK default (1 hour) — we cap reuse on our own clock
                // via `READ_HARD_RECYCLE` rather than burning the
                // `DurationSeconds` knob. Wrap in `CachingProvider` so
                // the bot's read-tool fan-out doesn't fan to STS.
                let provider = AssumeRoleProvider::builder(arn)
                    .session_name(session_name.as_ref())
                    .configure(&base)
                    .build()
                    .await;
                Some(SharedCredentialsProvider::new(CachingProvider::new(provider)))
            },
            None => None,
        };

        let mode = match (read_provider, cfg.write_role_arn.as_ref()) {
            (None, None) => Mode::Direct,
            (Some(rp), None) => Mode::ReadOnly { read_provider: rp },
            (Some(rp), Some(write_arn)) => Mode::Split {
                read_provider: rp,
                write_role_arn: Arc::from(write_arn.as_str()),
            },
            // Write-only with no read role makes no sense — every tool that
            // *can* call a write also calls reads first. We tolerate it by
            // mapping into Split with the *base* provider as the "read"
            // path, so an operator who explicitly configures only the
            // write role gets the literal behavior they asked for.
            (None, Some(write_arn)) => {
                let base_provider = base.credentials_provider().ok_or_else(|| {
                    StsBridgeError::ReadProviderBuild("base SdkConfig has no credentials provider".into())
                })?;
                Mode::Split {
                    read_provider: base_provider,
                    write_role_arn: Arc::from(write_arn.as_str()),
                }
            },
        };

        Ok(Self {
            base,
            mode,
            session_name,
        })
    }

    /// Provider used by read-tool calls. Always returns *something*: when
    /// no read role is configured, this is the base provider — exactly the
    /// shape `SigV4HttpClient::from_default_chain` used in Phase 1b, so the
    /// no-op path stays bit-for-bit equivalent.
    pub fn read_credentials_provider(&self) -> Result<SharedCredentialsProvider, StsBridgeError> {
        match &self.mode {
            Mode::Direct => self
                .base
                .credentials_provider()
                .ok_or_else(|| StsBridgeError::ReadProviderBuild("base SdkConfig has no credentials provider".into())),
            Mode::ReadOnly { read_provider } | Mode::Split { read_provider, .. } => Ok(read_provider.clone()),
        }
    }

    /// `true` when the write-tool path will assume the write role per call.
    /// `false` means writes share the read path's creds (Phase-0 reality).
    pub fn writes_use_dedicated_role(&self) -> bool {
        matches!(self.mode, Mode::Split { .. })
    }

    /// Resolve a single fresh credential set for a write-tool call.
    ///
    /// **Per-call semantics, by construction.** When a dedicated write
    /// role is configured we build a brand-new [`AssumeRoleProvider`],
    /// resolve credentials once, and drop the provider before returning.
    /// The returned [`Credentials`] are passed straight into the SigV4
    /// signer for one signed call and then dropped by the caller.
    ///
    /// When no write role is configured, this method returns the same
    /// snapshot the read path would see (i.e. base creds). The caller's
    /// audit log will record `mode=base` for that case.
    pub async fn assume_write_once(&self) -> Result<WriteCredentials, StsBridgeError> {
        match &self.mode {
            // No dedicated write role — give the caller whatever the base
            // provider would hand out. Note: we deliberately don't reuse
            // `read_credentials_provider()` here because it might return a
            // *cached* assume-role-as-read provider; a write tool with no
            // write-role-arn but with a read-role-arn should write *as
            // the read role*, which is what `read_credentials_provider`
            // resolves to. So fall through to it for that case.
            Mode::Direct => {
                let provider = self.base.credentials_provider().ok_or_else(|| {
                    StsBridgeError::AssumeWriteFailed("base SdkConfig has no credentials provider".into())
                })?;
                let creds = resolve(&provider).await?;
                Ok(WriteCredentials {
                    creds,
                    mode: WriteMode::Base,
                })
            },
            Mode::ReadOnly { read_provider } => {
                // Read role configured but no write role — writes inherit
                // the read role. The audit log marks this so the operator
                // can see writes are NOT going through a per-call STS hop.
                let creds = resolve(read_provider).await?;
                Ok(WriteCredentials {
                    creds,
                    mode: WriteMode::ReadRole,
                })
            },
            Mode::Split { write_role_arn, .. } => {
                debug!(
                    target: "kiro_taskei_mcp",
                    role_arn = %write_role_arn,
                    "assuming write role (per-call)"
                );
                // Build a *fresh* provider every call. No caching layer is
                // attached, so the next call also goes to STS. The returned
                // provider is dropped at the end of this scope; nothing
                // outside this function holds it.
                let provider = AssumeRoleProvider::builder(write_role_arn.as_ref())
                    .session_name(self.session_name.as_ref())
                    .session_length(Duration::from_secs(STS_MIN_DURATION_SECS))
                    .configure(&self.base)
                    .build()
                    .await;
                let creds = resolve_provider(&provider).await?;
                Ok(WriteCredentials {
                    creds,
                    mode: WriteMode::WriteRole,
                })
            },
        }
    }

    /// A view that exposes only the read path. Phase 1d's
    /// `families/<x>/read/` modules will hold this type instead of the
    /// full bridge — that's the lint boundary keeping
    /// `assume_write_once` out of read code.
    pub fn read_only(&self) -> ReadOnlyView {
        ReadOnlyView { inner: self.clone() }
    }
}

/// A handle that exposes ONLY the read credential path. Constructable
/// only via [`StsBridge::read_only`]. Read-side modules in Phase 1d will
/// take a `ReadOnlyView` instead of a `StsBridge`; that makes
/// "read code escalates to writes" a compile error rather than a code
/// review concern.
#[derive(Clone)]
pub struct ReadOnlyView {
    inner: StsBridge,
}

impl ReadOnlyView {
    pub fn read_credentials_provider(&self) -> Result<SharedCredentialsProvider, StsBridgeError> {
        self.inner.read_credentials_provider()
    }
}

/// Bundle of credentials from [`StsBridge::assume_write_once`] plus a
/// machine-readable tag describing how they were produced. The tag is
/// expected to surface on audit lines so downstream queries can
/// distinguish "wrote via dedicated write role" from "wrote via base
/// creds because no write role was configured".
#[derive(Debug)]
pub struct WriteCredentials {
    pub creds: Credentials,
    pub mode: WriteMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    /// Base provider creds (no role configured at all).
    Base,
    /// Read-role creds because the read role is set but no write role is.
    ReadRole,
    /// Per-call assumed write role.
    WriteRole,
}

impl WriteMode {
    pub fn as_str(self) -> &'static str {
        match self {
            WriteMode::Base => "base",
            WriteMode::ReadRole => "read_role",
            WriteMode::WriteRole => "write_role",
        }
    }
}

/// Caches a single [`ProvideCredentials`] snapshot. The first call resolves
/// against the inner provider; subsequent calls return the cached snapshot
/// until either:
///
/// 1. The cached creds' `expiry` is within [`READ_REFRESH_MARGIN`], or
/// 2. The cache itself has held the snapshot longer than [`READ_HARD_RECYCLE`] (defends against an
///    issuer that hands out "never expires" creds we'd otherwise hold forever).
///
/// Concurrent refreshes serialize on a `tokio::Mutex`. We deliberately do
/// not implement stampede-protection beyond that: this cache fronts a
/// single-process binary, the kiro-bot runtime spawns the shim once per
/// agent, and even a 100-tool burst will fan in to one STS call across
/// the cache's lifetime.
struct CachingProvider<P> {
    inner: P,
    state: Mutex<CacheState>,
}

#[derive(Default)]
struct CacheState {
    snapshot: Option<Credentials>,
    cached_at: Option<SystemTime>,
}

impl<P> CachingProvider<P>
where
    P: ProvideCredentials + Send + Sync + 'static,
{
    fn new(inner: P) -> Self {
        Self {
            inner,
            state: Mutex::new(CacheState::default()),
        }
    }

    async fn get_creds(&self) -> Result<Credentials, StsBridgeError> {
        let mut guard = self.state.lock().await;
        let now = SystemTime::now();
        if let (Some(snap), Some(stamped)) = (guard.snapshot.as_ref(), guard.cached_at) {
            let too_old = now
                .duration_since(stamped)
                .map(|d| d >= READ_HARD_RECYCLE)
                .unwrap_or(false);
            let near_expiry = match snap.expiry() {
                Some(exp) => exp.duration_since(now).map(|d| d < READ_REFRESH_MARGIN).unwrap_or(true),
                // No expiry on the snapshot — treat as "good for now",
                // refresh only on hard-recycle.
                None => false,
            };
            if !too_old && !near_expiry {
                return Ok(snap.clone());
            }
            debug!(
                target: "kiro_taskei_mcp",
                too_old, near_expiry, "read-role cache miss; refreshing via STS"
            );
        }

        let fresh = self.inner.provide_credentials().await.map_err(|e| {
            let msg = e.to_string();
            error!(
                target: "taskei_audit",
                error = %msg,
                "read-role AssumeRole refresh failed"
            );
            StsBridgeError::AssumeWriteFailed(msg)
        })?;
        guard.snapshot = Some(fresh.clone());
        guard.cached_at = Some(now);
        Ok(fresh)
    }
}

impl<P> std::fmt::Debug for CachingProvider<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachingProvider").finish_non_exhaustive()
    }
}

impl<P> ProvideCredentials for CachingProvider<P>
where
    P: ProvideCredentials + Send + Sync + 'static,
{
    fn provide_credentials<'a>(&'a self) -> creds_future::ProvideCredentials<'a>
    where
        Self: 'a,
    {
        creds_future::ProvideCredentials::new(async move {
            self.get_creds().await.map_err(|e| {
                aws_credential_types::provider::error::CredentialsError::provider_error(Box::new(
                    std::io::Error::other(e.to_string()),
                ))
            })
        })
    }
}

async fn resolve(provider: &SharedCredentialsProvider) -> Result<Credentials, StsBridgeError> {
    provider.provide_credentials().await.map_err(|e| {
        let msg = e.to_string();
        error!(target: "taskei_audit", error = %msg, "credential resolution failed in sts_bridge");
        StsBridgeError::AssumeWriteFailed(msg)
    })
}

async fn resolve_provider<P>(provider: &P) -> Result<Credentials, StsBridgeError>
where
    P: ProvideCredentials,
{
    provider.provide_credentials().await.map_err(|e| {
        let msg = e.to_string();
        error!(target: "taskei_audit", error = %msg, "AssumeRole call failed");
        StsBridgeError::AssumeWriteFailed(msg)
    })
}

fn default_session_name() -> String {
    // Per-process suffix so two replicas of the bot don't collide on
    // session-name in CloudTrail. We avoid `Date.now()` / random and use a
    // monotonic-but-cheap counter via SystemTime; a few-second skew between
    // replicas is fine because IAM doesn't key off session name.
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{SESSION_NAME_PREFIX}-{stamp}")
}

/// Light helper used by callers that already have a [`Credentials`] in
/// hand (e.g., from [`StsBridge::assume_write_once`]) and want to feed
/// them into [`crate::sigv4_client::SigV4HttpClient::for_test`]-shaped
/// constructors. Returns a one-shot provider that yields exactly the
/// passed-in credentials and never refreshes — suitable only for the
/// single signed call that follows.
pub fn one_shot_provider(creds: Credentials) -> SharedCredentialsProvider {
    SharedCredentialsProvider::new(creds)
}

// Sanity assertion that gives a compiler error if the constants drift
// below STS minimums. This is `const`, so the build fails at compile time
// rather than dropping a 4xx in production.
const _: () = {
    assert!(STS_MIN_DURATION_SECS >= 900, "STS rejects DurationSeconds below 900");
};

#[cfg(test)]
mod tests {
    use std::sync::atomic::{
        AtomicUsize,
        Ordering,
    };

    use aws_config::SdkConfig;
    use aws_credential_types::Credentials;
    use aws_credential_types::credential_fn::provide_credentials_fn;
    use aws_credential_types::provider::SharedCredentialsProvider;

    use super::*;

    fn base_with_static_creds() -> SdkConfig {
        let creds = Credentials::new(
            "AKIDBASE",
            "secret",
            None,
            Some(SystemTime::now() + Duration::from_secs(3600)),
            "static-base",
        );
        SdkConfig::builder()
            .credentials_provider(SharedCredentialsProvider::new(creds))
            .region(aws_config::Region::new("us-east-1"))
            .build()
    }

    /// Build a Split-mode bridge whose read provider is a counted-call
    /// fake fronted by [`CachingProvider`]. The write side uses a marker
    /// ARN; tests in this module exercise read-side behavior in Split
    /// mode without needing a live STS endpoint. End-to-end write-call
    /// behavior in Split mode is covered by Phase 1c's live-ECS smoke
    /// (called out in the PR test plan), not by a unit test, because
    /// faking `AssumeRoleProvider`'s HTTP path adds infrastructure with
    /// no payoff over the live smoke.
    fn split_bridge_with_fake_read(read_creds: Credentials) -> (StsBridge, Arc<AtomicUsize>) {
        let read_calls = Arc::new(AtomicUsize::new(0));
        let read_calls_inner = read_calls.clone();
        let read_provider_raw = provide_credentials_fn(move || {
            read_calls_inner.fetch_add(1, Ordering::SeqCst);
            let creds = read_creds.clone();
            async move { Ok(creds) }
        });
        let read_provider = SharedCredentialsProvider::new(CachingProvider::new(read_provider_raw));

        let bridge = StsBridge {
            base: base_with_static_creds(),
            mode: Mode::Split {
                read_provider,
                write_role_arn: Arc::from("arn:aws:iam::1:role/write"),
            },
            session_name: Arc::from("test-session"),
        };
        (bridge, read_calls)
    }

    #[tokio::test]
    async fn no_op_when_both_roles_unset() {
        let bridge = StsBridge::from_base(base_with_static_creds(), StsBridgeConfig::default())
            .await
            .expect("bridge should build");
        assert!(matches!(bridge.mode, Mode::Direct));
        assert!(!bridge.writes_use_dedicated_role());

        // Read path returns base creds.
        let provider = bridge.read_credentials_provider().expect("provider");
        let creds = provider.provide_credentials().await.expect("creds");
        assert_eq!(creds.access_key_id(), "AKIDBASE");

        // Write path also returns base creds, tagged Base.
        let wc = bridge.assume_write_once().await.expect("write creds");
        assert_eq!(wc.mode, WriteMode::Base);
        assert_eq!(wc.creds.access_key_id(), "AKIDBASE");
    }

    #[tokio::test]
    async fn read_only_view_does_not_expose_write() {
        // Compile-test (in spirit): ReadOnlyView lacks `assume_write_once`.
        // We re-prove that here by using only the read method and asserting
        // the type is what we expect.
        let bridge = StsBridge::from_base(base_with_static_creds(), StsBridgeConfig::default())
            .await
            .expect("bridge");
        let view: ReadOnlyView = bridge.read_only();
        let provider = view.read_credentials_provider().expect("provider");
        let creds = provider.provide_credentials().await.expect("creds");
        assert_eq!(creds.access_key_id(), "AKIDBASE");
        // Negative assertion (commented because it must be a compile-fail
        // test, not a runtime test — phase 1d adds the trybuild fixture):
        // let _ = view.assume_write_once(); // <- must not compile
    }

    #[tokio::test]
    async fn config_no_op_flag_round_trips() {
        let cfg = StsBridgeConfig::new(None, None);
        assert!(cfg.is_no_op());
        let cfg = StsBridgeConfig::new(Some("arn:aws:iam::1:role/r".into()), None);
        assert!(!cfg.is_no_op());
    }

    #[test]
    fn write_mode_as_str_is_audit_stable() {
        // The audit log keys off these strings; renaming them is a
        // breaking change for downstream alarms. Pin the contract.
        assert_eq!(WriteMode::Base.as_str(), "base");
        assert_eq!(WriteMode::ReadRole.as_str(), "read_role");
        assert_eq!(WriteMode::WriteRole.as_str(), "write_role");
    }

    #[tokio::test]
    async fn caching_provider_reuses_snapshot_until_near_expiry() {
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_inner = calls.clone();
        // Fresh creds: 1 hour out — well past the 5-min refresh margin.
        let issued = Arc::new(Mutex::new(0u32));
        let issued_inner = issued.clone();

        let raw = provide_credentials_fn(move || {
            let calls = calls_inner.clone();
            let issued = issued_inner.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                let mut g = issued.lock().await;
                *g += 1;
                Ok(Credentials::new(
                    format!("AKID{}", *g),
                    "secret",
                    Some("token".into()),
                    Some(SystemTime::now() + Duration::from_secs(3600)),
                    "test",
                ))
            }
        });
        let cache = CachingProvider::new(raw);

        let a = cache.get_creds().await.unwrap();
        let b = cache.get_creds().await.unwrap();
        let c = cache.get_creds().await.unwrap();
        // All three resolves see the same cached snapshot.
        assert_eq!(a.access_key_id(), "AKID1");
        assert_eq!(b.access_key_id(), "AKID1");
        assert_eq!(c.access_key_id(), "AKID1");
        // Inner provider was hit exactly once.
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn caching_provider_refreshes_when_snapshot_near_expiry() {
        // First call hands out a snapshot expiring inside the refresh
        // margin. Second call must re-resolve.
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_inner = calls.clone();
        let raw = provide_credentials_fn(move || {
            let calls = calls_inner.clone();
            async move {
                let n = calls.fetch_add(1, Ordering::SeqCst);
                let exp_in = if n == 0 {
                    // 60s out — well inside the 5-min refresh margin.
                    Duration::from_secs(60)
                } else {
                    Duration::from_secs(3600)
                };
                Ok(Credentials::new(
                    format!("AKID{}", n + 1),
                    "secret",
                    Some("token".into()),
                    Some(SystemTime::now() + exp_in),
                    "test",
                ))
            }
        });
        let cache = CachingProvider::new(raw);

        let first = cache.get_creds().await.unwrap();
        assert_eq!(first.access_key_id(), "AKID1");
        let second = cache.get_creds().await.unwrap();
        assert_eq!(
            second.access_key_id(),
            "AKID2",
            "near-expiry snapshot should trigger refresh"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn read_only_mode_tags_write_as_read_role() {
        // Build a ReadOnly bridge by hand (no write_role_arn). Read
        // provider hands out a known AKID; assume_write_once must hand
        // back the same creds and tag them ReadRole.
        let read_creds = Credentials::new(
            "AKIDREAD",
            "secret",
            None,
            Some(SystemTime::now() + Duration::from_secs(3600)),
            "read",
        );
        let raw = {
            let creds = read_creds.clone();
            provide_credentials_fn(move || {
                let creds = creds.clone();
                async move { Ok(creds) }
            })
        };
        let read_provider = SharedCredentialsProvider::new(CachingProvider::new(raw));
        let bridge = StsBridge {
            base: base_with_static_creds(),
            mode: Mode::ReadOnly { read_provider },
            session_name: Arc::from("t"),
        };

        let wc = bridge.assume_write_once().await.expect("write creds");
        assert_eq!(wc.mode, WriteMode::ReadRole);
        assert_eq!(wc.creds.access_key_id(), "AKIDREAD");
        assert!(!bridge.writes_use_dedicated_role());
    }

    #[tokio::test]
    async fn split_mode_read_path_serves_assumed_creds() {
        // Read path in Split mode goes through the cached read provider.
        // We feed it a fake provider so we can inspect what comes out
        // without contacting STS.
        let (bridge, read_calls) = split_bridge_with_fake_read(Credentials::new(
            "AKIDREADSPLIT",
            "secret",
            Some("tok".into()),
            Some(SystemTime::now() + Duration::from_secs(3600)),
            "split-read",
        ));
        assert!(bridge.writes_use_dedicated_role());

        let provider = bridge.read_credentials_provider().expect("provider");
        let a = provider.provide_credentials().await.expect("creds");
        let b = provider.provide_credentials().await.expect("creds");
        assert_eq!(a.access_key_id(), "AKIDREADSPLIT");
        assert_eq!(b.access_key_id(), "AKIDREADSPLIT");
        // Inner provider hit exactly once because the cache fronted the
        // second call. This is the property the kiro-bot read-tool
        // fan-out relies on — without it, we'd burn one STS call per
        // tool invocation.
        assert_eq!(read_calls.load(Ordering::SeqCst), 1);
    }
}
