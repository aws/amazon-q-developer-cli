//! Hidden internal subcommands under `chat _ ...`.
//!
//! Test/IPC surface, not user-facing. Output contract: a single JSON
//! line on stdout shaped by [`CliInternalOutput`], exit code mirrors
//! the success flag.

use std::path::{
    Path,
    PathBuf,
};
use std::process::ExitCode;
use std::sync::Arc;

use chat_cli_v2::agent::kas::v2_to_kas::{
    ConvertArgs as V2ConvertArgs,
    convert_v2_to_kas,
};
use chat_cli_v2::agent::kas::{
    ExportSessionOptions,
    ImportSessionOptions,
    ImportTarget,
    SessionArchiveError,
    WriteKasSessionOptions,
    default_kas_sessions_root,
    export_session,
    import_session,
    write_kas_session_dir,
};
use chat_cli_v2::agent::session::SessionDb;
use chat_cli_v2::auth::kas_token::{
    AcpCallbackToken,
    KasAuthMethod,
    KasProvider,
    resolve_kas_token_for_callback,
};
use chat_cli_v2::database::Database;
use clap::{
    Args,
    Subcommand,
    ValueEnum,
};
use eyre::Result;
use serde::{
    Deserialize,
    Serialize,
};
use typeshare::typeshare;

// ─── Wire types (typeshare-shared with the TUI) ──────────────────────

/// Stable error classifier. SCREAMING_SNAKE_CASE on the wire.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, strum::EnumString, strum::Display)]
#[typeshare]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// Source session id was not found in any searched store.
    SessionNotFound,
}

/// Auth method advertised to KAS in `_kiro/auth/getAccessToken`. KAS maps
/// each value to a `TokenType` request header. Auth types that need no
/// header (Builder ID / IdC / Social) omit this field. Mirrors
/// `chat_cli_v2::auth::kas_token::KasAuthMethod` so the typeshare-shared
/// surface stays self-contained; convert via `From`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[typeshare]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    ExternalIdp,
}

impl From<KasAuthMethod> for AuthMethod {
    fn from(v: KasAuthMethod) -> Self {
        match v {
            KasAuthMethod::ExternalIdp => Self::ExternalIdp,
        }
    }
}

/// Sign-in provider advertised to KAS in `_kiro/auth/getAccessToken`. KAS's
/// `GovernanceService` treats only `Enterprise` / `ExternalIdp` as
/// enterprise-managed (others skip the GetProfile call). Mirrors
/// `chat_cli_v2::auth::kas_token::KasProvider`; convert via `From`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[typeshare]
pub enum Provider {
    #[serde(rename = "Enterprise")]
    Enterprise,
    #[serde(rename = "ExternalIdp")]
    ExternalIdp,
    #[serde(rename = "BuilderId")]
    BuilderId,
    #[serde(rename = "Google")]
    Google,
    #[serde(rename = "Github")]
    Github,
}

impl From<KasProvider> for Provider {
    fn from(v: KasProvider) -> Self {
        match v {
            KasProvider::Enterprise => Self::Enterprise,
            KasProvider::ExternalIdp => Self::ExternalIdp,
            KasProvider::BuilderId => Self::BuilderId,
            KasProvider::Google => Self::Google,
            KasProvider::Github => Self::Github,
        }
    }
}

/// Single JSON line emitted by every `chat _` subcommand. Wire shape:
/// `{ "kind": "...", "data": {...} }`. Variant names match
/// [`InternalChatSubcommand`] 1:1, plus an `Error` variant for failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[typeshare]
#[serde(tag = "kind", content = "data", rename_all = "camelCase")]
pub enum CliInternalOutput {
    /// `export-session`.
    ExportSession { path: String },
    /// `import-session`.
    ImportSession { path: String },
    /// `get-kas-token`.
    #[serde(rename_all = "camelCase")]
    GetKasToken {
        access_token: String,
        expires_at: String,
        profile_arn: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        auth_method: Option<AuthMethod>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider: Option<Provider>,
    },
    /// `ensure-session`.
    #[serde(rename_all = "camelCase")]
    EnsureSession { session_id: String },
    /// `test-seed-v1`.
    #[serde(rename_all = "camelCase")]
    TestSeedV1 { conversation_id: String },
    /// Any subcommand's failure path.
    Error {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<ErrorCode>,
    },
}

