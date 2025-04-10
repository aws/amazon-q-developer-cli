use std::fmt::Display;
use std::path::{
    Path,
    PathBuf,
};
use std::str::FromStr;

use clap::ValueEnum;
use fig_os_shim::Env;
use regex::Regex;
use serde::{
    Deserialize,
    Serialize,
};
use tokio::process::Command;

use crate::consts::build::SKIP_FISH_TESTS;
use crate::env_var::Q_ZDOTDIR;
use crate::process_info::get_parent_process_exe;
use crate::{
    Error,
    directories,
};

/// All supported shells
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "camelCase")]
pub enum Shell {
    /// Bash shell
    Bash,
    /// Zsh shell
    Zsh,
    /// Fish shell
    Fish,
    /// Nu shell
    Nu,
}

impl Display for Shell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Shell {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "bash" => Ok(Shell::Bash),
            "zsh" => Ok(Shell::Zsh),
            "fish" => Ok(Shell::Fish),
            "nu" => Ok(Shell::Nu),
            _ => Err(()),
        }
    }
}

impl Shell {
    pub fn all() -> &'static [Self] {
        &[Shell::Bash, Shell::Zsh, Shell::Fish, Shell::Nu]
    }

    /// All shells to run unit / integration tests with
    pub fn all_test() -> Vec<Self> {
        let mut shells = vec![Shell::Bash, Shell::Zsh];
        if !SKIP_FISH_TESTS {
            shells.push(Shell::Fish);
        }
        shells
    }

    /// Try to find the name of common shells in the input
    pub fn try_find_shell(input: impl AsRef<Path>) -> Option<Self> {
        let input = input.as_ref().file_name()?.to_str()?;
        if input.contains("bash") {
            Some(Shell::Bash)
        } else if input.contains("zsh") {
            Some(Shell::Zsh)
        } else if input.contains("fish") {
            Some(Shell::Fish)
        } else if input == "nu" || input == "nushell" {
            Some(Shell::Nu)
        } else {
            None
        }
    }

    /// Gets the current shell of the parent process
    pub fn current_shell() -> Option<Self> {
        let parent_exe = get_parent_process_exe()?;
        let parent_exe_name = parent_exe.to_str()?;
        Self::try_find_shell(parent_exe_name)
    }

    pub async fn current_shell_version() -> Result<(Self, String), Error> {
        let parent_exe = get_parent_process_exe().ok_or(Error::NoParentProcess)?;
        let Some(shell) = Self::try_find_shell(&parent_exe) else {
            return Err(Error::UnknownShell(parent_exe.to_string_lossy().into()));
        };

        Ok((shell, shell_version(&shell, &parent_exe).await?))
    }

    /// Get the directory for the shell that contains the dotfiles
    pub fn get_config_directory(&self, env: &Env) -> Result<PathBuf, directories::DirectoryError> {
        match self {
            Shell::Bash => Ok(directories::home_dir()?),
            Shell::Zsh => match env
                .get_os("ZDOTDIR")
                .or_else(|| env.get_os(Q_ZDOTDIR))
                .map(PathBuf::from)
            {
                Some(dir) => Ok(dir),
                None => Ok(directories::home_dir()?),
            },
            Shell::Fish => match env.get_os("__fish_config_dir").map(PathBuf::from) {
                Some(dir) => Ok(dir),
                None => Ok(directories::home_dir()?.join(".config").join("fish")),
            },
            Shell::Nu => Ok(directories::config_dir()?.join("nushell")),
        }
    }

    pub fn get_data_path(&self) -> Result<PathBuf, directories::DirectoryError> {
        Ok(directories::fig_data_dir()?.join("shell").join(format!("{self}.json")))
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Shell::Bash => "bash",
            Shell::Zsh => "zsh",
            Shell::Fish => "fish",
            Shell::Nu => "nu",
        }
    }

    pub fn is_bash(&self) -> bool {
        matches!(self, Shell::Bash)
    }

    pub fn is_zsh(&self) -> bool {
        matches!(self, Shell::Zsh)
    }

    pub fn is_fish(&self) -> bool {
        matches!(self, Shell::Fish)
    }

    pub fn is_nu(&self) -> bool {
        matches!(self, Shell::Nu)
    }
}

