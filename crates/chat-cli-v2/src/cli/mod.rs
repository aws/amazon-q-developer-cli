use crate::theme::StyledText;
use crate::util::env_var::is_log_stdout_enabled;
pub mod agent;
pub mod chat;
mod debug;
mod diagnostics;
pub mod feed;
mod issue;
mod mcp;
mod settings;
mod user;

use std::fmt::Display;
use std::io::{
    Write as _,
    stdout,
};
use std::process::ExitCode;

use agent::AgentArgs;
use anstream::println;
use clap::{
    ArgAction,
    CommandFactory,
    Parser,
    Subcommand,
    ValueEnum,
};
use eyre::{
    Result,
    bail,
};
use feed::Feed;
use serde::Serialize;
use tracing::{
    Level,
    debug,
};
pub use user::is_logged_in;

use crate::cli::chat::ChatArgs;
use crate::cli::mcp::McpSubcommand;
use crate::cli::user::{
    LoginArgs,
    WhoamiArgs,
};
use crate::logging::{
    LogArgs,
    initialize_logging,
};
use crate::os::Os;
use crate::util::CLI_BINARY_NAME;
use crate::util::paths::logs_dir;

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

/// The Kiro CLI
#[deny(missing_docs)]
#[derive(Debug, PartialEq, Subcommand)]
pub enum RootSubcommand {
    /// Manage agents
    Agent(AgentArgs),
    /// AI assistant in your terminal
    Chat(ChatArgs),
    /// Log in to Kiro
    Login(LoginArgs),
    /// Log out of Kiro
    Logout,
    /// Print info about the current login session
    Whoami(WhoamiArgs),
    /// Show the profile associated with this idc user
    Profile,
    /// Customize appearance & behavior
    #[command(alias("setting"))]
    Settings(settings::SettingsArgs),
    /// Run diagnostic tests
    #[command(alias("diagnostics"))]
    Diagnostic(diagnostics::DiagnosticArgs),
    /// Create a new Github issue
    Issue(issue::IssueArgs),
    /// Version
    #[command(hide = true)]
    Version {
        /// Show the changelog (use --changelog=all for all versions, or --changelog=x.x.x for a
        /// specific version)
        #[arg(long, num_args = 0..=1, default_missing_value = "")]
        changelog: Option<String>,
    },
    /// Model Context Protocol (MCP)
    #[command(subcommand)]
    Mcp(McpSubcommand),
    /// Start Agent Client Protocol (ACP) agent
    #[command(hide = true)]
    Acp {
        /// Name of the agent to use when starting the first session
        #[arg(long)]
        agent: Option<String>,
        /// Model ID to use when starting the first session
        #[arg(long)]
        model: Option<String>,
        /// Auto-approve all tool permission requests
        #[arg(long, short = 'a')]
        trust_all_tools: bool,
        /// Trust only this set of tools
        #[arg(long, value_delimiter = ',', value_name = "TOOL_NAMES")]
        trust_tools: Option<Vec<String>>,
        /// Initial effort level (e.g. low, medium, high, xhigh, max)
        #[arg(long)]
        effort: Option<String>,
    },
    /// ACP test client
    #[command(hide = true)]
    AcpClient {
        /// Path to the ACP agent executable
        #[arg(long)]
        agent: String,
    },
    /// Record voice and print transcription to stdout (used by TUI).
    #[cfg(feature = "voice")]
    #[command(hide = true)]
    Voice {
        /// Push-to-talk mode (disables silence auto-stop)
        #[arg(long)]
        ptt: bool,
    },
    /// Start a voice recording server for remote/cloud desktop use.
    #[cfg(feature = "voice")]
    #[command(hide = true)]
    VoiceServe {
        /// Port to listen on.
        #[arg(long, default_value = "19876")]
        port: u16,
        /// Address to bind to.
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,
    },
    /// Set up voice mode for a cloud desktop (run locally).
    #[cfg(feature = "voice")]
    #[command(name = "voice-cloud-setup", hide = true)]
    VoiceCloudSetup {
        /// Cloud desktop hostname or SSH config alias
        host: String,
        /// Port for voice server
        #[arg(long, default_value = "19876")]
        port: u16,
        /// Path to kiro binary on cloud desktop
        #[arg(long)]
        remote_bin: Option<String>,
        /// SSH identity file
        #[arg(long, short = 'i')]
        identity: Option<String>,
    },
}

