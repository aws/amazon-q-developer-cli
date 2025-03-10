mod cli_context;
pub mod desktop;
pub mod pid_file;
mod region_check;
pub mod spinner;
pub mod stylize_diff;

use std::env;
use std::ffi::OsStr;
use std::fmt::Display;
use std::io::{
    ErrorKind,
    stdout,
};
use std::path::{
    Path,
    PathBuf,
};
use std::process::{
    Command,
    ExitCode,
};
use std::sync::{
    LazyLock,
    Mutex,
};
use std::time::Duration;

use anstream::println;
use anstream::stream::IsTerminal;
use cfg_if::cfg_if;
pub use cli_context::CliContext;
use crossterm::style::Stylize;
use desktop::desktop_app_running;
use dialoguer::Select;
use dialoguer::theme::ColorfulTheme;
use eyre::{
    Context,
    ContextCompat,
    Result,
    bail,
};
use fig_ipc::local::quit_command;
use fig_util::consts::APP_BUNDLE_ID;
use fig_util::{
    CLI_BINARY_NAME,
    PRODUCT_NAME,
};
use globset::{
    Glob,
    GlobSet,
    GlobSetBuilder,
};
use regex::Regex;
pub use region_check::region_check;
pub use stylize_diff::stylize_diff;
use syntect::highlighting::ThemeSet;
use tracing::warn;

static SET_CTRLC_HANDLER: Mutex<bool> = Mutex::new(false);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

/// Glob patterns against full paths
pub fn glob_dir(glob: &GlobSet, directory: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    // List files in the directory
    let dir = std::fs::read_dir(directory)?;

    for entry in dir {
        let path = entry?.path();

        // Check if the file matches the glob pattern
        if glob.is_match(&path) {
            files.push(path);
        }
    }

    Ok(files)
}

/// Glob patterns against the file name
pub fn glob_files(glob: &GlobSet, directory: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    // List files in the directory
    let dir = std::fs::read_dir(directory)?;

    for entry in dir {
        let entry = entry?;
        let path = entry.path();
        let file_name = path.file_name();

        // Check if the file matches the glob pattern
        if let Some(file_name) = file_name {
            if glob.is_match(file_name) {
                files.push(path);
            }
        }
    }

    Ok(files)
}

pub fn glob<I, S>(patterns: I) -> Result<GlobSet>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(pattern.as_ref())?);
    }
    Ok(builder.build()?)
}

pub fn app_path_from_bundle_id(bundle_id: impl AsRef<OsStr>) -> Option<String> {
    cfg_if! {
        if #[cfg(target_os = "macos")] {
            let installed_apps = std::process::Command::new("mdfind")
                .arg("kMDItemCFBundleIdentifier")
                .arg("=")
                .arg(bundle_id)
                .output()
                .ok()?;

            let path = String::from_utf8_lossy(&installed_apps.stdout);
            Some(path.trim().split('\n').next()?.into())
        } else {
            let _bundle_id = bundle_id;
            None
        }
    }
}

pub async fn quit_fig(verbose: bool) -> Result<ExitCode> {
    if fig_util::system_info::in_cloudshell() {
        bail!("Restarting {PRODUCT_NAME} is not supported in CloudShell");
    }

    if fig_util::system_info::is_remote() {
        bail!("Please restart {PRODUCT_NAME} from your host machine");
    }

    if !desktop_app_running() {
        if verbose {
            println!("{PRODUCT_NAME} app is not running");
        }
        return Ok(ExitCode::SUCCESS);
    }

    if verbose {
        println!("Quitting {PRODUCT_NAME} app");
    }

    if quit_command().await.is_err() {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let second_try = quit_command().await;
        if second_try.is_err() {
            cfg_if! {
                if #[cfg(target_os = "linux")] {
                    use fig_util::APP_PROCESS_NAME;
                    if let Ok(output) = Command::new("killall").arg(APP_PROCESS_NAME).output() {
                        if output.status.success() {
                            return Ok(ExitCode::SUCCESS);
                        }
                    }
                } else if #[cfg(target_os = "macos")] {
                    if let Ok(info) = get_app_info() {
                        let pid = Regex::new(r"pid = (\S+)")
                            .unwrap()
                            .captures(&info)
                            .and_then(|c| c.get(1));
                        if let Some(pid) = pid {
                            let success = Command::new("kill")
                                .arg("-KILL")
                                .arg(pid.as_str())
                                .status()
                                .map(|res| res.success());
                            if let Ok(true) = success {
                                return Ok(ExitCode::SUCCESS);
                            }
                        }
                    }
                } else if #[cfg(target_os = "windows")] {
                    // TODO(chay): Add windows behavior here
                }
            }
            if verbose {
                println!("Unable to quit {PRODUCT_NAME} app");
            }

            second_try?;
        }
    }

    // telem_join.map(|f| async { f.await.ok() });

    Ok(ExitCode::SUCCESS)
}

pub fn is_executable_in_path(program: impl AsRef<Path>) -> bool {
    match env::var_os("PATH") {
        Some(path) => env::split_paths(&path).any(|p| p.join(&program).is_file()),
        _ => false,
    }
}

pub fn app_not_running_message() -> String {
    format!(
        "\n{}\n{PRODUCT_NAME} app might not be running, to launch {PRODUCT_NAME} run: {}\n",
        format!("Unable to connect to {PRODUCT_NAME} app").bold(),
        format!("{CLI_BINARY_NAME} launch").magenta()
    )
}

