use std::fs::File;
use std::io::{
    ErrorKind,
    Write,
};
use std::path::PathBuf;

use async_trait::async_trait;
use cfg_if::cfg_if;
use clap::ValueEnum;
use fig_os_shim::Env;
use fig_util::{
    CLI_BINARY_NAME,
    PRODUCT_NAME,
    Shell,
    directories,
};
use regex::{
    Regex,
    RegexSet,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::error::{
    ErrorExt,
    Result,
};
use crate::{
    Error,
    FileIntegration,
    Integration,
    backup_file,
};

pub mod inline_shell_completion_plugin {
    pub const ZSH_SCRIPT: &str = concat!(
        "\n",
        include_str!(concat!(env!("OUT_DIR"), "/inline_shell_completion.zsh")),
        "\n"
    );

    #[cfg(test)]
    mod tests {
        use fig_util::CLI_BINARY_NAME;

        use super::*;

        #[test]
        fn script_checks() {
            // Ensure script has license
            assert!(ZSH_SCRIPT.contains("Copyright"));

            // Ensure script has _q_autosuggest_strategy_inline_shell_completion()
            assert!(ZSH_SCRIPT.contains(&format!(
                "_{CLI_BINARY_NAME}_autosuggest_strategy_inline_shell_completion()"
            )));

            // Ensure script adds precmd hook
            assert!(ZSH_SCRIPT.contains(&format!("add-zsh-hook precmd _{CLI_BINARY_NAME}_autosuggest_start")));
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum When {
    Pre,
    Post,
}

impl When {
    pub fn all() -> [When; 2] {
        [Self::Pre, Self::Post]
    }
}

impl std::fmt::Display for When {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            When::Pre => write!(f, "pre"),
            When::Post => write!(f, "post"),
        }
    }
}

fn integration_file_name(dotfile_name: &str, when: &When, shell: &Shell) -> String {
    format!(
        "{}.{when}.{shell}",
        Regex::new(r"^\.").unwrap().replace_all(dotfile_name, ""),
    )
}

pub trait ShellExt {
    fn get_shell_integrations(&self, env: &Env) -> Result<Vec<Box<dyn ShellIntegration>>>;
    /// Script integrations are installed into ~/.fig/shell
    fn get_script_integrations(&self) -> Result<Vec<ShellScriptShellIntegration>>;
    fn get_fig_integration_source(&self, when: &When) -> &'static str;
}

impl ShellExt for Shell {
    fn get_script_integrations(&self) -> Result<Vec<ShellScriptShellIntegration>> {
        let mut integrations = vec![];

        for file in match self {
            Shell::Bash => [".bashrc", ".bash_profile", ".bash_login", ".profile"].iter(),
            Shell::Zsh => [".zshrc", ".zprofile"].iter(),
            Shell::Fish | Shell::Nu => [].iter(),
        } {
            for when in &When::all() {
                let path = directories::fig_data_dir()?
                    .join("shell")
                    .join(integration_file_name(file, when, self));

                integrations.push(ShellScriptShellIntegration {
                    shell: *self,
                    when: *when,
                    path,
                });
            }
        }

        Ok(integrations)
    }

    fn get_shell_integrations(&self, env: &Env) -> Result<Vec<Box<dyn ShellIntegration>>> {
        let config_dir = self.get_config_directory(env)?;

        let integrations: Vec<Box<dyn ShellIntegration>> = match self {
            Shell::Bash => {
                let mut configs = vec![".bashrc"];
                let other_configs = [".profile", ".bash_login", ".bash_profile"];

                configs.extend(other_configs.into_iter().filter(|f| config_dir.join(f).exists()));

                // Include .profile if none of [.profile, .bash_login, .bash_profile] exist.
                if configs.len() == 1 {
                    configs.push(other_configs[0]);
                }

                configs
                    .into_iter()
                    .map(|filename| {
                        Box::new(DotfileShellIntegration {
                            pre: true,
                            post: true,
                            shell: *self,
                            dotfile_directory: config_dir.clone(),
                            dotfile_name: filename,
                        }) as Box<dyn ShellIntegration>
                    })
                    .collect()
            },
            Shell::Zsh => vec![".zshrc", ".zprofile"]
                .into_iter()
                .map(|filename| {
                    Box::new(DotfileShellIntegration {
                        pre: true,
                        post: true,
                        shell: *self,
                        dotfile_directory: config_dir.clone(),
                        dotfile_name: filename,
                    }) as Box<dyn ShellIntegration>
                })
                .collect(),
            Shell::Fish => {
                let fish_config_dir = config_dir.join("conf.d");
                vec![
                    Box::new(ShellScriptShellIntegration {
                        when: When::Pre,
                        shell: *self,
                        path: fish_config_dir.join("00_fig_pre.fish"),
                    }),
                    Box::new(ShellScriptShellIntegration {
                        when: When::Post,
                        shell: *self,
                        path: fish_config_dir.join("99_fig_post.fish"),
                    }),
                ]
            },
            Shell::Nu => vec![],
        };

        Ok(integrations)
    }

    fn get_fig_integration_source(&self, when: &When) -> &'static str {
        match (self, when) {
            (Shell::Fish, When::Pre) => include_str!("scripts/pre.fish"),
            (Shell::Fish, When::Post) => include_str!("scripts/post.fish"),
            (Shell::Zsh, When::Pre) => include_str!("scripts/pre.sh"),
            (Shell::Zsh, When::Post) => include_str!("scripts/post.zsh"),
            (Shell::Bash, When::Pre) => {
                concat!(
                    "function __fig_source_bash_preexec() {\n",
                    include_str!("scripts/bash-preexec.sh"),
                    "}\n",
                    "__fig_source_bash_preexec\n",
                    "function __bp_adjust_histcontrol() { :; }\n",
                    include_str!("scripts/pre.sh")
                )
            },
            (Shell::Bash, When::Post) => {
                concat!(
                    "function __fig_source_bash_preexec() {\n",
                    include_str!("scripts/bash-preexec.sh"),
                    "}\n",
                    "__fig_source_bash_preexec\n",
                    "function __bp_adjust_histcontrol() { :; }\n",
                    include_str!("scripts/post.bash")
                )
            },
            (Shell::Nu, When::Pre) => include_str!("scripts/pre.nu"),
            (Shell::Nu, When::Post) => include_str!("scripts/post.nu"),
        }
    }
}

pub trait ShellIntegration: Send + Sync + Integration + ShellIntegrationClone {
    // The unique name of the integration file
    fn file_name(&self) -> &str;
    fn get_shell(&self) -> Shell;
    fn path(&self) -> PathBuf;
}

impl std::fmt::Display for dyn ShellIntegration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.get_shell(), self.path().display())
    }
}

