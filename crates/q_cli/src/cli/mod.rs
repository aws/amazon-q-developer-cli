//! CLI functionality

pub mod app;
mod chat;
mod completion;
mod debug;
mod diagnostics;
mod doctor;
mod hook;
mod init;
mod inline;
mod installation;
mod integrations;
pub mod internal;
mod issue;
mod settings;
mod telemetry;
mod theme;
mod translate;
mod uninstall;
mod update;
mod user;

use std::io::{
    Write as _,
    stdout,
};
use std::process::ExitCode;

use anstream::{
    eprintln,
    println,
};
use clap::{
    ArgAction,
    CommandFactory,
    Parser,
    Subcommand,
    ValueEnum,
};
use crossterm::style::Stylize;
use eyre::{
    Result,
    WrapErr,
    bail,
};
use fig_auth::is_logged_in;
use fig_ipc::local::open_ui_element;
use fig_log::{
    LogArgs,
    initialize_logging,
};
use fig_proto::local::UiElement;
use fig_util::{
    CLI_BINARY_NAME,
    PRODUCT_NAME,
    directories,
    manifest,
    system_info,
};
use internal::InternalSubcommand;
use serde::Serialize;
use tracing::{
    Level,
    debug,
};

use self::integrations::IntegrationsSubcommands;
use self::user::RootUserSubcommand;
use crate::util::CliContext;
use crate::util::desktop::{
    LaunchArgs,
    launch_fig_desktop,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    /// Outputs the results as markdown
    #[default]
    Plain,
    /// Outputs the results as JSON
    Json,
    /// Outputs the results as pretty print JSON
    JsonPretty,
}

