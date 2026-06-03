//! Hidden internal subcommands under `chat _ ...`.
//!
//! Test-and-IPC surface, not user-facing. The slash-command handlers
//! `/chat save` and `/chat load` (in the TUI) shell out to these via
//! `child_process.spawnSync`, parse the JSON line on stdout, and
//! surface the result. The e2e test harness drives the same surface
//! directly. Args are explicit (no env-var reliance) so callers can
//! sandbox the operation with a dedicated `--base-path` and `--cwd`.
//!
//! Subcommands:
//! - `export-session` / `import-session` - portable zip archive of a KAS session. Used by `/chat
//!   save` and `/chat load`.
//! - `ensure-session` - cross-engine semantic conversion + idempotency probe. Converts a session
//!   from one engine's storage format to another (V2 -> KAS, etc.) when the source id is missing in
//!   the target, and no-ops when it already exists.
//! - `derive-messages` - exposes V2's `EventLog::derive_messages` so converter tests can use V2's
//!   own derived messages as the "expected" side when comparing against KAS replay output.
//!
//! Output contract:
//! - Single JSON line on stdout, exit code mirrors `success`.
//! - Success (exit 0): payload shape varies by subcommand. Common fields: `"success": true` plus
//!   subcommand-specific data.
//! - Failure (exit 1): `{"success": false, "error": "<message>"}`
//!
//! No human-readable mode. Errors come out as JSON too so the TUI
//! handler always parses with `JSON.parse(stdout)` and never has to
//! disambiguate stdout vs stderr text.

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
    resolve_kas_token_for_callback,
};
use chat_cli_v2::database::Database;
use clap::{
    Args,
    Subcommand,
    ValueEnum,
};
use eyre::Result;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum ChatCommand {
    /// Internal subcommands. Surface for tests and TUI IPC. May break
    /// without notice. Not for direct end-user use.
    #[command(name = "_", subcommand, hide = true)]
    Internal(InternalChatSubcommand),
}

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum InternalChatSubcommand {
    /// Export a session to a portable zip archive. JSON output.
    ExportSession(ExportSessionArgs),
    /// Import a session from a portable zip archive. JSON output.
    ImportSession(ImportSessionArgs),
    /// Resolve and refresh-if-expired the active OIDC access token for
    /// KAS. JSON output. Wire surface for the `_kiro/auth/getAccessToken`
    /// ACP extension that KAS calls in `--auth=acp-callback` mode.
    ///
    /// Output (success): `{"success": true, "accessToken": "...",
    /// "expiresAt": "...", "profileArn"?: "..."}`.
    GetKasToken(GetKasTokenArgs),
    /// Convert a session between V2 and KAS formats if needed, and
    /// ensure it is loadable in the target engine. JSON output.
    /// Same-engine source-target pairs are no-ops; cross-engine
    /// pairs always mint a new target-format session, even if the
    /// same source has been converted before. Source format `auto`
    /// searches all non-target formats for the id.
    EnsureSession(EnsureSessionArgs),
    /// Derive the V2 model-context messages from a V2 session log.
    /// JSON output. Used as the "expected" side in cross-engine
    /// converter tests.
    DeriveMessages(DeriveMessagesArgs),
    /// Seed a V1 (classic) conversation row into a sandbox SQLite
    /// from a fixture JSON file. Test infrastructure only - the
    /// command refuses to run unless `--db-path` is provided. Used
    /// by integ tests to pre-populate a V1 session before launching
    /// the TUI; reuses the rust-side `ConversationState` fixtures
    /// from `crates/chat-cli/src/cli/chat/v1_export/fixtures/` so
    /// SQLite schema knowledge stays in rust.
    TestSeedV1(TestSeedV1Args),
}

/// Source session format for [`EnsureSessionArgs`]. `auto` triggers a
/// targeted lookup of the id across all non-target formats - the
/// converter probes each engine's sessions root for an id match. The
/// remaining variants name a specific source engine and skip the
/// lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SourceFormat {
    /// Search all non-target formats for the id.
    Auto,
    /// Classic (V1) session format - SQLite-backed.
    Classic,
    /// V2 session format - filesystem-backed at `~/.kiro/sessions/cli`.
    V2,
    /// KAS session format - filesystem-backed at `~/.kiro/sessions/`,
    /// bucketed by workspace hash.
    Kas,
}

