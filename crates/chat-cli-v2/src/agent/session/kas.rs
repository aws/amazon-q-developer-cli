//! Short-lived ACP client for KAS session operations (list, delete).
//!
//! Spawns a KAS child process, initializes ACP, and dispatches the native
//! `session/list` request and the `_kiro/session/delete` ext_method (ACP has
//! no native session delete yet). Child is killed on drop.
//!
//! Must be run inside a `tokio::task::LocalSet` because `ClientSideConnection`
//! uses `spawn_local` internally.

use std::path::Path;

use acp::Agent as _;
use agent_client_protocol as acp;
use eyre::{
    Result,
    WrapErr,
};
use tokio::process::Child;
use tokio_util::compat::{
    TokioAsyncReadCompatExt,
    TokioAsyncWriteCompatExt,
};
use tracing::debug;

use crate::agent::acp::schema::SessionInfoEntry;

/// Interface for KAS session management operations.
#[async_trait::async_trait(?Send)]
pub trait KasSessionClient {
    /// List KAS sessions for the given workspace `cwd`.
    async fn list_sessions(&self, cwd: &Path) -> Result<Vec<SessionInfoEntry>>;

    /// Delete a KAS session by id.
    ///
    /// `session_source` selects the store the delete routes to: `Some("remote")`
    /// for a cloud session, `None` (or `Some("local")`) for the on-disk store.
    /// Sent as the top-level `sessionSource` param on `_kiro/session/delete`,
    /// omitted when `None`.
    ///
    /// Returns the backend's `{ success }` value without treating transport
    /// success as deletion success.
    async fn delete_session(&self, session_id: &str, session_source: Option<&str>) -> Result<bool>;
}

/// ACP-backed [`KasSessionClient`]. Owns a connected KAS child and issues
/// native ACP `session/list` for listing plus the `_kiro/session/delete`
/// ext_method for deletion.
pub struct KasAcpSessionClient {
    conn: acp::ClientSideConnection,
    _child: Child,
    /// Whether the handshake advertised a `remote` session source. Gates the
    /// `sessionSource: "all"` list request; `false` absent the cap keeps the
    /// request byte-identical.
    remote_sessions_advertised: bool,
    /// Whether the handshake advertised the user list scope. The remote store
    /// owns the user slice, fetched only when the list also asks
    /// `listScope: "both"`; this gates sending it.
    user_list_scope_advertised: bool,
}

impl KasAcpSessionClient {
    /// Initialize ACP over a pre-spawned KAS child process.
    ///
    /// The child must have piped stdin/stdout. Use
    /// `chat_cli::cli::spawn_kas_process` to create the child with the
    /// correct args and token path.
    pub async fn connect(mut child: Child) -> Result<Self> {
        debug!("initializing ACP connection to KAS child");

        let outgoing = child
            .stdin
            .take()
            .ok_or_else(|| eyre::eyre!("KAS child has no piped stdin"))?
            .compat_write();
        let incoming = child
            .stdout
            .take()
            .ok_or_else(|| eyre::eyre!("KAS child has no piped stdout"))?
            .compat();

        let (conn, handle_io) = acp::ClientSideConnection::new(MinimalAcpClient, outgoing, incoming, |fut| {
            tokio::task::spawn_local(fut);
        });
        tokio::task::spawn_local(handle_io);

        // 15s cap on handshake so a hung child doesn't block the CLI indefinitely.
        let init_fut = conn.initialize(
            acp::InitializeRequest::new(acp::ProtocolVersion::V1)
                .client_info(Some(acp::Implementation::new("kiro-cli", env!("CARGO_PKG_VERSION")))),
        );
        let init = match tokio::time::timeout(std::time::Duration::from_secs(15), init_fut).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(e)) => return Err(e).wrap_err("ACP initialize handshake failed"),
            Err(_) => {
                return Err(eyre::eyre!("ACP initialize timed out after 15s"));
            },
        };

        // Gate remote-row listing on the handshake advertising a `remote`
        // session source; `false` absent the cap.
        let remote_sessions_advertised = advertises_remote_session_source(&init.agent_capabilities.meta);
        let user_list_scope_advertised = advertises_user_list_scope(&init.agent_capabilities.meta);

        debug!(
            remote_sessions_advertised,
            user_list_scope_advertised, "KAS session client connected"
        );

        Ok(Self {
            conn,
            _child: child,
            remote_sessions_advertised,
            user_list_scope_advertised,
        })
    }
}