pub fn login_message() -> String {
    format!(
        "{}\nLooks like you aren't logged in to {PRODUCT_NAME}, to login run: {}",
        "Not logged in".bold(),
        format!("{CLI_BINARY_NAME} login").magenta()
    )
}

pub fn match_regex(regex: impl AsRef<str>, input: impl AsRef<str>) -> Option<String> {
    Some(
        Regex::new(regex.as_ref())
            .unwrap()
            .captures(input.as_ref())?
            .get(1)?
            .as_str()
            .into(),
    )
}

pub fn choose(prompt: impl Display, options: &[impl ToString]) -> Result<Option<usize>> {
    if options.is_empty() {
        bail!("no options passed to choose")
    }

    if !stdout().is_terminal() {
        warn!("called choose while stdout is not a terminal");
        return Ok(Some(0));
    }

    let mut set_ctrlc_handler = SET_CTRLC_HANDLER.lock().expect("SET_CTRLC_HANDLER is poisoned");
    if !*set_ctrlc_handler {
        ctrlc::set_handler(move || {
            let term = dialoguer::console::Term::stdout();
            let _ = term.show_cursor();
        })
        .context("Failed to set ctrlc handler")?;
        *set_ctrlc_handler = true;
    }
    drop(set_ctrlc_handler);

    match Select::with_theme(&dialoguer_theme())
        .items(options)
        .default(0)
        .with_prompt(prompt.to_string())
        .interact_opt()
    {
        Ok(ok) => Ok(ok),
        Err(dialoguer::Error::IO(io)) if io.kind() == ErrorKind::Interrupted => Ok(None),
        Err(e) => Err(e).wrap_err("Failed to choose"),
    }
}

pub fn input(prompt: &str, initial_text: Option<&str>) -> Result<String> {
    if !stdout().is_terminal() {
        warn!("called input while stdout is not a terminal");
        return Ok(String::new());
    }

    let theme = dialoguer_theme();
    let mut input = dialoguer::Input::with_theme(&theme).with_prompt(prompt);

    if let Some(initial_text) = initial_text {
        input = input.with_initial_text(initial_text);
    }

    Ok(input.interact_text()?)
}

pub fn get_running_app_info(bundle_id: impl AsRef<str>, field: impl AsRef<str>) -> Result<String> {
    let info = Command::new("lsappinfo")
        .args(["info", "-only", field.as_ref(), "-app", bundle_id.as_ref()])
        .output()?;
    let info = String::from_utf8(info.stdout)?;
    let value = info
        .split('=')
        .nth(1)
        .context(eyre::eyre!("Could not get field value for {}", field.as_ref()))?
        .replace('"', "");
    Ok(value.trim().into())
}

pub fn get_app_info() -> Result<String> {
    let output = Command::new("lsappinfo")
        .args(["info", "-app", APP_BUNDLE_ID])
        .output()?;
    let result = String::from_utf8(output.stdout)?;
    Ok(result.trim().into())
}

pub fn dialoguer_theme() -> ColorfulTheme {
    ColorfulTheme {
        prompt_prefix: dialoguer::console::style("?".into()).for_stderr().magenta(),
        ..ColorfulTheme::default()
    }
}

#[cfg(target_os = "macos")]
pub async fn is_brew_reinstall() -> bool {
    let regex = regex::bytes::Regex::new(r"brew(\.\w+)?\s+(upgrade|reinstall|install)").unwrap();

    tokio::process::Command::new("ps")
        .args(["aux", "-o", "args"])
        .output()
        .await
        .is_ok_and(|output| regex.is_match(&output.stdout))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use fig_util::APP_PROCESS_NAME;

    use super::*;

    #[test]
    fn regex() {
        let regex_test = |regex: &str, input: &str, expected: Option<&str>| {
            assert_eq!(match_regex(regex, input), expected.map(|s| s.into()));
        };

        regex_test(r"foo=(\S+)", "foo=bar", Some("bar"));
        regex_test(r"foo=(\S+)", "bar=foo", None);
        regex_test(r"foo=(\S+)", "foo=bar baz", Some("bar"));
        regex_test(r"foo=(\S+)", "foo=", None);
    }

    #[test]
    fn exe_path() {
        #[cfg(unix)]
        assert!(is_executable_in_path("cargo"));

        #[cfg(windows)]
        assert!(is_executable_in_path("cargo.exe"));
    }

    #[test]
    fn globs() {
        let set = glob(["*.txt", "*.md"]).unwrap();
        assert!(set.is_match("README.md"));
        assert!(set.is_match("LICENSE.txt"));
    }

    #[ignore]
    #[test]
    fn sysinfo_test() {
        use sysinfo::{
            ProcessRefreshKind,
            RefreshKind,
            System,
        };

        let app_process_name = OsString::from(APP_PROCESS_NAME);
        let system = System::new_with_specifics(RefreshKind::new().with_processes(ProcessRefreshKind::new()));
        cfg_if! {
            if #[cfg(windows)] {
                let mut processes = system.processes_by_name(&app_process_name);
                assert!(processes.next().is_some());
            } else {
                let mut processes = system.processes_by_exact_name(&app_process_name);
                assert!(processes.next().is_some());
            }
        }
    }
}