pub trait ShellIntegrationClone {
    fn clone_box(&self) -> Box<dyn ShellIntegration>;
}

impl<T> ShellIntegrationClone for T
where
    T: 'static + ShellIntegration + Clone,
{
    fn clone_box(&self) -> Box<dyn ShellIntegration> {
        Box::new(self.clone())
    }
}

// We can now implement Clone manually by forwarding to clone_box.
impl Clone for Box<dyn ShellIntegration> {
    fn clone(&self) -> Box<dyn ShellIntegration> {
        self.clone_box()
    }
}

#[derive(Debug, Clone)]
pub struct ShellScriptShellIntegration {
    pub shell: Shell,
    pub when: When,
    pub path: PathBuf,
}

fn get_prefix(s: &str) -> &str {
    match s.find('.') {
        Some(i) => &s[..i],
        None => s,
    }
}

impl ShellScriptShellIntegration {
    fn get_file_integration(&self) -> FileIntegration {
        FileIntegration {
            path: self.path.clone(),
            contents: self.get_contents(),
            #[cfg(unix)]
            mode: None,
        }
    }

    fn get_name(&self) -> Option<&str> {
        self.path.file_name().and_then(|s| s.to_str())
    }

    #[allow(clippy::needless_return)]
    fn get_contents(&self) -> String {
        let Self { shell, when, path } = self;
        let rcfile = match path.file_name().and_then(|x| x.to_str()) {
            Some(name) => format!(" --rcfile {}", get_prefix(name)),
            None => "".into(),
        };
        cfg_if!(
            if #[cfg(target_os = "macos")] {
                return match self.shell {
                    // Check if ~/.local/bin/{CLI_BINARY_NAME} is executable before eval
                    Shell::Bash | Shell::Zsh => format!("[ -x ~/.local/bin/{CLI_BINARY_NAME} ] && eval \"$(~/.local/bin/{CLI_BINARY_NAME} init {shell} {when}{rcfile})\""),
                    Shell::Fish => format!("test -x ~/.local/bin/{CLI_BINARY_NAME}; and eval (~/.local/bin/{CLI_BINARY_NAME} init {shell} {when}{rcfile} | string split0)"),
                    Shell::Nu => "".into(),
                }
            } else {
                let add_to_path_line = match self.shell {
                    Shell::Bash | Shell::Zsh => indoc::indoc! {r#"
                        _Q_LOCAL_BIN="$HOME/.local/bin"
                        [[ ":$PATH:" != *":$_Q_LOCAL_BIN:"* ]] && PATH="${PATH:+"$PATH:"}$_Q_LOCAL_BIN"
                        unset _Q_LOCAL_BIN
                    "#},
                    Shell::Fish => "contains $HOME/.local/bin $PATH; or set -a PATH $HOME/.local/bin",
                    Shell::Nu => "",
                };

                let source_line = match self.shell {
                    Shell::Fish => format!("command -qv {CLI_BINARY_NAME}; and eval ({CLI_BINARY_NAME} init {shell} {when}{rcfile} | string split0)"),
                    Shell::Bash | Shell::Zsh => {
                        // Check that the current shell is bash
                        let bash_pre = if self.shell.is_bash() { "[ -n \"$BASH_VERSION\" ] && " } else { "" };
                        format!("{bash_pre}command -v {CLI_BINARY_NAME} >/dev/null 2>&1 && eval \"$({CLI_BINARY_NAME} init {shell} {when}{rcfile})\"")
                    }
                    Shell::Nu => "".into(),
                };

                return format!("{add_to_path_line}\n{source_line}\n");
            }
        );
    }
}