/// True when the KAS initialize handshake advertised a `remote` session source
/// (`agentCapabilities._meta.kiro.sessionSources ∋ "remote"`). Defensive against
/// a missing/malformed `_meta` (returns
/// `false`), so absent the capability the list request stays local-only.
///
/// `pub` so it can be unit-tested from a crate whose in-crate `#[cfg(test)]`
/// actually runs (`chat_cli_v2` itself is `#![cfg(not(test))]`).
pub fn advertises_remote_session_source(caps_meta: &Option<serde_json::Map<String, serde_json::Value>>) -> bool {
    caps_meta
        .as_ref()
        .and_then(|m| m.get("kiro"))
        .and_then(|k| k.get("sessionSources"))
        .and_then(|s| s.as_array())
        .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some("remote")))
}

/// True when the KAS initialize handshake advertised the `user` list scope
/// (`agentCapabilities._meta.kiro.sessionListScopes ∋ "user"`). Gates sending
/// `listScope: "both"` on a `session/list`, which is what makes KAS actually
/// fetch the user-scoped remote rows. `false` absent the cap (dark-safe).
pub fn advertises_user_list_scope(caps_meta: &Option<serde_json::Map<String, serde_json::Value>>) -> bool {
    caps_meta
        .as_ref()
        .and_then(|m| m.get("kiro"))
        .and_then(|k| k.get("sessionListScopes"))
        .and_then(|s| s.as_array())
        .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some("user")))
}

