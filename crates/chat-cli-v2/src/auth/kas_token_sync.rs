//! Mirrors auth tokens to the KAS token file consumed by kiro-agent's
//! `FileAuthProvider`. kiro-cli is sole writer; the SQLite secret store
//! is source of truth, the file is a derived projection. The refresh
//! token is never mirrored -- KAS surfaces `TokenExpiredError` on expiry
//! and lets kiro-cli refresh and rewrite, avoiding double-refresh races.
//!
//! Path: `<data_local_dir>/<CLI_NAME>/kiro-auth-token-cli.json`. Always
//! this exact path; not overridable.
//!
//! `KIRO_KAS_TOKEN_PATH` is KAS's own env var for direct kiro-agent
//! invocations. When kiro-cli spawns KAS, every path converges on
//! `--token-path=...`: direct-spawn sites (`spawn_kas_process`,
//! `execute_kas_serve`) pass it on the command line; the TUI launch path
//! (`launch_acp_interactive` for `AgentEngine::Kas`) sets
//! `KIRO_KAS_TOKEN_PATH` on the bun child and the TUI re-emits
//! `--token-path=...`. kiro-agent always sees `--token-path` and the env
//! var is ignored. kiro-cli's [`kas_token_path`] resolver also ignores
//! it: an external value must not mutate where kiro-cli writes its state.
//!
//! Companion to kiro-cli-autocomplete PR #591 (removed the older writer
//! in `fig_auth`).

use std::path::{
    Path,
    PathBuf,
};
use std::time::Duration;

use serde::Serialize;
use tracing::{
    debug,
    info,
    warn,
};

use crate::auth::builder_id::BuilderIdToken;
use crate::auth::external_idp::ExternalIdpToken;
use crate::auth::social::SocialToken;
use crate::constants::CLI_NAME;
use crate::database::Database;

const KAS_TOKEN_FILE: &str = "kiro-auth-token-cli.json";

/// Caps the pre-spawn refresh round-trip so a flaky network can't block
/// KAS startup. On timeout we mirror whatever the store currently holds
/// and let KAS surface `TokenExpiredError` on its first call.
const POPULATE_REFRESH_TIMEOUT: Duration = Duration::from_secs(10);

/// JSON shape consumed by KAS `FileAuthProvider`. No `refreshToken` field — see module docs.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KasToken {
    pub access_token: String,
    pub expires_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_arn: Option<String>,
    pub auth_method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
}

pub(crate) trait ToKasToken {
    fn to_kas_token(&self) -> KasToken;
}

impl ToKasToken for BuilderIdToken {
    fn to_kas_token(&self) -> KasToken {
        KasToken {
            access_token: self.access_token.0.clone(),
            expires_at: format_time(&self.expires_at),
            region: self.region.clone(),
            profile_arn: None,
            auth_method: "IdC".to_string(),
            start_url: self.start_url.clone(),
            provider: None,
            issuer_url: None,
            token_endpoint: None,
            client_id: None,
        }
    }
}

impl ToKasToken for SocialToken {
    fn to_kas_token(&self) -> KasToken {
        KasToken {
            access_token: self.access_token.0.clone(),
            expires_at: format_time(&self.expires_at),
            region: None,
            profile_arn: self.profile_arn.clone(),
            auth_method: "social".to_string(),
            start_url: None,
            // SocialProvider serializes as a lowercase string ("google" / "github").
            provider: serde_json::to_value(self.provider)
                .ok()
                .and_then(|v| v.as_str().map(String::from)),
            issuer_url: None,
            token_endpoint: None,
            client_id: None,
        }
    }
}

impl ToKasToken for ExternalIdpToken {
    fn to_kas_token(&self) -> KasToken {
        KasToken {
            access_token: self.access_token.0.clone(),
            expires_at: format_time(&self.expires_at),
            region: None,
            profile_arn: None,
            auth_method: "external_idp".to_string(),
            start_url: None,
            provider: None,
            issuer_url: Some(self.issuer_url.clone()),
            token_endpoint: Some(self.token_endpoint.clone()),
            client_id: Some(self.client_id.clone()),
        }
    }
}

/// Always `<data_local_dir>/<CLI_NAME>/kiro-auth-token-cli.json`. Not
/// overridable; see module docs for why `KIRO_KAS_TOKEN_PATH` is not
/// honored here.
pub fn kas_token_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join(CLI_NAME).join(KAS_TOKEN_FILE))
}