#[async_trait]
impl Integration for ShellScriptShellIntegration {
    async fn is_installed(&self) -> Result<()> {
        self.get_file_integration().is_installed().await
    }

    async fn install(&self) -> Result<()> {
        self.get_file_integration().install().await
    }

    async fn uninstall(&self) -> Result<()> {
        self.get_file_integration().uninstall().await
    }

    fn describe(&self) -> String {
        format!("{} {}", self.shell, self.when)
    }

    async fn migrate(&self) -> Result<()> {
        self.get_file_integration().install().await
    }
}

impl ShellIntegration for ShellScriptShellIntegration {
    fn file_name(&self) -> &str {
        self.get_name().unwrap_or("unknown_script")
    }

    fn get_shell(&self) -> Shell {
        self.shell
    }

    fn path(&self) -> PathBuf {
        self.path.clone()
    }
}

/// zsh and bash integration where we modify a dotfile with pre/post hooks that reference script
/// files.
#[derive(Debug, Clone)]
pub struct DotfileShellIntegration {
    pub shell: Shell,
    pub pre: bool,
    pub post: bool,
    pub dotfile_directory: PathBuf,
    pub dotfile_name: &'static str,
}

impl DotfileShellIntegration {
    fn dotfile_path(&self) -> PathBuf {
        self.dotfile_directory.join(self.dotfile_name)
    }

    fn legacy_script_integration(&self, when: When) -> Result<ShellScriptShellIntegration> {
        let integration_file_name = format!(
            "{}.{}.{}",
            Regex::new(r"^\.").unwrap().replace_all(self.dotfile_name, ""),
            when,
            self.shell
        );
        Ok(ShellScriptShellIntegration {
            shell: self.shell,
            when,
            path: directories::old_fig_data_dir()?
                .join("shell")
                .join(integration_file_name),
        })
    }

    fn script_integration(&self, when: When) -> Result<ShellScriptShellIntegration> {
        let integration_file_name = format!(
            "{}.{}.{}",
            Regex::new(r"^\.").unwrap().replace_all(self.dotfile_name, ""),
            when,
            self.shell
        );
        Ok(ShellScriptShellIntegration {
            shell: self.shell,
            when,
            path: directories::fig_data_dir()?.join("shell").join(integration_file_name),
        })
    }

    #[allow(clippy::unused_self)]
    fn description(&self, when: When) -> String {
        match when {
            When::Pre => format!("# {PRODUCT_NAME} pre block. Keep at the top of this file."),
            When::Post => format!("# {PRODUCT_NAME} post block. Keep at the bottom of this file."),
        }
    }

