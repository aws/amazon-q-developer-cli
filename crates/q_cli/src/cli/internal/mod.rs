mod generate_ssh;
mod inline_shell_completion;
pub mod local_state;
mod multiplexer;
pub mod should_figterm_launch;

use std::collections::HashSet;
use std::io::{
    Read,
    Write,
    stdout,
};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use anstream::println;
use bytes::{
    Buf,
    BytesMut,
};
use clap::{
    ArgGroup,
    Args,
    Subcommand,
    ValueEnum,
};
use crossterm::style::Stylize;
use eyre::{
    Context,
    ContextCompat,
    Result,
    bail,
};
use fig_install::InstallComponents;
#[cfg(target_os = "macos")]
use fig_integrations::input_method::InputMethod;
use fig_ipc::local::send_hook_to_socket;
use fig_ipc::{
    BufferedUnixStream,
    SendMessage,
    SendRecvMessage,
};
use fig_os_shim::{
    Context as OsContext,
    Os,
};
use fig_proto::ReflectMessage;
use fig_proto::figterm::figterm_request_message::Request as FigtermRequest;
use fig_proto::figterm::{
    FigtermRequestMessage,
    NotifySshSessionStartedRequest,
    UpdateShellContextRequest,
};
use fig_proto::hooks::{
    new_callback_hook,
    new_event_hook,
};
use fig_proto::local::EnvironmentVariable;
use fig_proto::util::get_shell;
use fig_util::directories::{
    figterm_socket_path,
    logs_dir,
};
use fig_util::env_var::QTERM_SESSION_ID;
use fig_util::{
    CLI_BINARY_NAME,
    directories,
};
use multiplexer::MultiplexerArgs;
use rand::distr::{
    Alphanumeric,
    SampleString,
};
use sysinfo::System;
use tokio::io::{
    AsyncReadExt,
    AsyncWriteExt,
};
use tokio::process::Command;
use tokio::select;
use tracing::{
    debug,
    error,
    info,
    trace,
    warn,
};

use self::inline_shell_completion::{
    inline_shell_completion,
    inline_shell_completion_accept,
};
use crate::cli::installation::install_cli;
use crate::util::desktop::{
    LaunchArgs,
    launch_fig_desktop,
};

#[derive(Debug, Args, PartialEq, Eq)]
#[command(group(
        ArgGroup::new("output")
            .args(&["filename", "exit_code"])
            .multiple(true)
            .requires_all(&["filename", "exit_code"])
            ))]