/// Best-effort write to the KAS file. No-op under `cfg!(test)` to protect the
/// developer's real KAS file when tests trigger `Token::save()`.
pub(crate) async fn write_token<T: ToKasToken>(token: &T) {
    if cfg!(test) {
        return;
    }
    let Some(path) = kas_token_path() else {
        warn!("Cannot determine KAS token path; skipping sync");
        return;
    };
    write_token_at(token, &path);
}

fn write_token_at<T: ToKasToken>(token: &T, path: &Path) {
    let kas = token.to_kas_token();
    match write_to_path(path, &kas) {
        Ok(()) => debug!(?path, "Synced auth token to KAS file"),
        Err(err) => warn!(?err, ?path, "Failed to write KAS token file"),
    }
}

/// Best-effort delete. NotFound is silent. No-op under `cfg!(test)`.
pub fn delete_kas_token_file() {
    if cfg!(test) {
        return;
    }
    let Some(path) = kas_token_path() else {
        return;
    };
    delete_kas_token_file_at(&path);
}

fn delete_kas_token_file_at(path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => debug!(?path, "Deleted KAS token file"),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {},
        Err(err) => warn!(?err, ?path, "Failed to delete KAS token file"),
    }
}

/// Seed the KAS file from the secret store before spawning KAS. Picks tokens
/// in [`UnifiedBearerResolver`] order (ExternalIdp → BuilderId → Social) so
/// KAS authenticates as the same identity as the rest of the CLI. If the
/// store is empty, removes any orphaned file. No-op under `cfg!(test)`.
///
/// Opens its own [`Database`] handle so V1 callers (whose `Database` type is
/// distinct from V2's) need no plumbing.
///
/// [`UnifiedBearerResolver`]: crate::auth::UnifiedBearerResolver
pub async fn populate_kas_from_store() {
    let Some(path) = kas_token_path() else {
        return;
    };
    populate_kas_from_store_at_path(&path).await;
}

/// Like [`populate_kas_from_store`] but writes to an explicit path. Use this
/// when the spawn site has its own override (e.g. `kiro-cli acp --token-path`)
/// so the seeded file matches what KAS actually reads.
pub async fn populate_kas_from_store_at_path(path: &Path) {
    if cfg!(test) {
        return;
    }
    let database = match Database::new().await {
        Ok(db) => db,
        Err(err) => {
            // Can't tell who the current identity is, so drop any existing
            // file rather than risk serving KAS a stale identity's token.
            // Trades a possible spurious re-login for correctness.
            warn!(
                ?err,
                "Failed to open database for KAS token sync; deleting any stale KAS file"
            );
            delete_kas_token_file_at(path);
            return;
        },
    };
    // Refresh-if-expired before mirroring so KAS doesn't immediately throw
    // `TokenExpiredError`. Time-boxed; on timeout we fall through and
    // mirror the stored token as-is.
    if tokio::time::timeout(POPULATE_REFRESH_TIMEOUT, refresh_priority_token(&database))
        .await
        .is_err()
    {
        warn!(
            timeout_secs = POPULATE_REFRESH_TIMEOUT.as_secs(),
            "Token refresh timed out during KAS populate; falling back to stored token"
        );
    }
    populate_kas_from_store_at(&database, path).await;
}

/// Refresh-if-expired side effect on the highest-priority present token.
/// Priority tracks [`UnifiedBearerResolver`]: ExternalIdp -> BuilderId ->
/// Social. Stops at the first present token; further refreshes wouldn't
/// be mirrored anyway.
///
/// `Token::load` errors are swallowed: the populate step that follows
/// reads the (possibly unrefreshed) token via `get_secret` and writes it.
/// Better a stale mirror than no mirror. `get_secret` errors are treated
/// as "not present" so a transient hit on the top key falls through; if
/// the store is truly broken, populate fails the same way and the
/// orphan-cleanup path runs.
///
/// [`UnifiedBearerResolver`]: crate::auth::UnifiedBearerResolver
async fn refresh_priority_token(database: &Database) {
    match priority_token_key(database).await {
        Some(k) if k == ExternalIdpToken::SECRET_KEY => {
            let _ = ExternalIdpToken::load(database).await;
        },
        Some(k) if k == BuilderIdToken::SECRET_KEY => {
            let _ = BuilderIdToken::load(database, None).await;
        },
        Some(k) if k == SocialToken::SECRET_KEY => {
            let _ = SocialToken::load(database).await;
        },
        _ => {},
    }
}