    fn legacy_description(when: When) -> String {
        match when {
            When::Pre => "# CodeWhisperer pre block. Keep at the top of this file.",
            When::Post => "# CodeWhisperer post block. Keep at the bottom of this file.",
        }
        .into()
    }

    fn legacy_regexes(&self, when: When) -> Result<RegexSet> {
        let shell = self.shell;

        let eval_line = match shell {
            Shell::Fish => format!("eval ({CLI_BINARY_NAME} init {shell} {when} | string split0)"),
            _ => format!("eval \"$({CLI_BINARY_NAME} init {shell} {when})\""),
        };

        let old_eval_source = match when {
            When::Pre => match self.shell {
                Shell::Fish => format!("set -Ua fish_user_paths $HOME/.local/bin\n{eval_line}"),
                _ => format!("export PATH=\"${{PATH}}:${{HOME}}/.local/bin\"\n{eval_line}"),
            },
            When::Post => eval_line,
        };

        let old_file_regex = match when {
            When::Pre => r"\[ -s ~/\.fig/shell/pre\.sh \] && source ~/\.fig/shell/pre\.sh\n?",
            When::Post => r"\[ -s ~/\.fig/fig\.sh \] && source ~/\.fig/fig\.sh\n?",
        };
        let old_eval_regex = format!(
            r#"(?m)(?:{}\n)?^{}\n{{0,2}}"#,
            regex::escape(&DotfileShellIntegration::legacy_description(when)),
            regex::escape(&old_eval_source),
        );
        let old_source_regex_1 = format!(
            r#"(?m)(?:{}\n)?^{}\n{{0,2}}"#,
            regex::escape(&DotfileShellIntegration::legacy_description(when)),
            regex::escape(&self.legacy_source_text_1(when)?),
        );
        let old_source_regex_2 = format!(
            r#"(?m)(?:{}\n)?^{}\n{{0,2}}"#,
            regex::escape(&DotfileShellIntegration::legacy_description(when)),
            regex::escape(&self.legacy_source_text_2(when)?),
        );

        let old_brand_regex = self.old_brand_regex(when)?;

        Ok(RegexSet::new([
            old_file_regex,
            &old_eval_regex,
            &old_source_regex_1,
            &old_source_regex_2,
            &old_brand_regex,
        ])?)
    }

    fn legacy_source_text_1(&self, when: When) -> Result<String> {
        let home = directories::home_dir()?;
        let integration_path = self.script_integration(when)?.path;
        let path = integration_path.strip_prefix(home)?;
        Ok(format!(". \"$HOME/{}\"", path.display()))
    }

    fn legacy_source_text_2(&self, when: When) -> Result<String> {
        let home = directories::home_dir()?;
        let integration_path = self.script_integration(when)?.path;
        let path = format!("\"$HOME/{}\"", integration_path.strip_prefix(home)?.display());

        match self.shell {
            Shell::Fish => Ok(format!("if test -f {path}; . {path}; end")),
            _ => Ok(format!("[[ -f {path} ]] && . {path}")),
        }
    }

    fn legacy_source_text_3(&self, when: When) -> Result<String> {
        let home = directories::home_dir()?;
        let integration_path = self.legacy_script_integration(when)?.path;
        let path = regex::escape(&format!(
            "\"${{HOME}}/{}\"",
            integration_path.strip_prefix(home)?.display()
        ));

        match self.shell {
            Shell::Fish => Ok(format!(r"test\s*\-f\s*{path};\s*and\s+builtin\s+source\s+{path}")),
            _ => Ok(format!(r"\[\[\s*\-f\s*{path}\s*\]\]\s*&&\s*builtin\s+source\s*{path}")),
        }
    }

    fn source_text(&self, when: When) -> Result<String> {
        let home = directories::home_dir()?;
        let integration_path = self.script_integration(when)?.path;
        let path = format!("\"${{HOME}}/{}\"", integration_path.strip_prefix(home)?.display());

        match self.shell {
            Shell::Fish => Ok(format!("test -f {path}; and builtin source {path}")),
            _ => Ok(format!("[[ -f {path} ]] && builtin source {path}")),
        }
    }

