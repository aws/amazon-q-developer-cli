//! Hidden internal subcommands under `chat _ ...`.
//!
//! Test-and-IPC surface, not user-facing. The slash-command handlers
//! `/chat save` and `/chat load` (in the TUI) shell out to these via
//! `child_process.spawnSync`, parse the JSON line on stdout, and
//! surface the result. The e2e test harness drives the same surface
//! directly. Args are explicit (no env-var reliance) so callers can
//! sandbox the operation with a dedicated `--base-path` and `--cwd`.
//!
//! Output contract:
//! - Single JSON line on stdout, exit code mirrors `result`.
//! - Success (exit 0): `{"result": true, "path": "<absolute path>"}`
//! - Failure (exit 1): `{"result": false, "error": "<message>"}`
//!
//! No human-readable mode. Errors come out as JSON too so the TUI
//! handler always parses with `JSON.parse(stdout)` and never has to
//! disambiguate stdout vs stderr text.

use std::path::{
    Path,
    PathBuf,
};
use std::process::ExitCode;

use chat_cli_v2::agent::kas::{
    ExportSessionOptions,
    ImportSessionOptions,
    SessionArchiveError,
    default_kas_sessions_root,
    export_session,
    import_session,
};
use clap::{
    Args,
    Subcommand,
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

#[derive(Serialize)]
#[serde(untagged)]
enum JsonOutput<'a> {
    /// `{"result": true, "path": ...}` - exit 0.
    Success {
        result: bool,
        /// Absolute path the operation produced. Export: zip archive.
        /// Import: extracted session directory; the new session id is
        /// the basename.
        path: &'a str,
    },
    /// `{"result": false, "error": ...}` - exit 1.
    Error {
        result: bool,
        /// Human-readable error message. Exact text is not stable -
        /// callers should not pattern-match on it.
        error: &'a str,
    },
}

impl ChatCommand {
    pub fn execute(self) -> Result<ExitCode> {
        match self {
            Self::Internal(InternalChatSubcommand::ExportSession(args)) => Ok(args.execute()),
            Self::Internal(InternalChatSubcommand::ImportSession(args)) => Ok(args.execute()),
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
        import_session(ImportSessionOptions {
            kas_sessions_root,
            archive_path: self.archive,
            workspace_paths: vec![self.cwd],
        })
    }
}

fn emit_success(path: &Path) -> ExitCode {
    let s = path.to_string_lossy();
    let payload = JsonOutput::Success { result: true, path: &s };
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

fn emit_error(message: &str) -> ExitCode {
    let payload = JsonOutput::Error {
        result: false,
        error: message,
    };
    match serde_json::to_string(&payload) {
        Ok(line) => println!("{line}"),
        Err(_) => {
            // Last-ditch: if even the error envelope can't be
            // serialized, emit a minimal hand-rolled JSON line so the
            // contract holds.
            let escaped = message.replace('\\', "\\\\").replace('"', "\\\"");
            println!("{{\"result\":false,\"error\":\"{escaped}\"}}");
        },
    }
    ExitCode::FAILURE
}
