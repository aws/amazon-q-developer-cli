//! Token data exposed to KAS over the `_kiro/auth/getAccessToken` ACP
//! extension.
//!
//! This module is the contract between chat-cli (the host) and
//! `AcpCallbackAuthProvider` (the KAS-side auth provider) for
//! `--auth=acp-callback` mode. KAS calls back to the host whenever it
//! needs a fresh access token; the host owns the OIDC refresh token and
//! either returns its cached AT or performs a refresh under
//! [`crate::auth::refresh_coordinator::with_refresh_lock`].
//!
//! No `refreshToken` field. The refresh token never leaves chat-cli's
//! SQLite store.
//!
//! No `region` field. KAS derives region from `profileArn` segment 3
//! (matches kiro-cli's own resolver in `api_client::endpoints`), so the
//! host MUST supply a `profileArn` for every auth type.

use serde::Serialize;

use crate::api_client::BUILDER_ID_PROFILE_ARN;
use crate::auth::AuthError;
use crate::auth::builder_id::{
    BuilderIdToken,
    TokenType,
};
use crate::auth::external_idp::ExternalIdpToken;
use crate::auth::social::{
    SocialProvider,
    SocialToken,
};
use crate::database::Database;

/// Method dispatched to ACP `Client::ext_method` after the runtime strips the
/// leading `_` from the wire method `_kiro/auth/getAccessToken`. Listed once
/// here so dispatchers can match against it without drift.
pub const KAS_AUTH_EXT_METHOD: &str = "kiro/auth/getAccessToken";

/// Auth method advertised to KAS in the `_kiro/auth/getAccessToken` response.
/// KAS maps each value to a `TokenType` request header. Auth types that need
/// no header (Builder ID / IdC / Social) send nothing. Add a variant here when
/// a new auth type needs a `TokenType` header (e.g. API key -> `api_key`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum KasAuthMethod {
    /// `TokenType: EXTERNAL_IDP`.
    #[serde(rename = "external_idp")]
    ExternalIdp,
}

/// Sign-in provider advertised to KAS in the `_kiro/auth/getAccessToken`
/// response. KAS's `GovernanceService` uses this to decide whether the user is
/// under enterprise governance — only `Enterprise` and `ExternalIdp` are
/// treated as enterprise-managed; everything else (Builder ID, social) skips
/// the GetProfile call entirely.
///
/// Distinct from [`KasAuthMethod`], which only drives the `TokenType` request
/// header. `profileArn` cannot be used for this decision because every
/// acp-callback token carries one (Builder ID gets a hardcoded routing ARN).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum KasProvider {
    /// IAM Identity Center — enterprise-managed.
    #[serde(rename = "Enterprise")]
    Enterprise,
    /// External IdP federation — enterprise-managed.
    #[serde(rename = "ExternalIdp")]
    ExternalIdp,
    /// Builder ID free tier — not enterprise.
    #[serde(rename = "BuilderId")]
    BuilderId,
    /// Social sign-in (Google) — not enterprise.
    #[serde(rename = "Google")]
    Google,
    /// Social sign-in (GitHub) — not enterprise.
    #[serde(rename = "Github")]
    Github,
}

/// Token data returned to KAS for the `_kiro/auth/getAccessToken`
/// extension. Wire shape matches the response type defined in
/// `kiro-agent`'s `acp-type-covenant/client-capabilities/index.ts`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpCallbackToken {
    /// OIDC access token. Treated as opaque by callers.
    pub access_token: String,
    /// ISO-8601 (RFC 3339) expiry timestamp. KAS rejects responses where
    /// this is inside its 3-minute pre-expiry buffer (would force an
    /// immediate re-refresh loop), so callers MUST NOT serve a token
    /// they're about to refresh themselves.
    pub expires_at: String,
    /// Profile ARN bound to this token. KAS parses the AWS region from
    /// segment 3 of the ARN (`arn:aws:codewhisperer:<region>:...`); every
    /// downstream API call also requires this field (see
    /// `api_client::ProfileResolver::require_arn`). Missing here -> the
    /// resolver MUST return an error rather than letting KAS silently
    /// fall back to its default region.
    pub profile_arn: String,
    /// Auth method KAS maps to a `TokenType` request header. `None` for auth
    /// types that need none (Builder ID / IdC / Social); omitted from the wire.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_method: Option<KasAuthMethod>,
    /// Sign-in provider KAS uses to decide enterprise governance. `None` is
    /// omitted from the wire; resolved tokens always set it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<KasProvider>,
}