    fn source_regex(&self, when: When, constrain_position: bool) -> Result<Regex> {
        let regex = format!(
            r#"{}(?:{}\n)?{}\n{{0,2}}{}"#,
            if constrain_position && when == When::Pre {
                "^"
            } else {
                ""
            },
            regex::escape(&self.description(when)),
            regex::escape(&self.source_text(when)?),
            if constrain_position && when == When::Post {
                "$"
            } else {
                ""
            },
        );
        Ok(Regex::new(&regex)?)
    }

    fn remove_from_text(&self, text: impl Into<String>, when: When) -> Result<String> {
        let source_regex = self.source_regex(when, false)?;
        let mut regexes = vec![source_regex];
        regexes.extend(
            self.legacy_regexes(when)?
                .patterns()
                .iter()
                .map(|r| Regex::new(r).unwrap()),
        );
        Ok(regexes
            .iter()
            .fold::<String, _>(text.into(), |acc, reg| reg.replace_all(&acc, "").into()))
    }

    fn matches_text(&self, text: &str, when: When) -> Result<()> {
        let dotfile = self.dotfile_path();
        if self.legacy_regexes(when)?.is_match(text) {
            let message = format!("{} has legacy {} integration.", dotfile.display(), when);
            return Err(Error::LegacyInstallation(message.into()));
        }
        if !self.source_regex(when, false)?.is_match(text) {
            let message = format!("{} does not source {} integration", dotfile.display(), when);
            return Err(Error::NotInstalled(message.into()));
        }
        if !self.source_regex(when, true)?.is_match(text) {
            let position = match when {
                When::Pre => "first",
                When::Post => "last",
            };
            let message = format!(
                "{} does not source {} integration {}",
                dotfile.display(),
                when,
                position
            );
            return Err(Error::ImproperInstallation(message.into()));
        }
        Ok(())
    }

    fn old_brand_regex(&self, when: When) -> Result<String> {
        Ok(format!(
            r#"(?m)(?:\s*{}\s*\n)?^\s*{}\s*\n{{0,2}}"#,
            regex::escape(&DotfileShellIntegration::legacy_description(when)),
            self.legacy_source_text_3(when)?,
        ))
    }

    async fn install_inner(&self) -> Result<()> {
        let dotfile = self.dotfile_path();
        let mut contents = if dotfile.exists() {
            backup_file(&dotfile, fig_util::directories::utc_backup_dir().ok())?;
            self.uninstall().await?;
            std::fs::read_to_string(&dotfile)?
        } else {
            String::new()
        };

        let original_contents = contents.clone();

        if self.pre {
            self.script_integration(When::Pre)?.install().await?;
            let (shebang, post_shebang) = split_shebang(&contents);
            contents = format!(
                "{}{}\n{}\n{}",
                shebang,
                self.description(When::Pre),
                self.source_text(When::Pre)?,
                post_shebang,
            );
        }

        if self.post {
            self.script_integration(When::Post)?.install().await?;
            contents = format!(
                "{}\n{}\n{}\n",
                contents,
                self.description(When::Post),
                self.source_text(When::Post)?,
            );
        }

        if contents.ne(&original_contents) {
            let mut file = File::create(&dotfile).with_path(self.path())?;
            file.write_all(contents.as_bytes())?;
        }
        Ok(())
    }
}

#[async_trait]
impl Integration for DotfileShellIntegration {
    fn describe(&self) -> String {
        format!(
            "{}{}{} into {}",
            self.shell,
            if self.pre { " pre" } else { "" },
            if self.post { " post" } else { "" },
            self.dotfile_name,
        )
    }

    async fn install(&self) -> Result<()> {
        if self.is_installed().await.is_ok() {
            return Ok(());
        }
        self.install_inner().await?;
        Ok(())
    }

    async fn uninstall(&self) -> Result<()> {
        let dotfile = self.dotfile_path();
        if dotfile.exists() {
            let mut contents = std::fs::read_to_string(&dotfile)?;

            // Remove comment lines
            contents = Regex::new(r"(?mi)^#.*fig.*var.*$\n?")?
                .replace_all(&contents, "")
                .into();

            contents = Regex::new(r"(?mi)^#.*Please make sure this block is at the .* of this file.*$\n?")?
                .replace_all(&contents, "")
                .into();

            if self.pre {
                contents = self.remove_from_text(&contents, When::Pre)?;
            }

            if self.post {
                contents = self.remove_from_text(&contents, When::Post)?;
            }

            contents = contents.trim().to_string();
            contents.push('\n');

            std::fs::write(&dotfile, contents.as_bytes()).with_path(self.path())?;
        }

        if self.pre {
            self.script_integration(When::Pre)?.uninstall().await?;
        }

        if self.post {
            self.script_integration(When::Post)?.uninstall().await?;
        }

        Ok(())
    }

