#[cfg(target_os = "linux")]
mod cleanup;
pub mod cli;
mod event_handler;
pub mod history;
pub mod inline;
pub mod input;
pub mod interceptor;
pub mod ipc;
pub mod logger;
mod message;
pub mod term;
pub mod update;

use std::env;
#[cfg(unix)]
use std::ffi::{
    CString,
    OsStr,
};
use std::iter::repeat;
use std::sync::{
    LazyLock,
    Mutex,
    RwLock,
};
use std::time::{
    Duration,
    SystemTime,
};

use alacritty_terminal::Term;
use alacritty_terminal::ansi::Processor;
use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::{
    ShellState,
    SizeInfo,
    TextBuffer,
};
use anyhow::{
    Context as _,
    Result,
    anyhow,
};
use bytes::BytesMut;
use cfg_if::cfg_if;
use clap::Parser;
use cli::Cli;
use fig_log::{
    LogArgs,
    initialize_logging,
};
use fig_os_shim::{
    Context,
    Env,
};
use fig_proto::local::{
    self,
    EnvironmentVariable,
    TerminalCursorCoordinates,
};
use fig_proto::remote::Hostbound;
use fig_proto::remote_hooks::{
    hook_to_message,
    new_edit_buffer_hook,
};
use fig_settings::state;
use fig_util::consts::CLI_BINARY_NAME;
use fig_util::env_var::{
    Q_LOG_LEVEL,
    Q_PARENT,
    Q_SHELL,
    Q_TERM,
    QTERM_SESSION_ID,
};
use fig_util::process_info::{
    Pid,
    PidExt,
};
#[cfg(unix)]
use fig_util::pty::unix::open_pty;
#[cfg(windows)]
use fig_util::pty::win::open_pty;
use fig_util::pty::{
    AsyncMasterPty,
    AsyncMasterPtyExt,
    CommandBuilder,
};
use fig_util::{
    PRODUCT_NAME,
    PTY_BINARY_NAME,
    Terminal as FigTerminal,
    directories,
};
use flume::{
    Receiver,
    Sender,
};
#[cfg(unix)]
use nix::unistd::execvp;
use portable_pty::PtySize;
use tokio::io::{
    self,
    AsyncWriteExt,
};
use tokio::sync::oneshot;
use tokio::{
    runtime,
    select,
};
use tracing::{
    debug,
    error,
    info,
    trace,
    warn,
};

use crate::event_handler::EventHandler;
use crate::input::{
    InputEvent,
    KeyCode,
    KeyCodeEncodeModes,
    KeyboardEncoding,
    Modifiers,
};
use crate::interceptor::KeyInterceptor;
use crate::ipc::{
    spawn_figterm_ipc,
    spawn_remote_ipc,
};
use crate::message::{
    process_figterm_message,
    process_remote_message,
};
use crate::term::{
    SystemTerminal,
    Terminal,
};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

const BUFFER_SIZE: usize = 16384;

static INSERT_ON_NEW_CMD: Mutex<Option<(String, bool, bool)>> = Mutex::new(None);
static INSERTION_LOCKED_AT: RwLock<Option<SystemTime>> = RwLock::new(None);
static EXPECTED_BUFFER: Mutex<String> = Mutex::new(String::new());

static SHELL_ENVIRONMENT_VARIABLES: Mutex<Vec<EnvironmentVariable>> = Mutex::new(Vec::new());
static SHELL_ALIAS: Mutex<Option<String>> = Mutex::new(None);

static USER_ENABLED_SHELLS: LazyLock<Vec<String>> = LazyLock::new(|| {
    fig_settings::state::get("user.enabled-shells")
        .ok()
        .flatten()
        .unwrap_or_default()
});

static HOSTNAME: LazyLock<Option<String>> = LazyLock::new(sysinfo::System::host_name);

pub enum MainLoopEvent {
    Insert {
        insert: Vec<u8>,
        unlock: bool,
        bracketed: bool,
        execute: bool,
    },
    UnlockInterception,
    SetImmediateMode(bool),
    PromptSSH {
        uuid: String,
        remote_host: String,
    },
    SetCsiU,
    UnsetCsiU,
}

