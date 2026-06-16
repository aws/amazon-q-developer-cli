use std::fmt::Display;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::ValueEnum;
use eyre::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum AgentEngine {
    #[default]
    #[value(alias = "rust")]
    V2,
    V1,
    // The next-generation agent engine. Surfaced to users as "v3"; "kas" is
    // accepted as a backwards-compatible alias and remains the wire/storage
    // identifier (see `Display`). Keep the value description out of `--help`
    // by using a plain comment rather than a doc comment.
    #[value(name = "v3", alias = "kas")]
    Kas,
}

impl Display for AgentEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::V1 => write!(f, "v1"),
            Self::V2 => write!(f, "v2"),
            Self::Kas => write!(f, "kas"),
        }
    }
}

impl AgentEngine {
    /// User-facing label for the engine. Mirrors the clap value names, so the
    /// next-generation engine is presented as "v3" rather than its internal
    /// "kas" identifier.
    pub fn user_label(&self) -> &'static str {
        match self {
            Self::V1 => "v1",
            Self::V2 => "v2",
            Self::Kas => "v3",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum AgentMode {
    // "vibe" is accepted as a backwards-compatible alias for the canonical
    // "default" mode id (the wire identifier sent to the V3 agent).
    #[default]
    #[value(alias = "vibe")]
    Default,
    Spec,
}

impl Display for AgentMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Default => write!(f, "default"),
            Self::Spec => write!(f, "spec"),
        }
    }
}

/// Whether the V2 launcher should drive an interactive TUI session or run a
/// one-shot non-interactive session that streams output to stdout.
#[derive(Debug, Clone)]
pub enum Interactivity {
    /// Spawn the full TUI and run an interactive session.
    Interactive,
    /// Run a one-shot non-interactive session. `input` is the user prompt that
    /// drives the single turn; it should have already been resolved from CLI
    /// args or stdin by the caller.
    NonInteractive { input: String },
}

/// Options controlling how a chat session is launched.
#[derive(Debug, Clone)]
pub struct LaunchOptions {
    /// Which agent engine to use: native Rust ACP or the KAS TypeScript agent.
    pub agent_engine: AgentEngine,
    /// Optional initial mode for agents that support it (e.g. KAS default/spec).
    pub mode: Option<AgentMode>,
    /// Whether to drive an interactive TUI or a non-interactive one-shot turn.
    pub interactivity: Interactivity,
    /// Auto-approve all tool permission requests.
    pub trust_all_tools: bool,
    /// Agent name override.
    pub agent: Option<String>,
    /// Model override.
    pub model: Option<String>,
    /// Specific tools to trust.
    pub trust_tools: Option<Vec<String>>,
}

impl LaunchOptions {
    /// Build [`LaunchOptions`] for an interactive TUI session.
    pub fn interactive(agent_engine: AgentEngine, mode: Option<AgentMode>) -> Self {
        Self {
            agent_engine,
            mode,
            interactivity: Interactivity::Interactive,
            trust_all_tools: false,
            agent: None,
            model: None,
            trust_tools: None,
        }
    }

    /// Build [`LaunchOptions`] for a non-interactive one-shot session driven by `input`.
    pub fn non_interactive(
        agent_engine: AgentEngine,
        mode: Option<AgentMode>,
        input: String,
        trust_all_tools: bool,
        agent: Option<String>,
        model: Option<String>,
        trust_tools: Option<Vec<String>>,
    ) -> Self {
        Self {
            agent_engine,
            mode,
            interactivity: Interactivity::NonInteractive { input },
            trust_all_tools,
            agent,
            model,
            trust_tools,
        }
    }
}

/// Paths to the bun executable and TUI JS file to use
#[derive(Debug, Clone)]
pub struct TuiAssetPaths {
    pub bun_path: PathBuf,
    pub tui_js_path: PathBuf,
}

/// Spawn the TUI process using the given asset paths and forward CLI arguments.
/// Handles signal-based cleanup to prevent orphaned bun processes.
pub async fn launch_tui(asset_paths: &TuiAssetPaths) -> Result<ExitCode> {
    let args: Vec<String> = std::env::args().collect();
    let current_exe = std::env::current_exe()?;
    let mut child = tokio::process::Command::new(&asset_paths.bun_path)
        .arg(&asset_paths.tui_js_path)
        .args(&args[1..])
        .env(crate::util::consts::env_var::KIRO_CHAT_CLI_BIN, &current_exe)
        // Limit JSC garbage collector to 1 marker thread. By default JSC
        // uses up to min(4, core_count) marker threads on Apple Silicon
        // (see overrideDefaults() and computeNumberOfGCMarkers in Options.cpp).
        // https://github.com/WebKit/WebKit/blob/4a93a28675f1cacb54b7e280d5d4a2ca3bf7557c/Source/JavaScriptCore/runtime/Options.cpp#L455
        // Related: https://github.com/anthropics/claude-code/issues/38092
        // Related: https://github.com/oven-sh/bun/issues/17723
        .env("JSC_numberOfGCMarkers", "1")
        .env("KIRO_FEED_FILE", crate::util::paths::feed_json_path()?)
        .kill_on_drop(true)
        .spawn()?;

    // Kill the child on SIGTERM, SIGHUP, or Ctrl-C to prevent orphaned
    // bun processes at 100% CPU. Without this, the default signal handler
    // terminates the process without running destructors.
    let status;
    #[cfg(unix)]
    {
        use tokio::signal::unix::{
            SignalKind,
            signal,
        };
        let mut sigterm = signal(SignalKind::terminate())?;
        let mut sighup = signal(SignalKind::hangup())?;
        tokio::select! {
            s = child.wait() => {
                status = Some(s?);
            }
            _ = sigterm.recv() => {
                let _ = child.kill().await;
                status = None;
            }
            _ = sighup.recv() => {
                let _ = child.kill().await;
                status = None;
            }
            _ = tokio::signal::ctrl_c() => {
                let _ = child.kill().await;
                status = None;
            }
        }
    }
    #[cfg(not(unix))]
    {
        status = Some(child.wait().await?);
    }

    let exit_code = status
        .and_then(|s| s.code())
        .map_or(ExitCode::FAILURE, |e| ExitCode::from(e as u8));

    Ok(exit_code)
}