    async fn is_installed(&self) -> Result<()> {
        let dotfile = self.dotfile_path();

        let filtered_contents: String = match std::fs::read_to_string(&dotfile).with_path(&dotfile) {
            // Remove comments and empty lines.
            Ok(contents) => {
                // Check for existence of ignore flag
                if Regex::new(r"(?mi)^\s*#\s*fig ignore\s?.*$")
                    .unwrap()
                    .is_match(&contents)
                {
                    return Ok(());
                }

                Regex::new(r"(?m)^\s*(#.*)?\n")
                    .unwrap()
                    .replace_all(&contents, "")
                    .into()
            },
            Err(Error::Io(err)) if err.kind() == ErrorKind::NotFound => {
                return Err(Error::FileDoesNotExist(dotfile.into()));
            },
            Err(err) => return Err(err),
        };

        let filtered_contents = filtered_contents.trim();

        if self.pre {
            self.matches_text(filtered_contents, When::Pre)?;
            self.script_integration(When::Pre)?.is_installed().await?;
        }

        if self.post {
            self.matches_text(filtered_contents, When::Post)?;
            self.script_integration(When::Post)?.is_installed().await?;
        }

        Ok(())
    }

    async fn migrate(&self) -> Result<()> {
        match self.is_installed().await {
            Ok(_) => Ok(()),
            Err(Error::LegacyInstallation(_)) => {
                self.install_inner().await?;
                Ok(())
            },
            Err(err) => Err(err),
        }
    }
}

impl ShellIntegration for DotfileShellIntegration {
    fn get_shell(&self) -> Shell {
        self.shell
    }

    fn path(&self) -> PathBuf {
        self.dotfile_path()
    }

    fn file_name(&self) -> &str {
        self.dotfile_name
    }
}

/// Splits the line containing the shebang (if any) with the rest of the string.
/// If the shebang exists, the newline is included. Otherwise, an empty slice is returned.
fn split_shebang(contents: &str) -> (&str, &str) {
    if contents.starts_with("#!") {
        match contents.find('\n') {
            Some(i) => (&contents[..i + 1], &contents[i + 1..]),
            None => ("", contents),
        }
    } else {
        ("", contents)
    }
}

#[cfg(test)]
mod test {
    use std::io::Write;
    use std::process::{
        Command,
        Stdio,
    };

    use fig_util::build::SKIP_SHELLCHECK_TESTS;
    use fig_util::directories::{
        home_dir,
        old_fig_data_dir,
    };

    use super::*;

