//! Launches the Kiro Crew CLI, installing it first if it is not present.

use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args;
use eyre::{
    Result,
    WrapErr,
    bail,
};

use crate::os::Os;

const BINARY_NAME: &str = "kirocrew";
#[cfg(unix)]
const INSTALL_SCRIPT_URL: &str = "https://download.crew.kiro.dev/cli.sh";
#[cfg(windows)]
const REPO_URL: &str = "https://github.com/kirodotdev/KiroCrew";

/// Arguments for the `crew` subcommand.
#[derive(Debug, PartialEq, Args)]
pub struct CrewArgs {
    /// Install Kiro Crew without prompting if it is not already installed. Only applies before
    /// the first trailing argument; use "--" to forward a literal --yes to Kiro Crew
    #[arg(long, short = 'y')]
    yes: bool,
    /// Arguments forwarded to the Kiro Crew CLI
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

impl CrewArgs {
    pub async fn execute(self, os: &Os) -> Result<ExitCode> {
        let binary = match resolve_binary(os) {
            Some(binary) => binary,
            None => self.install(os)?,
        };
        launch(binary, self.args)
    }

    #[cfg(unix)]
    fn install(&self, os: &Os) -> Result<PathBuf> {
        use std::io::IsTerminal;

        match install_gate(self.yes, std::io::stdout().is_terminal()) {
            InstallGate::Install => {},
            InstallGate::Prompt => {
                match crate::util::choose("Kiro Crew is not installed. Install now?", &["Yes", "No"])? {
                    Some(0) => {},
                    _ => bail!("Kiro Crew installation was declined"),
                }
            },
            InstallGate::Refuse => bail!(
                "Kiro Crew is not installed. Re-run with --yes to install it, or run:\n  curl -fsSL {INSTALL_SCRIPT_URL} | sh"
            ),
        }

        println!("Installing Kiro Crew...");
        let status = install_command()
            .status()
            .wrap_err("failed to run the Kiro Crew installer")?;
        if !status.success() {
            bail!("Kiro Crew installation failed");
        }

        match resolve_binary(os) {
            Some(binary) => Ok(binary),
            None => bail!(
                "Kiro Crew was installed, but the {BINARY_NAME} binary could not be found. Try adding ~/.local/bin to your PATH."
            ),
        }
    }

    #[cfg(windows)]
    fn install(&self, _os: &Os) -> Result<PathBuf> {
        bail!("Kiro Crew is not installed. Please download and build from {REPO_URL}");
    }
}

#[cfg(unix)]
fn launch(binary: PathBuf, args: Vec<String>) -> Result<ExitCode> {
    use std::os::unix::process::CommandExt;

    let err = std::process::Command::new(binary).args(args).exec();
    Err(err).wrap_err("failed to launch Kiro Crew")
}

#[cfg(windows)]
fn launch(binary: PathBuf, args: Vec<String>) -> Result<ExitCode> {
    let status = std::process::Command::new(binary)
        .args(args)
        .status()
        .wrap_err("failed to launch Kiro Crew")?;
    Ok(match status.code() {
        Some(0) | None => ExitCode::SUCCESS,
        Some(code) => ExitCode::from(code.clamp(0, u8::MAX as i32) as u8),
    })
}

/// What to do when the binary is missing, based on the `--yes` flag and
/// whether a user is attending the terminal.
#[cfg(unix)]
#[derive(Debug, PartialEq)]
enum InstallGate {
    Install,
    Prompt,
    Refuse,
}

#[cfg(unix)]
fn install_gate(yes: bool, interactive: bool) -> InstallGate {
    match (yes, interactive) {
        (true, _) => InstallGate::Install,
        (false, true) => InstallGate::Prompt,
        (false, false) => InstallGate::Refuse,
    }
}

/// Locates the binary on `PATH`, falling back to `~/.local/bin` since the
/// installer may place it in a directory not yet on the user's `PATH`.
fn resolve_binary(os: &Os) -> Option<PathBuf> {
    let fallback = crate::util::paths::home_dir(os)
        .ok()
        .map(|home| home.join(".local/bin"));
    resolve_binary_in(std::env::var_os("PATH").as_deref(), fallback)
}

fn resolve_binary_in(path_var: Option<&OsStr>, fallback_dir: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(path_var) = path_var
        && let Ok(found) = which::which_in(BINARY_NAME, Some(path_var), "/")
    {
        return Some(found);
    }
    if let Some(dir) = fallback_dir
        && let Ok(found) = which::which_in(BINARY_NAME, Some(&dir), "/")
    {
        return Some(found);
    }
    None
}

#[cfg(unix)]
fn install_command() -> std::process::Command {
    let mut command = std::process::Command::new("sh");
    command.arg("-c").arg(format!("curl -fsSL {INSTALL_SCRIPT_URL} | sh"));
    command
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::util::test::assert_parse;

    fn make_executable(dir: &std::path::Path) -> PathBuf {
        #[cfg(windows)]
        let path = dir.join(format!("{BINARY_NAME}.exe"));
        #[cfg(unix)]
        let path = dir.join(BINARY_NAME);
        fs::write(&path, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms).unwrap();
        }
        path
    }