impl RootSubcommand {
    /// Whether the command should have an associated telemetry event.
    ///
    /// Emitting telemetry takes a long time so the answer is usually no.
    pub fn valid_for_telemetry(&self) -> bool {
        matches!(self, Self::Chat(_) | Self::Login(_) | Self::Profile | Self::Issue(_))
    }

    pub async fn execute(self, os: &mut Os) -> Result<ExitCode> {
        // Check for auth on subcommands that require it.
        if !is_logged_in(&mut os.database).await {
            if matches!(self, Self::Chat(ref args) if args.no_interactive) {
                bail!(
                    "Not logged in. Set the KIRO_API_KEY environment variable or run `{CLI_BINARY_NAME} login` first."
                );
            } else if matches!(self, Self::Chat(_)) {
                let options = ["Yes", "No"];
                match crate::util::choose(" You are not logged in. Login now?", &options)? {
                    Some(0) => {},
                    _ => bail!("Login is required to use chat"),
                }

                LoginArgs::default().execute(os).await?;
            } else if matches!(self, Self::Profile) {
                bail!(
                    "You are not logged in, please log in with {}",
                    StyledText::command(&format!("{CLI_BINARY_NAME} login"))
                );
            }
        }

        // Daily heartbeat check
        if os.database.record_heartbeat_if_needed() {
            os.telemetry.send_daily_heartbeat().ok();
        }

        // Send executed telemetry.
        if self.valid_for_telemetry() {
            os.telemetry
                .send_cli_subcommand_executed(&os.database, &self)
                .await
                .ok();
        }

        match self {
            Self::Agent(args) => args.execute(os).await,
            Self::Chat(_) => unreachable!("Chat is handled by TUI launcher in Cli::run()"),
            Self::Diagnostic(args) => args.execute(os).await,
            Self::Login(args) => args.execute(os).await,
            Self::Logout => user::logout(os).await,
            Self::Whoami(args) => args.execute(os).await,
            Self::Profile => user::profile(os).await,
            Self::Settings(settings_args) => settings_args.execute(os).await,
            Self::Issue(args) => args.execute(os).await,
            Self::Version { changelog } => Cli::print_version(changelog),
            Self::Mcp(args) => args.execute(os, &mut std::io::stderr()).await,
            Self::Acp {
                agent,
                model,
                trust_all_tools,
                trust_tools,
                effort,
            } => {
                let spawn_args = ::agent::types::AcpSpawnArgs {
                    agent,
                    model,
                    trust_all_tools,
                    trust_tools,
                    agent_engine: None,
                    effort,
                };
                use std::sync::Arc;

                use crate::agent::session::legacy_compat::NoOpLegacySessionExporter;
                crate::agent::acp::acp_agent::execute(os, spawn_args, Arc::new(NoOpLegacySessionExporter)).await
            },
            Self::AcpClient { agent } => crate::agent::acp::acp_client::execute(agent).await,
            #[cfg(feature = "voice")]
            Self::Voice { ptt } => {
                use crate::database::settings::Setting;
                let server_url = os.database.settings.get_string(Setting::VoiceServerUrl);
                let backend = if server_url.is_some() {
                    "RemoteServer".to_string()
                } else {
                    "LocalWhisper".to_string()
                };
                let silence_timeout = if ptt {
                    None
                } else {
                    Some(
                        os.database
                            .settings
                            .get_int(Setting::VoiceSilenceTimeout)
                            .and_then(|v| v.try_into().ok())
                            .unwrap_or(5u64),
                    )
                };
                let language = os.database.settings.get_string(Setting::VoiceLanguage);
                let model_size = os.database.settings.get_string(Setting::VoiceModelSize);
                let result =
                    voice::voice_handler::voice_only_mode(server_url, silence_timeout, language, model_size).await;
                let (telem_result, reason, reason_desc) = match &result {
                    Ok(_) => (crate::telemetry::TelemetryResult::Succeeded, None, None),
                    Err(e) => (
                        crate::telemetry::TelemetryResult::Failed,
                        Some("VoiceError".to_string()),
                        Some(e.to_string()),
                    ),
                };
                let input_method = if ptt { "PTT" } else { "SlashCommand" };
                os.telemetry
                    .send_voice_input(
                        None,
                        telem_result,
                        reason,
                        reason_desc,
                        backend,
                        input_method.to_string(),
                        None,
                        None,
                        None,
                        None,
                        None,
                    )
                    .ok();
                result
            },
            #[cfg(feature = "voice")]
            Self::VoiceServe { port, bind } => voice::voice_serve::run_voice_server(&bind, port).await,
            #[cfg(feature = "voice")]
            Self::VoiceCloudSetup {
                host,
                port,
                remote_bin,
                identity,
            } => {
                voice::voice_cloud_setup::run_voice_cloud_setup(&host, port, remote_bin.as_deref(), identity.as_deref())
                    .await
            },
        }
    }
}