/// SECRET_KEY of the highest-priority present token, or `None`. Pure
/// decision; no side effects, no network. Extracted so the priority
/// order is unit-testable without triggering `Token::load` network I/O
/// (`ExternalIdpToken::load` lacks a `cfg!(test)` short-circuit). MUST
/// track [`populate_kas_from_store_at`]'s order; drift would let KAS
/// authenticate as a different identity than the in-process client.
async fn priority_token_key(database: &Database) -> Option<&'static str> {
    if matches!(database.get_secret(ExternalIdpToken::SECRET_KEY).await, Ok(Some(_))) {
        return Some(ExternalIdpToken::SECRET_KEY);
    }
    if matches!(database.get_secret(BuilderIdToken::SECRET_KEY).await, Ok(Some(_))) {
        return Some(BuilderIdToken::SECRET_KEY);
    }
    if matches!(database.get_secret(SocialToken::SECRET_KEY).await, Ok(Some(_))) {
        return Some(SocialToken::SECRET_KEY);
    }
    None
}

async fn populate_kas_from_store_at(database: &Database, path: &Path) {
    if try_populate_external_idp(database, path).await {
        return;
    }
    if try_populate_builder_id(database, path).await {
        return;
    }
    if try_populate_social(database, path).await {
        return;
    }
    // Empty store, or store has only an API key (which KAS file-provider
    // doesn't accept) — clean up any orphaned file.
    delete_kas_token_file_at(path);
}

async fn try_populate_external_idp(database: &Database, path: &Path) -> bool {
    let Ok(Some(secret)) = database.get_secret(ExternalIdpToken::SECRET_KEY).await else {
        return false;
    };
    let Ok(token) = serde_json::from_str::<ExternalIdpToken>(&secret.0) else {
        warn!("Failed to deserialize ExternalIdp token");
        return false;
    };
    write_token_at(&token, path);
    info!(auth = "ExternalIdp", "Populated KAS file from secret store");
    true
}

async fn try_populate_builder_id(database: &Database, path: &Path) -> bool {
    let Ok(Some(secret)) = database.get_secret(BuilderIdToken::SECRET_KEY).await else {
        return false;
    };
    let Ok(token) = serde_json::from_str::<BuilderIdToken>(&secret.0) else {
        warn!("Failed to deserialize BuilderId token");
        return false;
    };
    write_token_at(&token, path);
    info!(auth = "BuilderId", "Populated KAS file from secret store");
    true
}

async fn try_populate_social(database: &Database, path: &Path) -> bool {
    let Ok(Some(secret)) = database.get_secret(SocialToken::SECRET_KEY).await else {
        return false;
    };
    let Ok(token) = serde_json::from_str::<SocialToken>(&secret.0) else {
        warn!("Failed to deserialize Social token");
        return false;
    };
    write_token_at(&token, path);
    info!(auth = "Social", "Populated KAS file from secret store");
    true
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no parent directory"))?;
    std::fs::create_dir_all(parent)?;
    // NamedTempFile::new_in lands on Unix with mode 0o600 and `persist`'s
    // rename(2) preserves it, so the sidecar is owner-only without an
    // explicit chmod. Windows relies on NTFS ACLs in the user data dir.
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    use std::io::Write;
    tmp.write_all(bytes)?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

fn write_to_path(path: &Path, token: &KasToken) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(token).map_err(std::io::Error::other)?;
    write_atomic(path, &json)
}