impl OutputFormat {
    pub fn print<T, TFn, J, JFn>(&self, text_fn: TFn, json_fn: JFn)
    where
        T: std::fmt::Display,
        TFn: FnOnce() -> T,
        J: Serialize,
        JFn: FnOnce() -> J,
    {
        match self {
            OutputFormat::Plain => println!("{}", text_fn()),
            OutputFormat::Json => println!("{}", serde_json::to_string(&json_fn()).unwrap()),
            OutputFormat::JsonPretty => println!("{}", serde_json::to_string_pretty(&json_fn()).unwrap()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Processes {
    /// Desktop Process
    App,
}

/// The Amazon Q CLI
#[deny(missing_docs)]
#[derive(Debug, PartialEq, Subcommand)]
pub enum CliRootCommands {
    /// Hook commands
    #[command(subcommand, hide = true)]
    Hook(hook::HookSubcommand),
    /// Debug the app
    #[command(subcommand)]
    Debug(debug::DebugSubcommand),
    /// Customize appearance & behavior
    #[command(alias("setting"))]
    Settings(settings::SettingsArgs),
    /// Setup cli components
    #[command(alias("install"))]
    Setup(internal::InstallArgs),
    /// Uninstall Amazon Q
    #[command(hide = true)]
    Uninstall {
        /// Force uninstall
        #[arg(long, short = 'y')]
        no_confirm: bool,
    },
    /// Update the Amazon Q application
    #[command(alias("upgrade"))]
    Update(update::UpdateArgs),
    /// Run diagnostic tests
    #[command(alias("diagnostics"))]
    Diagnostic(diagnostics::DiagnosticArgs),
    /// Generate the dotfiles for the given shell
    Init(init::InitArgs),
    /// Get or set theme
    Theme(theme::ThemeArgs),
    /// Create a new Github issue
    Issue(issue::IssueArgs),
    /// Root level user subcommands
    #[command(flatten)]
    RootUser(user::RootUserSubcommand),
    /// Manage your account
    #[command(subcommand)]
    User(user::UserSubcommand),
    /// Fix and diagnose common issues
    Doctor(doctor::DoctorArgs),
    /// Generate CLI completion spec
    #[command(hide = true)]
    Completion(completion::CompletionArgs),
    /// Internal subcommands
    #[command(subcommand, hide = true)]
    Internal(internal::InternalSubcommand),
    /// Launch the desktop app
    Launch,
    /// Quit the desktop app
    Quit,
    /// Restart the desktop app
    Restart {
        /// The process to restart
        #[arg(value_enum, default_value_t = Processes::App, hide = true)]
        process: Processes,
    },
    /// Manage system integrations
    #[command(subcommand, alias("integration"))]
    Integrations(IntegrationsSubcommands),
    /// Natural Language to Shell translation
    #[command(alias("ai"))]
    Translate(translate::TranslateArgs),
    /// Enable/disable telemetry
    #[command(subcommand, hide = true)]
    Telemetry(telemetry::TelemetrySubcommand),
    /// Version
    #[command(hide = true)]
    Version,
    /// Open the dashboard
    Dashboard,
    /// AI assistant in your terminal
    #[command(alias("q"))]
    Chat {
        /// Enabling this flag allows the model to execute all commands without first accepting
        /// them.
        #[arg(short, long)]
        accept_all: bool,
        /// The first question to ask
        input: Option<String>,
        /// Context profile to use
        #[arg(long = "profile")]
        profile: Option<String>,
    },
    /// Inline shell completions
    #[command(subcommand)]
    Inline(inline::InlineSubcommand),
}

impl CliRootCommands {
    fn name(&self) -> &'static str {
        match self {
            CliRootCommands::Hook(_) => "hook",
            CliRootCommands::Debug(_) => "debug",
            CliRootCommands::Settings(_) => "settings",
            CliRootCommands::Setup(_) => "setup",
            CliRootCommands::Uninstall { .. } => "uninstall",
            CliRootCommands::Update(_) => "update",
            CliRootCommands::Diagnostic(_) => "diagnostics",
            CliRootCommands::Init(_) => "init",
            CliRootCommands::Theme(_) => "theme",
            CliRootCommands::Issue(_) => "issue",
            CliRootCommands::RootUser(RootUserSubcommand::Login) => "login",
            CliRootCommands::RootUser(RootUserSubcommand::Logout) => "logout",
            CliRootCommands::RootUser(RootUserSubcommand::Whoami { .. }) => "whoami",
            CliRootCommands::User(_) => "user",
            CliRootCommands::Doctor(_) => "doctor",
            CliRootCommands::Completion(_) => "completion",
            CliRootCommands::Internal(_) => "internal",
            CliRootCommands::Launch => "launch",
            CliRootCommands::Quit => "quit",
            CliRootCommands::Restart { .. } => "restart",
            CliRootCommands::Integrations(_) => "integrations",
            CliRootCommands::Translate(_) => "translate",
            CliRootCommands::Telemetry(_) => "telemetry",
            CliRootCommands::Version => "version",
            CliRootCommands::Dashboard => "dashboard",
            CliRootCommands::Chat { .. } => "chat",
            CliRootCommands::Inline(_) => "inline",
        }
    }
}

const HELP_TEXT: &str = color_print::cstr! {"

<magenta,em>q</magenta,em> (Amazon Q CLI)

<magenta,em>Popular Subcommands</magenta,em>              <black!><em>Usage:</em> q [subcommand]</black!>
╭────────────────────────────────────────────────────╮
│ <em>chat</em>         <black!>Chat with Amazon Q</black!>                    │
│ <em>translate</em>    <black!>Natural Language to Shell translation</black!> │
│ <em>doctor</em>       <black!>Debug installation issues</black!>             │ 
│ <em>settings</em>     <black!>Customize appearance & behavior</black!>       │
│ <em>quit</em>         <black!>Quit the app</black!>                          │
╰────────────────────────────────────────────────────╯

<black!>To see all subcommands, use:</black!>
 <black!>❯</black!> q --help-all
ㅤ
"};

#[derive(Debug, Parser, PartialEq, Default)]
#[command(version, about, name = CLI_BINARY_NAME, help_template = HELP_TEXT)]
pub struct Cli {
    #[command(subcommand)]
    pub subcommand: Option<CliRootCommands>,
    /// Increase logging verbosity
    #[arg(long, short = 'v', action = ArgAction::Count, global = true)]
    pub verbose: u8,
    /// Print help for all subcommands
    #[arg(long)]
    help_all: bool,
}

impl Cli {
    pub async fn execute(self) -> Result<ExitCode> {
        // Initialize our logger and keep around the guard so logging can perform as expected.
        let _log_guard = initialize_logging(LogArgs {
            log_level: match self.verbose > 0 {
                true => Some(
                    match self.verbose {
                        1 => Level::WARN,
                        2 => Level::INFO,
                        3 => Level::DEBUG,
                        _ => Level::TRACE,
                    }
                    .to_string(),
                ),
                false => None,
            },
            log_to_stdout: std::env::var_os("Q_LOG_STDOUT").is_some() || self.verbose > 0,
            log_file_path: match self.subcommand {
                Some(CliRootCommands::Chat { .. }) => Some("chat.log".to_owned()),
                Some(CliRootCommands::Translate(..)) => Some("translate.log".to_owned()),
                Some(CliRootCommands::Internal(InternalSubcommand::Multiplexer(_))) => Some("mux.log".to_owned()),
                _ => match fig_log::get_log_level_max() >= Level::DEBUG {
                    true => Some("cli.log".to_owned()),
                    false => None,
                },
            }
            .map(|name| directories::logs_dir().expect("home dir must be set").join(name)),
            delete_old_log_file: false,
        });

        debug!(command =? std::env::args().collect::<Vec<_>>(), "Command ran");

        self.send_telemetry().await;

        if self.help_all {
            return self.print_help_all();
        }

        let cli_context = CliContext::new();

        match self.subcommand {
            Some(subcommand) => match subcommand {
                CliRootCommands::Setup(args) => {
                    let no_confirm = args.no_confirm;
                    let force = args.force;
                    let global = args.global;
                    installation::install_cli(args.into(), no_confirm, force, global).await
                },
                CliRootCommands::Uninstall { no_confirm } => uninstall::uninstall_command(no_confirm).await,
                CliRootCommands::Update(args) => args.execute().await,
                CliRootCommands::Diagnostic(args) => args.execute().await,
                CliRootCommands::Init(args) => args.execute().await,
                CliRootCommands::User(user) => user.execute().await,
                CliRootCommands::RootUser(root_user) => root_user.execute().await,
                CliRootCommands::Doctor(args) => args.execute().await,
                CliRootCommands::Hook(hook_subcommand) => hook_subcommand.execute().await,
                CliRootCommands::Theme(theme_args) => theme_args.execute().await,
                CliRootCommands::Settings(settings_args) => settings_args.execute(&cli_context).await,
                CliRootCommands::Debug(debug_subcommand) => debug_subcommand.execute().await,
                CliRootCommands::Issue(args) => args.execute().await,
                CliRootCommands::Completion(args) => args.execute(),
                CliRootCommands::Internal(internal_subcommand) => internal_subcommand.execute().await,
                CliRootCommands::Launch => launch_dashboard(false).await,
                CliRootCommands::Quit => crate::util::quit_fig(true).await,
                CliRootCommands::Restart { .. } => {
                    app::restart_fig().await?;
                    launch_dashboard(false).await
                },
                CliRootCommands::Integrations(subcommand) => subcommand.execute().await,
                CliRootCommands::Translate(args) => args.execute().await,
                CliRootCommands::Telemetry(subcommand) => subcommand.execute().await,
                CliRootCommands::Version => Self::print_version(),
                CliRootCommands::Dashboard => launch_dashboard(false).await,
                CliRootCommands::Chat {
                    accept_all,
                    input,
                    profile,
                } => chat::chat(input, accept_all, profile).await,
                CliRootCommands::Inline(subcommand) => subcommand.execute(&cli_context).await,
            },
            // Root command
            None => launch_dashboard(true).await,
        }
    }

    async fn send_telemetry(&self) {
        match &self.subcommand {
            None
            | Some(
                CliRootCommands::Init(_)
                | CliRootCommands::Internal(_)
                | CliRootCommands::Completion(_)
                | CliRootCommands::Hook(_),
            ) => {},
            Some(subcommand) => {
                fig_telemetry::send_cli_subcommand_executed(subcommand.name()).await;
            },
        }
    }

    #[allow(clippy::unused_self)]
    fn print_help_all(&self) -> Result<ExitCode> {
        let mut cmd = Self::command().help_template("{all-args}");
        eprintln!();
        eprintln!(
            "{}\n    {CLI_BINARY_NAME} [OPTIONS] [SUBCOMMAND]\n",
            "USAGE:".bold().underlined(),
        );
        cmd.print_long_help()?;
        Ok(ExitCode::SUCCESS)
    }

    #[allow(clippy::unused_self)]
    fn print_version() -> Result<ExitCode> {
        let _ = writeln!(stdout(), "{}", Self::command().render_version());
        Ok(ExitCode::SUCCESS)
    }
}

async fn launch_dashboard(help_fallback: bool) -> Result<ExitCode> {
    if manifest::is_minimal() || system_info::is_remote() {
        if help_fallback {
            Cli::command().print_help()?;
            return Ok(ExitCode::SUCCESS);
        } else {
            bail!("Launching the dashboard is not supported in minimal mode");
        }
    }

    launch_fig_desktop(LaunchArgs {
        wait_for_socket: true,
        open_dashboard: true,
        immediate_update: true,
        verbose: true,
    })?;

    let route = match is_logged_in().await {
        true => Some("/".into()),
        false => None,
    };

    println!("Opening {PRODUCT_NAME} dashboard");

    open_ui_element(UiElement::MissionControl, route)
        .await
        .context("Failed to open dashboard")?;

    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn debug_assert() {
        Cli::command().debug_assert();
    }

    macro_rules! assert_parse {
        (
            [ $($args:expr),+ ],
            $subcommand:expr
        ) => {
            assert_eq!(
                Cli::parse_from([CLI_BINARY_NAME, $($args),*]),
                Cli {
                    subcommand: Some($subcommand),
                    ..Default::default()
                }
            );
        };
    }

    /// Test flag parsing for the top level [Cli]
    #[test]
    fn test_flags() {
        assert_eq!(Cli::parse_from([CLI_BINARY_NAME, "-v"]), Cli {
            subcommand: None,
            verbose: 1,
            help_all: false,
        });

        assert_eq!(Cli::parse_from([CLI_BINARY_NAME, "-vvv"]), Cli {
            subcommand: None,
            verbose: 3,
            help_all: false,
        });

        assert_eq!(Cli::parse_from([CLI_BINARY_NAME, "--help-all"]), Cli {
            subcommand: None,
            verbose: 0,
            help_all: true,
        });

        assert_eq!(Cli::parse_from([CLI_BINARY_NAME, "chat", "-vv"]), Cli {
            subcommand: Some(CliRootCommands::Chat {
                accept_all: false,
                input: None,
                profile: None,
            },),
            verbose: 2,
            help_all: false,
        });
    }

    /// This test validates that the restart command maintains the same CLI facing definition
    ///
    /// If this changes, you must also change how it is called from within fig_install
    /// and (possibly) other locations as well
    #[test]
    fn test_restart() {
        assert_parse!(["restart", "app"], CliRootCommands::Restart {
            process: Processes::App
        });
    }

    /// This test validates that the internal input method installation command maintains the same
    /// CLI facing definition
    ///
    /// If this changes, you must also change how it is called from within
    /// fig_integrations::input_method
    #[cfg(target_os = "macos")]
    #[test]
    fn test_input_method_installation() {
        use internal::InternalSubcommand;
        assert_parse!(
            [
                "_",
                "attempt-to-finish-input-method-installation",
                "/path/to/bundle.app"
            ],
            CliRootCommands::Internal(InternalSubcommand::AttemptToFinishInputMethodInstallation {
                bundle_path: Some(std::path::PathBuf::from("/path/to/bundle.app"))
            })
        );
    }

    #[test]
    fn test_inline_shell_completion() {
        use internal::InternalSubcommand;

        assert_parse!(
            ["_", "inline-shell-completion", "--buffer", ""],
            CliRootCommands::Internal(InternalSubcommand::InlineShellCompletion { buffer: "".to_string() })
        );

        assert_parse!(
            ["_", "inline-shell-completion", "--buffer", "foo"],
            CliRootCommands::Internal(InternalSubcommand::InlineShellCompletion {
                buffer: "foo".to_string()
            })
        );

        assert_parse!(
            ["_", "inline-shell-completion", "--buffer", "-"],
            CliRootCommands::Internal(InternalSubcommand::InlineShellCompletion {
                buffer: "-".to_string()
            })
        );

        assert_parse!(
            ["_", "inline-shell-completion", "--buffer", "--"],
            CliRootCommands::Internal(InternalSubcommand::InlineShellCompletion {
                buffer: "--".to_string()
            })
        );

        assert_parse!(
            ["_", "inline-shell-completion", "--buffer", "--foo bar"],
            CliRootCommands::Internal(InternalSubcommand::InlineShellCompletion {
                buffer: "--foo bar".to_string()
            })
        );

        assert_parse!(
            [
                "_",
                "inline-shell-completion-accept",
                "--buffer",
                "abc",
                "--suggestion",
                "def"
            ],
            CliRootCommands::Internal(InternalSubcommand::InlineShellCompletionAccept {
                buffer: "abc".to_string(),
                suggestion: "def".to_string()
            })
        );
    }

    #[test]
    fn test_doctor() {
        assert_parse!(
            ["doctor"],
            CliRootCommands::Doctor(doctor::DoctorArgs {
                all: false,
                strict: false,
            })
        );
        assert_parse!(
            ["doctor", "--all"],
            CliRootCommands::Doctor(doctor::DoctorArgs {
                all: true,
                strict: false,
            })
        );
        assert_parse!(
            ["doctor", "--strict"],
            CliRootCommands::Doctor(doctor::DoctorArgs {
                all: false,
                strict: true,
            })
        );
        assert_parse!(
            ["doctor", "-a", "-s"],
            CliRootCommands::Doctor(doctor::DoctorArgs {
                all: true,
                strict: true,
            })
        );
    }

    #[test]
    fn test_chat_with_context_profile() {
        assert_parse!(["chat", "--profile", "my-profile"], CliRootCommands::Chat {
            accept_all: false,
            input: None,
            profile: Some("my-profile".to_string()),
        });
    }

    #[test]
    fn test_chat_with_context_profile_and_input() {
        assert_parse!(["chat", "--profile", "my-profile", "Hello"], CliRootCommands::Chat {
            accept_all: false,
            input: Some("Hello".to_string()),
            profile: Some("my-profile".to_string()),
        });
    }

    #[test]
    fn test_chat_with_context_profile_and_accept_all() {
        assert_parse!(
            ["chat", "--profile", "my-profile", "--accept-all"],
            CliRootCommands::Chat {
                accept_all: true,
                input: None,
                profile: Some("my-profile".to_string()),
            }
        );
    }
}
