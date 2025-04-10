mod fix_permissions;

use std::fmt::Write as _;
use std::io::{
    Read,
    Write as _,
};
use std::path::Path;
use std::process::{
    Command,
    ExitCode,
};
use std::str::FromStr;

use anstream::{
    eprintln,
    println,
};
use clap::{
    Subcommand,
    ValueEnum,
};
use crossterm::ExecutableCommand;
use crossterm::style::Stylize;
use crossterm::terminal::{
    disable_raw_mode,
    enable_raw_mode,
};
use eyre::{
    Context,
    ContextCompat,
    Result,
    bail,
};
use fig_ipc::local::{
    devtools_command,
    prompt_accessibility_command,
    set_debug_mode,
    toggle_debug_mode,
};
use fig_os_shim::Env;
use fig_util::consts::APP_BUNDLE_ID;
use fig_util::env_var::Q_DEBUG_SHELL;
use fig_util::macos::BUNDLE_CONTENTS_MACOS_PATH;
use fig_util::manifest::FileType;
use fig_util::{
    APP_BUNDLE_NAME,
    CLI_BINARY_NAME,
    PRODUCT_NAME,
    PTY_BINARY_NAME,
    Shell,
    directories,
};
use owo_colors::OwoColorize;
use tempfile::{
    NamedTempFile,
    TempDir,
};
use tracing::error;

use crate::cli::launch_fig_desktop;
use crate::util::desktop::LaunchArgs;
use crate::util::{
    get_app_info,
    glob,
    glob_dir,
    quit_fig,
};

#[derive(Debug, ValueEnum, Clone, PartialEq, Eq)]
pub enum Build {
    Production,
    #[value(alias = "staging")]
    Beta,
    #[value(hide = true, alias = "dev")]
    Develop,
}

impl std::fmt::Display for Build {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Build::Production => f.write_str("production"),
            Build::Beta => f.write_str("beta"),
            Build::Develop => f.write_str("develop"),
        }
    }
}

#[derive(Debug, ValueEnum, Clone, PartialEq, Eq)]
pub enum App {
    Dashboard,
    Autocomplete,
}

impl std::fmt::Display for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            App::Dashboard => f.write_str("dashboard"),
            App::Autocomplete => f.write_str("autocomplete"),
        }
    }
}

#[derive(Debug, ValueEnum, Clone, PartialEq, Eq)]
pub enum AutocompleteWindowDebug {
    On,
    Off,
}

#[derive(Debug, ValueEnum, Clone, PartialEq, Eq)]
pub enum AccessibilityAction {
    Refresh,
    Reset,
    Prompt,
    Open,
    Status,
}

#[cfg(target_os = "macos")]
use fig_integrations::{
    Integration,
    input_method::InputMethod,
};

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, PartialEq, Eq, ValueEnum)]
pub enum TISAction {
    Enable,
    Disable,
    Select,
    Deselect,
}

#[cfg(target_os = "macos")]
use std::path::PathBuf;

use super::diagnostics::get_diagnostics;

#[cfg(target_os = "macos")]
#[derive(Debug, Subcommand, Clone, PartialEq, Eq)]
pub enum InputMethodDebugAction {
    Install {
        bundle_path: Option<PathBuf>,
    },
    Uninstall {
        bundle_path: Option<PathBuf>,
    },
    List,
    Status {
        bundle_path: Option<PathBuf>,
    },
    Source {
        bundle_identifier: String,
        #[arg(value_enum)]
        action: TISAction,
    },
}