/// Reads a string field from a `session/list` row's `_meta.kiro` bag — e.g.
/// `description` (the KAS session name / first prompt) or `createdAt`. Returns
/// `None`
/// when `_meta`, `kiro`, the key, or its string form is absent — so a local row
/// (no `_meta.kiro`) yields `None` and the merged list keeps the standard ACP
/// field. `pub` so it can be unit-tested from a crate whose `#[cfg(test)]` runs.
pub fn kiro_meta_string(meta: &Option<serde_json::Map<String, serde_json::Value>>, key: &str) -> Option<String> {
    meta.as_ref()
        .and_then(|m| m.get("kiro"))
        .and_then(|k| k.get(key))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// Reads WHERE a `session/list` row runs from the nested
/// `_meta.kiro.executionTarget.kind` string (`"local"` | `"cloud-sandbox"`,
/// or a future/renamed placement). Returns `None` only when `executionTarget`
/// itself is absent — a local row (no `_meta.kiro`) yields `None` and is
/// treated as local. An `executionTarget` that is PRESENT but unreadable (a
/// bare string, a missing or non-string `kind`) reads as `Some("unknown")`
/// rather than `None`, so a wire-shape change lands on the hidden side of the
/// rollout gate instead of silently passing as local (fail-closed). Distinct
/// from [`kiro_meta_string`] because the value is nested one level deeper (an
/// object with a `kind`), not a flat string. `pub` so it can be unit-tested
/// from a crate whose `#[cfg(test)]` runs (this crate is `#![cfg(not(test))]`).
pub fn kiro_execution_target(meta: &Option<serde_json::Map<String, serde_json::Value>>) -> Option<String> {
    let target = meta
        .as_ref()
        .and_then(|m| m.get("kiro"))
        .and_then(|k| k.get("executionTarget"))?;
    Some(
        target
            .get("kind")
            .and_then(|kind| kind.as_str())
            .unwrap_or("unknown")
            .to_string(),
    )
}

#[async_trait::async_trait(?Send)]
impl KasSessionClient for KasAcpSessionClient {
    async fn list_sessions(&self, cwd: &Path) -> Result<Vec<SessionInfoEntry>> {
        debug!(cwd = %cwd.display(), "listing KAS sessions via native session/list");
        let mut request = acp::ListSessionsRequest::new().cwd(Some(cwd.to_path_buf()));
        if self.remote_sessions_advertised {
            // Ask for both stores so remote rows appear alongside local ones.
            // `sessionSource: "all"` alone isn't enough: the remote store owns the
            // user slice, which KAS fetches only when `listScope` asks for it
            // (default `"workspace"` skips remote). Send `"both"` when the user
            // scope is advertised. Gated on the caps; absent them, no `_meta` is
            // sent and the request is byte-identical to a local-only list.
            let mut kiro = serde_json::Map::new();
            kiro.insert("sessionSource".into(), serde_json::json!("all"));
            if self.user_list_scope_advertised {
                kiro.insert("listScope".into(), serde_json::json!("both"));
            }
            request.meta = serde_json::json!({ "kiro": kiro }).as_object().cloned();
        }
        let resp = self
            .conn
            .list_sessions(request)
            .await
            .wrap_err("failed to list KAS sessions")?;
        let entries: Vec<SessionInfoEntry> = resp
            .sessions
            .into_iter()
            .map(|info| {
                // Name + age for remote rows: KAS carries the session name as
                // `description` and the timestamp as `createdAt` under `_meta.kiro`.
                // Use them as fallbacks
                // when the standard ACP `title`/`updatedAt` are absent on a remote
                // row; a local row has no `_meta.kiro` -> None -> standard fields win
                // (dark-safe, byte-identical local listing).
                let meta_description = kiro_meta_string(&info.meta, "description");
                let meta_created_at = kiro_meta_string(&info.meta, "createdAt");
                SessionInfoEntry {
                    session_id: info.session_id.0.to_string(),
                    // Read WHERE the session runs from `_meta.kiro.executionTarget.kind`.
                    // `None` today (KAS advertises no remote store, so
                    // no row carries it) -> the merged picker treats it as local. Remove the
                    // read once remote rows are always present; the field itself is harmless.
                    execution_target: kiro_execution_target(&info.meta),
                    cwd: info.cwd,
                    title: info.title.or(meta_description),
                    updated_at: info.updated_at.or(meta_created_at),
                    message_count: None,
                    // Cold snapshot of activity status from `_meta.kiro.status`;
                    // `None` for a local row (no `_meta.kiro`).
                    status: kiro_meta_string(&info.meta, "status"),
                }
            })
            .collect();
        debug!(count = entries.len(), "received KAS sessions");
        Ok(entries)
    }

    async fn delete_session(&self, session_id: &str, session_source: Option<&str>) -> Result<bool> {
        debug!(%session_id, ?session_source, "deleting KAS session via _kiro/session/delete ext_method");
        let request = DeleteSessionRequest {
            session_id,
            session_source,
        };
        let raw = acp::RawValue::from_string(serde_json::to_string(&request)?)?;
        let resp = self
            .conn
            .ext_method(acp::ExtRequest::new("kiro/session/delete", raw.into()))
            .await
            .wrap_err_with(|| format!("failed to delete KAS session '{session_id}'"))?;
        let DeleteSessionResponse { success } =
            serde_json::from_str(resp.0.get()).wrap_err("failed to parse KAS _kiro/session/delete response")?;
        debug!(%session_id, success, "KAS session delete completed");
        Ok(success)
    }
}

/// Wire-format request for `_kiro/session/delete`.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DeleteSessionRequest<'a> {
    session_id: &'a str,
    /// Top-level store selector: `"remote"` routes to the cloud store; omitted
    /// (`None`) means the default on-disk store. `"all"` is never sent
    /// (KAS rejects it at the boundary).
    #[serde(skip_serializing_if = "Option::is_none")]
    session_source: Option<&'a str>,
}

/// Wire-format response for `_kiro/session/delete`. KAS returns `{ success: bool }`.
#[derive(Debug, serde::Deserialize)]
struct DeleteSessionResponse {
    success: bool,
}

/// No-op [`acp::Client`]. The session client only issues outbound requests,
/// so inbound notifications are dropped and inbound requests return
/// `method_not_found`.
struct MinimalAcpClient;

#[async_trait::async_trait(?Send)]
impl acp::Client for MinimalAcpClient {
    async fn session_notification(&self, _: acp::SessionNotification) -> acp::Result<()> {
        Ok(())
    }