const BASH_RE: &str = r"GNU bash, version (\d+\.\d+\.\d+)";
const ZSH_RE: &str = r"(\d+\.\d+)";
const FISH_RE: &str = r"(\d+\.\d+\.\d+)";

async fn shell_version(shell: &Shell, exe_path: &Path) -> Result<String, Error> {
    let err = || Error::ShellVersion(*shell);
    match shell {
        Shell::Bash => {
            let re = Regex::new(BASH_RE).unwrap();
            let version_output = Command::new(exe_path).arg("--version").output().await?;
            let version_capture = re.captures(std::str::from_utf8(&version_output.stdout)?);
            Ok(version_capture.ok_or_else(err)?.get(1).ok_or_else(err)?.as_str().into())
        },
        Shell::Zsh => {
            let re = Regex::new(ZSH_RE).unwrap();
            let version_output = Command::new(exe_path).arg("--version").output().await?;
            let version_capture = re.captures(std::str::from_utf8(&version_output.stdout)?);
            Ok(version_capture.ok_or_else(err)?.get(1).ok_or_else(err)?.as_str().into())
        },
        Shell::Fish => {
            let re = Regex::new(FISH_RE).unwrap();
            let version_output = Command::new(exe_path).arg("--version").output().await?;
            let version_capture = re.captures(std::str::from_utf8(&version_output.stdout)?);
            Ok(version_capture.ok_or_else(err)?.get(1).ok_or_else(err)?.as_str().into())
        },
        Shell::Nu => {
            let version_output = Command::new(exe_path).arg("--version").output().await?;
            Ok(std::str::from_utf8(&version_output.stdout)?.trim().into())
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::SKIP_FISH_TESTS;

    #[tokio::test]
    async fn test_shell_version() {
        let tests = [
            (Shell::Bash, "/bin/bash", false),
            (Shell::Bash, "bash", false),
            (Shell::Zsh, "/bin/zsh", false),
            (Shell::Fish, "fish", SKIP_FISH_TESTS),
        ];

        for (shell, exe_path_str, skip) in tests {
            if skip {
                continue;
            }

            let exe_path = Path::new(exe_path_str);
            let version: String = shell_version(&shell, exe_path)
                .await
                .unwrap_or_else(|err| panic!("exe {} failed. Error: {:?}", exe_path_str, err));
            println!("{}: {version:?}\n", exe_path.display());
        }
    }

    #[test]
    fn test_bash_re() {
        let re = Regex::new(BASH_RE).unwrap();

        let bash_3_version = indoc::indoc! {r"
            GNU bash, version 3.2.57(1)-release (x86_64-apple-darwin23)
            Copyright (C) 2007 Free Software Foundation, Inc.
        "};
        assert_eq!(re.captures(bash_3_version).unwrap().get(1).unwrap().as_str(), "3.2.57");

        let bash_5_version = indoc::indoc! {r"
            GNU bash, version 5.2.26(1)-release (aarch64-apple-darwin23.2.0)
            Copyright (C) 2022 Free Software Foundation, Inc.
            License GPLv3+: GNU GPL version 3 or later <http://gnu.org/licenses/gpl.html>

            This is free software; you are free to change and redistribute it.
            There is NO WARRANTY, to the extent permitted by law.
        "};
        assert_eq!(re.captures(bash_5_version).unwrap().get(1).unwrap().as_str(), "5.2.26");
    }

    #[test]
    fn test_zsh_re() {
        let re = Regex::new(ZSH_RE).unwrap();
        let zsh_version = "zsh 5.9 (arm-apple-darwin22.1.0)";
        assert_eq!(re.captures(zsh_version).unwrap().get(1).unwrap().as_str(), "5.9");
    }

    #[test]
    fn test_fish_re() {
        let re = Regex::new(FISH_RE).unwrap();
        let fish_version = "fish 3.6.1";
        assert_eq!(re.captures(fish_version).unwrap().get(1).unwrap().as_str(), "3.6.1");
    }
}