fn format_time(t: &time::OffsetDateTime) -> String {
    t.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| t.to_string())
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;

    use super::*;
    use crate::auth::builder_id::OAuthFlow;
    use crate::auth::social::SocialProvider;
    use crate::database::Secret;

    fn builder_id() -> BuilderIdToken {
        BuilderIdToken {
            access_token: Secret("builder-access-tok".into()),
            expires_at: OffsetDateTime::from_unix_timestamp(1700000000).unwrap(),
            refresh_token: Some(Secret("builder-refresh-tok".into())),
            region: Some("us-east-1".into()),
            start_url: Some("https://view.awsapps.com/start".into()),
            oauth_flow: OAuthFlow::DeviceCode,
            scopes: None,
        }
    }

    fn social() -> SocialToken {
        SocialToken {
            access_token: Secret("social-access-tok".into()),
            expires_at: OffsetDateTime::from_unix_timestamp(1700000000).unwrap(),
            refresh_token: Some(Secret("social-refresh-tok".into())),
            provider: SocialProvider::Google,
            profile_arn: Some("arn:aws:iam::123456789012:profile/Test".into()),
        }
    }

    fn external_idp() -> ExternalIdpToken {
        ExternalIdpToken {
            access_token: Secret("idp-access-tok".into()),
            expires_at: OffsetDateTime::from_unix_timestamp(1700000000).unwrap(),
            refresh_token: Some(Secret("idp-refresh-tok".into())),
            issuer_url: "https://idp.example.com".into(),
            token_endpoint: "https://idp.example.com/token".into(),
            client_id: "client-123".into(),
        }
    }

    fn write_and_read(token: &impl ToKasToken) -> serde_json::Value {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token.json");
        write_to_path(&path, &token.to_kas_token()).unwrap();
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
    }

    /// Refresh-token MUST never reach the KAS file: KAS would self-refresh
    /// and burn the RT, racing kiro-cli's own refresh path.
    #[test]
    fn refresh_token_never_written() {
        for json in [
            write_and_read(&builder_id()),
            write_and_read(&social()),
            write_and_read(&external_idp()),
        ] {
            assert!(
                json.get("refreshToken").is_none(),
                "refreshToken must not appear: {json}"
            );
        }
    }

    #[test]
    fn auth_method_per_token_type() {
        assert_eq!(write_and_read(&builder_id())["authMethod"], "IdC");
        assert_eq!(write_and_read(&social())["authMethod"], "social");
        assert_eq!(write_and_read(&external_idp())["authMethod"], "external_idp");
    }

    #[test]
    fn type_specific_fields_round_trip() {
        let b = write_and_read(&builder_id());
        assert_eq!(b["startUrl"], "https://view.awsapps.com/start");
        assert_eq!(b["region"], "us-east-1");

        let s = write_and_read(&social());
        assert_eq!(s["provider"], "google");
        assert_eq!(s["profileArn"], "arn:aws:iam::123456789012:profile/Test");

        let i = write_and_read(&external_idp());
        assert_eq!(i["issuerUrl"], "https://idp.example.com");
        assert_eq!(i["tokenEndpoint"], "https://idp.example.com/token");
        assert_eq!(i["clientId"], "client-123");
    }

    #[test]
    fn write_atomic_leaves_no_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.json");
        write_atomic(&path, b"hello").unwrap();
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name() != "a.json")
            .collect();
        assert!(leftovers.is_empty(), "leftover temp file: {leftovers:?}");
    }

    #[test]
    fn write_atomic_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a").join("b").join("token.json");
        write_atomic(&path, b"x").unwrap();
        assert!(path.exists());
    }

    #[test]
    fn delete_missing_file_is_silent() {
        let dir = tempfile::tempdir().unwrap();
        delete_kas_token_file_at(&dir.path().join("nope.json"));
    }

    /// Populate priority MUST match V2 [`UnifiedBearerResolver`]: ExternalIdp
    /// → BuilderId → Social. If they diverge, KAS sees a different identity
    /// than the API client, which is silently broken auth.
    #[tokio::test]
    async fn populate_priority_external_idp_wins() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token.json");
        let database = Database::new().await.unwrap();
        database
            .set_secret(
                ExternalIdpToken::SECRET_KEY,
                &serde_json::to_string(&external_idp()).unwrap(),
            )
            .await
            .unwrap();
        database
            .set_secret(
                BuilderIdToken::SECRET_KEY,
                &serde_json::to_string(&builder_id()).unwrap(),
            )
            .await
            .unwrap();
        database
            .set_secret(SocialToken::SECRET_KEY, &serde_json::to_string(&social()).unwrap())
            .await
            .unwrap();

        populate_kas_from_store_at(&database, &path).await;
        let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(json["authMethod"], "external_idp");
    }

    #[tokio::test]
    async fn populate_priority_builder_id_over_social() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token.json");
        let database = Database::new().await.unwrap();
        database
            .set_secret(
                BuilderIdToken::SECRET_KEY,
                &serde_json::to_string(&builder_id()).unwrap(),
            )
            .await
            .unwrap();
        database
            .set_secret(SocialToken::SECRET_KEY, &serde_json::to_string(&social()).unwrap())
            .await
            .unwrap();

        populate_kas_from_store_at(&database, &path).await;
        let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(json["authMethod"], "IdC");
    }

    #[tokio::test]
    async fn populate_empty_store_clears_orphan_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token.json");
        std::fs::write(&path, "stale").unwrap();
        let database = Database::new().await.unwrap();
        populate_kas_from_store_at(&database, &path).await;
        assert!(!path.exists());
    }

    /// Refresh priority MUST match populate priority (ExternalIdp -> BuilderId
    /// -> Social) so the token we trigger refresh on is the same one
    /// populate writes. Drift would let KAS authenticate as one identity
    /// while the in-process API client uses another.
    #[tokio::test]
    async fn priority_token_key_external_idp_wins() {
        let database = Database::new().await.unwrap();
        database
            .set_secret(
                ExternalIdpToken::SECRET_KEY,
                &serde_json::to_string(&external_idp()).unwrap(),
            )
            .await
            .unwrap();
        database
            .set_secret(
                BuilderIdToken::SECRET_KEY,
                &serde_json::to_string(&builder_id()).unwrap(),
            )
            .await
            .unwrap();
        database
            .set_secret(SocialToken::SECRET_KEY, &serde_json::to_string(&social()).unwrap())
            .await
            .unwrap();
        assert_eq!(priority_token_key(&database).await, Some(ExternalIdpToken::SECRET_KEY));
    }

    #[tokio::test]
    async fn priority_token_key_builder_id_over_social() {
        let database = Database::new().await.unwrap();
        database
            .set_secret(
                BuilderIdToken::SECRET_KEY,
                &serde_json::to_string(&builder_id()).unwrap(),
            )
            .await
            .unwrap();
        database
            .set_secret(SocialToken::SECRET_KEY, &serde_json::to_string(&social()).unwrap())
            .await
            .unwrap();
        assert_eq!(priority_token_key(&database).await, Some(BuilderIdToken::SECRET_KEY));
    }

    #[tokio::test]
    async fn priority_token_key_social_alone() {
        let database = Database::new().await.unwrap();
        database
            .set_secret(SocialToken::SECRET_KEY, &serde_json::to_string(&social()).unwrap())
            .await
            .unwrap();
        assert_eq!(priority_token_key(&database).await, Some(SocialToken::SECRET_KEY));
    }

    #[tokio::test]
    async fn priority_token_key_empty_store_returns_none() {
        let database = Database::new().await.unwrap();
        assert_eq!(priority_token_key(&database).await, None);
    }

    /// `KIRO_KAS_TOKEN_PATH` is KAS's own user-facing env var. kiro-cli's
    /// own path resolver must NOT honor it: an external value must not be
    /// allowed to mutate kiro-cli's choice of where to write its own state.
    /// See module docs.
    #[test]
    #[serial_test::serial(env_kas_token_path)]
    fn kas_token_path_ignores_env_var() {
        // SAFETY: serialized via #[serial_test::serial] with the matching test below.
        unsafe { std::env::set_var("KIRO_KAS_TOKEN_PATH", "/tmp/custom-kas-token.json") };
        let path = kas_token_path().expect("data_local_dir must resolve in test env");
        // SAFETY: same serial guard.
        unsafe { std::env::remove_var("KIRO_KAS_TOKEN_PATH") };
        assert_ne!(
            path,
            PathBuf::from("/tmp/custom-kas-token.json"),
            "resolver must ignore KIRO_KAS_TOKEN_PATH",
        );
        let data_dir = dirs::data_local_dir()
            .expect("data_local_dir must resolve in test env")
            .join(CLI_NAME);
        assert!(
            path.starts_with(&data_dir),
            "kas token path {path:?} must live under data dir {data_dir:?}",
        );
        assert_eq!(path.file_name().and_then(|s| s.to_str()), Some(KAS_TOKEN_FILE));
    }

    /// Default location must live under the data dir, NOT `~/.aws/sso/cache/`.
    /// The autocomplete-side and the SQLite store already use the data dir;
    /// the KAS token file is co-located so all kiro-cli state lives in one
    /// XDG-respecting place.
    #[test]
    #[serial_test::serial(env_kas_token_path)]
    fn kas_token_path_default_is_under_data_dir() {
        // SAFETY: serialized via #[serial_test::serial] with the matching test above.
        unsafe { std::env::remove_var("KIRO_KAS_TOKEN_PATH") };
        let path = kas_token_path().expect("data_local_dir must resolve in test env");
        let data_dir = dirs::data_local_dir()
            .expect("data_local_dir must resolve in test env")
            .join(CLI_NAME);
        assert!(
            path.starts_with(&data_dir),
            "kas token path {path:?} must live under data dir {data_dir:?}",
        );
        assert_eq!(path.file_name().and_then(|s| s.to_str()), Some(KAS_TOKEN_FILE));
    }
}