    async fn request_permission(
        &self,
        _: acp::RequestPermissionRequest,
    ) -> acp::Result<acp::RequestPermissionResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn write_text_file(&self, _: acp::WriteTextFileRequest) -> acp::Result<acp::WriteTextFileResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn read_text_file(&self, _: acp::ReadTextFileRequest) -> acp::Result<acp::ReadTextFileResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn create_terminal(&self, _: acp::CreateTerminalRequest) -> acp::Result<acp::CreateTerminalResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn terminal_output(&self, _: acp::TerminalOutputRequest) -> acp::Result<acp::TerminalOutputResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn release_terminal(&self, _: acp::ReleaseTerminalRequest) -> acp::Result<acp::ReleaseTerminalResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn wait_for_terminal_exit(
        &self,
        _: acp::WaitForTerminalExitRequest,
    ) -> acp::Result<acp::WaitForTerminalExitResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn kill_terminal(&self, _: acp::KillTerminalRequest) -> acp::Result<acp::KillTerminalResponse> {
        Err(acp::Error::method_not_found())
    }

    async fn ext_method(&self, args: acp::ExtRequest) -> acp::Result<acp::ExtResponse> {
        crate::auth::kas_token::handle_ext_method(args).await
    }

    async fn ext_notification(&self, _: acp::ExtNotification) -> acp::Result<()> {
        Ok(())
    }
}

/// Test-only mocks for [`KasSessionClient`]. Only intended for use from
/// downstream crates' tests. Compiled unconditionally because cross-crate
/// `#[cfg(test)]` items aren't visible.
pub mod test {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::path::Path;

    use eyre::Result;

    use super::{
        KasSessionClient,
        SessionInfoEntry,
    };

    /// In-memory stub of [`KasSessionClient`]. Stubbed results are consumed
    /// in FIFO order, one per call; a call with no queued result panics.
    #[derive(Default)]
    pub struct KasMockSessionClient {
        list_results: RefCell<VecDeque<Result<Vec<SessionInfoEntry>>>>,
        delete_results: RefCell<VecDeque<Result<bool>>>,
    }

    impl KasMockSessionClient {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn with_list(self, entries: Vec<SessionInfoEntry>) -> Self {
            self.list_results.borrow_mut().push_back(Ok(entries));
            self
        }

        pub fn with_list_err(self, message: impl Into<String>) -> Self {
            let msg = message.into();
            self.list_results.borrow_mut().push_back(Err(eyre::eyre!(msg)));
            self
        }

        pub fn with_delete_result(self, deleted: bool) -> Self {
            self.delete_results.borrow_mut().push_back(Ok(deleted));
            self
        }

        pub fn with_delete_ok(self) -> Self {
            self.with_delete_result(true)
        }

        pub fn with_delete_err(self, message: impl Into<String>) -> Self {
            let msg = message.into();
            self.delete_results.borrow_mut().push_back(Err(eyre::eyre!(msg)));
            self
        }
    }

    #[async_trait::async_trait(?Send)]
    impl KasSessionClient for KasMockSessionClient {
        async fn list_sessions(&self, _cwd: &Path) -> Result<Vec<SessionInfoEntry>> {
            match self.list_results.borrow_mut().pop_front() {
                Some(Ok(v)) => Ok(v),
                Some(Err(e)) => Err(eyre::eyre!("{e:#}")),
                None => panic!("KasMockSessionClient: list_sessions called but not stubbed"),
            }
        }

        async fn delete_session(&self, _id: &str, _session_source: Option<&str>) -> Result<bool> {
            match self.delete_results.borrow_mut().pop_front() {
                Some(Ok(deleted)) => Ok(deleted),
                Some(Err(e)) => Err(eyre::eyre!("{e:#}")),
                None => panic!("KasMockSessionClient: delete_session called but not stubbed"),
            }
        }
    }

    #[test]
    fn delete_response_preserves_backend_rejection() {
        let response: super::DeleteSessionResponse =
            serde_json::from_value(serde_json::json!({ "success": false })).unwrap();
        assert!(!response.success);
    }

    /// The top-level `sessionSource` param is sent only when routing a remote
    /// delete; a local/default delete omits it (byte-identical to the prior wire).
    #[test]
    fn delete_request_serializes_session_source_only_when_present() {
        use super::DeleteSessionRequest;
        let remote = DeleteSessionRequest {
            session_id: "spc-9f2",
            session_source: Some("remote"),
        };
        assert_eq!(
            serde_json::to_value(&remote).unwrap(),
            serde_json::json!({ "sessionId": "spc-9f2", "sessionSource": "remote" })
        );

        let local = DeleteSessionRequest {
            session_id: "abc123",
            session_source: None,
        };
        assert_eq!(
            serde_json::to_value(&local).unwrap(),
            serde_json::json!({ "sessionId": "abc123" })
        );
    }
}
