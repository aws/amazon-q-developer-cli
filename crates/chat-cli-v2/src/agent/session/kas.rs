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
    /// KAS does not currently report whether the session actually existed,
    /// so the success case is `Result<()>` rather than `Result<bool>`.
    async fn delete_session(&self, session_id: &str, session_source: Option<&str>) -> Result<()>;
}

/// ACP-backed [`KasSessionClient`]. Owns a connected KAS child and issues
/// native ACP `session/list` for listing plus the `_kiro/session/delete`
/// ext_method for deletion.
pub struct KasAcpSessionClient {
    conn: acp::ClientSideConnection,
    _child: Child,
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
        match tokio::time::timeout(std::time::Duration::from_secs(15), init_fut).await {
            Ok(Ok(_)) => {},
            Ok(Err(e)) => return Err(e).wrap_err("ACP initialize handshake failed"),
            Err(_) => {
                return Err(eyre::eyre!("ACP initialize timed out after 15s"));
            },
        }

        debug!("KAS session client connected");

        Ok(Self { conn, _child: child })
    }
}

#[async_trait::async_trait(?Send)]
impl KasSessionClient for KasAcpSessionClient {
    async fn list_sessions(&self, cwd: &Path) -> Result<Vec<SessionInfoEntry>> {
        debug!(cwd = %cwd.display(), "listing KAS sessions via native session/list");
        let resp = self
            .conn
            .list_sessions(acp::ListSessionsRequest::new().cwd(Some(cwd.to_path_buf())))
            .await
            .wrap_err("failed to list KAS sessions")?;
        let entries: Vec<SessionInfoEntry> = resp
            .sessions
            .into_iter()
            .map(|info| SessionInfoEntry {
                session_id: info.session_id.0.to_string(),
                cwd: info.cwd,
                title: info.title,
                updated_at: info.updated_at,
                message_count: None,
            })
            .collect();
        debug!(count = entries.len(), "received KAS sessions");
        Ok(entries)
    }

    async fn delete_session(&self, session_id: &str, session_source: Option<&str>) -> Result<()> {
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
        let DeleteSessionResponse { success: _ } =
            serde_json::from_str(resp.0.get()).wrap_err("failed to parse KAS _kiro/session/delete response")?;
        debug!(%session_id, "KAS session delete dispatched");
        Ok(())
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
    use std::path::Path;

    use eyre::Result;

    use super::{
        KasSessionClient,
        SessionInfoEntry,
    };

    /// In-memory stub of [`KasSessionClient`]. Each stubbed result is
    /// consumed on first call; a second unstubbed call panics.
    #[derive(Default)]
    pub struct KasMockSessionClient {
        list_result: RefCell<Option<Result<Vec<SessionInfoEntry>>>>,
        delete_result: RefCell<Option<Result<()>>>,
    }

    impl KasMockSessionClient {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn with_list(self, entries: Vec<SessionInfoEntry>) -> Self {
            *self.list_result.borrow_mut() = Some(Ok(entries));
            self
        }

        pub fn with_list_err(self, message: impl Into<String>) -> Self {
            let msg = message.into();
            *self.list_result.borrow_mut() = Some(Err(eyre::eyre!(msg)));
            self
        }

        pub fn with_delete_ok(self) -> Self {
            *self.delete_result.borrow_mut() = Some(Ok(()));
            self
        }

        pub fn with_delete_err(self, message: impl Into<String>) -> Self {
            let msg = message.into();
            *self.delete_result.borrow_mut() = Some(Err(eyre::eyre!(msg)));
            self
        }
    }

    #[async_trait::async_trait(?Send)]
    impl KasSessionClient for KasMockSessionClient {
        async fn list_sessions(&self, _cwd: &Path) -> Result<Vec<SessionInfoEntry>> {
            match self.list_result.borrow_mut().take() {
                Some(Ok(v)) => Ok(v),
                Some(Err(e)) => Err(eyre::eyre!("{e:#}")),
                None => panic!("KasMockSessionClient: list_sessions called but not stubbed"),
            }
        }

        async fn delete_session(&self, _id: &str, _session_source: Option<&str>) -> Result<()> {
            match self.delete_result.borrow_mut().take() {
                Some(Ok(())) => Ok(()),
                Some(Err(e)) => Err(eyre::eyre!("{e:#}")),
                None => panic!("KasMockSessionClient: delete_session called but not stubbed"),
            }
        }
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