impl CliInternalOutput {
    fn export_session(path: impl Into<String>) -> Self {
        Self::ExportSession { path: path.into() }
    }

    fn import_session(path: impl Into<String>) -> Self {
        Self::ImportSession { path: path.into() }
    }

    fn get_kas_token(token: &AcpCallbackToken) -> Self {
        Self::GetKasToken {
            access_token: token.access_token.clone(),
            expires_at: token.expires_at.clone(),
            profile_arn: token.profile_arn.clone(),
            auth_method: token.auth_method.map(AuthMethod::from),
            provider: token.provider.map(Provider::from),
        }
    }

    fn ensure_session(session_id: impl Into<String>) -> Self {
        Self::EnsureSession {
            session_id: session_id.into(),
        }
    }

    fn test_seed_v1(conversation_id: impl Into<String>) -> Self {
        Self::TestSeedV1 {
            conversation_id: conversation_id.into(),
        }
    }

    fn error(message: impl Into<String>, code: Option<ErrorCode>) -> Self {
        Self::Error {
            message: message.into(),
            code,
        }
    }

    fn exit_code(&self) -> ExitCode {
        match self {
            Self::Error { .. } => ExitCode::FAILURE,
            _ => ExitCode::SUCCESS,
        }
    }
}

/// Print `output` as a single JSON line on stdout and return the
/// matching `ExitCode`. Hand-rolls a fallback line if the envelope
/// itself fails to serialize so the wire contract is never violated.
fn emit(output: &CliInternalOutput) -> ExitCode {
    match serde_json::to_string(output) {
        Ok(line) => println!("{line}"),
        Err(e) => {
            let escaped = format!("internal: failed to serialize output: {e}")
                .replace('\\', "\\\\")
                .replace('"', "\\\"");
            println!("{{\"kind\":\"error\",\"data\":{{\"message\":\"{escaped}\"}}}}");
            return ExitCode::FAILURE;
        },
    }
    output.exit_code()
}

// ─── Top-level dispatcher ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum ChatCommand {
    /// Internal subcommands. Surface for tests and TUI IPC. May break
    /// without notice. Not for direct end-user use.
    #[command(name = "_", subcommand, hide = true)]
    Internal(InternalChatSubcommand),
}

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum InternalChatSubcommand {
    /// Export a session to a portable zip archive.
    ExportSession(ExportSessionArgs),
    /// Import a session from a portable zip archive.
    ImportSession(ImportSessionArgs),
    /// Resolve and refresh-if-expired the active OIDC access token for
    /// KAS. Wire surface for the `_kiro/auth/getAccessToken` ACP
    /// extension.
    GetKasToken(GetKasTokenArgs),
    /// Convert a session between V2 and KAS formats if needed, and
    /// ensure it is loadable in the target engine.
    EnsureSession(EnsureSessionArgs),
    /// Derive the V2 model-context messages from a V2 session log.
    /// Used as the "expected" side in cross-engine converter tests.
    DeriveMessages(DeriveMessagesArgs),
    /// Seed a V1 conversation row into a sandbox SQLite from a fixture
    /// JSON file. Refuses to run unless `KIRO_TEST_DB_PATH` is set.
    TestSeedV1(TestSeedV1Args),
}

impl ChatCommand {
    pub async fn execute(self) -> Result<ExitCode> {
        match self {
            Self::Internal(InternalChatSubcommand::ExportSession(args)) => Ok(args.execute()),
            Self::Internal(InternalChatSubcommand::ImportSession(args)) => Ok(args.execute().await),
            Self::Internal(InternalChatSubcommand::GetKasToken(args)) => Ok(args.execute().await),
            Self::Internal(InternalChatSubcommand::EnsureSession(args)) => Ok(args.execute().await),
            Self::Internal(InternalChatSubcommand::DeriveMessages(args)) => Ok(args.execute()),
            Self::Internal(InternalChatSubcommand::TestSeedV1(args)) => Ok(args.execute().await),
        }
    }
}