/// Resolve the highest-priority token in the SQLite store, refreshing it
/// under the cross-process refresh lock if it has crossed expiry. Priority
/// matches [`UnifiedBearerResolver`]: ExternalIdp -> BuilderId -> Social.
/// Returns `Ok(None)` if the store has no usable token (user is not
/// logged in). Returns `Err(AuthError::ProfileNotSelected)` if a token is
/// present but no profile ARN can be resolved (e.g. IdC user whose DB
/// profile row is missing). Other errors propagate as-is.
///
/// The `profileArn` on the returned token is sourced per-arm:
/// - ExternalIdp / IdC: `database.get_auth_profile()` (set by `select_profile_interactive` at
///   login)
/// - BuilderId free tier: [`BUILDER_ID_PROFILE_ARN`]
/// - Social: `SocialToken.profile_arn` (set by the social auth response; load-time guard rejects
///   tokens missing this field)
///
/// [`UnifiedBearerResolver`]: crate::auth::UnifiedBearerResolver
pub async fn resolve_kas_token_for_callback(database: &Database) -> Result<Option<AcpCallbackToken>, AuthError> {
    if let Some(token) = ExternalIdpToken::coordinated_refresh(database).await? {
        return Ok(Some(AcpCallbackToken {
            access_token: token.access_token.0.clone(),
            expires_at: format_time(&token.expires_at),
            profile_arn: profile_arn_from_db(database)?,
            auth_method: Some(KasAuthMethod::ExternalIdp),
            provider: Some(KasProvider::ExternalIdp),
        }));
    }
    if let Some(token) = BuilderIdToken::coordinated_refresh(database, None).await? {
        let (profile_arn, provider) = match token.token_type() {
            TokenType::BuilderId => (BUILDER_ID_PROFILE_ARN.to_string(), KasProvider::BuilderId),
            TokenType::IamIdentityCenter => (profile_arn_from_db(database)?, KasProvider::Enterprise),
        };
        return Ok(Some(AcpCallbackToken {
            access_token: token.access_token.0.clone(),
            expires_at: format_time(&token.expires_at),
            profile_arn,
            auth_method: None,
            provider: Some(provider),
        }));
    }
    if let Some(token) = SocialToken::coordinated_refresh(database).await? {
        // `SocialToken::load` rejects tokens missing `profile_arn`, so any
        // token reaching here must carry one. Treat absence as the same
        // unrecoverable failure we surface elsewhere.
        let profile_arn = token.profile_arn.clone().ok_or(AuthError::ProfileNotSelected)?;
        let provider = match token.provider {
            SocialProvider::Google => KasProvider::Google,
            SocialProvider::Github => KasProvider::Github,
        };
        return Ok(Some(AcpCallbackToken {
            access_token: token.access_token.0.clone(),
            expires_at: format_time(&token.expires_at),
            profile_arn,
            auth_method: None,
            provider: Some(provider),
        }));
    }
    Ok(None)
}

fn profile_arn_from_db(database: &Database) -> Result<String, AuthError> {
    database
        .get_auth_profile()
        .ok()
        .flatten()
        .map(|p| p.arn)
        .ok_or(AuthError::ProfileNotSelected)
}

fn format_time(t: &time::OffsetDateTime) -> String {
    t.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| t.to_string())
}