#[derive(Debug, PartialEq, Subcommand)]
pub enum DebugSubcommand {
    /// Debug the app
    App,
    /// Switch to another branch of a Fig.js app
    Build {
        #[arg(value_enum)]
        app: App,
        #[arg(value_enum)]
        build: Option<Build>,
    },
    /// Toggle/set autocomplete window debug mode
    AutocompleteWindow {
        #[arg(value_enum)]
        mode: Option<AutocompleteWindowDebug>,
    },
    /// Show debug logs
    Logs {
        #[arg(long)]
        level: Option<String>,
        files: Vec<String>,
    },
    /// Input method debugger
    #[cfg(target_os = "macos")]
    InputMethod {
        #[command(subcommand)]
        action: Option<InputMethodDebugAction>,
    },
    /// Prompt accessibility
    PromptAccessibility,
    /// Sample desktop process
    Sample,
    /// Debug application codesigning
    #[cfg(target_os = "macos")]
    VerifyCodesign,
    /// Accessibility
    Accessibility {
        #[arg(value_enum)]
        action: Option<AccessibilityAction>,
    },
    /// Key Tester
    KeyTester,
    /// Watches diagnostics
    Diagnostics {
        #[arg(long)]
        watch: bool,
        #[arg(long, requires("watch"), default_value_t = 0.25)]
        rate: f64,
    },
    /// Queries remote repository for updates given the specified metadata
    QueryIndex {
        #[arg(short, long)]
        channel: String,
        #[arg(short, long)]
        target_triple: String,
        #[arg(short = 'V', long)]
        variant: String,
        #[arg(short = 'e', long)]
        version: String,
        #[arg(short = 'r', long)]
        enable_rollout: bool,
        #[arg(short, long)]
        override_threshold: Option<u8>,
        #[arg(short, long)]
        file_type: String,
    },
    /// Open up the devtools of a specific webview
    Devtools {
        app: App,
    },
    /// Displays remote index
    GetIndex {
        channel: String,
        #[arg(short, long, default_value = "false")]
        /// Display using debug formatting
        debug: bool,
    },
    /// Lists installed IntelliJ variants
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[command(name = "list-intellij-variants")]
    ListIntelliJVariants,
    /// Disables sourcing of user shell config and instead uses a minimal shell config
    Shell,
    /// Update the shell config permissions to have the correct owner and access rights
    FixPermissions,
    RefreshAuthToken,
}