impl Default for RootSubcommand {
    fn default() -> Self {
        Self::Chat(ChatArgs::default())
    }
}

impl Display for RootSubcommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Agent(_) => "agent",
            Self::Chat(_) => "chat",
            Self::Login(_) => "login",
            Self::Logout => "logout",
            Self::Whoami(_) => "whoami",
            Self::Profile => "profile",
            Self::Settings(_) => "settings",
            Self::Diagnostic(_) => "diagnostic",
            Self::Issue(_) => "issue",
            Self::Version { .. } => "version",
            Self::Mcp(_) => "mcp",
            Self::Acp { .. } => "acp",
            Self::AcpClient { .. } => "acp-client",
            #[cfg(feature = "voice")]
            Self::Voice { .. } => "voice",
            #[cfg(feature = "voice")]
            Self::VoiceServe { .. } => "voice-serve",
            #[cfg(feature = "voice")]
            Self::VoiceCloudSetup { .. } => "voice-cloud-setup",
        };

        write!(f, "{name}")
    }
}

impl RootSubcommand {
    /// Subcommand name as recorded in `CliSubcommandExecuted` telemetry.
    ///
    /// Identical to `Display` except that root-level `--list` (alias for
    /// `--resume-picker`) is recorded as `chat:list` so we can track adoption
    /// of the alias separately from `--resume-picker`.
    pub fn telemetry_name(&self) -> String {
        if matches!(self, Self::Chat(_)) && std::env::args().any(|a| a == "--list") {
            return "chat:list".to_string();
        }
        self.to_string()
    }
}

#[derive(Debug, Parser, PartialEq, Default)]
#[command(version, about, name = crate::util::CHAT_BINARY_NAME)]
pub struct Cli {
    #[command(subcommand)]
    pub subcommand: Option<RootSubcommand>,
    /// Increase logging verbosity
    #[arg(long, short = 'v', action = ArgAction::Count, global = true)]
    pub verbose: u8,
    /// Resume the most recent conversation from this directory
    #[arg(short, long)]
    resume: bool,
    /// Resume a specific conversation by session ID
    #[arg(long, value_name = "SESSION_ID", conflicts_with_all = ["resume", "resume_picker"])]
    resume_id: Option<String>,
    /// Interactively select a conversation to resume from this directory
    #[arg(long, conflicts_with = "resume", visible_alias = "list")]
    resume_picker: bool,
}