// ─── export-session ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct ExportSessionArgs {
    /// Session id to export.
    #[arg(long)]
    pub id: String,
    /// Workspace path the session is bucketed under.
    #[arg(long)]
    pub cwd: String,
    /// Output zip path. `.zip` is appended if no extension is present.
    #[arg(long)]
    pub out: PathBuf,
    /// Sessions root. Defaults to `KIRO_HOME/.kiro/sessions`.
    #[arg(long)]
    pub base_path: Option<PathBuf>,
    /// Overwrite an existing output file.
    #[arg(long)]
    pub force: bool,
}

impl ExportSessionArgs {
    fn execute(self) -> ExitCode {
        match self.run() {
            Ok(path) => emit(&CliInternalOutput::export_session(path.to_string_lossy())),
            Err(err) => emit(&CliInternalOutput::error(err.to_string(), None)),
        }
    }

    fn run(self) -> Result<PathBuf, SessionArchiveError> {
        let kas_sessions_root = match self.base_path {
            Some(p) => p,
            None => default_kas_sessions_root()?,
        };
        export_session(ExportSessionOptions {
            kas_sessions_root,
            session_id: self.id,
            workspace_paths: vec![self.cwd],
            out_path: self.out,
            force: self.force,
        })
    }
}

// ─── import-session ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct ImportSessionArgs {
    /// Archive path.
    #[arg(long)]
    pub archive: PathBuf,
    /// Workspace path the imported session will be bucketed under.
    #[arg(long)]
    pub cwd: String,
    /// Sessions root. Defaults to `KIRO_HOME/.kiro/sessions`.
    #[arg(long)]
    pub base_path: Option<PathBuf>,
}

impl ImportSessionArgs {
    async fn execute(self) -> ExitCode {
        match self.run().await {
            Ok(path) => emit(&CliInternalOutput::import_session(path.to_string_lossy())),
            Err(err) => emit(&CliInternalOutput::error(err.to_string(), None)),
        }
    }

    async fn run(self) -> Result<PathBuf, SessionArchiveError> {
        let kas_sessions_root = match self.base_path {
            Some(p) => p,
            None => default_kas_sessions_root()?,
        };
        let v2_sessions_dir = chat_cli_v2::util::paths::sessions_dir()
            .map_err(|e| SessionArchiveError::msg(format!("failed to resolve V2 sessions dir: {e}")))?;

        let db = crate::database::Database::new_default().await.map_err(|e| {
            SessionArchiveError::io(format!("failed to open V1 database: {e}"), std::io::Error::other(e))
        })?;
        let exporter: Arc<dyn chat_cli_v2::agent::session::legacy_compat::LegacySessionExporter> = Arc::new(
            crate::cli::chat::v1_export::LegacySessionExporterImpl::new(Arc::new(db)),
        );

        let result = import_session(
            ImportSessionOptions {
                kas_sessions_root,
                v2_sessions_dir,
                archive_path: self.archive,
                workspace_paths: vec![self.cwd],
            },
            ImportTarget::Kas,
            &exporter,
        )?;
        Ok(result.path)
    }
}

// ─── get-kas-token ───────────────────────────────────────────────────

/// Mirrors the `_kiro/auth/getAccessToken` ACP request shape so a thin
/// host handler can pass agent-supplied fields through unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct GetKasTokenArgs {
    /// Why KAS asked for a fresh token (informational; logged only).
    #[arg(long)]
    pub reason: Option<String>,
    /// ISO-8601 expiry of KAS's currently-cached token (informational).
    #[arg(long)]
    pub current_expires_at: Option<String>,
}

impl GetKasTokenArgs {
    async fn execute(self) -> ExitCode {
        let _ = self;
        let database = match Database::new().await {
            Ok(db) => db,
            Err(err) => {
                return emit(&CliInternalOutput::error(
                    format!("failed to open auth store: {err}"),
                    None,
                ));
            },
        };
        match resolve_kas_token_for_callback(&database).await {
            Ok(Some(token)) => emit(&CliInternalOutput::get_kas_token(&token)),
            Ok(None) => emit(&CliInternalOutput::error(
                "You are not logged in. Please log in with `kiro-cli login`.",
                None,
            )),
            Err(err) => emit(&CliInternalOutput::error(format!("auth refresh failed: {err}"), None)),
        }
    }
}

// ─── ensure-session ──────────────────────────────────────────────────

/// `auto` probes each non-target store for the id; explicit variants skip the lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SourceFormat {
    Auto,
    Classic,
    V2,
    Kas,
}