impl DebugSubcommand {
    pub async fn execute(&self) -> Result<ExitCode> {
        let env = Env::new();
        match self {
            DebugSubcommand::App => {
                if !cfg!(target_os = "macos") {
                    bail!("app is only supported on macOS");
                }

                let app_info = get_app_info().unwrap_or_else(|_| "".into());
                if app_info.is_empty() {
                    println!("{PRODUCT_NAME} is not currently running. Attempting to start...");
                    if Command::new("open")
                        .args(["-g", "-b", APP_BUNDLE_ID])
                        .spawn()?
                        .wait()
                        .is_err()
                    {
                        bail!("Could not start {PRODUCT_NAME}");
                    }
                }
                let fig_path = crate::util::get_running_app_info(APP_BUNDLE_ID, "bundlepath")?;
                let front_app = Command::new("lsappinfo").arg("front").output()?;
                let terminal_name = String::from_utf8(front_app.stdout)
                    .ok()
                    .and_then(|app| crate::util::get_running_app_info(app, "name").ok());
                let terminal_text = match terminal_name {
                    Some(terminal) => format!(" ({terminal})"),
                    None => "".into(),
                };

                println!("Running the {APP_BUNDLE_NAME} executable directly from {fig_path}.");
                println!("You will need to grant accessibility permissions to the current terminal{terminal_text}!");

                Command::new(
                    Path::new(&fig_path)
                        .join(BUNDLE_CONTENTS_MACOS_PATH)
                        .join(CLI_BINARY_NAME),
                )
                .spawn()?
                .wait()?;
            },
            DebugSubcommand::Build { build, app } => match build {
                Some(build) => {
                    fig_settings::settings::set_value(format!("developer.{app}.build"), match build {
                        Build::Production => serde_json::Value::Null,
                        Build::Beta => "beta".into(),
                        Build::Develop => "develop".into(),
                    })?;
                    println!(
                        "{PRODUCT_NAME} will now use the {} build of {}",
                        build.magenta(),
                        app.magenta()
                    );
                },
                None => {
                    let current_build = fig_settings::settings::get_string_opt(format!("developer.{app}.build"));
                    let current_build = match current_build.as_deref() {
                        Some("staging" | "beta") => Build::Beta,
                        Some("develop" | "dev") => Build::Develop,
                        _ => Build::Production,
                    };
                    println!("{current_build}");
                },
            },
            DebugSubcommand::AutocompleteWindow { mode } => {
                let result = match mode {
                    Some(AutocompleteWindowDebug::On) => set_debug_mode(true).await,
                    Some(AutocompleteWindowDebug::Off) => set_debug_mode(false).await,
                    None => toggle_debug_mode().await,
                };
                if result.is_err() {
                    println!("Could not update debug mode");
                    return result.map(|_| ExitCode::SUCCESS).map_err(eyre::Report::from);
                }
            },
            DebugSubcommand::Logs { level, files } => {
                let level = std::sync::Arc::new(level.clone());
                let files = std::sync::Arc::new(files.clone());

                fig_settings::state::set_value("developer.logging", true)?;

                if files.is_empty() || files.iter().any(|f| f == "fig_desktop") {
                    if let Err(err) =
                        fig_ipc::local::set_log_level(level.as_ref().clone().unwrap_or_else(|| "DEBUG".into())).await
                    {
                        println!("Could not set log level for fig_desktop: {err}");
                    }
                }

                tokio::spawn(async move {
                    tokio::signal::ctrl_c().await.unwrap();
                    let code = match fig_settings::state::set_value("developer.logging", false) {
                        Ok(_) => 0,
                        Err(_) => 1,
                    };

                    if let Err(err) = fig_ipc::local::set_log_level("INFO".into()).await {
                        println!("Could not restore log level for fig_desktop: {err}");
                    }

                    std::process::exit(code);
                });

                let logs_dir = directories::logs_dir()?;

                let log_paths = if files.is_empty() {
                    let pattern = logs_dir.join("*.log");
                    let globset = glob([pattern.to_str().unwrap()])?;
                    glob_dir(&globset, &logs_dir)?
                } else {
                    let mut files = files.as_ref().clone();
                    let mut paths = Vec::new();

                    if files.iter().any(|f| f == PTY_BINARY_NAME) {
                        // Remove figterm from the list of files to open
                        files.retain(|f| f != PTY_BINARY_NAME);

                        // Add figterm*.log to the list of files to open
                        let pattern = logs_dir.join(format!("{PTY_BINARY_NAME}*.log"));
                        let globset = glob([pattern.to_str().unwrap()])?;
                        let qterm_logs = glob_dir(&globset, &logs_dir)?;
                        paths.extend(qterm_logs);
                    }

                    // Push any remaining files to open
                    paths.extend(files.iter().map(|file| logs_dir.join(format!("{file}.log"))));

                    paths
                };

                Command::new("tail")
                    .arg("-n0")
                    .arg("-qf")
                    .args(log_paths)
                    .spawn()?
                    .wait()?;
            },
            #[cfg(target_os = "macos")]
            DebugSubcommand::InputMethod { action } => {
                let action = match action {
                    Some(action) => action,
                    None => &InputMethodDebugAction::Status { bundle_path: None },
                };

                match action {
                    InputMethodDebugAction::Install { bundle_path } => {
                        let input_method = match bundle_path {
                            Some(bundle_path) => {
                                let bundle_path = if bundle_path.is_relative() {
                                    let mut path = std::env::current_dir()?;
                                    path.push(bundle_path);
                                    path
                                } else {
                                    bundle_path.clone()
                                };

                                InputMethod { bundle_path }
                            },
                            None => InputMethod::default(),
                        };

                        input_method.install().await?;

                        println!(
                            "Successfully installed input method '{}'",
                            input_method.bundle_id().unwrap()
                        );
                    },
                    InputMethodDebugAction::Uninstall { bundle_path } => {
                        let input_method = match bundle_path {
                            Some(bundle_path) => {
                                let bundle_path = if bundle_path.is_relative() {
                                    let mut path = std::env::current_dir()?;
                                    path.push(bundle_path);
                                    path
                                } else {
                                    bundle_path.clone()
                                };

                                InputMethod { bundle_path }
                            },
                            None => InputMethod::default(),
                        };

                        input_method.uninstall().await?;

                        println!(
                            "Successfully uninstalled input method '{}'",
                            input_method.bundle_id().unwrap()
                        );
                    },
                    InputMethodDebugAction::List => match InputMethod::list_all_input_sources(None, true) {
                        Some(sources) => sources.iter().for_each(|source| println!("{source:#?}")),
                        None => return Err(eyre::eyre!("Could not load input sources")),
                    },
                    InputMethodDebugAction::Status { bundle_path } => {
                        let input_method = match bundle_path {
                            Some(bundle_path) => {
                                let bundle_path = if bundle_path.is_relative() {
                                    let mut path = std::env::current_dir()?;
                                    path.push(bundle_path);
                                    path
                                } else {
                                    bundle_path.clone()
                                };

                                InputMethod { bundle_path }
                            },
                            None => InputMethod::default(),
                        };

                        println!("Installed? {}", input_method.is_installed().await.is_ok());
                        println!("{:#?}", input_method.input_source()?);
                    },
                    InputMethodDebugAction::Source {
                        bundle_identifier,
                        action,
                    } => {
                        return match InputMethod::list_input_sources_for_bundle_id(bundle_identifier.as_str()) {
                            Some(sources) => {
                                sources
                                    .into_iter()
                                    .map(|source| match action {
                                        TISAction::Enable => source.enable(),
                                        TISAction::Disable => source.disable(),
                                        TISAction::Select => source.select(),
                                        TISAction::Deselect => source.deselect(),
                                    })
                                    .collect::<Result<Vec<()>, fig_integrations::input_method::InputMethodError>>()?;
                                Ok(ExitCode::SUCCESS)
                            },
                            None => return Err(eyre::eyre!("Could not find an input source with this identifier")),
                        };
                    },
                }
            },
            DebugSubcommand::PromptAccessibility => {
                if !cfg!(target_os = "macos") {
                    bail!("prompt-accessibility is only supported on macOS");
                }

                prompt_accessibility_command()
                    .await
                    .context("Could not prompt for accessibility permissions.")?;
            },
            DebugSubcommand::Sample => {
                if !cfg!(target_os = "macos") {
                    bail!("sample is only supported on macOS");
                }

                let output = Command::new("lsappinfo")
                    .args(["info", "-only", "-pid", "-app", APP_BUNDLE_ID])
                    .output()?;
                let pid_str = String::from_utf8(output.stdout)?;
                let pid = pid_str
                    .split('=')
                    .nth(1)
                    .context("Could not get desktop app pid")?
                    .trim();
                let outfile = Path::new("/tmp").join("fig-sample");

                println!(
                    "Sampling desktop app ({}). Writing output to {}",
                    pid,
                    outfile.display()
                );
                let result = Command::new("sample")
                    .arg("-f")
                    .arg::<&Path>(outfile.as_ref())
                    .arg(pid)
                    .spawn()?
                    .wait();
                if result.is_err() {
                    println!("Could not sample desktop app.");
                    eyre::bail!("Failed to sample desktop app.");
                }
                println!("\n\n\n-------\nFinished writing to {}", outfile.display());
                println!("Please send this file to the development team");
                println!("Or attach it to a Github issue (run '{}')", "fig issue".magenta());
            },
            #[cfg(target_os = "macos")]
            DebugSubcommand::VerifyCodesign => {
                Command::new("codesign")
                    .arg("-vvvv")
                    .arg(fig_util::app_bundle_path())
                    .spawn()?
                    .wait()?;
            },
            DebugSubcommand::Accessibility { action } => {
                if !cfg!(target_os = "macos") {
                    bail!("accessibility is only supported on macOS");
                }

                match action {
                    Some(AccessibilityAction::Refresh) => {
                        quit_fig(true).await?;

                        Command::new("tccutil")
                            .args(["reset", "Accessibility", APP_BUNDLE_ID])
                            .spawn()?
                            .wait()?;

                        launch_fig_desktop(LaunchArgs {
                            wait_for_socket: true,
                            open_dashboard: false,
                            immediate_update: true,
                            verbose: true,
                        })?;
                    },
                    Some(AccessibilityAction::Reset) => {
                        quit_fig(true).await?;

                        Command::new("tccutil")
                            .args(["reset", "Accessibility", APP_BUNDLE_ID])
                            .spawn()?
                            .wait()?;
                    },
                    Some(AccessibilityAction::Prompt) => {
                        launch_fig_desktop(LaunchArgs {
                            wait_for_socket: true,
                            open_dashboard: false,
                            immediate_update: true,
                            verbose: true,
                        })?;

                        prompt_accessibility_command()
                            .await
                            .context("Could not prompt for accessibility permissions.")?;
                    },
                    Some(AccessibilityAction::Open) => {
                        Command::new("open")
                            .args(["x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"])
                            .spawn()?
                            .wait()?;
                    },
                    Some(AccessibilityAction::Status) | None => {
                        cfg_if::cfg_if! {
                            if #[cfg(target_os = "macos")] {
                                let diagnostic = get_diagnostics().await?;
                                println!("Accessibility Enabled: {}", diagnostic.accessibility);
                            } else {
                                println!("Unable to get accessibility status on this platform");
                            }
                        }
                    },
                }
            },
            DebugSubcommand::KeyTester => {
                println!("{} (use {} to quit)", "Testing Key Input".bold(), "ctrl-d".magenta());

                enable_raw_mode()?;

                let mut stdout = std::io::stdout();
                let mut stdin = std::io::stdin();

                loop {
                    let mut buff = [0; 1024];
                    let bytes = stdin.read(&mut buff)?;
                    let input = &buff[0..bytes];

                    stdout.execute(crossterm::style::Print(format!(
                        "{bytes} bytes : \"{}\" : {:x?}",
                        input.escape_ascii(),
                        input
                    )))?;

                    let (_, rows) = crossterm::terminal::size()?;
                    let (_, cursor_row) = crossterm::cursor::position()?;
                    if cursor_row >= rows.saturating_sub(1) {
                        stdout.execute(crossterm::terminal::ScrollUp(1))?;
                    }
                    stdout.execute(crossterm::cursor::MoveToNextLine(1))?;

                    // ctrl-d
                    if [4] == input {
                        break;
                    }
                }

                disable_raw_mode()?;
                println!("ctrl-d");
            },
            DebugSubcommand::Diagnostics { watch, rate } => {
                if *watch {
                    crossterm::execute!(
                        std::io::stdout(),
                        crossterm::terminal::EnterAlternateScreen,
                        crossterm::cursor::Hide,
                    )?;

                    tokio::spawn(async {
                        tokio::signal::ctrl_c().await.unwrap();
                        crossterm::execute!(
                            std::io::stdout(),
                            crossterm::terminal::LeaveAlternateScreen,
                            crossterm::cursor::Show,
                        )
                        .unwrap();
                        std::process::exit(0);
                    });
                }

                loop {
                    let diagnostic = get_diagnostics().await?;
                    let term_width = crossterm::terminal::size().unwrap().0 as usize;

                    let mut out = String::new();

                    let edit_buffer = diagnostic.edit_buffer_string.as_deref().map(|s| {
                        let mut s = s.to_owned();
                        if let Some(index) = diagnostic.edit_buffer_cursor {
                            s.insert_str(index as usize, &"│".magenta().to_string());
                        }
                        s = s.replace('\n', "\\n");
                        s = s.replace('\t', "\\t");
                        s = s.replace('\r', "\\r");
                        s.trim().to_string()
                    });

                    writeln!(&mut out, "{}", "Edit Buffer".bold())?;
                    writeln!(&mut out, "{}", "━".repeat(term_width))?;

                    if diagnostic.shell_context.as_ref().is_some_and(|c| c.preexec()) {
                        writeln!(&mut out, "{}", "<Running Process>".dim())?;
                    } else {
                        writeln!(&mut out, "{}", edit_buffer.unwrap_or_else(|| "None".into()))?;
                    }

                    writeln!(&mut out, "{}", "━".repeat(term_width))?;

                    writeln!(&mut out)?;

                    if let Some(shell_context) = &diagnostic.shell_context {
                        writeln!(&mut out, "{}", "Shell Context".bold())?;
                        writeln!(&mut out, "{}", "━".repeat(term_width))?;
                        writeln!(
                            &mut out,
                            "Session ID: {}",
                            shell_context.session_id.as_deref().unwrap_or("None")
                        )?;
                        writeln!(
                            &mut out,
                            "Process Name: {}",
                            shell_context.process_name.as_deref().unwrap_or("None")
                        )?;
                        writeln!(
                            &mut out,
                            "Current Working Directory: {}",
                            shell_context.current_working_directory.as_deref().unwrap_or("None")
                        )?;
                        writeln!(&mut out, "TTY: {}", shell_context.ttys.as_deref().unwrap_or("None"))?;
                        writeln!(
                            &mut out,
                            "Preexec: {}",
                            shell_context
                                .preexec
                                .map_or_else(|| "None".to_string(), |s| s.to_string())
                        )?;
                        writeln!(
                            &mut out,
                            "OSCLock: {}",
                            shell_context
                                .osc_lock
                                .map_or_else(|| "None".to_string(), |s| s.to_string())
                        )?;
                    }

                    writeln!(
                        &mut out,
                        "Intercept: {}, Global Intercept: {}",
                        diagnostic.intercept_enabled(),
                        diagnostic.intercept_global_enabled(),
                    )?;

                    if *watch {
                        crossterm::queue!(
                            std::io::stdout(),
                            crossterm::terminal::Clear(crossterm::terminal::ClearType::Purge),
                            crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                            crossterm::cursor::MoveTo(0, 0),
                            crossterm::style::Print(format!(
                                "{PRODUCT_NAME} Diagnostics (use {} to quit)\n\n",
                                "ctrl-c".magenta()
                            )),
                            crossterm::style::Print(out),
                        )?;
                        std::io::stdout().flush()?;
                    } else {
                        println!("{out}");
                    }

                    if !watch {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_secs_f64(*rate)).await;
                }
            },
            DebugSubcommand::QueryIndex {
                channel,
                target_triple,
                variant,
                version: current_version,
                enable_rollout,
                override_threshold,
                file_type,
            } => {
                use fig_util::manifest::{
                    Channel,
                    TargetTriple,
                    Variant,
                };

                let result = fig_install::index::pull(&Channel::from_str(channel)?)
                    .await?
                    .find_next_version(
                        &TargetTriple::from_str(target_triple)?,
                        &Variant::from_str(variant)?,
                        Some(&FileType::from_str(file_type)?),
                        current_version,
                        !enable_rollout,
                        *override_threshold,
                    );

                println!("{result:#?}");
            },
            Self::Devtools { app } => {
                launch_fig_desktop(LaunchArgs {
                    wait_for_socket: true,
                    open_dashboard: false,
                    immediate_update: true,
                    verbose: true,
                })?;

                devtools_command(match app {
                    App::Dashboard => fig_proto::local::devtools_command::Window::DevtoolsDashboard,
                    App::Autocomplete => fig_proto::local::devtools_command::Window::DevtoolsAutocomplete,
                })
                .await
                .context("Could not open devtools window")?;
            },
            DebugSubcommand::GetIndex { channel, debug } => {
                use fig_util::manifest::Channel;
                let index = fig_install::index::pull(&Channel::from_str(channel)?).await?;
                if *debug {
                    println!("{index:#?}");
                } else {
                    let json = serde_json::to_string_pretty(&index)?;
                    println!("{json}");
                }
            },
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            DebugSubcommand::ListIntelliJVariants => {
                for integration in fig_integrations::intellij::variants_installed().await? {
                    println!("{}", integration.variant.application_name());
                    #[cfg(target_os = "macos")]
                    println!("  - {:?}", integration.application_folder());
                }
            },
            DebugSubcommand::Shell => {
                let mut profile = NamedTempFile::new()?;
                let tmp_dir = TempDir::new()?;

                let mut command = Command::new(PTY_BINARY_NAME);
                command.env(Q_DEBUG_SHELL, "1").arg("--");

                match Shell::current_shell() {
                    Some(shell) => {
                        let mut command = match shell {
                            Shell::Bash => {
                                writeln!(profile, "eval \"$({CLI_BINARY_NAME} init bash post)\"")?;
                                command
                                    .args(["bash", "--noprofile", "--norc", "--rcfile"])
                                    .arg(profile.path());
                                command
                            },
                            Shell::Zsh => {
                                std::fs::write(
                                    tmp_dir.path().join(".zshrc"),
                                    format!("eval \"$({CLI_BINARY_NAME} init zsh post)\""),
                                )
                                .unwrap();

                                command.args(["zsh"]).env("ZDOTDIR", tmp_dir.path());
                                command
                            },
                            Shell::Fish => {
                                command.args([
                                    "fish",
                                    "--no-config",
                                    "-C",
                                    &format!("{CLI_BINARY_NAME} init fish post | source"),
                                ]);
                                command
                            },
                            Shell::Nu => eyre::bail!("Unsupported shell for debug"),
                        };

                        println!("Starting {PRODUCT_NAME} debug shell");
                        println!();

                        profile.as_file().sync_all()?;
                        let mut output = command.spawn()?;
                        if !output.wait()?.success() {
                            panic!();
                        }

                        println!();
                        println!("Ending {PRODUCT_NAME} debug shell");
                        println!();
                    },
                    None => error!("Could not determine current shell or shell not supported"),
                }
            },
            DebugSubcommand::FixPermissions => {
                fix_permissions::fix_permissions(&env)?;
            },
            DebugSubcommand::RefreshAuthToken => match fig_auth::refresh_token().await? {
                Some(_) => eprintln!("Refreshed token"),
                None => {
                    eprintln!("No token to refresh");
                    return Ok(ExitCode::FAILURE);
                },
            },
        }
        Ok(ExitCode::SUCCESS)
    }
}