impl Cli {
    pub async fn execute(self) -> Result<ExitCode> {
        let subcommand = self.subcommand.unwrap_or_else(|| {
            RootSubcommand::Chat(ChatArgs {
                resume: self.resume,
                resume_id: self.resume_id,
                resume_picker: self.resume_picker,
                ..Default::default()
            })
        });

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
            log_to_stdout: is_log_stdout_enabled() || self.verbose > 0,
            log_file_path: std::env::var("KIRO_CHAT_LOG_FILE")
                .ok()
                .map(std::path::PathBuf::from)
                .or_else(|| match subcommand {
                    RootSubcommand::Chat { .. } | RootSubcommand::Acp { .. } => {
                        Some(logs_dir().expect("home dir must be set").join("kiro-chat.log"))
                    },
                    _ => None,
                }),
            delete_old_log_file: false,
        });

        debug!(command =? std::env::args().collect::<Vec<_>>(), "Command being ran");

        // If Chat command and TUI is embedded, extract and run TUI instead
        let mut os = Os::new().await?;

        let result = if matches!(&subcommand, RootSubcommand::Chat(args) if !args.legacy_mode) {
            let asset_paths = crate::embedded_tui::extract_tui_assets_if_needed(&os).await?;
            // Write feed.json alongside TUI assets so the TUI reads it from disk
            // instead of receiving the entire changelog (~100KB) as an env var.
            let feed_path = crate::util::paths::feed_json_path()?;
            std::fs::write(&feed_path, include_str!("feed.json"))?;
            // Expose internal-user status to the TUI via env var (derived from SSO start URL).
            if is_internal_user(&os.database) {
                // SAFETY: single-threaded at this point — TUI subprocess hasn't spawned yet.
                unsafe { std::env::set_var("KIRO_INTERNAL", "1") };
            }
            // Bridge Feature::Lite rollout (internal + nightly) to the TUI subprocess.
            // The TS resolveUiMode() reads this to decide whether to honor lite-mode requests.
            if crate::rollout::Rollout::is_enabled(crate::rollout::Feature::Lite) {
                // SAFETY: single-threaded at this point — TUI subprocess hasn't spawned yet.
                unsafe { std::env::set_var("KIRO_LITE_ROLLOUT_ENABLED", "1") };
            }
            crate::launch_options::launch_tui(&asset_paths).await
        } else {
            subcommand.execute(&mut os).await
        };

        let telemetry_result = os.telemetry.finish().await;
        let exit_code = result?;
        telemetry_result?;

        Ok(exit_code)
    }

    fn print_changelog_entry(entry: &feed::Entry) -> Result<()> {
        println!("Version {} ({})", entry.version, entry.date);

        if entry.changes.is_empty() {
            println!("  No changes recorded for this version.");
        } else {
            for change in &entry.changes {
                let type_label = match change.change_type.as_str() {
                    "added" => "Added",
                    "fixed" => "Fixed",
                    "changed" => "Changed",
                    other => other,
                };

                println!("  - {}: {}", type_label, change.description);
            }
        }

        println!();
        Ok(())
    }

    fn print_version(changelog: Option<String>) -> Result<ExitCode> {
        // If no changelog is requested, display normal version information
        if changelog.is_none() {
            let _ = writeln!(stdout(), "{}", Self::command().render_version());
            return Ok(ExitCode::SUCCESS);
        }

        let changelog_value = changelog.unwrap_or_default();
        let feed = Feed::load();

        // Display changelog for all versions
        if changelog_value == "all" {
            let entries = feed.get_all_changelogs();
            if entries.is_empty() {
                println!("No changelog information available.");
            } else {
                println!("Changelog for all versions:");
                for entry in entries {
                    Self::print_changelog_entry(&entry)?;
                }
            }
            return Ok(ExitCode::SUCCESS);
        }

        // Display changelog for a specific version (--changelog=x.x.x)
        if !changelog_value.is_empty() {
            match feed.get_version_changelog(&changelog_value) {
                Some(entry) => {
                    println!("Changelog for version {}:", changelog_value);
                    Self::print_changelog_entry(&entry)?;
                    return Ok(ExitCode::SUCCESS);
                },
                None => {
                    println!("No changelog information available for version {}.", changelog_value);
                    return Ok(ExitCode::SUCCESS);
                },
            }
        }

        // Display changelog for the current version (--changelog only)
        let current_version = env!("CARGO_PKG_VERSION");
        match feed.get_version_changelog(current_version) {
            Some(entry) => {
                println!("Changelog for version {}:", current_version);
                Self::print_changelog_entry(&entry)?;
            },
            None => {
                println!("No changelog information available for version {}.", current_version);
            },
        }

        Ok(ExitCode::SUCCESS)
    }
}

/// Returns `true` if the user authenticated via the Amazon-internal SSO start URL.
fn is_internal_user(database: &crate::database::Database) -> bool {
    database
        .get_start_url()
        .ok()
        .flatten()
        .is_some_and(|url| url == crate::auth::AMZN_START_URL)
}