/// The active engine the host is launching. V1 is never a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum TargetFormat {
    V2,
    Kas,
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct EnsureSessionArgs {
    #[arg(long, value_enum)]
    pub source_format: SourceFormat,
    /// V2 uses a UUID; KAS uses the `sess_<uuid>` form.
    #[arg(long)]
    pub source_session_id: String,
    #[arg(long, value_enum)]
    pub target_format: TargetFormat,
    /// Workspace path the converted session will be bucketed under.
    #[arg(long)]
    pub cwd: String,
}

struct RunError {
    message: String,
    code: Option<ErrorCode>,
}

impl RunError {
    fn message(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: None,
        }
    }

    fn not_found(session_id: &str) -> Self {
        Self {
            message: format!("session not found: {session_id}"),
            code: Some(ErrorCode::SessionNotFound),
        }
    }
}

impl EnsureSessionArgs {
    async fn execute(self) -> ExitCode {
        match self.run().await {
            Ok(target_id) => emit(&CliInternalOutput::ensure_session(target_id)),
            Err(err) => emit(&CliInternalOutput::error(err.message, err.code)),
        }
    }

    async fn run(self) -> Result<String, RunError> {
        match (self.source_format, self.target_format) {
            (SourceFormat::Kas, TargetFormat::Kas) => Ok(self.source_session_id.clone()),
            (SourceFormat::Kas, TargetFormat::V2) => Err(RunError::message(
                "ensure-session: KAS source -> V2 target not supported",
            )),

            (SourceFormat::V2, TargetFormat::Kas) => self.run_v2_to_kas(),
            (SourceFormat::V2, TargetFormat::V2) => {
                let v2_sessions_dir = chat_cli_v2::util::paths::sessions_dir()
                    .map_err(|e| RunError::message(format!("failed to resolve V2 sessions dir: {e}")))?;
                self.ensure_v2_present(&v2_sessions_dir)
            },

            (SourceFormat::Classic, TargetFormat::Kas) => self.run_v1_to_kas().await,
            (SourceFormat::Classic, TargetFormat::V2) => self.run_v1_to_v2().await,

            (SourceFormat::Auto, target) => self.run_auto(target).await,
        }
    }

    async fn run_auto(&self, target: TargetFormat) -> Result<String, RunError> {
        let v2_sessions_dir = chat_cli_v2::util::paths::sessions_dir()
            .map_err(|e| RunError::message(format!("failed to resolve V2 sessions dir: {e}")))?;

        if v2_session_exists(&v2_sessions_dir, &self.source_session_id) {
            return match target {
                TargetFormat::Kas => self.run_v2_to_kas(),
                TargetFormat::V2 => self.ensure_v2_present(&v2_sessions_dir),
            };
        }

        if v1_conversation_exists(&self.source_session_id).await? {
            return match target {
                TargetFormat::Kas => self.run_v1_to_kas().await,
                TargetFormat::V2 => self.run_v1_to_v2().await,
            };
        }

        if kas_session_exists(&self.cwd, &self.source_session_id)? {
            return match target {
                TargetFormat::Kas => Ok(self.source_session_id.clone()),
                TargetFormat::V2 => Err(RunError::message(
                    "ensure-session: KAS source -> V2 target not supported",
                )),
            };
        }

        Err(RunError::not_found(&self.source_session_id))
    }

    fn ensure_v2_present(&self, v2_sessions_dir: &Path) -> Result<String, RunError> {
        if chat_cli_v2::agent::session::metadata_path(v2_sessions_dir, &self.source_session_id).exists() {
            Ok(self.source_session_id.clone())
        } else {
            Err(RunError::not_found(&self.source_session_id))
        }
    }

    fn run_v2_to_kas(&self) -> Result<String, RunError> {
        let v2_sessions_dir = chat_cli_v2::util::paths::sessions_dir()
            .map_err(|e| RunError::message(format!("failed to resolve V2 sessions dir: {e}")))?;
        self.do_v2_to_kas(&v2_sessions_dir)
    }