    fn run_shellcheck(source: String) {
        if SKIP_SHELLCHECK_TESTS {
            return;
        }

        let shell_arg = "--shell=bash";
        let mut child = Command::new("shellcheck")
            .args([shell_arg, "--color=always", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        let mut stdin = child.stdin.take().unwrap();
        std::thread::spawn(move || {
            stdin.write_all(source.as_bytes()).unwrap();
        });

        let output = child.wait_with_output().unwrap();
        if !output.status.success() {
            let stdout = String::from_utf8(output.stdout).unwrap();
            let stderr = String::from_utf8(output.stderr).unwrap();

            if !stdout.is_empty() {
                println!("{stdout}");
            }

            if !stderr.is_empty() {
                eprintln!("{stderr}");
            }

            if stdout.contains("error") {
                panic!();
            }
        }
    }

    fn check_script(shell: Shell, when: When) {
        run_shellcheck(shell.get_fig_integration_source(&when).to_owned());
    }

    #[test]
    fn shellcheck_bash_pre() {
        check_script(Shell::Bash, When::Pre);
    }

    #[test]
    fn shellcheck_bash_post() {
        check_script(Shell::Bash, When::Post);
    }

    #[test]
    fn test_legacy_codewhisperer_regex() {
        let re = regex::Regex::new(
            &DotfileShellIntegration {
                pre: true,
                post: true,
                shell: Shell::Zsh,
                dotfile_directory: "".into(),
                dotfile_name: ".zshrc",
            }
            .old_brand_regex(When::Pre)
            .unwrap(),
        )
        .unwrap();

        println!("re: {re}");

        let data_dir = old_fig_data_dir().unwrap();
        let dir = data_dir.strip_prefix(home_dir().unwrap()).unwrap().display();

        // base case
        let doc = &indoc::formatdoc! {r#"
            # CodeWhisperer pre block. Keep at the top of this file.
            [[ -f "${{HOME}}/{dir}/shell/zshrc.pre.zsh" ]] && builtin source "${{HOME}}/{dir}/shell/zshrc.pre.zsh"
        "#};
        let replaced = re.replace_all(doc, "");
        assert_eq!(replaced, "");

        // different comment case
        let doc = indoc::formatdoc! {r#"
            # different comment
            [[ -f "${{HOME}}/{dir}/shell/zshrc.pre.zsh" ]] && builtin source "${{HOME}}/{dir}/shell/zshrc.pre.zsh"
        "#};
        let replaced = re.replace_all(&doc, "");
        assert_eq!(replaced, "# different comment\n");

        // spaces in command
        let doc = indoc::formatdoc! {r#"
            [[  -f  "${{HOME}}/{dir}/shell/zshrc.pre.zsh"  ]]  &&    builtin  source  "${{HOME}}/{dir}/shell/zshrc.pre.zsh" 
        "#};
        let replaced = re.replace_all(&doc, "");
        assert_eq!(replaced, "");

        // non match which looks similar
        let doc = indoc::formatdoc! {r#"
            [[ -f "${{HOME}}/{dir}/shell/zshrc.pre.zsh" ]] && source "${{HOME}}/{dir}/shell/zshrc.pre.zsh"
            
            # CodeWhisperer pre block. Keep at the top of this file.
            [[ -f ${{HOME}}/shell/zshrc.pre.zsh" ]] && builtin source "${{HOME}}/shell/zshrc.pre.zsh"
        "#};
        let replaced = re.replace_all(&doc, "");
        assert_eq!(replaced, doc);

        // multiple lines
        let doc = indoc::formatdoc! {r#"
            # CodeWhisperer pre block. Keep at the top of this file.
            [[ -f "${{HOME}}/{dir}/shell/zshrc.pre.zsh" ]] && builtin source "${{HOME}}/{dir}/shell/zshrc.pre.zsh"
            [[ -f "${{HOME}}/{dir}/shell/zshrc.pre.zsh" ]] && builtin source "${{HOME}}/{dir}/shell/zshrc.pre.zsh"

            [[ -f "${{HOME}}/{dir}/shell/zshrc.pre.zsh" ]] && builtin source "${{HOME}}/{dir}/shell/zshrc.pre.zsh"
        "#};
        let replaced = re.replace_all(&doc, "");
        assert_eq!(replaced, "");
    }

    #[test]
    fn test_split_shebang() {
        let shebang = "#!/usr/bin/env sh";
        let contents = "echo hello world";
        let with_shebang = format!("{}\n{}", shebang, contents);
        let with_shebang_no_lf = format!("{}{}", shebang, contents);
        let without_shebang = contents;
        assert_eq!(
            (format!("{shebang}\n").as_str(), contents),
            split_shebang(&with_shebang),
            "split with shebang and linefeed"
        );
        assert_eq!(
            ("", format!("{shebang}{contents}").as_str()),
            split_shebang(&with_shebang_no_lf),
            "split with shebang and no linefeed"
        );
        assert_eq!(("", contents), split_shebang(without_shebang), "split with no shebang");
    }

    #[cfg(target_os = "linux")]
    fn all_dotfile_shell_integrations() -> Vec<ShellScriptShellIntegration> {
        Shell::all()
            .iter()
            .flat_map(|shell| shell.get_script_integrations().unwrap())
            .collect()
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn dotfile_shell_integrations_snapshot() {
        for integration in all_dotfile_shell_integrations() {
            let integration_name = format!(
                "{} {}",
                integration.describe(),
                integration.path.file_name().unwrap().to_str().unwrap()
            )
            .replace(' ', "_");
            insta::assert_snapshot!(integration_name, integration.get_contents());
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn dotfile_shell_integrations_shellcheck() {
        for integration in all_dotfile_shell_integrations() {
            run_shellcheck(integration.get_contents());
        }
    }
}