#[cfg(test)]
mod test {
    use chat::WrapMode::{
        Always,
        Auto,
        Never,
    };

    use super::*;
    use crate::util::CHAT_BINARY_NAME;
    use crate::util::test::assert_parse;

    #[test]
    fn debug_assert() {
        Cli::command().debug_assert();
    }

    /// Test flag parsing for the top level [Cli]
    #[test]
    fn test_flags() {
        assert_eq!(Cli::parse_from([CHAT_BINARY_NAME, "-v"]), Cli {
            subcommand: None,
            verbose: 1,
            resume: false,
            resume_id: None,
            resume_picker: false,
        });

        assert_eq!(Cli::parse_from([CHAT_BINARY_NAME, "-vvv"]), Cli {
            subcommand: None,
            verbose: 3,
            resume: false,
            resume_id: None,
            resume_picker: false,
        });

        assert_eq!(Cli::parse_from([CHAT_BINARY_NAME, "chat", "-vv"]), Cli {
            subcommand: Some(RootSubcommand::Chat(ChatArgs {
                resume: false,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                delete_session: None,
                input: None,
                agent: None,
                model: None,
                effort: None,
                trust_all_tools: false,
                trust_tools: None,
                no_interactive: false,
                legacy_mode: false,
                wrap: None,
            })),
            verbose: 2,
            resume: false,
            resume_id: None,
            resume_picker: false,
        });
    }

    #[test]
    fn test_version_changelog() {
        assert_parse!(["version", "--changelog"], RootSubcommand::Version {
            changelog: Some("".to_string()),
        });
    }

    #[test]
    fn test_version_changelog_all() {
        assert_parse!(["version", "--changelog=all"], RootSubcommand::Version {
            changelog: Some("all".to_string()),
        });
    }

    #[test]
    fn test_version_changelog_specific() {
        assert_parse!(["version", "--changelog=1.8.0"], RootSubcommand::Version {
            changelog: Some("1.8.0".to_string()),
        });
    }

    #[test]
    fn test_chat_with_context_profile() {
        assert_parse!(
            ["chat", "--profile", "my-profile"],
            RootSubcommand::Chat(ChatArgs {
                resume: false,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                delete_session: None,
                input: None,
                agent: Some("my-profile".to_string()),
                model: None,
                effort: None,
                trust_all_tools: false,
                trust_tools: None,
                no_interactive: false,
                legacy_mode: false,
                wrap: None,
            })
        );
    }

    #[test]
    fn test_chat_with_context_profile_and_input() {
        assert_parse!(
            ["chat", "--profile", "my-profile", "Hello"],
            RootSubcommand::Chat(ChatArgs {
                resume: false,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                delete_session: None,
                input: Some("Hello".to_string()),
                agent: Some("my-profile".to_string()),
                model: None,
                effort: None,
                trust_all_tools: false,
                trust_tools: None,
                no_interactive: false,
                legacy_mode: false,
                wrap: None,
            })
        );
    }

    #[test]
    fn test_chat_with_context_profile_and_accept_all() {
        assert_parse!(
            ["chat", "--profile", "my-profile", "--trust-all-tools"],
            RootSubcommand::Chat(ChatArgs {
                resume: false,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                delete_session: None,
                input: None,
                agent: Some("my-profile".to_string()),
                model: None,
                effort: None,
                trust_all_tools: true,
                trust_tools: None,
                no_interactive: false,
                legacy_mode: false,
                wrap: None,
            })
        );
    }

    #[test]
    fn test_chat_with_no_interactive_and_resume() {
        assert_parse!(
            ["chat", "--no-interactive", "--resume"],
            RootSubcommand::Chat(ChatArgs {
                resume: true,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                delete_session: None,
                input: None,
                agent: None,
                model: None,
                effort: None,
                trust_all_tools: false,
                trust_tools: None,
                no_interactive: true,
                legacy_mode: false,
                wrap: None,
            })
        );
        assert_parse!(
            ["chat", "--non-interactive", "-r"],
            RootSubcommand::Chat(ChatArgs {
                resume: true,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                delete_session: None,
                input: None,
                agent: None,
                model: None,
                effort: None,
                trust_all_tools: false,
                trust_tools: None,
                no_interactive: true,
                legacy_mode: false,
                wrap: None,
            })
        );
    }