    #[test]
    fn parse_leading_yes_is_install_flag() {
        assert_parse!(
            ["crew", "--yes"],
            crate::cli::RootSubcommand::Crew(CrewArgs {
                yes: true,
                args: vec![],
            })
        );
    }

    #[test]
    fn parse_trailing_yes_is_forwarded() {
        assert_parse!(
            ["crew", "run", "--yes"],
            crate::cli::RootSubcommand::Crew(CrewArgs {
                yes: false,
                args: vec!["run".into(), "--yes".into()],
            })
        );
    }

    #[test]
    fn parse_double_dash_forwards_yes() {
        assert_parse!(
            ["crew", "--", "--yes"],
            crate::cli::RootSubcommand::Crew(CrewArgs {
                yes: false,
                args: vec!["--yes".into()],
            })
        );
    }

    #[cfg(unix)]
    #[test]
    fn gate_yes_installs_without_prompt() {
        assert_eq!(install_gate(true, true), InstallGate::Install);
        assert_eq!(install_gate(true, false), InstallGate::Install);
    }

    #[cfg(unix)]
    #[test]
    fn gate_interactive_prompts() {
        assert_eq!(install_gate(false, true), InstallGate::Prompt);
    }

    #[cfg(unix)]
    #[test]
    fn gate_non_interactive_refuses() {
        assert_eq!(install_gate(false, false), InstallGate::Refuse);
    }

    #[test]
    fn resolves_binary_on_path() {
        let dir = tempfile::tempdir().unwrap();
        let expected = make_executable(dir.path());
        assert_eq!(resolve_binary_in(Some(dir.path().as_os_str()), None), Some(expected));
    }

    #[test]
    fn resolves_binary_from_fallback_dir() {
        let path_dir = tempfile::tempdir().unwrap();
        let fallback_dir = tempfile::tempdir().unwrap();
        let expected = make_executable(fallback_dir.path());
        assert_eq!(
            resolve_binary_in(Some(path_dir.path().as_os_str()), Some(fallback_dir.path().to_owned())),
            Some(expected)
        );
    }

    #[test]
    fn path_takes_precedence_over_fallback() {
        let path_dir = tempfile::tempdir().unwrap();
        let fallback_dir = tempfile::tempdir().unwrap();
        let expected = make_executable(path_dir.path());
        make_executable(fallback_dir.path());
        assert_eq!(
            resolve_binary_in(Some(path_dir.path().as_os_str()), Some(fallback_dir.path().to_owned())),
            Some(expected)
        );
    }

    #[test]
    fn missing_binary_resolves_to_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(resolve_binary_in(Some(dir.path().as_os_str()), None), None);
        assert_eq!(resolve_binary_in(None, None), None);
    }

    #[cfg(unix)]
    #[test]
    fn non_executable_file_is_skipped() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(BINARY_NAME);
        fs::write(&path, "").unwrap();
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&path, perms).unwrap();
        assert_eq!(resolve_binary_in(Some(dir.path().as_os_str()), None), None);
    }

    #[cfg(unix)]
    #[test]
    fn install_command_pipes_script_to_sh() {
        let command = install_command();
        assert_eq!(command.get_program(), "sh");
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args, vec![
            OsStr::new("-c"),
            OsStr::new("curl -fsSL https://download.crew.kiro.dev/cli.sh | sh")
        ]);
    }
}