fn shell_state_to_context(shell_state: &ShellState) -> local::ShellContext {
    let terminal = FigTerminal::parent_terminal(&Context::new()).map(|s| s.to_string());

    local::ShellContext {
        pid: shell_state.local_context.pid,
        ttys: shell_state.local_context.tty.clone(),
        process_name: shell_state.local_context.shell.clone(),
        shell_path: shell_state
            .local_context
            .shell_path
            .clone()
            .map(|path| path.display().to_string()),
        wsl_distro: shell_state.local_context.wsl_distro.clone(),
        current_working_directory: shell_state
            .local_context
            .current_working_directory
            .clone()
            .map(|cwd| cwd.display().to_string()),
        session_id: shell_state.local_context.session_id.clone(),
        terminal,
        hostname: shell_state
            .local_context
            .username
            .as_deref()
            .and_then(|username| HOSTNAME.as_deref().map(|hostname| format!("{username}@{hostname}"))),
        environment_variables: SHELL_ENVIRONMENT_VARIABLES.lock().unwrap().clone(),
        qterm_version: Some(env!("CARGO_PKG_VERSION").into()),
        preexec: Some(shell_state.preexec),
        osc_lock: Some(shell_state.osc_lock),
        alias: SHELL_ALIAS.lock().unwrap().clone(),
    }
}