pub struct CallbackArgs {
    handler_id: String,
    #[arg(group = "output")]
    filename: Option<String>,
    #[arg(group = "output")]
    exit_code: Option<i64>,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct InstallArgs {
    /// Install only the shell integrations
    #[arg(long)]
    pub dotfiles: bool,
    /// Prompt input method installation
    #[arg(long)]
    pub input_method: bool,
    /// Don't confirm automatic installation.
    #[arg(long)]
    pub no_confirm: bool,
    /// Force installation of q
    #[arg(long)]
    pub force: bool,
    /// Install q globally
    #[arg(long)]
    pub global: bool,
}

impl From<InstallArgs> for InstallComponents {
    fn from(args: InstallArgs) -> Self {
        let InstallArgs {
            dotfiles, input_method, ..
        } = args;
        if dotfiles || input_method {
            let mut install_components = InstallComponents::empty();
            install_components.set(InstallComponents::SHELL_INTEGRATIONS, dotfiles);
            install_components.set(InstallComponents::INPUT_METHOD, input_method);
            install_components
        } else {
            InstallComponents::all()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum StateComponent {
    Figterm,
    WebNotifications,
    Platform,
}

#[derive(Debug, PartialEq, Eq, Subcommand)]
#[command(hide = true, alias = "_")]
pub enum InternalSubcommand {
    /// Command that is run during the PreCmd section of
    /// the Amazon Q integrations.
    PreCmd {
        #[arg(long, allow_hyphen_values = true)]
        alias: Option<String>,
    },
    /// Change the local-state file
    LocalState(local_state::LocalStateArgs),
    /// Callback used for the internal pseudoterminal
    Callback(CallbackArgs),
    /// Install the Amazon Q cli
    Install(InstallArgs),
    /// Uninstall the Amazon Q cli
    Uninstall {
        /// Uninstall only the shell integrations
        #[arg(long)]
        dotfiles: bool,
        /// Uninstall only the input method
        #[arg(long)]
        input_method: bool,
        /// Uninstall only the binary
        #[arg(long)]
        binary: bool,
        /// Uninstall only the ssh integration
        #[arg(long)]
        ssh: bool,
    },
    GetShell,
    Hostname,
    /// Detects if Figterm should be launched
    ///
    /// Exit code:
    /// - 0 execute figterm
    /// - 1 dont execute figterm
    /// - 2 fallback to Q_TERM env
    ShouldFigtermLaunch,
    Event {
        /// Name of the event.
        #[arg(long)]
        name: String,
        /// Payload of the event as a JSON string.
        #[arg(long)]
        payload: Option<String>,
        /// Apps to send the event to.
        #[arg(long)]
        apps: Vec<String>,
    },
    SocketsDir,
    StreamFromSocket,
    FigtermSocketPath {
        session_id: String,
    },
    #[command(group(
        ArgGroup::new("target")
            .multiple(false)
            .required(true)
    ))]
    Ipc {
        #[arg(long, group = "target")]
        app: bool,
        #[arg(long, group = "target")]
        figterm: Option<String>,
        #[arg(long)]
        json: String,
        #[arg(long)]
        recv: bool,
    },
    UninstallForAllUsers,
    RemoveDataDir {
        #[arg(long)]
        force: bool,
    },
    Uuidgen,
    #[cfg(target_os = "linux")]
    IbusBootstrap,
    #[cfg(target_os = "linux")]
    /// Checks for sandboxing
    DetectSandbox,
    OpenUninstallPage,
    /// Displays prompt to install remote shell integrations
    SshLocalCommand {
        remote_dest: String,
        uuid: String,
    },
    /// \[Deprecated\] Displays prompt to install remote shell integrations.
    PromptSsh {
        remote_dest: String,
    },
    #[cfg(target_os = "macos")]
    AttemptToFinishInputMethodInstallation {
        bundle_path: Option<PathBuf>,
    },
    DumpState {
        component: StateComponent,
    },
    FinishUpdate {
        #[arg(long)]
        relaunch_dashboard: bool,
        #[arg(long)]
        delete_bundle: Option<String>,
    },
    #[cfg(target_os = "macos")]
    SwapFiles {
        from: PathBuf,
        to: PathBuf,
        #[arg(long)]
        not_same_bundle_name: bool,
    },
    #[cfg(target_os = "macos")]
    BrewUninstall {
        #[arg(long)]
        zap: bool,
    },
    /// Generates an SSH configuration file
    ///
    /// This lets us bypass a bug in Include and vdollar_expand that causes environment variables to
    /// be expanded, even in files that are only referenced in match blocks that resolve to false
    GenerateSsh(generate_ssh::GenerateSshArgs),
    InlineShellCompletion {
        #[arg(long, allow_hyphen_values = true)]
        buffer: String,
    },
    InlineShellCompletionAccept {
        #[arg(long, allow_hyphen_values = true)]
        buffer: String,
        #[arg(long, allow_hyphen_values = true)]
        suggestion: String,
    },
    #[command(alias = "mux")]
    Multiplexer(MultiplexerArgs),
}

const BUFFER_SIZE: usize = 1024;

impl InternalSubcommand {
    pub async fn execute(self) -> Result<ExitCode> {
        let ctx = OsContext::new();
        match self {
            InternalSubcommand::Install(args) => {
                let no_confirm = args.no_confirm;
                let force = args.force;
                let global = args.global;
                install_cli(args.into(), no_confirm, force, global).await
            },
            InternalSubcommand::Uninstall {
                dotfiles,
                input_method,
                binary,
                ssh,
            } => {
                let components = if dotfiles || binary || ssh || input_method {
                    let mut uninstall_components = InstallComponents::empty();
                    uninstall_components.set(InstallComponents::SHELL_INTEGRATIONS, dotfiles);
                    uninstall_components.set(InstallComponents::INPUT_METHOD, input_method);
                    uninstall_components.set(InstallComponents::BINARY, binary);
                    uninstall_components.set(InstallComponents::SSH, ssh);
                    uninstall_components
                } else {
                    InstallComponents::all()
                };
                if components.contains(InstallComponents::BINARY) {
                    if option_env!("Q_IS_PACKAGE_MANAGED").is_some() {
                        println!("Please uninstall using your package manager");
                    } else {
                        fig_install::uninstall(InstallComponents::BINARY, Arc::clone(&ctx)).await?;
                        println!("\n{}\n", "The binary was successfully uninstalled".bold());
                    }
                }

                let mut components = components;
                components.set(InstallComponents::BINARY, false);
                fig_install::uninstall(components, Arc::clone(&ctx)).await?;
                Ok(ExitCode::SUCCESS)
            },
            InternalSubcommand::PreCmd { alias } => Ok(pre_cmd(alias).await),
            InternalSubcommand::LocalState(local_state) => {
                local_state.execute().await?;
                Ok(ExitCode::SUCCESS)
            },
            InternalSubcommand::Callback(CallbackArgs {
                handler_id,
                filename,
                exit_code,
            }) => {
                trace!("handlerId: {handler_id}");

                let (filename, exit_code) = match (filename, exit_code) {
                    (Some(filename), Some(exit_code)) => {
                        trace!("callback specified filepath ({filename}) and exitCode ({exit_code}) to output!");
                        (filename, exit_code)
                    },
                    _ => {
                        let file_id = Alphanumeric.sample_string(&mut rand::rng(), 9);
                        let tmp_filename = format!("fig-callback-{file_id}");
                        let tmp_path = PathBuf::from("/tmp").join(tmp_filename);
                        let mut tmp_file = std::fs::File::create(&tmp_path)?;
                        let mut buffer = [0u8; BUFFER_SIZE];
                        let mut stdin = std::io::stdin();
                        trace!("Created tmp file: {}", tmp_path.display());

                        loop {
                            let size = stdin.read(&mut buffer)?;
                            if size == 0 {
                                break;
                            }
                            tmp_file.write_all(&buffer[..size])?;
                            trace!("Read {size} bytes\n{}", std::str::from_utf8(&buffer[..size])?);
                        }

                        let filename: String = tmp_path.to_str().context("invalid file path")?.into();
                        trace!("Done reading from stdin!");
                        (filename, -1)
                    },
                };
                let hook = new_callback_hook(&handler_id, &filename, exit_code);

                info!(
                    "Sending 'handlerId: {handler_id}, filename: {filename}, exitcode: {exit_code}' over unix socket!\n"
                );

                match send_hook_to_socket(hook).await {
                    Ok(()) => debug!("Successfully sent hook"),
                    Err(e) => debug!("Couldn't send hook {e}"),
                }

                Ok(ExitCode::SUCCESS)
            },
            InternalSubcommand::GetShell => match get_shell() {
                Ok(shell) => {
                    if write!(stdout(), "{shell}").is_ok() {
                        return Ok(ExitCode::SUCCESS);
                    }
                    Ok(ExitCode::FAILURE)
                },
                Err(_) => Ok(ExitCode::FAILURE),
            },
            InternalSubcommand::Hostname => {
                if let Some(hostname) = System::host_name() {
                    if write!(stdout(), "{hostname}").is_ok() {
                        return Ok(ExitCode::SUCCESS);
                    }
                }
                Ok(ExitCode::FAILURE)
            },
            InternalSubcommand::ShouldFigtermLaunch => {
                Ok(should_figterm_launch::should_figterm_launch(&OsContext::new()))
            },
            InternalSubcommand::Event { payload, apps, name } => {
                let hook = new_event_hook(name, payload, apps);
                send_hook_to_socket(hook).await?;
                Ok(ExitCode::SUCCESS)
            },
            InternalSubcommand::Ipc {
                app,
                figterm,
                json,
                recv,
            } => {
                let message = fig_proto::FigMessage::json(serde_json::from_str::<serde_json::Value>(&json)?)?;

                let socket = if app {
                    directories::desktop_socket_path().expect("Failed to get socket path")
                } else if let Some(ref figterm) = figterm {
                    directories::figterm_socket_path(figterm).expect("Failed to get socket path")
                } else {
                    bail!("No destination for message");
                };

                let mut conn = BufferedUnixStream::connect(socket).await?;

                if recv {
                    macro_rules! recv {
                        ($abc:path) => {{
                            let response: Option<$abc> = conn.send_recv_message(message).await?;
                            match response {
                                Some(response) => {
                                    let message = response.transcode_to_dynamic();
                                    println!("{}", serde_json::to_string(&message)?)
                                },
                                None => bail!("Received EOF while waiting for response"),
                            }
                        }};
                    }

                    if app {
                        recv!(fig_proto::local::CommandResponse);
                    } else if figterm.is_some() {
                        recv!(fig_proto::figterm::FigtermResponseMessage);
                    }
                } else {
                    conn.send_message(message).await?;
                }

                Ok(ExitCode::SUCCESS)
            },
            InternalSubcommand::SocketsDir => {
                writeln!(stdout(), "{}", directories::sockets_dir_utf8()?).ok();
                Ok(ExitCode::SUCCESS)
            },
            InternalSubcommand::FigtermSocketPath { session_id } => {
                writeln!(
                    stdout(),
                    "{}",
                    directories::figterm_socket_path(session_id)?.to_string_lossy()
                )
                .ok();
                Ok(ExitCode::SUCCESS)
            },
            InternalSubcommand::UninstallForAllUsers => {
                if !cfg!(target_os = "linux") {
                    bail!("uninstall-for-all-users is only supported on Linux");
                }

                let out = Command::new("users").output().await?;
                let users = String::from_utf8_lossy(&out.stdout);

                let mut integrations_uninstalled = false;
                let mut data_dir_removed = false;

                // let emit = tokio::spawn(fig_telemetry::emit_track(TrackEvent::new(
                //     TrackEventType::UninstalledApp,
                //     TrackSource::Cli,
                //     env!("CARGO_PKG_VERSION").into(),
                //     std::iter::empty::<(&str, &str)>(),
                // )));

                let users = users
                    .split(' ')
                    .map(|line| line.trim())
                    .filter(|line| !line.is_empty())
                    .collect::<HashSet<_>>();

                for user in users {
                    println!("Uninstalling additional components for user: {}", user);

                    debug!(user, "Opening uninstall page");
                    let user_clone = user.to_string();
                    // Spawning a separate task since this will block unless and until the user
                    // answers the survey or closes their browser.
                    tokio::spawn(async move {
                        let user = user_clone;
                        match Command::new("runuser")
                            .args(["-u", &user, "--", CLI_BINARY_NAME, "_", "open-uninstall-page"])
                            .output()
                            .await
                        {
                            Ok(output) if output.status.success() => {
                                debug!(user, "Opened uninstall page");
                            },
                            Ok(output) => error!(user, ?output, "Failed to open uninstall page"),
                            Err(err) => error!(user, ?err, "Failed to open uninstall page"),
                        }
                    });

                    debug!(user, "Uninstalling integrations");
                    match Command::new("runuser")
                        .args([
                            "-u",
                            user,
                            "--",
                            CLI_BINARY_NAME,
                            "integrations",
                            "uninstall",
                            "--silent",
                            "all",
                        ])
                        .output()
                        .await
                    {
                        Ok(output) if output.status.success() => {
                            debug!(user, "Uninstalled integrations");
                            integrations_uninstalled = true;
                        },
                        Ok(output) => error!(user, ?output, "Failed to uninstall integrations"),
                        Err(err) => error!(user, ?err, "Failed to uninstall integrations"),
                    }

                    debug!(user, "Removing data dir");
                    match Command::new("runuser")
                        .args(["-u", user, "--", CLI_BINARY_NAME, "_", "remove-data-dir", "--force"])
                        .output()
                        .await
                    {
                        Ok(output) if output.status.success() => {
                            debug!(user, "Removed data dir");
                            data_dir_removed = true;
                        },
                        Ok(output) => error!(user, ?output, "Failed to remove data dir"),
                        Err(err) => error!(user, ?err, "Failed to remove data dir"),
                    }
                }

                // emit.await.ok();

                if !integrations_uninstalled {
                    bail!("Failed to uninstall completely: integrations were not uninstalled");
                }

                if !data_dir_removed {
                    bail!("Failed to uninstall completely: data directory was not removed");
                }

                Ok(ExitCode::SUCCESS)
            },
            InternalSubcommand::RemoveDataDir { force } => remove_data_dir(ctx, force).await,
            InternalSubcommand::StreamFromSocket => {
                let mut stdout = tokio::io::stdout();
                let mut stdin = tokio::io::stdin();

                let mut stdout_buf = BytesMut::with_capacity(1024);
                let mut stream_buf = BytesMut::with_capacity(1024);

                let socket = directories::remote_socket_path()?;
                while let Ok(mut stream) = BufferedUnixStream::connect_timeout(&socket, Duration::from_secs(5)).await {
                    loop {
                        select! {
                            n = stream.read_buf(&mut stdout_buf) => {
                                match n {
                                    Ok(0) | Err(_) => {
                                        break;
                                    }
                                    Ok(mut n) => {
                                        while !stdout_buf.is_empty() {
                                            let m = stdout.write(&stdout_buf[..n]).await?;
                                            stdout.flush().await?;
                                            stdout_buf.advance(m);
                                            n -= m;
                                        }
                                        stdout_buf.clear();
                                    }
                                }
                            }
                            n = stdin.read_buf(&mut stream_buf) => {
                                match n {
                                    Ok(0) | Err(_) => {
                                        break;
                                    }
                                    Ok(mut n) => {
                                        while !stream_buf.is_empty() {
                                            let m = stream.write(&stream_buf[..n]).await?;
                                            stream.flush().await?;
                                            stream_buf.advance(m);
                                            n -= m;
                                        }
                                        stream_buf.clear();
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(ExitCode::SUCCESS)
            },
            InternalSubcommand::Uuidgen => {
                let _ = writeln!(stdout(), "{}", uuid::Uuid::new_v4());
                Ok(ExitCode::SUCCESS)
            },
            #[cfg(target_os = "linux")]
            InternalSubcommand::IbusBootstrap => {
                use std::ffi::OsString;

                use sysinfo::{
                    ProcessRefreshKind,
                    RefreshKind,
                };
                use tokio::process::Command;

                let system = tokio::task::block_in_place(|| {
                    System::new_with_specifics(RefreshKind::new().with_processes(ProcessRefreshKind::new()))
                });
                let ibus_daemon = OsString::from("ibus-daemon");
                if system.processes_by_name(&ibus_daemon).next().is_none() {
                    info!("Launching 'ibus-daemon'");
                    match Command::new("ibus-daemon").arg("-drxR").output().await {
                        Ok(std::process::Output { status, stdout, stderr }) if !status.success() => {
                            let stdout = String::from_utf8_lossy(&stdout);
                            let stderr = String::from_utf8_lossy(&stderr);
                            eyre::bail!(
                                "Failed to run 'ibus-daemon -drxR': status={status:?} stdout={stdout:?} stderr={stderr:?}"
                            );
                        },
                        Err(err) => eyre::bail!("Failed to run 'ibus-daemon -drxR': {err}"),
                        Ok(_) => writeln!(stdout(), "ibus-daemon is now running").ok(),
                    };
                } else {
                    writeln!(stdout(), "ibus-daemon is already running").ok();
                }
                Ok(ExitCode::SUCCESS)
            },
            #[cfg(target_os = "linux")]
            InternalSubcommand::DetectSandbox => {
                use fig_util::system_info::linux::SandboxKind;
                let exit_code = match fig_util::system_info::linux::detect_sandbox() {
                    SandboxKind::None => {
                        println!("No sandbox detected");
                        0
                    },
                    SandboxKind::Flatpak => {
                        println!("You are in a Flatpak");
                        1
                    },
                    SandboxKind::Snap => {
                        println!("You are in a Snap");
                        1
                    },
                    SandboxKind::Docker => {
                        println!("You are in a Docker container");
                        1
                    },
                    SandboxKind::Container(None) => {
                        println!("You are in a generic container");
                        1
                    },
                    SandboxKind::Container(Some(engine)) => {
                        println!("You are in a {engine} container");
                        1
                    },
                };
                Ok(ExitCode::from(exit_code))
            },
            InternalSubcommand::OpenUninstallPage => {
                let url = fig_install::UNINSTALL_URL;
                match fig_util::open_url(url) {
                    Ok(()) => Ok(ExitCode::SUCCESS),
                    Err(err) => {
                        warn!(%err, "Failed to open uninstall directly, trying to open via desktop app");

                        match fig_ipc::local::send_command_to_socket(fig_proto::local::command::Command::OpenBrowser(
                            fig_proto::local::OpenBrowserCommand { url: url.into() },
                        ))
                        .await
                        {
                            Ok(_) => Ok(ExitCode::SUCCESS),
                            Err(err) => {
                                error!(%err, "Failed to open uninstall via desktop, no more options");
                                Ok(ExitCode::FAILURE)
                            },
                        }
                    },
                }
            },
            InternalSubcommand::PromptSsh { .. } => Ok(ExitCode::SUCCESS),
            InternalSubcommand::SshLocalCommand { remote_dest, uuid } => {
                // Ensure desktop app is running to avoid SSH errors on stdout when local side of
                // RemoteForward isn't listening
                launch_fig_desktop(LaunchArgs {
                    wait_for_socket: true,
                    open_dashboard: false,
                    immediate_update: false,
                    verbose: false,
                })
                .ok();

                if let Ok(session_id) = std::env::var(QTERM_SESSION_ID) {
                    let mut conn =
                        BufferedUnixStream::connect(fig_util::directories::figterm_socket_path(&session_id)?).await?;
                    conn.send_message(FigtermRequestMessage {
                        request: Some(FigtermRequest::NotifySshSessionStarted(
                            NotifySshSessionStartedRequest {
                                uuid,
                                remote_host: remote_dest,
                            },
                        )),
                    })
                    .await?;
                };

                Ok(ExitCode::SUCCESS)
            },
            #[cfg(target_os = "macos")]
            InternalSubcommand::AttemptToFinishInputMethodInstallation { bundle_path } => {
                match InputMethod::finish_input_method_installation(bundle_path) {
                    Ok(_) => Ok(ExitCode::SUCCESS),
                    Err(err) => {
                        println!(
                            "{}",
                            serde_json::to_string(&err).expect("InputMethodError should be serializable")
                        );
                        Ok(ExitCode::FAILURE)
                    },
                }
            },
            InternalSubcommand::DumpState { component } => {
                use fig_proto::local::dump_state_command::Type as StateCommandType;

                let state = fig_ipc::local::dump_state_command(match component {
                    StateComponent::Figterm => StateCommandType::DumpStateFigterm,
                    StateComponent::WebNotifications => StateCommandType::DumpStateWebNotifications,
                    StateComponent::Platform => StateCommandType::DumpStatePlatform,
                })
                .await
                .context("Failed to send dump state command")?;

                println!("{}", state.json);
                Ok(ExitCode::SUCCESS)
            },
            InternalSubcommand::FinishUpdate {
                relaunch_dashboard,
                delete_bundle,
            } => {
                // Wait some time for the previous installation to close
                tokio::time::sleep(Duration::from_millis(100)).await;

                crate::util::quit_fig(false).await.ok();

                tokio::time::sleep(Duration::from_millis(200)).await;

                if let Some(bundle_path) = delete_bundle {
                    let path = std::path::Path::new(&bundle_path);
                    if path.exists() {
                        tokio::fs::remove_dir_all(&path)
                            .await
                            .map_err(|err| tracing::warn!("Failed to remove {path:?}: {err}"))
                            .ok();
                    }

                    tokio::time::sleep(Duration::from_millis(200)).await;
                }

                launch_fig_desktop(LaunchArgs {
                    wait_for_socket: false,
                    open_dashboard: relaunch_dashboard,
                    immediate_update: false,
                    verbose: false,
                })
                .ok();

                Ok(ExitCode::SUCCESS)
            },
            #[cfg(target_os = "macos")]
            InternalSubcommand::SwapFiles {
                from,
                to,
                not_same_bundle_name,
            } => {
                use std::io::stderr;
                use std::os::unix::prelude::OsStrExt;

                let from_cstr = match std::ffi::CString::new(from.as_os_str().as_bytes()).context("Invalid from path") {
                    Ok(cstr) => cstr,
                    Err(err) => {
                        writeln!(stderr(), "Invalid from path: {err}").ok();
                        return Ok(ExitCode::FAILURE);
                    },
                };

                let to_cstr = match std::ffi::CString::new(to.as_os_str().as_bytes()) {
                    Ok(cstr) => cstr,
                    Err(err) => {
                        writeln!(stderr(), "Invalid to path: {err}").ok();
                        return Ok(ExitCode::FAILURE);
                    },
                };

                match fig_install::macos::install(from_cstr, to_cstr, !not_same_bundle_name) {
                    Ok(_) => {
                        writeln!(stdout(), "success").ok();
                        Ok(ExitCode::SUCCESS)
                    },
                    Err(err) => {
                        writeln!(stderr(), "Failed to swap files: {err}").ok();
                        Ok(ExitCode::FAILURE)
                    },
                }
            },
            #[cfg(target_os = "macos")]
            InternalSubcommand::BrewUninstall { zap } => {
                use fig_install::UNINSTALL_URL;

                let brew_is_reinstalling = crate::util::is_brew_reinstall().await;

                if brew_is_reinstalling {
                    // If we're reinstalling, we don't want to uninstall
                    return Ok(ExitCode::SUCCESS);
                } else if let Err(err) = fig_util::open_url_async(UNINSTALL_URL).await {
                    error!(%err, %UNINSTALL_URL, "Failed to open uninstall url");
                }

                let components = if zap {
                    // All except the desktop app
                    InstallComponents::all() & !InstallComponents::DESKTOP_APP
                } else {
                    InstallComponents::SHELL_INTEGRATIONS | InstallComponents::SSH
                };
                fig_install::uninstall(components, fig_os_shim::Context::new())
                    .await
                    .ok();
                Ok(ExitCode::SUCCESS)
            },
            InternalSubcommand::GenerateSsh(args) => args.execute().await,
            InternalSubcommand::InlineShellCompletion { buffer } => Ok(inline_shell_completion(buffer).await),
            InternalSubcommand::InlineShellCompletionAccept { buffer, suggestion } => {
                Ok(inline_shell_completion_accept(buffer, suggestion).await)
            },
            InternalSubcommand::Multiplexer(args) => match multiplexer::execute(args).await {
                Ok(()) => Ok(ExitCode::SUCCESS),
                Err(err) => {
                    error!("{err}");
                    let path = logs_dir()?.join("mux-crash.log");
                    tokio::fs::write(path, format!("{err:?}")).await?;
                    Err(err)
                },
            },
        }
    }
}

pub async fn pre_cmd(alias: Option<String>) -> ExitCode {
    let Ok(session_id) = std::env::var(QTERM_SESSION_ID) else {
        return ExitCode::FAILURE;
    };

    match figterm_socket_path(&session_id) {
        Ok(figterm_path) => match fig_ipc::socket_connect(figterm_path).await {
            Ok(mut figterm_stream) => {
                let message = FigtermRequestMessage {
                    request: Some(FigtermRequest::UpdateShellContext(UpdateShellContextRequest {
                        update_environment_variables: true,
                        environment_variables: std::env::vars()
                            .map(|(key, value)| EnvironmentVariable {
                                key,
                                value: Some(value),
                            })
                            .collect(),
                        update_alias: true,
                        alias,
                    })),
                };
                if let Err(err) = figterm_stream.send_message(message).await {
                    error!(%err, %session_id, "Failed to send UpdateShellContext to Figterm");
                    ExitCode::FAILURE
                } else {
                    ExitCode::SUCCESS
                }
            },
            Err(err) => {
                error!(%err, %session_id, "Failed to connect to Figterm socket");
                ExitCode::FAILURE
            },
        },
        Err(err) => {
            error!(%err, %session_id, "Failed to get Figterm socket path");
            ExitCode::FAILURE
        },
    }
}

async fn remove_data_dir(ctx: Arc<OsContext>, force: bool) -> Result<ExitCode> {
    if ctx.platform().os() != Os::Linux {
        bail!("remove-data-dir is only supported on Linux");
    }
    if !force {
        bail!("remove-data-dir is dangerous! If you meant to run this command, pass --force");
    }

    let data_dir = directories::fig_data_dir_ctx(&ctx)?;
    match ctx.fs().remove_dir_all(&data_dir).await {
        Ok(_) => (),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => (),
        Err(err) => bail!("Failed to remove data dir: {:?}", err),
    }

    let webview_dir = directories::local_webview_data_dir(&ctx)?;
    match ctx.fs().remove_dir_all(&webview_dir).await {
        Ok(_) => (),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => (),
        Err(err) => bail!("Failed to remove data dir: {:?}", err),
    }

    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[derive(Debug, Parser, PartialEq, Eq)]
    pub struct MockCli {
        #[command(subcommand)]
        pub subcommand: InternalSubcommand,
    }

    #[test]
    fn parse_pre_cmd() {
        assert_eq!(MockCli::parse_from(["_", "pre-cmd"]), MockCli {
            subcommand: InternalSubcommand::PreCmd { alias: None }
        });

        let alias = format!("a='{CLI_BINARY_NAME} a'\nrd=rmdir");
        assert_eq!(MockCli::parse_from(["_", "pre-cmd", "--alias", &alias]), MockCli {
            subcommand: InternalSubcommand::PreCmd { alias: Some(alias) }
        });

        let hyphen_alias = "-='cd -'\n...=../..\nga='git add'";
        assert_eq!(
            MockCli::parse_from(["_", "pre-cmd", "--alias", hyphen_alias]),
            MockCli {
                subcommand: InternalSubcommand::PreCmd {
                    alias: Some(hyphen_alias.to_owned())
                }
            }
        );
    }

    #[tokio::test]
    async fn test_remove_data_dir() {
        assert!(
            remove_data_dir(OsContext::builder().with_os(Os::Mac).build_fake(), true)
                .await
                .is_err(),
            "should fail on Mac"
        );

        let ctx = OsContext::builder()
            .with_os(Os::Linux)
            .with_test_home()
            .await
            .unwrap()
            .build_fake();

        assert!(
            remove_data_dir(Arc::clone(&ctx), false).await.is_err(),
            "should error if force not true"
        );
        assert!(
            remove_data_dir(Arc::clone(&ctx), true).await.is_ok(),
            "should succeed if directory doesn't exist"
        );

        // Verify that data dir is created and deleted correctly.
        let fs = ctx.fs();
        let data_dir = directories::fig_data_dir_ctx(&ctx).unwrap();
        fs.create_dir_all(&data_dir).await.unwrap();
        assert!(fs.exists(&data_dir), "data dir exists prior");
        assert!(remove_data_dir(Arc::clone(&ctx), true).await.is_ok());
        assert!(!fs.exists(&data_dir), "data dir should be deleted");
    }
}