    fn do_v2_to_kas(&self, v2_sessions_dir: &Path) -> Result<String, RunError> {
        let kas_root = default_kas_sessions_root()
            .map_err(|e| RunError::message(format!("failed to resolve KAS sessions root: {e}")))?;

        let db =
            SessionDb::load_with_sessions_dir(v2_sessions_dir, &self.source_session_id, None).map_err(|e| match e {
                chat_cli_v2::agent::session::SessionError::NotFound(_) => RunError::not_found(&self.source_session_id),
                other => RunError::message(format!("failed to load V2 session {}: {other}", self.source_session_id)),
            })?;
        let session_data = db.session();
        let workspace_paths = vec![self.cwd.clone()];

        let entries = db
            .load_log_entries()
            .map_err(|e| RunError::message(format!("failed to load V2 log entries: {e}")))?;

        let output = convert_v2_to_kas(V2ConvertArgs {
            session: session_data,
            entries,
            workspace_paths: workspace_paths.clone(),
        })
        .map_err(|e| RunError::message(format!("V2 -> KAS conversion failed: {e}")))?;

        let target_id = output.metadata.id.clone();
        write_kas_session_dir(WriteKasSessionOptions {
            kas_sessions_root: kas_root,
            workspace_paths,
            session_id: target_id.clone(),
            metadata: output.metadata,
            messages: output.messages,
        })
        .map_err(|e| RunError::message(format!("failed to write KAS session: {e}")))?;
        Ok(target_id)
    }

    async fn run_v1_to_v2(&self) -> Result<String, RunError> {
        let v2_sessions_dir = chat_cli_v2::util::paths::sessions_dir()
            .map_err(|e| RunError::message(format!("failed to resolve V2 sessions dir: {e}")))?;
        self.do_v1_to_v2(&v2_sessions_dir).await?;
        Ok(self.source_session_id.clone())
    }

    async fn do_v1_to_v2(&self, dest_dir: &Path) -> Result<(), RunError> {
        use chat_cli_v2::agent::session::legacy_compat::{
            LegacyExportError,
            LegacySessionExporter,
        };

        let db = crate::database::Database::new_default()
            .await
            .map_err(|e| RunError::message(format!("failed to open V1 database: {e}")))?;
        let exporter = crate::cli::chat::v1_export::LegacySessionExporterImpl::new(Arc::new(db));
        match exporter.export_session(&self.source_session_id, dest_dir) {
            Ok(()) => Ok(()),
            Err(LegacyExportError::NotFound { .. }) => Err(RunError::not_found(&self.source_session_id)),
            Err(e) => Err(RunError::message(format!("V1 -> V2 export failed: {e}"))),
        }
    }

    /// V1 -> V2 -> KAS via a tempdir for the V2 byproduct so it never
    /// surfaces in the user's V2 listing.
    async fn run_v1_to_kas(&self) -> Result<String, RunError> {
        let temp = tempfile::tempdir().map_err(|e| RunError::message(format!("failed to create temp V2 dir: {e}")))?;
        self.do_v1_to_v2(temp.path()).await?;
        self.do_v2_to_kas(temp.path())
    }
}

fn v2_session_exists(sessions_dir: &Path, session_id: &str) -> bool {
    chat_cli_v2::agent::session::metadata_path(sessions_dir, session_id).exists()
}

async fn v1_conversation_exists(conversation_id: &str) -> Result<bool, RunError> {
    let db = crate::database::Database::new_default()
        .await
        .map_err(|e| RunError::message(format!("failed to open V1 database: {e}")))?;
    let entry = db
        .get_conversation_by_id_with_cwd(conversation_id)
        .map_err(|e| RunError::message(format!("failed to query V1 database: {e}")))?;
    Ok(entry.is_some())
}

fn kas_session_exists(cwd: &str, session_id: &str) -> Result<bool, RunError> {
    let kas_root = default_kas_sessions_root()
        .map_err(|e| RunError::message(format!("failed to resolve KAS sessions root: {e}")))?;
    let bucket = chat_cli_v2::agent::kas::workspace_hash::compute_workspace_hash(&[cwd.to_string()]);
    let session_dir = kas_root.join(bucket).join(session_id);
    Ok(session_dir.join("session.json").exists())
}

// ─── derive-messages ─────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct DeriveMessagesArgs {
    /// Sessions root (V2 layout). Defaults to V2's standard location.
    #[arg(long)]
    pub sessions_dir: Option<PathBuf>,
    /// Session id (V2 UUID).
    #[arg(long)]
    pub session_id: String,
}