/// Dispatch an ACP `ext_method` call to the KAS auth callback handler.
/// Returns [`acp::Error::method_not_found`] for any other method.
///
/// Used by Rust ACP `Client` impls (`NonInteractiveAcpClient`,
/// `MinimalAcpClient`) that own no `Database` of their own; opens its
/// own short-lived handle. The TUI has its own TypeScript handler that
/// shells out to `chat _ get-kas-token`.
pub async fn handle_ext_method(
    args: agent_client_protocol::ExtRequest,
) -> std::result::Result<agent_client_protocol::ExtResponse, agent_client_protocol::Error> {
    if &*args.method != KAS_AUTH_EXT_METHOD {
        return Err(agent_client_protocol::Error::method_not_found());
    }
    let database = Database::new().await.map_err(|e| {
        agent_client_protocol::Error::internal_error().data(Some(serde_json::json!({
            "details": format!("failed to open auth store: {e}"),
        })))
    })?;
    handle_kas_auth_ext_method(&database).await
}

/// Handle a `_kiro/auth/getAccessToken` ACP `ext_method` call against a
/// caller-supplied [`Database`]. Use [`handle_ext_method`] from
/// production paths; tests use this directly to pass a test database.
///
/// Errors are returned as `acp::Error::internal_error` with the user-facing
/// message in `data.details`. KAS translates that into a `TokenExpiredError`.
pub(crate) async fn handle_kas_auth_ext_method(
    database: &Database,
) -> std::result::Result<agent_client_protocol::ExtResponse, agent_client_protocol::Error> {
    use std::sync::Arc;

    use agent_client_protocol::{
        Error as AcpError,
        ExtResponse,
    };

    fn fail(message: String) -> AcpError {
        AcpError::internal_error().data(Some(serde_json::json!({ "details": message })))
    }

    let token = match resolve_kas_token_for_callback(database).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            return Err(fail(
                "You are not logged in. Please log in with `kiro-cli login`.".into(),
            ));
        },
        // Surface `ProfileNotSelected` directly - its `Display` is already a
        // user-facing instruction. Wrap anything else as a refresh failure.
        Err(e @ AuthError::ProfileNotSelected) => return Err(fail(e.to_string())),
        Err(e) => return Err(fail(format!("auth refresh failed: {e}"))),
    };

    let raw = serde_json::value::to_raw_value(&token).map_err(|e| fail(format!("internal: serialize token: {e}")))?;
    Ok(ExtResponse::new(Arc::from(raw)))
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;

    use super::*;
    use crate::auth::builder_id::OAuthFlow;
    use crate::auth::social::SocialProvider;
    use crate::database::{
        AuthProfile,
        Secret,
    };

    fn unexpired() -> OffsetDateTime {
        // 1h in the future so `coordinated_refresh` short-circuits without
        // hitting an IdP. Refresh paths are exercised by per-token-type
        // refresh tests, not here.
        OffsetDateTime::now_utc() + time::Duration::hours(1)
    }

    fn unexpired_builder_id_freetier() -> BuilderIdToken {
        // `start_url = None` -> `TokenType::BuilderId` -> resolver should
        // return `BUILDER_ID_PROFILE_ARN`.
        BuilderIdToken {
            access_token: Secret("builder-access-tok".into()),
            expires_at: unexpired(),
            refresh_token: Some(Secret("builder-refresh-tok".into())),
            region: Some("us-east-1".into()),
            start_url: None,
            oauth_flow: OAuthFlow::DeviceCode,
            scopes: None,
        }
    }

    fn unexpired_idc() -> BuilderIdToken {
        // Non-default `start_url` -> `TokenType::IamIdentityCenter` ->
        // resolver should fall back to `database.get_auth_profile()`.
        BuilderIdToken {
            access_token: Secret("idc-access-tok".into()),
            expires_at: unexpired(),
            refresh_token: Some(Secret("idc-refresh-tok".into())),
            region: Some("eu-central-1".into()),
            start_url: Some("https://my-idc.awsapps.com/start".into()),
            oauth_flow: OAuthFlow::DeviceCode,
            scopes: None,
        }
    }

    fn unexpired_social() -> SocialToken {
        SocialToken {
            access_token: Secret("social-access-tok".into()),
            expires_at: unexpired(),
            refresh_token: Some(Secret("social-refresh-tok".into())),
            provider: SocialProvider::Google,
            profile_arn: Some("arn:aws:codewhisperer:us-east-1:111122223333:profile/Social".into()),
        }
    }

    fn unexpired_external_idp() -> ExternalIdpToken {
        ExternalIdpToken {
            access_token: Secret("idp-access-tok".into()),
            expires_at: unexpired(),
            refresh_token: Some(Secret("idp-refresh-tok".into())),
            issuer_url: "https://idp.example.com".into(),
            token_endpoint: "https://idp.example.com/token".into(),
            client_id: "client-123".into(),
            scopes: "openid offline_access api://client-123/res".into(),
        }
    }

    fn idc_profile() -> AuthProfile {
        AuthProfile {
            arn: "arn:aws:codewhisperer:eu-central-1:444455556666:profile/IdcEU".into(),
            profile_name: "IdcEU".into(),
        }
    }

    /// Resolution priority MUST match populate priority and
    /// `UnifiedBearerResolver`. Drift would let KAS authenticate as one
    /// identity while the in-process API client uses another.
    #[tokio::test]
    async fn resolve_callback_token_external_idp_wins() {
        let mut database = Database::new().await.unwrap();
        database.set_auth_profile(&idc_profile()).unwrap();
        database
            .set_secret(
                ExternalIdpToken::SECRET_KEY,
                &serde_json::to_string(&unexpired_external_idp()).unwrap(),
            )
            .await
            .unwrap();
        database
            .set_secret(
                BuilderIdToken::SECRET_KEY,
                &serde_json::to_string(&unexpired_builder_id_freetier()).unwrap(),
            )
            .await
            .unwrap();
        database
            .set_secret(
                SocialToken::SECRET_KEY,
                &serde_json::to_string(&unexpired_social()).unwrap(),
            )
            .await
            .unwrap();

        let token = resolve_kas_token_for_callback(&database)
            .await
            .unwrap()
            .expect("token resolved");
        assert_eq!(token.access_token, "idp-access-tok");
        // External IdP carries no profile of its own; resolver MUST fall
        // back to the AuthProfile row written by `select_profile_interactive`.
        assert_eq!(token.profile_arn, idc_profile().arn);
        assert_eq!(token.auth_method, Some(KasAuthMethod::ExternalIdp));
        assert_eq!(token.provider, Some(KasProvider::ExternalIdp));
    }

    /// BuilderId free-tier (`start_url = None`) MUST return the canonical
    /// hardcoded ARN, matching `ProfileResolver::for_builder_id()`.
    #[tokio::test]
    async fn resolve_callback_token_builder_id_freetier_uses_canonical_arn() {
        let database = Database::new().await.unwrap();
        database
            .set_secret(
                BuilderIdToken::SECRET_KEY,
                &serde_json::to_string(&unexpired_builder_id_freetier()).unwrap(),
            )
            .await
            .unwrap();

        let token = resolve_kas_token_for_callback(&database)
            .await
            .unwrap()
            .expect("token resolved");
        assert_eq!(token.access_token, "builder-access-tok");
        assert_eq!(token.profile_arn, BUILDER_ID_PROFILE_ARN);
        assert_eq!(token.auth_method, None);
        assert_eq!(token.provider, Some(KasProvider::BuilderId));
    }

    /// IdC (BuilderId with non-default `start_url`) MUST source the profile
    /// from `database.get_auth_profile()`. Pinning eu-central-1 here exercises
    /// the path that was broken when we were echoing `BuilderIdToken.region`
    /// (an OIDC-region field that diverges from the CW endpoint region).
    #[tokio::test]
    async fn resolve_callback_token_idc_uses_db_profile() {
        let mut database = Database::new().await.unwrap();
        database.set_auth_profile(&idc_profile()).unwrap();
        database
            .set_secret(
                BuilderIdToken::SECRET_KEY,
                &serde_json::to_string(&unexpired_idc()).unwrap(),
            )
            .await
            .unwrap();

        let token = resolve_kas_token_for_callback(&database)
            .await
            .unwrap()
            .expect("token resolved");
        assert_eq!(token.access_token, "idc-access-tok");
        assert_eq!(token.profile_arn, idc_profile().arn);
        assert_eq!(token.auth_method, None);
        assert_eq!(token.provider, Some(KasProvider::Enterprise));
    }

    #[tokio::test]
    async fn resolve_callback_token_social_uses_token_profile_arn() {
        let database = Database::new().await.unwrap();
        database
            .set_secret(
                SocialToken::SECRET_KEY,
                &serde_json::to_string(&unexpired_social()).unwrap(),
            )
            .await
            .unwrap();

        let token = resolve_kas_token_for_callback(&database)
            .await
            .unwrap()
            .expect("token resolved");
        assert_eq!(token.access_token, "social-access-tok");
        assert_eq!(
            token.profile_arn,
            "arn:aws:codewhisperer:us-east-1:111122223333:profile/Social"
        );
        assert_eq!(token.auth_method, None);
        assert_eq!(token.provider, Some(KasProvider::Google));
    }

    /// IdC user logged in but `database.get_auth_profile()` returns None
    /// (corrupt/pre-migration DB). MUST surface as `ProfileNotSelected`,
    /// NOT silently ship a token with no `profileArn` - downstream API
    /// calls require it (`api_client::ProfileResolver::require_arn`) and
    /// KAS would otherwise default to `us-east-1`.
    #[tokio::test]
    async fn resolve_callback_token_idc_without_profile_errors() {
        let database = Database::new().await.unwrap();
        database
            .set_secret(
                BuilderIdToken::SECRET_KEY,
                &serde_json::to_string(&unexpired_idc()).unwrap(),
            )
            .await
            .unwrap();

        let err = resolve_kas_token_for_callback(&database)
            .await
            .expect_err("ProfileNotSelected");
        assert!(matches!(err, AuthError::ProfileNotSelected), "{err:?}");
    }

    /// External IdP user logged in but no DB profile -> same error.
    #[tokio::test]
    async fn resolve_callback_token_external_idp_without_profile_errors() {
        let database = Database::new().await.unwrap();
        database
            .set_secret(
                ExternalIdpToken::SECRET_KEY,
                &serde_json::to_string(&unexpired_external_idp()).unwrap(),
            )
            .await
            .unwrap();

        let err = resolve_kas_token_for_callback(&database)
            .await
            .expect_err("ProfileNotSelected");
        assert!(matches!(err, AuthError::ProfileNotSelected), "{err:?}");
    }

    /// Empty store -> `Ok(None)`. The `chat _ get-kas-token` subcommand
    /// surfaces this as the "not logged in" error.
    #[tokio::test]
    async fn resolve_callback_token_empty_store_returns_none() {
        let database = Database::new().await.unwrap();
        let token = resolve_kas_token_for_callback(&database).await.unwrap();
        assert!(token.is_none());
    }

    /// `AcpCallbackToken` carries only fields safe to send over ACP. The
    /// refresh token MUST never be exposed to KAS, and `region` MUST NOT
    /// be set since KAS derives it from `profileArn`.
    #[test]
    fn acp_callback_token_serialization_matches_kas_wire_shape() {
        let token = AcpCallbackToken {
            access_token: "at".into(),
            expires_at: "2099-01-01T00:00:00Z".into(),
            profile_arn: "arn:aws:codewhisperer:us-east-1:1:profile/x".into(),
            auth_method: None,
            provider: None,
        };
        let json = serde_json::to_value(&token).unwrap();
        assert!(json.get("refreshToken").is_none(), "{json}");
        assert!(json.get("refresh_token").is_none(), "{json}");
        assert!(json.get("region").is_none(), "{json}");
        // KAS wire contract: camelCase, exactly these fields.
        let map = json.as_object().expect("object");
        let mut keys: Vec<_> = map.keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, vec!["accessToken", "expiresAt", "profileArn"]);
    }

    /// External IdP tokens MUST serialize `authMethod: "external_idp"` so KAS's
    /// `AcpCallbackAuthProvider` applies the `TokenType: EXTERNAL_IDP` header.
    /// Omitting it is the bug that made external-IDP CLI->KAS sessions fail.
    #[test]
    fn acp_callback_token_external_idp_serializes_auth_method() {
        let token = AcpCallbackToken {
            access_token: "at".into(),
            expires_at: "2099-01-01T00:00:00Z".into(),
            profile_arn: "arn:aws:codewhisperer:us-east-1:1:profile/x".into(),
            auth_method: Some(KasAuthMethod::ExternalIdp),
            provider: Some(KasProvider::ExternalIdp),
        };
        let json = serde_json::to_value(&token).unwrap();
        assert_eq!(json.get("authMethod").and_then(|v| v.as_str()), Some("external_idp"));
        assert_eq!(json.get("provider").and_then(|v| v.as_str()), Some("ExternalIdp"));
        assert!(json.get("auth_method").is_none(), "must be camelCase: {json}");
    }

    /// Happy path: `handle_kas_auth_ext_method` returns an `ExtResponse`
    /// whose body is the KAS wire shape. `region` MUST NOT cross the ACP
    /// boundary - KAS derives region from `profileArn`.
    #[tokio::test]
    async fn handle_ext_method_returns_kas_wire_shape() {
        let database = Database::new().await.unwrap();
        database
            .set_secret(
                BuilderIdToken::SECRET_KEY,
                &serde_json::to_string(&unexpired_builder_id_freetier()).unwrap(),
            )
            .await
            .unwrap();

        let response = handle_kas_auth_ext_method(&database).await.expect("response ok");
        let body: serde_json::Value = serde_json::from_str(response.0.get()).unwrap();
        let map = body.as_object().expect("object body");
        assert_eq!(
            map.get("accessToken").and_then(|v| v.as_str()),
            Some("builder-access-tok")
        );
        assert!(map.get("expiresAt").and_then(|v| v.as_str()).is_some());
        assert_eq!(
            map.get("profileArn").and_then(|v| v.as_str()),
            Some(BUILDER_ID_PROFILE_ARN)
        );
        assert!(map.get("region").is_none(), "{map:?}");
    }

    /// Empty store -> ACP error with the user-facing "not logged in" message
    /// in `data.details`. KAS translates this into a `TokenExpiredError`.
    #[tokio::test]
    async fn handle_ext_method_empty_store_returns_login_error() {
        let database = Database::new().await.unwrap();
        let err = handle_kas_auth_ext_method(&database)
            .await
            .expect_err("not-logged-in error");
        let details = err
            .data
            .as_ref()
            .and_then(|d| d.get("details"))
            .and_then(|d| d.as_str())
            .expect("data.details set");
        assert!(
            details.contains("not logged in") && details.contains("kiro-cli login"),
            "{details}"
        );
    }

    /// Logged in (token present) but no profile resolvable -> ACP error
    /// surfaces `AuthError::ProfileNotSelected`'s Display message verbatim,
    /// distinct from the "not logged in" path so the user gets the right
    /// recovery action.
    #[tokio::test]
    async fn handle_ext_method_missing_profile_returns_profile_error() {
        let database = Database::new().await.unwrap();
        database
            .set_secret(
                BuilderIdToken::SECRET_KEY,
                &serde_json::to_string(&unexpired_idc()).unwrap(),
            )
            .await
            .unwrap();

        let err = handle_kas_auth_ext_method(&database)
            .await
            .expect_err("ProfileNotSelected error");
        let details = err
            .data
            .as_ref()
            .and_then(|d| d.get("details"))
            .and_then(|d| d.as_str())
            .expect("data.details set");
        assert_eq!(details, AuthError::ProfileNotSelected.to_string());
    }
}