/// Target session format for [`EnsureSessionArgs`]. The active engine
/// the host is launching. KAS and V2 are valid targets; V1 is never
/// a target because cross-engine migration only goes forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum TargetFormat {
    /// V2 session format.
    V2,
    /// KAS session format.
    Kas,
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct ExportSessionArgs {
    /// Session id to export.
    #[arg(long)]
    pub id: String,
    /// Workspace path. Required.
    ///
    /// KAS partitions sessions on disk by `workspacePaths`. For the
    /// TUI, this is always just the current working directory. Used
    /// to avoid a full scan across all sessions on disk.
    #[arg(long)]
    pub cwd: String,
    /// Output zip path. `.zip` is appended if no extension is present.
    #[arg(long)]
    pub out: PathBuf,
    /// Sessions root. Defaults to KIRO_HOME/.kiro/sessions.
    #[arg(long)]
    pub base_path: Option<PathBuf>,
    /// Overwrite existing output file.
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct ImportSessionArgs {
    /// Archive path.
    #[arg(long)]
    pub archive: PathBuf,
    /// Workspace path. Required.
    ///
    /// Stores the session inside the computed `workspacePaths`
    /// directory. See [`ExportSessionArgs::cwd`].
    #[arg(long)]
    pub cwd: String,
    /// Sessions root. Defaults to KIRO_HOME/.kiro/sessions.
    #[arg(long)]
    pub base_path: Option<PathBuf>,
}

/// Args for `chat _ get-kas-token`. Mirrors the `_kiro/auth/getAccessToken`
/// ACP request shape so a thin host handler can pass agent-supplied fields
/// through unchanged. Both fields are informational; the resolver returns
/// the highest-priority cached token, refreshing under the cross-process
/// refresh lock if it has crossed expiry.
#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct GetKasTokenArgs {
    /// Why KAS asked for a fresh token. One of `expiring` (cached token is
    /// inside KAS's pre-expiry buffer) or `expired` (cached token is past
    /// its hard expiry). Currently logged only.
    #[arg(long)]
    pub reason: Option<String>,
    /// ISO-8601 expiry of KAS's currently-cached token, if any. Used by
    /// callers that want to short-circuit the resolver when their cache is
    /// fresher; the current implementation always re-reads the SQLite
    /// store.
    #[arg(long)]
    pub current_expires_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct EnsureSessionArgs {
    /// Source session format. `auto` triggers a lookup across all
    /// non-target formats. Explicit variants skip the lookup.
    ///
    /// Sandbox overrides for both source and target sessions roots
    /// come from `KIRO_HOME`. V2 reads from `<KIRO_HOME>/sessions/cli/`
    /// (or honors `KIRO_TEST_SESSIONS_DIR` as a direct override) and
    /// KAS reads/writes under `<KIRO_HOME>/sessions/`. The subcommand
    /// resolves both at the binary boundary rather than as flags so
    /// callers (TUI, e2e tests) don't have to forward them through.
    #[arg(long, value_enum)]
    pub source_format: SourceFormat,
    /// Source session id. For V2 this is a UUID; for KAS it is the
    /// `sess_<uuid>` form.
    #[arg(long)]
    pub source_session_id: String,
    /// Target session format. The active engine the host is launching.
    #[arg(long, value_enum)]
    pub target_format: TargetFormat,
    /// Workspace path. Required.
    ///
    /// KAS partitions sessions on disk by `workspacePaths`; the
    /// converter writes the resulting session into the directory whose
    /// hash derives from this path. For V2 targets this is recorded
    /// as the session's `cwd`.
    #[arg(long)]
    pub cwd: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct DeriveMessagesArgs {
    /// Sessions root (V2 layout). Defaults to V2's standard location
    /// (`KIRO_HOME/.kiro/sessions/cli`, overridable via
    /// `KIRO_TEST_SESSIONS_DIR`) when omitted.
    #[arg(long)]
    pub sessions_dir: Option<PathBuf>,
    /// Session id (V2 UUID).
    #[arg(long)]
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct TestSeedV1Args {
    /// Path to a `ConversationState` fixture JSON file. The fixture
    /// is parsed and inserted into `conversations_v2` verbatim.
    /// Tests typically point at one of the JSONs under
    /// `crates/chat-cli/src/cli/chat/v1_export/fixtures/`.
    #[arg(long)]
    pub fixture_path: PathBuf,
    /// Path key to bucket the conversation under in
    /// `conversations_v2.key`. Tests typically pass the same
    /// directory used as the TUI launch cwd so the converter's
    /// workspace-hash and V1's per-path lookup agree.
    #[arg(long)]
    pub cwd: String,
}

#[derive(Serialize)]
#[serde(untagged)]
enum JsonOutput<'a> {
    /// `{"success": true, "path": ...}` - exit 0.
    Success {
        success: bool,
        /// Absolute path the operation produced. Export: zip archive.
        /// Import: extracted session directory; the new session id is
        /// the basename.
        path: &'a str,
    },
    /// `{"success": true, "sessionId": ...}` - exit 0. Used by
    /// subcommands that resolve a target-engine session id rather
    /// than a path.
    SessionSuccess {
        success: bool,
        #[serde(rename = "sessionId")]
        session_id: &'a str,
    },
    /// `{"success": true, "conversationId": ...}` - exit 0. Used
    /// by `test-seed-v1` to return the seeded V1 conversation id
    /// so callers can drive subsequent steps without re-parsing
    /// the fixture JSON.
    ConversationSuccess {
        success: bool,
        #[serde(rename = "conversationId")]
        conversation_id: &'a str,
    },
    /// `{"success": false, "error": ...}` - exit 1.
    Error {
        success: bool,
        /// Human-readable error message. Exact text is not stable -
        /// callers should not pattern-match on it.
        error: &'a str,
        /// Stable machine-readable classifier. Present only for
        /// failures callers are expected to branch on (e.g.
        /// `"SOURCE_NOT_FOUND"`); absent for generic errors. Unlike
        /// `error`, this string IS stable and safe to match.
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<&'a str>,
    },
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

impl ExportSessionArgs {
    fn execute(self) -> ExitCode {
        match self.run() {
            Ok(path) => emit_success(&path),
            Err(err) => emit_error(&err.to_string()),
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

impl ImportSessionArgs {
    async fn execute(self) -> ExitCode {
        match self.run().await {
            Ok(path) => emit_success(&path),
            Err(err) => emit_error(&err.to_string()),
        }
    }

    async fn run(self) -> Result<PathBuf, SessionArchiveError> {
        let kas_sessions_root = match self.base_path {
            Some(p) => p,
            None => default_kas_sessions_root()?,
        };
        let v2_sessions_dir = chat_cli_v2::util::paths::sessions_dir()
            .map_err(|e| SessionArchiveError::msg(format!("failed to resolve V2 sessions dir: {e}")))?;

        // Construct the SQLite-backed V1 exporter so the importer can
        // route legacy-V1 archives through the V1 -> V2 -> KAS
        // composition. The exporter only touches SQLite for the
        // resume-id-by-id path; importing from a JSON file uses the
        // trait's `try_export_from_json` method which reads only the
        // file content.
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

impl GetKasTokenArgs {
    /// Resolve and refresh-if-expired. `_ = self` because both args are
    /// informational; the resolver does not key any decision off them.
    /// Surfaces three outcomes:
    ///   - Found token   -> success JSON line, exit 0
    ///   - Empty store   -> "not logged in" error, exit 1
    ///   - Lookup/refresh failed -> error JSON with the underlying message, exit 1
    async fn execute(self) -> ExitCode {
        let _ = self;
        let database = match Database::new().await {
            Ok(db) => db,
            Err(err) => return emit_error(&format!("failed to open auth store: {err}")),
        };
        match resolve_kas_token_for_callback(&database).await {
            Ok(Some(token)) => emit_token(&token),
            // Aligned with the user-facing message from `kiro-cli login` so
            // the TS-side handler can pass it straight through to KAS.
            Ok(None) => emit_error("You are not logged in. Please log in with `kiro-cli login`."),
            Err(err) => emit_error(&format!("auth refresh failed: {err}")),
        }
    }
}

/// Stable error code emitted when a requested source session does not
/// exist in any searched format. Callers branch on this to fall back
/// to a fresh session instead of surfacing a hard error.
const SESSION_NOT_FOUND: &str = "SESSION_NOT_FOUND";

/// A failure from [`EnsureSessionArgs::run`]. `code`, when set, is a
/// stable classifier callers may match on; `message` is human-readable
/// and not stable.
struct RunError {
    message: String,
    code: Option<&'static str>,
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
            code: Some(SESSION_NOT_FOUND),
        }
    }
}

impl EnsureSessionArgs {
    /// Resolve and convert a session for the target engine, writing
    /// it to the target's on-disk format if needed and emitting the
    /// target-native session id.
    ///
    /// Same-engine source-target pairs short-circuit: the source id
    /// is in the target format already, so the command emits it
    /// unchanged. Cross-engine pairs always produce a new
    /// target-format session - the source is read but never
    /// rewritten, and the new target id is independent of any prior
    /// conversion of the same source.
    async fn execute(self) -> ExitCode {
        match self.run().await {
            Ok(target_id) => emit_session_success(&target_id),
            Err(err) => emit_error_code(&err.message, err.code),
        }
    }

    async fn run(self) -> Result<String, RunError> {
        match (self.source_format, self.target_format) {
            // ---- KAS source ----
            // KAS lives in target format already; no conversion. KAS
            // ids are not loadable by V2 - that direction is not
            // supported (yet).
            (SourceFormat::Kas, TargetFormat::Kas) => Ok(self.source_session_id.clone()),
            (SourceFormat::Kas, TargetFormat::V2) => Err(RunError::message(
                "ensure-session: KAS source -> V2 target not supported",
            )),

            // ---- V2 source ----
            (SourceFormat::V2, TargetFormat::Kas) => self.run_v2_to_kas(),
            (SourceFormat::V2, TargetFormat::V2) => {
                let v2_sessions_dir = chat_cli_v2::util::paths::sessions_dir()
                    .map_err(|e| RunError::message(format!("failed to resolve V2 sessions dir: {e}")))?;
                self.ensure_v2_present(&v2_sessions_dir)
            },

            // ---- Classic (V1) source ----
            (SourceFormat::Classic, TargetFormat::Kas) => self.run_v1_to_kas().await,
            (SourceFormat::Classic, TargetFormat::V2) => self.run_v1_to_v2().await,

            // ---- Auto: probe each store directly ----
            // Probing order is deterministic (V2 -> V1 -> KAS) but
            // the result is the same regardless of probe order: at
            // most one store can hold an id of any given shape on
            // disk at once. Each probe is a stat or a single-row
            // SQLite lookup, all fast.
            (SourceFormat::Auto, target) => self.run_auto(target).await,
        }
    }

    /// Auto dispatch: probe each candidate store for the source id,
    /// then delegate to the explicit-format handler.
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

    /// Confirm a V2 session exists under `v2_sessions_dir` and return
    /// its id unchanged. Returns a [`SESSION_NOT_FOUND`] error when the
    /// id has no V2 session, letting the caller fall through to a fresh
    /// session.
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

    /// Convert the V2 session at `<v2_sessions_dir>/<source_session_id>.{json,jsonl}`
    /// into a KAS session under the caller's workspace bucket and
    /// return the minted KAS id. The V2 dir is parameterized so the
    /// V1 -> V2 -> KAS composition can target a tempdir for the V2
    /// byproduct rather than the user's real V2 store.
    fn do_v2_to_kas(&self, v2_sessions_dir: &Path) -> Result<String, RunError> {
        let kas_root = default_kas_sessions_root()
            .map_err(|e| RunError::message(format!("failed to resolve KAS sessions root: {e}")))?;

        let db =
            SessionDb::load_with_sessions_dir(v2_sessions_dir, &self.source_session_id, None).map_err(|e| match e {
                chat_cli_v2::agent::session::SessionError::NotFound(_) => RunError::not_found(&self.source_session_id),
                other => RunError::message(format!("failed to load V2 session {}: {other}", self.source_session_id)),
            })?;
        let session_data = db.session();

        // Workspace paths bucket the converted session under the
        // caller's runtime cwd. V2 stores sessions flat by id and
        // resume works from any directory; the converted KAS session
        // lands in the bucket of the cwd the caller passed, not the
        // bucket of the cwd recorded on the V2 session. The TUI
        // passes `process.cwd()` as `--cwd`.
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

    /// Export a V1 conversation to V2 format at the production V2
    /// sessions dir and return the V1 id (V1 -> V2 reuses the
    /// conversation id as the V2 session id).
    async fn run_v1_to_v2(&self) -> Result<String, RunError> {
        let v2_sessions_dir = chat_cli_v2::util::paths::sessions_dir()
            .map_err(|e| RunError::message(format!("failed to resolve V2 sessions dir: {e}")))?;
        self.do_v1_to_v2(&v2_sessions_dir).await?;
        Ok(self.source_session_id.clone())
    }

    /// Export a V1 conversation to V2 format at `dest_dir`. Used both
    /// directly by the V1 -> V2 path (with the production V2 dir) and
    /// internally by the V1 -> KAS path (with a tempdir so the V2
    /// byproduct never lands in the user-visible V2 store).
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

    /// Compose V1 -> V2 -> KAS via a tempdir for the V2 byproduct.
    /// The tempdir is dropped on return so the byproduct never
    /// surfaces in the user's V2 listing - the user only ever sees
    /// V1 (in classic) and KAS (after conversion) entries.
    async fn run_v1_to_kas(&self) -> Result<String, RunError> {
        let temp = tempfile::tempdir().map_err(|e| RunError::message(format!("failed to create temp V2 dir: {e}")))?;
        self.do_v1_to_v2(temp.path()).await?;
        self.do_v2_to_kas(temp.path())
    }
}

impl DeriveMessagesArgs {
    /// Returns `{"success": false, "error": "not implemented"}` with
    /// exit 1 until `EventLog::derive_messages` is wired in.
    fn execute(self) -> ExitCode {
        let _ = self;
        emit_error("not implemented")
    }
}

impl TestSeedV1Args {
    async fn execute(self) -> ExitCode {
        match self.run().await {
            Ok(conversation_id) => emit_conversation_success(&conversation_id),
            Err(err) => emit_error(&err),
        }
    }

    async fn run(self) -> Result<String, String> {
        // Refuse to run unless the caller has scoped the database to
        // a sandbox via `KIRO_TEST_DB_PATH`. This is the only signal
        // that a stray invocation will not corrupt the user's real
        // data store - the production path is hard-coded under
        // `dirs::data_local_dir()` and unavoidable otherwise.
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

fn emit_success(path: &Path) -> ExitCode {
    let s = path.to_string_lossy();
    let payload = JsonOutput::Success {
        success: true,
        path: &s,
    };
    // serde_json::to_string never fails for this shape; on the
    // off-chance it does, fall back to a plain panic message so the
    // caller still sees a parseable payload.
    match serde_json::to_string(&payload) {
        Ok(line) => println!("{line}"),
        Err(e) => {
            return emit_error(&format!("internal: failed to serialize success: {e}"));
        },
    }
    ExitCode::SUCCESS
}

/// Emit `{"success": true, "sessionId": ...}` and exit 0. Used by
/// subcommands whose result is a target-engine session id rather
/// than a path.
fn emit_session_success(session_id: &str) -> ExitCode {
    let payload = JsonOutput::SessionSuccess {
        success: true,
        session_id,
    };
    match serde_json::to_string(&payload) {
        Ok(line) => println!("{line}"),
        Err(e) => {
            return emit_error(&format!("internal: failed to serialize success: {e}"));
        },
    }
    ExitCode::SUCCESS
}

/// Emit `{"success": true, "conversationId": ...}` and exit 0.
/// Used by `test-seed-v1` so callers can chain on the seeded id
/// without re-parsing the fixture JSON.
fn emit_conversation_success(conversation_id: &str) -> ExitCode {
    let payload = JsonOutput::ConversationSuccess {
        success: true,
        conversation_id,
    };
    match serde_json::to_string(&payload) {
        Ok(line) => println!("{line}"),
        Err(e) => {
            return emit_error(&format!("internal: failed to serialize success: {e}"));
        },
    }
    ExitCode::SUCCESS
}

fn emit_error(message: &str) -> ExitCode {
    emit_error_code(message, None)
}

fn emit_error_code(message: &str, code: Option<&str>) -> ExitCode {
    let payload = JsonOutput::Error {
        success: false,
        error: message,
        code,
    };
    match serde_json::to_string(&payload) {
        Ok(line) => println!("{line}"),
        Err(_) => {
            // Last-ditch: if even the error envelope can't be
            // serialized, emit a minimal hand-rolled JSON line so the
            // contract holds.
            let escaped = message.replace('\\', "\\\\").replace('"', "\\\"");
            println!("{{\"success\":false,\"error\":\"{escaped}\"}}");
        },
    }
    ExitCode::FAILURE
}

/// Emit a success line for the GetKasToken subcommand. Spreads the token's
/// camelCase fields onto the success envelope so the JSON shape is:
/// `{"success": true, "accessToken": "...", "expiresAt": "...",
/// "profileArn"?: "..."}`. Mirrors the wire response shape the TUI handler
/// returns for `_kiro/auth/getAccessToken`.
fn emit_token(token: &AcpCallbackToken) -> ExitCode {
    let mut value = match serde_json::to_value(token) {
        Ok(v) => v,
        Err(e) => return emit_error(&format!("internal: failed to serialize token: {e}")),
    };
    let serde_json::Value::Object(ref mut map) = value else {
        return emit_error("internal: token serialized to non-object JSON");
    };
    map.insert("success".into(), serde_json::Value::Bool(true));
    println!("{value}");
    ExitCode::SUCCESS
}

/// `true` when a V2 session for `session_id` exists at `sessions_dir`.
/// Probes the on-disk metadata file - the same predicate `session/load`
/// itself uses.
fn v2_session_exists(sessions_dir: &Path, session_id: &str) -> bool {
    chat_cli_v2::agent::session::metadata_path(sessions_dir, session_id).exists()
}

/// `true` when a V1 conversation for `conversation_id` exists in the
/// classic SQLite store. The lookup honors `KIRO_TEST_DB_PATH` via
/// `database_path_static`.
async fn v1_conversation_exists(conversation_id: &str) -> Result<bool, RunError> {
    let db = crate::database::Database::new_default()
        .await
        .map_err(|e| RunError::message(format!("failed to open V1 database: {e}")))?;
    let entry = db
        .get_conversation_by_id_with_cwd(conversation_id)
        .map_err(|e| RunError::message(format!("failed to query V1 database: {e}")))?;
    Ok(entry.is_some())
}

/// `true` when a KAS session for `session_id` exists in any of the
/// workspace buckets reachable from `cwd`. Falls back to the standard
/// KAS root resolution path so `KIRO_HOME` overrides apply.
fn kas_session_exists(cwd: &str, session_id: &str) -> Result<bool, RunError> {
    let kas_root = default_kas_sessions_root()
        .map_err(|e| RunError::message(format!("failed to resolve KAS sessions root: {e}")))?;
    let bucket = chat_cli_v2::agent::kas::workspace_hash::compute_workspace_hash(&[cwd.to_string()]);
    let session_dir = kas_root.join(bucket).join(session_id);
    Ok(session_dir.join("session.json").exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Explicit KAS source: same id is returned unchanged regardless
    /// of the id shape - the caller's declared `source_format` is
    /// authoritative.
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

    /// A present V2 session resolves to its id unchanged: V2 ids are
    /// already in the target format, so a V2 target only needs to
    /// confirm the session exists before `session/load`.
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

    /// A missing V2 session surfaces the stable `SESSION_NOT_FOUND`
    /// code so the caller can fall through to a fresh session rather
    /// than surface a hard failure.
    #[test]
    fn ensure_v2_present_reports_not_found_for_missing_session() {
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
        assert_eq!(err.code, Some(SESSION_NOT_FOUND));
    }
}