#[allow(clippy::needless_return)]
fn get_cursor_coordinates(terminal: &dyn Terminal) -> Option<TerminalCursorCoordinates> {
    cfg_if! {
        if #[cfg(target_os = "windows")] {
            use term::cast;

            let coordinate = terminal.get_cursor_coordinate().ok()?;
            let screen_size = terminal.get_screen_size().ok()?;
            return Some(TerminalCursorCoordinates {
                x: cast(coordinate.cols).ok()?,
                y: cast(coordinate.rows).ok()?,
                xpixel: cast(screen_size.xpixel).ok()?,
                ypixel: cast(screen_size.ypixel).ok()?,
            });
        } else {
            let _terminal = terminal;
            return None;
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn _should_install_remote_ssh_integration(
    uuid: String,
    remote_host: String,
    main_loop_tx: Sender<MainLoopEvent>,
    remote_receiver: Receiver<fig_proto::remote::Clientbound>,
    remote_sender: Sender<Hostbound>,
    term: &Term<EventHandler>,
    pty_master: &mut Box<dyn AsyncMasterPty + Send + Sync>,
    key_interceptor: &mut KeyInterceptor,
) -> Option<bool> {
    use fig_proto::remote::clientbound;

    let remote_install_setting = fig_settings::settings::get_string_or("ssh.remote-prompt", "ask".into());
    if remote_install_setting == "never" {
        return Some(false);
    }

    let key = format!("ssh.remote-prompt.disable-host.{remote_host}");
    let disable_host = fig_settings::state::get_bool_or(key, false);
    if disable_host {
        return Some(false);
    }

    let prompt_timeout: u64 = fig_settings::settings::get_int_or("ssh.remote-prompt.timeout", 2000)
        .try_into()
        .unwrap_or(2000);

    // Wait for child ssh session to connect to local desktop instance.
    let got_child_connection = tokio::time::timeout(tokio::time::Duration::from_millis(prompt_timeout), async {
        loop {
            if let Ok(msg) = remote_receiver.recv_async().await {
                if let Some(clientbound::Packet::NotifyChildSessionStarted(clientbound::NotifyChildSessionStarted {
                    parent_id,
                })) = msg.packet
                {
                    if parent_id == uuid {
                        return true;
                    }
                } else {
                    process_remote_message(
                        msg,
                        main_loop_tx.clone(),
                        remote_sender.clone(),
                        term,
                        pty_master,
                        key_interceptor,
                    )
                    .await
                    .ok();
                }
            }
        }
    })
    .await
    .is_ok();

    if got_child_connection {
        return Some(false);
    }

    if remote_install_setting == "always" {
        return Some(true);
    }

    None
}

fn can_send_edit_buffer<T>(term: &Term<T>) -> bool
where
    T: EventListener,
{
    let shell_enabled = ["bash", "zsh", "fish", "nu", "dash"]
        .into_iter()
        .chain(USER_ENABLED_SHELLS.iter().map(|s| s.as_str()))
        .any(|s| {
            let shell_raw = term.shell_state().get_context().shell.as_deref();
            // we actually want to work with a nested figterm :)
            let shell = match shell_raw.and_then(|s| s.strip_suffix(" (figterm)")) {
                Some(s) => Some(s),
                None => shell_raw,
            };

            shell == Some(s)
        });
    let preexec = term.shell_state().preexec;

    let mut handle = INSERTION_LOCKED_AT.write().unwrap();
    let insertion_locked = match handle.as_ref() {
        Some(at) => {
            let lock_expired = at.elapsed().unwrap_or(Duration::ZERO) > Duration::from_millis(16);
            let should_unlock = lock_expired
                || term
                    .get_current_buffer()
                    .is_none_or(|buff| &buff.buffer == (&EXPECTED_BUFFER.lock().unwrap() as &String));
            if should_unlock {
                handle.take();
                if lock_expired {
                    trace!("insertion lock released because lock expired");
                } else {
                    trace!("insertion lock released because buffer looks like how we expect");
                }
                false
            } else {
                true
            }
        },
        None => false,
    };
    drop(handle);

    trace!(%shell_enabled, %preexec, %insertion_locked, "can_send_edit_buffer");

    shell_enabled && !insertion_locked && !preexec
}

const Q_DISABLE_AUTOCOMPLETE: &str = "Q_DISABLE_AUTOCOMPLETE";

fn autocomplete_enabled(env: &Env) -> bool {
    env.get_os(Q_DISABLE_AUTOCOMPLETE).is_none_or(|s| s.is_empty())
}

static AUTOCOMPLETE_ENABLED: LazyLock<bool> = LazyLock::new(|| autocomplete_enabled(&Env::new()));

async fn send_edit_buffer<T>(
    term: &Term<T>,
    sender: &Sender<Hostbound>,
    cursor_coordinates: Option<TerminalCursorCoordinates>,
) -> Result<()>
where
    T: EventListener,
{
    if !*AUTOCOMPLETE_ENABLED {
        return Ok(());
    }

    match term.get_current_buffer() {
        Some(edit_buffer) => {
            if let Some(cursor_idx) = edit_buffer.cursor_idx.and_then(|i| i.try_into().ok()) {
                debug!("edit_buffer: {edit_buffer:?}");
                trace!("buffer bytes: {:02X?}", edit_buffer.buffer.as_bytes());
                trace!("buffer chars: {:?}", edit_buffer.buffer.chars().collect::<Vec<_>>());

                let context = shell_state_to_context(term.shell_state());

                let edit_buffer_hook =
                    new_edit_buffer_hook(Some(context), edit_buffer.buffer, cursor_idx, 0, cursor_coordinates);
                let message = hook_to_message(edit_buffer_hook);

                trace!("Sending: {message:?}");

                sender.send_async(message).await?;
            }
            Ok(())
        },
        None => Err(anyhow!("No edit buffer to send")),
    }
}

fn get_parent_shell() -> Result<String> {
    match env::var(Q_SHELL).ok().filter(|s| !s.is_empty()) {
        Some(v) => Ok(v),
        None => match env::var("SHELL").ok().filter(|s| !s.is_empty()) {
            Some(shell) => Ok(shell),
            None => {
                anyhow::bail!("No Q_SHELL or SHELL found");
            },
        },
    }
}

fn build_shell_command(command: Option<&[String]>) -> Result<CommandBuilder> {
    let mut builder = match command {
        Some(command) => {
            let mut iter = command.iter().map(|s| s.as_str());

            let mut builder = CommandBuilder::new(iter.next().unwrap());
            for arg in iter {
                builder.arg(arg);
            }
            builder
        },
        None => {
            let parent_shell = get_parent_shell()?;
            let mut builder = CommandBuilder::new(parent_shell);

            if env::var("Q_IS_LOGIN_SHELL").ok().as_deref() == Some("1") {
                builder.arg("--login");
            }

            if let Some(execution_string) = env::var("Q_EXECUTION_STRING").ok().filter(|s| !s.is_empty()) {
                builder.args(["-c", &execution_string]);
            }

            if let Some(extra_args) = env::var("Q_SHELL_EXTRA_ARGS").ok().filter(|s| !s.is_empty()) {
                builder.args(extra_args.split_whitespace().filter(|arg| arg != &"--login"));
            }

            builder
        },
    };

    builder.env(Q_TERM, env!("CARGO_PKG_VERSION"));
    if env::var_os("TMUX").is_some() {
        builder.env("Q_TERM_TMUX", env!("CARGO_PKG_VERSION"));
    }

    // Clean up environment and launch shell.
    builder.env_remove(Q_SHELL);
    builder.env_remove("Q_IS_LOGIN_SHELL");
    builder.env_remove("Q_START_TEXT");
    builder.env_remove("Q_SHELL_EXTRA_ARGS");
    builder.env_remove("Q_EXECUTION_STRING");

    if let Ok(dir) = std::env::current_dir() {
        builder.cwd(dir);
    }

    Ok(builder)
}

#[cfg(unix)]
fn launch_shell(command: Option<&[String]>) -> Result<()> {
    let cmd = build_shell_command(command)?.as_command()?;
    let mut args: Vec<&OsStr> = std::vec![cmd.get_program()];
    args.extend(cmd.get_args());

    let cargs: Vec<_> = args
        .into_iter()
        .map(|arg| CString::new(arg.to_string_lossy().as_ref()).expect("Failed to convert arg to CString"))
        .collect();
    for (key, val) in cmd.get_envs() {
        match val {
            Some(value) => env::set_var(key, value),
            None => {
                env::remove_var(key);
            },
        }
    }

    execvp(&cargs[0], &cargs).expect("Failed to execvp");
    unreachable!()
}

fn figterm_main(command: Option<&[String]>) -> Result<()> {
    fig_settings::settings::init_global().ok();
    fig_telemetry::init_global_telemetry_emitter();

    let context = Context::new();

    let session_id = match std::env::var("MOCK_QTERM_SESSION_ID") {
        Ok(id) => id,
        Err(_) => uuid::Uuid::new_v4().simple().to_string(),
    };
    std::env::set_var(QTERM_SESSION_ID, &session_id);

    let parent_id = std::env::var(Q_PARENT).ok();

    let mut terminal = SystemTerminal::new_from_stdio()?;
    let screen_size = terminal.get_screen_size()?;

    let pty_size = PtySize {
        rows: screen_size.rows as u16,
        cols: screen_size.cols as u16,
        pixel_width: screen_size.xpixel as u16,
        pixel_height: screen_size.ypixel as u16,
    };

    let pty = open_pty(&pty_size).context("Failed to open pty")?;
    let command = build_shell_command(command)?;

    let pty_name = pty.slave.get_name().unwrap_or_else(|| session_id.clone());

    let _log_guard = match initialize_logging(LogArgs {
        log_level: None,
        log_to_stdout: false,
        log_file_path: Some(directories::logs_dir()?.join(format!("{PTY_BINARY_NAME}{pty_name}.log"))),
        delete_old_log_file: true,
    }) {
        Ok(logger_guard) => Some(logger_guard),
        Err(err) => {
            if !fig_settings::state::get_bool_or("pty.suppress_log_error", false) {
                // let id = capture_anyhow(&err);
                eprintln!("Fig failed to init logger: {err:?}");
            }
            None
        },
    };

    logger::stdio_debug_log(format!("pty name: {pty_name}"));
    logger::stdio_debug_log("Forking child shell process");

    #[cfg(unix)]
    {
        let pid = nix::unistd::getpid();
        logger::stdio_debug_log(format!("Parent pid: {pid}"));
    }

    let mut child = pty.slave.spawn_command(command)?;
    info!("Shell: {:?}", child.process_id());
    if let Some(pid) = child.process_id() {
        logger::stdio_debug_log(format!("Child pid: {pid}"));
    }

    let (child_tx, mut child_rx) = oneshot::channel();
    std::thread::spawn(move || child_tx.send(child.wait()));

    info!("Pid: {}", Pid::current());
    info!("Pty name: {pty_name}");

    let runtime = runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name_fn(|| {
            static ATOMIC_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let id = ATOMIC_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            format!("{PTY_BINARY_NAME}-runtime-worker-{id}")
        })
        .build()?;

    let runtime_result = runtime.block_on(async {
        update::check_for_update(&context);

        terminal.set_raw_mode()?;

        let (main_loop_tx, main_loop_rx) = flume::bounded::<MainLoopEvent>(16);

        let history_sender = history::spawn_history_task().await;

        // Spawn thread to handle figterm ipc
        let incoming_receiver = spawn_figterm_ipc(&session_id).await?;

        // Spawn thread to handle remote ipc
        let (remote_sender, remote_receiver, stop_ipc_tx) = spawn_remote_ipc(
            session_id.clone(),
            parent_id,
            main_loop_tx.clone()
        ).await?;

        let mut stdout = io::stdout();
        let mut master = pty.master.get_async_master_pty()?;

        let mut processor = Processor::new();
        let size = SizeInfo::new(pty_size.rows as usize, pty_size.cols as usize);
        let event_sender = EventHandler::new(remote_sender.clone(), history_sender.clone(), main_loop_tx.clone());
        let mut term = alacritty_terminal::Term::new(size, event_sender, 1, session_id.clone());

        #[cfg(target_os = "windows")]
        term.set_windows_delay_end_prompt(true);

        let mut write_buffer: Vec<u8> = vec![0; BUFFER_SIZE];

        let mut key_interceptor = KeyInterceptor::new();
        key_interceptor.load_key_intercepts()?;

        let mut edit_buffer_interval = tokio::time::interval(Duration::from_millis(16));

        let mut first_time = true;

        let input_rx = terminal.read_input()?;

        let key_code_encode_mode = KeyCodeEncodeModes {
            #[cfg(unix)]
            encoding: KeyboardEncoding::Xterm,
            #[cfg(windows)]
            encoding: KeyboardEncoding::Win32,
            application_cursor_keys: false,
            newline_mode: false,
        };

        let ai_enabled = fig_settings::settings::get_bool_or("ai.terminal-hash-sub", true);

        if let Ok(shell) = get_parent_shell() {
            let path = std::path::Path::new(&shell);
            let name = path.file_name().and_then(|name| name.to_str()).unwrap_or(shell.as_str());
            let title_osc = format!("\x1b]0;{name}\x07");
            if let Err(err) = stdout.write(title_osc.as_bytes()).await {
                error!("Failed to write title osc: {err}");
            }
        }

        let mut csi_u_set = false;

        let result: Result<()> = 'select_loop: loop {
            if first_time && term.shell_state().has_seen_prompt {
                trace!("Has seen prompt and first time");
                let initial_command = env::var("Q_START_TEXT").ok().filter(|s| !s.is_empty());
                if let Some(mut initial_command) = initial_command {
                    debug!("Sending initial text: {initial_command}");
                    initial_command.push('\n');
                    if let Err(err) = master.write_all(initial_command.as_bytes()).await {
                        error!("Failed to write initial command: {err}");
                    }
                }
                first_time = false;
            }

            let select_result: Result<()> = select! {
                biased;
                res = main_loop_rx.recv_async() => {
                    match res {
                        Ok(event) => {
                            match event {
                                MainLoopEvent::Insert { insert, unlock, bracketed, execute } => {
                                    use bstr::ByteSlice;
                                    if bracketed {
                                        if term.mode().contains(alacritty_terminal::term::TermMode::BRACKETED_PASTE) {
                                            master.write_all(b"\x1b[200~").await?;
                                            master.write_all(&insert.replace(b"\x1b", "")).await?;
                                            master.write_all(b"\x1b[201~").await?;
                                        } else {
                                            master.write_all(&insert.replace("\r\n", "\r").replace("\n", "\r")).await?;
                                        }
                                    } else {
                                        master.write_all(&insert).await?;
                                    }

                                    if execute {
                                        master.write_all(b"\r").await?; 
                                    }

                                    if unlock {
                                        key_interceptor.reset();
                                    }
                                },
                                MainLoopEvent::UnlockInterception => {
                                    key_interceptor.reset();
                                },
                                MainLoopEvent::SetImmediateMode(mode) => {
                                    if let Err(err) = terminal.set_immediate_mode(mode) {
                                        error!(%err, "Failed to set immediate mode");
                                    }
                                },
                                MainLoopEvent::SetCsiU => {
                                    // Send CSI > 1 u
                                    stdout.write_all(b"\x1b[>1u").await?;
                                    stdout.flush().await?;
                                    csi_u_set = true;
                                },
                                MainLoopEvent::UnsetCsiU => {
                                    // Send CSI < u
                                    stdout.write_all(b"\x1b[<u").await?;
                                    stdout.flush().await?;
                                    csi_u_set = false;
                                },
                                MainLoopEvent::PromptSSH { uuid: _, remote_host: _ } => {
                                    // let should_install = should_install_remote_ssh_integration(
                                    //     uuid,
                                    //     remote_host.clone(),
                                    //     main_loop_tx.clone(),
                                    //     remote_receiver.clone(),
                                    //     remote_sender.clone(),
                                    //     &term,
                                    //     &mut master,
                                    //     &mut key_interceptor,
                                    // ).await;

                                    // let should_install = match should_install {
                                    //     Some(val) => val,
                                    //     None => {
                                    //         prompt_remote_integration_install(
                                    //             remote_host,
                                    //             console_term.clone(),
                                    //             console_term_key_tx.clone(),
                                    //             &mut terminal,
                                    //             input_rx.clone(),
                                    //         ).await.unwrap_or(false)
                                    //     }
                                    // };

                                    // if should_install {
                                    //     let installation_command = "curl -fSsL https://fig.io/install-minimal.sh | bash; exec $SHELL\n";
                                    //     master.write_all(installation_command.as_bytes()).await?;
                                    // }
                                }
                            }
                        }
                        Err(err) => warn!("Failed to recv: {err}"),
                    };
                    Ok(())
                }
                res = input_rx.recv_async() => {
                    let mut input_res = Ok(());
                    match res {
                        Ok(events) => {
                            let mut write_buffer = BytesMut::new();
                            for event in events {
                                match event {
                                    Ok((raw, InputEvent::Key(event))) => {
                                        // Do not do most stuff during not preexec since that means a command is running
                                        let preexec = term.shell_state().preexec;

                                        debug!(?event, ?raw, %preexec,  "Got key event");

                                        if !preexec && ai_enabled && event.key == KeyCode::Enter && event.modifiers == input::Modifiers::NONE {
                                            if let Some(TextBuffer { buffer, cursor_idx }) = term.get_current_buffer() {
                                                let buffer = buffer.trim();
                                                if buffer.len() > 1 && buffer.starts_with('#') && term.columns() > buffer.len() {
                                                    write_buffer.extend(
                                                        &repeat(b'\x08')
                                                            .take(buffer.len()
                                                            .max(cursor_idx.unwrap_or(0)))
                                                            .collect::<Vec<_>>()
                                                    );
                                                    write_buffer.extend(
                                                        format!(
                                                            "{} translate '{}'\r",
                                                            CLI_BINARY_NAME,
                                                            buffer
                                                                .trim_start_matches('#')
                                                                .trim()
                                                                .replace('\'', "'\"'\"'")
                                                            ).as_bytes()
                                                    );
                                                    master.write_all(&write_buffer).await?;
                                                    continue 'select_loop;
                                                }
                                            }
                                        }

                                        // if we are in CSI u mode we try to encode first, otherwise we try to send the raw bytes first
                                        let raw = if csi_u_set {
                                            event.key.encode(event.modifiers, key_code_encode_mode, true)
                                                .ok()
                                                .map(|s| s.into_bytes().into()).or(raw)
                                        } else {
                                            raw.or_else(|| {
                                                event.key.encode(event.modifiers, key_code_encode_mode, true)
                                                    .ok()
                                                    .map(|s| s.into_bytes().into())
                                            })
                                        };

                                        let handled_action = if !preexec {
                                            if let Some(action) = key_interceptor.intercept_key(&event) {
                                                debug!(?action, "Intercepted action");
                                                let s = raw.clone()
                                                    .and_then(|b| String::from_utf8(b.to_vec()).ok())
                                                    .unwrap_or_default();
                                                let context = shell_state_to_context(term.shell_state());
                                                let hook = fig_proto::remote_hooks::new_intercepted_key_hook(context, action, s);
                                                remote_sender.send(hook_to_message(hook)).unwrap();

                                                if event.key == KeyCode::Escape {
                                                    key_interceptor.reset();
                                                }
                                                true
                                            } else {
                                                false
                                            }
                                        } else {
                                            false
                                        };

                                        if !handled_action {
                                            if let Some(bytes) = raw {
                                                if (event.key == KeyCode::Char('c') || event.key == KeyCode::Char('d'))
                                                    && event.modifiers == Modifiers::CTRL {
                                                    key_interceptor.reset();
                                                }
                                                write_buffer.extend(&bytes);
                                            }
                                        }
                                    }
                                    Ok((_, InputEvent::Resized)) => {
                                        terminal.flush()?;

                                        let size = terminal.get_screen_size()?;
                                        let pty_size = PtySize {
                                            rows: size.rows as u16,
                                            cols: size.cols as u16,
                                            pixel_width: size.xpixel as u16,
                                            pixel_height: size.ypixel as u16,
                                        };

                                        master.resize(pty_size)?;
                                        let window_size = SizeInfo::new(size.rows, size.cols);
                                        debug!("Window size changed: {window_size:?}");
                                        term.resize(window_size);
                                    }
                                    Ok((None, InputEvent::Paste(string))) => {
                                        // Pass through bracketed pastes.
                                        if term.mode().contains(alacritty_terminal::term::TermMode::BRACKETED_PASTE) {
                                            write_buffer.extend(b"\x1b[200~");
                                            write_buffer.extend(string.replace('\x1b', "").as_bytes());
                                            write_buffer.extend(b"\x1b[201~");
                                        } else {
                                            write_buffer.extend(string.replace("\r\n", "\r").replace('\n', "\r").as_bytes());
                                        }
                                    }
                                    Ok((raw, _)) => {
                                        if let Some(raw) = raw {
                                            info!("Fallback write");
                                            write_buffer.extend(&raw);
                                        } else {
                                            info!("Unhandled input event with no raw pass-through data");
                                        }
                                    }
                                    Err(err) => {
                                        error!("Failed receiving input from stdin: {err}");
                                        input_res = Err(err);
                                        break;
                                    }
                                };
                            }
                            master.write_all(&write_buffer).await?;
                        }
                        Err(err) => {
                            warn!("Failed recv: {err}");
                        }
                    };
                    input_res
                }
                res = master.read(&mut write_buffer) => {
                    #[cfg(feature = "profiling_early_exit")]
                    break 'select_loop Ok(());
                    match res {
                        Ok(0) => {
                            trace!("EOF from master");
                            break 'select_loop Ok(());
                        },
                        Ok(size) => {
                            trace!("Read {size} bytes from master");

                            let old_delayed_count = term.get_delayed_events_count();
                            for byte in &write_buffer[..size] {
                                processor.advance(&mut term, *byte);
                            }

                            let delayed_count = term.get_delayed_events_count();

                            // We have delayed events and did not receive delayed events. Flush all
                            // delayed events now.
                            if delayed_count > 0 && delayed_count == old_delayed_count {
                                term.flush_delayed_events();
                            }

                            stdout.write_all(&write_buffer[..size]).await?;
                            stdout.flush().await?;

                            if write_buffer.capacity() == write_buffer.len() {
                                write_buffer.reserve(write_buffer.len());
                            }

                            if can_send_edit_buffer(&term) {
                                let cursor_coordinates = get_cursor_coordinates(&terminal);
                                if let Err(err) = send_edit_buffer(&term, &remote_sender, cursor_coordinates).await {
                                    warn!("Failed to send edit buffer: {err}");
                                }
                            }

                            Ok(())
                        }
                        Err(err) => {
                            error!("Failed to read from master: {err}");
                            break 'select_loop Ok(());
                        }
                    }
                }
                msg = remote_receiver.recv_async() => {
                    match msg {
                        Ok(message) => {
                            trace!("Received message from socket: {message:?}");
                            process_remote_message(
                                message,
                                main_loop_tx.clone(),
                                remote_sender.clone(),
                                &term,
                                &mut master,
                                &mut key_interceptor
                            ).await?;
                        }
                        Err(err) => {
                            error!("Failed to receive message from socket: {err}");
                        }
                    }
                    Ok(())
                }
                msg = incoming_receiver.recv_async() => {
                    match msg {
                        Ok((message, sender)) => {
                            debug!("Received message from figterm listener: {message:?}");
                            process_figterm_message(
                                message,
                                main_loop_tx.clone(),
                                sender.clone(),
                                &term,
                                &history_sender,
                                &mut master,
                                &mut key_interceptor,
                                &session_id,
                            ).await?;
                        }
                        Err(err) => {
                            error!("Failed to receive message from socket: {err}");
                        }
                    }
                    Ok(())
                }
                // Check if to send the edit buffer because of timeout
                _ = edit_buffer_interval.tick() => {
                    let send_eb = INSERTION_LOCKED_AT.read().unwrap().is_some();
                    if send_eb && can_send_edit_buffer(&term) {
                        let cursor_coordinates = get_cursor_coordinates(&terminal);
                        if let Err(err) = send_edit_buffer(&term, &remote_sender, cursor_coordinates).await {
                            warn!(%err, "Failed to send edit buffer");
                        }
                    }
                    Ok(())
                }
                _ = &mut child_rx => {
                    trace!("Shell process exited");
                    break 'select_loop Ok(());
                }
            };

            if let Err(err) = select_result {
                error!("Error in select loop: {err}");
                break 'select_loop Err(err);
            }
        };

        let _ = stop_ipc_tx.send(());
        fig_telemetry::finish_telemetry().await;

        result
    });

    // Reading from stdin is a blocking task on a separate thread:
    // https://github.com/tokio-rs/tokio/issues/2466
    // We must explicitly shutdown the runtime to exit.
    // This can cause resource leaks if we aren't careful about tasks we spawn.
    runtime.shutdown_background();

    // attempt cleanup
    #[cfg(target_os = "linux")]
    cleanup::cleanup()?;

    runtime_result
}

fn main() {
    let cli = Cli::parse();
    let command = cli.command.as_deref();

    logger::stdio_debug_log(format!("{Q_LOG_LEVEL}={}", fig_log::get_log_level()));

    if !state::get_bool_or("qterm.enabled", true) {
        println!("[NOTE] qterm is disabled. Autocomplete will not work.");
        logger::stdio_debug_log("qterm is disabled. `qterm.enabled` == false");
        return;
    }

    match figterm_main(command) {
        Ok(()) => {
            info!("Exiting");
        },
        Err(err) => {
            error!("Error in async runtime: {err}");
            println!("{PRODUCT_NAME} had an Error!: {err:?}");
            // capture_anyhow(&err);

            // Fallback to normal shell
            #[cfg(unix)]
            if let Err(err) = launch_shell(command) {
                // capture_anyhow(&err);
                logger::stdio_debug_log(err.to_string());
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autocomplete_enabled_test() {
        assert!(autocomplete_enabled(&Env::new_fake()));
        assert!(autocomplete_enabled(&Env::from_slice(&[(Q_DISABLE_AUTOCOMPLETE, "")])));
        assert!(!autocomplete_enabled(&Env::from_slice(&[(
            Q_DISABLE_AUTOCOMPLETE,
            "1"
        )])));
        assert!(!autocomplete_enabled(&Env::from_slice(&[(
            Q_DISABLE_AUTOCOMPLETE,
            "1"
        )])));
    }
}