    #[test]
    fn test_chat_with_resume_id() {
        assert_parse!(
            ["chat", "--resume-id", "abc-123"],
            RootSubcommand::Chat(ChatArgs {
                resume: false,
                resume_id: Some("abc-123".to_string()),
                resume_picker: false,
                list_sessions: false,
                delete_session: None,
                input: None,
                agent: None,
                model: None,
                effort: None,
                trust_all_tools: false,
                trust_tools: None,
                no_interactive: false,
                legacy_mode: false,
                wrap: None,
            })
        );
    }

    #[test]
    fn test_chat_with_tool_trust_all() {
        assert_parse!(
            ["chat", "--trust-all-tools"],
            RootSubcommand::Chat(ChatArgs {
                resume: false,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                delete_session: None,
                input: None,
                agent: None,
                model: None,
                effort: None,
                trust_all_tools: true,
                trust_tools: None,
                no_interactive: false,
                legacy_mode: false,
                wrap: None,
            })
        );
    }

    #[test]
    fn test_chat_with_tool_trust_none() {
        assert_parse!(
            ["chat", "--trust-tools="],
            RootSubcommand::Chat(ChatArgs {
                resume: false,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                delete_session: None,
                input: None,
                agent: None,
                model: None,
                effort: None,
                trust_all_tools: false,
                trust_tools: Some(vec!["".to_string()]),
                no_interactive: false,
                legacy_mode: false,
                wrap: None,
            })
        );
    }

    #[test]
    fn test_chat_with_tool_trust_some() {
        assert_parse!(
            ["chat", "--trust-tools=fs_read,fs_write"],
            RootSubcommand::Chat(ChatArgs {
                resume: false,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                delete_session: None,
                input: None,
                agent: None,
                model: None,
                effort: None,
                trust_all_tools: false,
                trust_tools: Some(vec!["fs_read".to_string(), "fs_write".to_string()]),
                no_interactive: false,
                legacy_mode: false,
                wrap: None,
            })
        );
    }

    #[test]
    fn test_chat_with_different_wrap_modes() {
        assert_parse!(
            ["chat", "-w", "never"],
            RootSubcommand::Chat(ChatArgs {
                resume: false,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                delete_session: None,
                input: None,
                agent: None,
                model: None,
                effort: None,
                trust_all_tools: false,
                trust_tools: None,
                no_interactive: false,
                legacy_mode: false,
                wrap: Some(Never),
            })
        );
        assert_parse!(
            ["chat", "--wrap", "always"],
            RootSubcommand::Chat(ChatArgs {
                resume: false,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                delete_session: None,
                input: None,
                agent: None,
                model: None,
                effort: None,
                trust_all_tools: false,
                trust_tools: None,
                no_interactive: false,
                legacy_mode: false,
                wrap: Some(Always),
            })
        );
        assert_parse!(
            ["chat", "--wrap", "auto"],
            RootSubcommand::Chat(ChatArgs {
                resume: false,
                resume_id: None,
                resume_picker: false,
                list_sessions: false,
                delete_session: None,
                input: None,
                agent: None,
                model: None,
                effort: None,
                trust_all_tools: false,
                trust_tools: None,
                no_interactive: false,
                legacy_mode: false,
                wrap: Some(Auto),
            })
        );
    }

    #[tokio::test]
    async fn test_is_internal_user_with_amzn_start_url() {
        let mut db = crate::database::Database::new().await.unwrap();
        db.set_start_url("https://amzn.awsapps.com/start".to_string()).unwrap();
        assert!(super::is_internal_user(&db));
    }

    #[tokio::test]
    async fn test_is_internal_user_with_external_start_url() {
        let mut db = crate::database::Database::new().await.unwrap();
        db.set_start_url("https://view.awsapps.com/start".to_string()).unwrap();
        assert!(!super::is_internal_user(&db));
    }

    #[tokio::test]
    async fn test_is_internal_user_with_no_start_url() {
        let db = crate::database::Database::new().await.unwrap();
        assert!(!super::is_internal_user(&db));
    }
}