impl DeriveMessagesArgs {
    fn execute(self) -> ExitCode {
        let _ = self;
        emit(&CliInternalOutput::error("not implemented", None))
    }
}

// ─── test-seed-v1 ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct TestSeedV1Args {
    #[arg(long)]
    pub fixture_path: PathBuf,
    /// Path key for `conversations_v2.key`. Tests typically pass the
    /// TUI launch cwd so the V1 lookup and the converter's
    /// workspace-hash agree.
    #[arg(long)]
    pub cwd: String,
}

impl TestSeedV1Args {
    async fn execute(self) -> ExitCode {
        match self.run().await {
            Ok(conversation_id) => emit(&CliInternalOutput::test_seed_v1(conversation_id)),
            Err(err) => emit(&CliInternalOutput::error(err, None)),
        }
    }

    async fn run(self) -> Result<String, String> {
        if std::env::var(crate::util::consts::env_var::KIRO_TEST_DB_PATH).is_err() {
            return Err(format!(
                "{} must be set; refusing to seed the production database",
                crate::util::consts::env_var::KIRO_TEST_DB_PATH
            ));
        }

        let raw = std::fs::read_to_string(&self.fixture_path).map_err(|e| format!("read fixture: {e}"))?;
        let state: crate::cli::chat::conversation::ConversationState =
            serde_json::from_str(&raw).map_err(|e| format!("parse fixture JSON: {e}"))?;
        let conversation_id = state.conversation_id().to_string();

        let mut db = crate::database::Database::new_default()
            .await
            .map_err(|e| format!("open database: {e}"))?;
        db.set_conversation_by_path(&self.cwd, &state)
            .map_err(|e| format!("insert conversation: {e}"))?;
        Ok(conversation_id)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    // ─── CliInternalOutput / ErrorCode wire format ───────────────────

    #[test]
    fn export_session_serializes_with_kind_and_path() {
        let json_str = serde_json::to_string(&CliInternalOutput::export_session("/tmp/a.zip")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed, json!({"kind": "exportSession", "data": {"path": "/tmp/a.zip"}}));
    }

    #[test]
    fn import_session_serializes_with_kind_and_path() {
        let json_str = serde_json::to_string(&CliInternalOutput::import_session("/tmp/sess")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed, json!({"kind": "importSession", "data": {"path": "/tmp/sess"}}));
    }

    #[test]
    fn ensure_session_serializes_with_camel_case_session_id() {
        let json_str = serde_json::to_string(&CliInternalOutput::ensure_session("sess_abc")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(
            parsed,
            json!({"kind": "ensureSession", "data": {"sessionId": "sess_abc"}})
        );
    }

    #[test]
    fn test_seed_v1_serializes_with_camel_case_conversation_id() {
        let json_str = serde_json::to_string(&CliInternalOutput::test_seed_v1("conv-1")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(
            parsed,
            json!({"kind": "testSeedV1", "data": {"conversationId": "conv-1"}})
        );
    }

    #[test]
    fn get_kas_token_serializes_three_fields_in_camel_case() {
        let token = AcpCallbackToken {
            access_token: "at".into(),
            expires_at: "2099-01-01T00:00:00Z".into(),
            profile_arn: "arn:aws:codewhisperer:us-east-1:1:profile/x".into(),
            auth_method: None,
            provider: None,
        };
        let json_str = serde_json::to_string(&CliInternalOutput::get_kas_token(&token)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(
            parsed,
            json!({
                "kind": "getKasToken",
                "data": {
                    "accessToken": "at",
                    "expiresAt": "2099-01-01T00:00:00Z",
                    "profileArn": "arn:aws:codewhisperer:us-east-1:1:profile/x",
                }
            })
        );
    }

    #[test]
    fn get_kas_token_serializes_auth_method_in_snake_case_when_set() {
        let token = AcpCallbackToken {
            access_token: "at".into(),
            expires_at: "2099-01-01T00:00:00Z".into(),
            profile_arn: "arn:aws:codewhisperer:us-east-1:1:profile/x".into(),
            auth_method: Some(KasAuthMethod::ExternalIdp),
            provider: Some(KasProvider::ExternalIdp),
        };
        let json_str = serde_json::to_string(&CliInternalOutput::get_kas_token(&token)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(
            parsed["data"]["authMethod"],
            serde_json::Value::String("external_idp".into())
        );
        assert_eq!(
            parsed["data"]["provider"],
            serde_json::Value::String("ExternalIdp".into())
        );
    }

    #[test]
    fn error_without_code_omits_code_field() {
        let json_str = serde_json::to_string(&CliInternalOutput::error("oops", None)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed, json!({"kind": "error", "data": {"message": "oops"}}));
    }

    #[test]
    fn error_with_code_serializes_code_in_screaming_snake_case() {
        let json_str =
            serde_json::to_string(&CliInternalOutput::error("oops", Some(ErrorCode::SessionNotFound))).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(
            parsed,
            json!({"kind": "error", "data": {"message": "oops", "code": "SESSION_NOT_FOUND"}})
        );
    }

    #[test]
    fn success_round_trips_via_serde() {
        let original = CliInternalOutput::ensure_session("sess_xyz");
        let json_str = serde_json::to_string(&original).unwrap();
        let parsed: CliInternalOutput = serde_json::from_str(&json_str).unwrap();
        match parsed {
            CliInternalOutput::EnsureSession { session_id } => assert_eq!(session_id, "sess_xyz"),
            other => panic!("expected EnsureSession, got {other:?}"),
        }
    }

    #[test]
    fn error_round_trips_via_serde() {
        let original = CliInternalOutput::error("oops", Some(ErrorCode::SessionNotFound));
        let json_str = serde_json::to_string(&original).unwrap();
        let parsed: CliInternalOutput = serde_json::from_str(&json_str).unwrap();
        match parsed {
            CliInternalOutput::Error { message, code } => {
                assert_eq!(message, "oops");
                assert_eq!(code, Some(ErrorCode::SessionNotFound));
            },
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn error_code_session_not_found_round_trips_from_str() {
        let parsed: ErrorCode = "SESSION_NOT_FOUND".parse().unwrap();
        assert_eq!(parsed, ErrorCode::SessionNotFound);
    }

    #[test]
    fn error_code_session_not_found_display_is_screaming_snake_case() {
        assert_eq!(ErrorCode::SessionNotFound.to_string(), "SESSION_NOT_FOUND");
    }

    #[test]
    fn exit_code_is_failure_for_error_variant() {
        let out = CliInternalOutput::error("oops", None);
        assert!(matches!(out.exit_code(), e if format!("{e:?}") == format!("{:?}", ExitCode::FAILURE)));
    }

    // ─── EnsureSessionArgs behavior ──────────────────────────────────

    #[tokio::test]
    async fn run_to_kas_with_explicit_kas_source_is_noop() {
        let args = EnsureSessionArgs {
            source_format: SourceFormat::Kas,
            source_session_id: "anything-the-caller-says".to_string(),
            target_format: TargetFormat::Kas,
            cwd: "/tmp".to_string(),
        };
        let result = args.run().await;
        assert_eq!(result.ok().as_deref(), Some("anything-the-caller-says"));
    }

    #[test]
    fn ensure_v2_present_returns_id_when_session_exists() {
        let dir = tempfile::tempdir().unwrap();
        let id = "8e247382-c779-440f-b1db-ed9bad1afaee";
        std::fs::write(chat_cli_v2::agent::session::metadata_path(dir.path(), id), "{}").unwrap();
        let args = EnsureSessionArgs {
            source_format: SourceFormat::V2,
            source_session_id: id.to_string(),
            target_format: TargetFormat::V2,
            cwd: "/tmp".to_string(),
        };
        let result = args.ensure_v2_present(dir.path());
        assert_eq!(result.ok().as_deref(), Some(id));
    }

    #[test]
    fn ensure_v2_present_reports_session_not_found_for_missing_session() {
        let dir = tempfile::tempdir().unwrap();
        let args = EnsureSessionArgs {
            source_format: SourceFormat::Auto,
            source_session_id: "does-not-exist".to_string(),
            target_format: TargetFormat::V2,
            cwd: "/tmp".to_string(),
        };
        let err = args
            .ensure_v2_present(dir.path())
            .err()
            .expect("missing session should error");
        assert_eq!(err.code, Some(ErrorCode::SessionNotFound));
    }
}
