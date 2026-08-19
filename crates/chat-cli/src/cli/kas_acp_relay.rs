//! Transparent ACP stdio relay for CLI-owned KAS authentication.
//!
//! Complete, bounded NDJSON frames unrelated to authentication are forwarded
//! byte-for-byte. The relay consumes `_kiro/auth/getAccessToken` requests,
//! resolves credentials through the existing CLI auth handler, and writes the
//! matching JSON-RPC response back to the v3 engine without exposing either
//! request or response to the external client.
//!
//! Unix descendant tracking closes a gap left by process-group cleanup: v3
//! command and hook runners create separate groups, while SIGKILL bypasses the
//! engine's exit-hook reaper. Identity-checked polling records descendants
//! before they detach or are reparented, because ancestry cannot be recovered
//! afterward. Relay teardown intentionally terminates every recorded survivor
//! so the opt-in proxy does not orphan work that it launched.

use std::collections::HashMap;
#[cfg(unix)]
use std::collections::HashSet;
use std::future::Future;
use std::io::{
    self,
    Read as _,
    Write as _,
};
#[cfg(target_os = "linux")]
use std::os::fd::{
    AsRawFd,
    FromRawFd,
    OwnedFd,
};
use std::pin::Pin;
use std::process::{
    ExitCode,
    ExitStatus,
};
use std::sync::Arc;
#[cfg(unix)]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};
use std::task::{
    Context,
    Poll,
};
use std::time::Duration;
#[cfg(unix)]
use std::time::Instant;

use agent_client_protocol::{
    Error as AcpError,
    ExtRequest,
    ExtResponse,
    JsonRpcMessage,
    RequestId,
    Response,
};
use eyre::{
    Context as _,
    ContextCompat as _,
    Result,
    bail,
};
use futures::stream::{
    FuturesUnordered,
    StreamExt as _,
};
use serde_json::Value;
use serde_json::value::RawValue;
use tokio::io::{
    AsyncBufRead,
    AsyncBufReadExt as _,
    AsyncRead,
    AsyncWrite,
    AsyncWriteExt as _,
    BufReader,
    ReadBuf,
};
use tokio::process::Child;
use tokio::sync::{
    Mutex,
    Notify,
    mpsc,
    oneshot,
};

const KAS_AUTH_WIRE_METHOD: &str = "_kiro/auth/getAccessToken";
const KAS_EXIT_TIMEOUT: Duration = Duration::from_secs(2);
const KAS_GRACEFUL_EXIT_TIMEOUT: Duration = Duration::from_secs(5);
const KAS_WRITE_TIMEOUT: Duration = Duration::from_secs(2);
const AUTH_DRAIN_TIMEOUT: Duration =
    Duration::from_secs(chat_cli_v2::auth::kas_token::MAX_KAS_TOKEN_RESOLUTION_DURATION.as_secs() + 30);
#[cfg(unix)]
const KAS_TERMINATE_GRACE: Duration = Duration::from_millis(500);
#[cfg(unix)]
const PROCESS_TRACK_INTERVAL: Duration = Duration::from_millis(10);
/// ACP can carry images and other large payloads, so keep the cap generous
/// while preventing an unterminated frame from growing memory without bound.
const MAX_ACP_FRAME_BYTES: usize = 64 * 1024 * 1024;
const MAX_RETAINED_FRAME_CAPACITY: usize = 1024 * 1024;
const STDIO_CHANNEL_CAPACITY: usize = 2;
const STDIN_CHUNK_BYTES: usize = 8192;

type ResponseOwners = Arc<ResponseOwnership>;
type SharedKasStdin<W> = Arc<Mutex<Option<W>>>;
type AuthResult = std::result::Result<ExtResponse, AcpError>;

#[derive(Default)]
struct ResponseOwnership {
    owners: Mutex<HashMap<RequestId, ResponseOwner>>,
    pending_internal_responses: AtomicUsize,
    updates: Notify,
}

impl ResponseOwnership {
    async fn lock(&self) -> tokio::sync::MutexGuard<'_, HashMap<RequestId, ResponseOwner>> {
        self.owners.lock().await
    }

    fn track_internal_response(self: &Arc<Self>) -> PendingInternalResponse {
        self.pending_internal_responses.fetch_add(1, Ordering::AcqRel);
        self.changed();
        PendingInternalResponse(Arc::clone(self))
    }

    fn changed(&self) {
        self.updates.notify_waiters();
    }
}

struct PendingInternalResponse(ResponseOwners);

impl Drop for PendingInternalResponse {
    fn drop(&mut self) {
        if self.0.pending_internal_responses.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.0.changed();
        }
    }
}

enum ResponseOwner {
    Publishing { queued: Option<Vec<u8>> },
    Exposed,
    Forwarding,
}

enum ClientFrame {
    Forward,
    Response(RequestId),
    Drop,
}

enum KasFrame {
    Auth { id: RequestId, params: Arc<RawValue> },
    InvalidAuth,
    Forward { request_id: Option<RequestId> },
}

enum RelayCompletion {
    Child,
    Signal(ExitCode),
}

/// Async stdin backed by a detached blocking thread. Tokio's own stdin uses an
/// uncancellable runtime blocking task, which can hang runtime shutdown.
struct DetachedStdin {
    receiver: mpsc::Receiver<Vec<u8>>,
    buffered: Vec<u8>,
    position: usize,
}

impl DetachedStdin {
    fn new() -> Result<(Self, oneshot::Receiver<io::Result<()>>)> {
        let (sender, receiver) = mpsc::channel(STDIO_CHANNEL_CAPACITY);
        let (end_sender, end_receiver) = oneshot::channel();
        std::thread::Builder::new()
            .name("acp-auth-stdin".into())
            .spawn(move || {
                let stdin = io::stdin();
                let mut stdin = stdin.lock();
                let mut chunk = [0u8; STDIN_CHUNK_BYTES];
                let result = loop {
                    match stdin.read(&mut chunk) {
                        Ok(0) => break Ok(()),
                        Ok(read) if sender.blocking_send(chunk[..read].to_vec()).is_err() => break Ok(()),
                        Ok(_) => {},
                        Err(error) => break Err(error),
                    }
                };
                drop(sender);
                let _ = end_sender.send(result);
            })
            .context("failed to spawn ACP stdin reader")?;
        Ok((
            Self {
                receiver,
                buffered: Vec::new(),
                position: 0,
            },
            end_receiver,
        ))
    }
}

impl AsyncRead for DetachedStdin {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, output: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        if output.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        if self.position < self.buffered.len() {
            let remaining = &self.buffered[self.position..];
            let read = remaining.len().min(output.remaining());
            output.put_slice(&remaining[..read]);
            self.position += read;
            return Poll::Ready(Ok(()));
        }

        match self.receiver.poll_recv(cx) {
            Poll::Ready(Some(chunk)) => {
                let read = chunk.len().min(output.remaining());
                output.put_slice(&chunk[..read]);
                if read < chunk.len() {
                    self.buffered = chunk;
                    self.position = read;
                } else {
                    self.buffered.clear();
                    self.position = 0;
                }
                Poll::Ready(Ok(()))
            },
            Poll::Ready(None) => Poll::Ready(Ok(())),
            Poll::Pending => Poll::Pending,
        }
    }
}

enum StdoutCommand {
    Write(Vec<u8>, oneshot::Sender<io::Result<()>>),
    Flush(oneshot::Sender<io::Result<()>>),
}

/// Async frame sink backed by a detached blocking stdout thread. A parent that
/// stops draining stdout cannot prevent the Tokio runtime from shutting down.
struct DetachedStdout {
    sender: mpsc::Sender<StdoutCommand>,
}

impl DetachedStdout {
    fn new() -> Result<Self> {
        let (sender, mut receiver) = mpsc::channel(STDIO_CHANNEL_CAPACITY);
        std::thread::Builder::new()
            .name("acp-auth-stdout".into())
            .spawn(move || {
                let stdout = io::stdout();
                let mut stdout = stdout.lock();
                while let Some(command) = receiver.blocking_recv() {
                    match command {
                        StdoutCommand::Write(frame, response) => {
                            let result = stdout.write_all(&frame).and_then(|()| stdout.flush());
                            let _ = response.send(result);
                        },
                        StdoutCommand::Flush(response) => {
                            let _ = response.send(stdout.flush());
                        },
                    }
                }
            })
            .context("failed to spawn ACP stdout writer")?;
        Ok(Self { sender })
    }
}

pub(super) struct RelayIo {
    client_stdin: DetachedStdin,
    external_input_end: oneshot::Receiver<io::Result<()>>,
    client_stdout: DetachedStdout,
}

impl RelayIo {
    pub(super) fn new() -> Result<Self> {
        let (client_stdin, external_input_end) = DetachedStdin::new()?;
        let client_stdout = DetachedStdout::new()?;
        Ok(Self {
            client_stdin,
            external_input_end,
            client_stdout,
        })
    }
}

trait FrameSink {
    async fn write_frame(&mut self, frame: &[u8]) -> io::Result<()>;
    async fn flush_frames(&mut self) -> io::Result<()>;
}

#[cfg(test)]
struct AsyncFrameSink<W>(W);

#[cfg(test)]
impl<W> FrameSink for AsyncFrameSink<W>
where
    W: AsyncWrite + Unpin,
{
    async fn write_frame(&mut self, frame: &[u8]) -> io::Result<()> {
        self.0.write_all(frame).await
    }

    async fn flush_frames(&mut self) -> io::Result<()> {
        self.0.flush().await
    }
}

impl FrameSink for DetachedStdout {
    async fn write_frame(&mut self, frame: &[u8]) -> io::Result<()> {
        let (response, result) = oneshot::channel();
        self.sender
            .send(StdoutCommand::Write(frame.to_vec(), response))
            .await
            .map_err(|_send_error| io::Error::new(io::ErrorKind::BrokenPipe, "ACP stdout writer stopped"))?;
        result.await.map_err(io::Error::other)?
    }

    async fn flush_frames(&mut self) -> io::Result<()> {
        let (response, result) = oneshot::channel();
        self.sender
            .send(StdoutCommand::Flush(response))
            .await
            .map_err(|_send_error| io::Error::new(io::ErrorKind::BrokenPipe, "ACP stdout writer stopped"))?;
        result.await.map_err(io::Error::other)?
    }
}

#[derive(Default)]
struct AuthTracker {
    active: AtomicUsize,
    idle: Notify,
}

impl AuthTracker {
    fn spawn<Fut>(self: &Arc<Self>, future: Fut) -> oneshot::Receiver<AuthResult>
    where
        Fut: Future<Output = AuthResult> + Send + 'static,
    {
        struct ActiveGuard(Arc<AuthTracker>);

        impl Drop for ActiveGuard {
            fn drop(&mut self) {
                if self.0.active.fetch_sub(1, Ordering::AcqRel) == 1 {
                    self.0.idle.notify_waiters();
                }
            }
        }

        self.active.fetch_add(1, Ordering::AcqRel);
        let tracker = Arc::clone(self);
        let (response, result) = oneshot::channel();
        std::mem::drop(tokio::spawn(async move {
            let _guard = ActiveGuard(tracker);
            let _ = response.send(future.await);
        }));
        result
    }

    async fn wait_idle(&self) {
        loop {
            let notified = self.idle.notified();
            if self.active.load(Ordering::Acquire) == 0 {
                return;
            }
            notified.await;
        }
    }
}

pub(super) struct ShutdownSignals {
    #[cfg(unix)]
    interrupt: tokio::signal::unix::Signal,
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
    #[cfg(unix)]
    hangup: tokio::signal::unix::Signal,
    #[cfg(windows)]
    ctrl_c: tokio::signal::windows::CtrlC,
    #[cfg(windows)]
    ctrl_break: tokio::signal::windows::CtrlBreak,
    #[cfg(windows)]
    ctrl_close: tokio::signal::windows::CtrlClose,
}

impl ShutdownSignals {
    pub(super) fn new() -> Result<Self> {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{
                SignalKind,
                signal,
            };
            Ok(Self {
                interrupt: signal(SignalKind::interrupt()).context("failed to register SIGINT handler")?,
                terminate: signal(SignalKind::terminate()).context("failed to register SIGTERM handler")?,
                hangup: signal(SignalKind::hangup()).context("failed to register SIGHUP handler")?,
            })
        }
        #[cfg(windows)]
        {
            use tokio::signal::windows::{
                ctrl_break,
                ctrl_c,
                ctrl_close,
            };
            Ok(Self {
                ctrl_c: ctrl_c().context("failed to register Ctrl-C handler")?,
                ctrl_break: ctrl_break().context("failed to register Ctrl-Break handler")?,
                ctrl_close: ctrl_close().context("failed to register console-close handler")?,
            })
        }
        #[cfg(not(any(unix, windows)))]
        Ok(Self {})
    }

    pub(super) async fn recv(&mut self) -> ExitCode {
        #[cfg(unix)]
        {
            tokio::select! {
                _ = self.interrupt.recv() => ExitCode::from(128 + libc::SIGINT as u8),
                _ = self.terminate.recv() => ExitCode::from(128 + libc::SIGTERM as u8),
                _ = self.hangup.recv() => ExitCode::from(128 + libc::SIGHUP as u8),
            }
        }
        #[cfg(windows)]
        {
            tokio::select! {
                _ = self.ctrl_c.recv() => ExitCode::from(130),
                _ = self.ctrl_break.recv() => ExitCode::from(130),
                _ = self.ctrl_close.recv() => ExitCode::from(1),
            }
        }
        #[cfg(not(any(unix, windows)))]
        std::future::pending().await
    }
}

pub(super) fn configure_proxied_command(command: &mut tokio::process::Command) {
    #[cfg(unix)]
    command.process_group(0);
    #[cfg(windows)]
    command.creation_flags(windows::Win32::System::Threading::CREATE_SUSPENDED.0);
    #[cfg(not(any(unix, windows)))]
    let _ = command;
}

struct ProcessTree {
    #[cfg(unix)]
    unix: UnixProcessTree,
    #[cfg(windows)]
    windows: WindowsJob,
}

impl ProcessTree {
    fn new(child: &Child, pid: u32) -> Result<Self> {
        #[cfg(unix)]
        {
            let _ = child;
            Ok(Self {
                unix: UnixProcessTree::new(pid)?,
            })
        }
        #[cfg(windows)]
        {
            let _ = pid;
            Ok(Self {
                windows: WindowsJob::new(child)?,
            })
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (child, pid);
            Ok(Self {})
        }
    }

    async fn terminate_and_reap(&mut self, child: &mut Child, child_already_exited: bool) -> Result<ExitStatus> {
        #[cfg(not(unix))]
        let _ = child_already_exited;
        #[cfg(unix)]
        {
            self.unix.terminate_and_reap(child, child_already_exited).await
        }
        #[cfg(windows)]
        {
            let termination = self.windows.terminate();
            let status = tokio::time::timeout(KAS_EXIT_TIMEOUT, child.wait()).await;
            match (termination, status) {
                (Ok(()), Ok(status)) => status.context("failed to reap the v3 engine process"),
                (Ok(()), Err(_)) => bail!("timed out reaping the v3 engine process after Job Object termination"),
                (Err(error), Ok(Ok(_))) => Err(error),
                (Err(error), Ok(Err(wait_error))) => {
                    tracing::warn!(%wait_error, "failed to reap the v3 engine process after a Job Object termination error");
                    Err(error)
                },
                (Err(error), Err(_)) => {
                    Err(error).context("timed out reaping the v3 engine process after a Job Object termination error")
                },
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            if child.id().is_some() {
                child
                    .start_kill()
                    .context("failed to terminate the v3 engine process")?;
            }
            tokio::time::timeout(KAS_EXIT_TIMEOUT, child.wait())
                .await
                .map_err(|_elapsed| eyre::eyre!("timed out reaping the v3 engine process"))?
                .context("failed to reap the v3 engine process")
        }
    }
}

#[cfg(unix)]
#[derive(Clone, Copy, PartialEq, Eq)]
struct ProcessIdentity {
    #[cfg(target_os = "linux")]
    start_time_ticks: u64,
    #[cfg(target_os = "macos")]
    start_time_seconds: u64,
    #[cfg(target_os = "macos")]
    start_time_microseconds: u64,
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pid: u32,
}

#[cfg(unix)]
#[derive(Clone, Copy)]
struct ProcessRecord {
    parent_pid: u32,
    identity: ProcessIdentity,
    zombie: bool,
}

#[cfg(unix)]
struct UnixProcessTree {
    root_pid: u32,
    root_identity: ProcessIdentity,
    known: Arc<std::sync::Mutex<HashMap<u32, ProcessIdentity>>>,
    stop: Arc<AtomicBool>,
    tracker: Option<std::thread::JoinHandle<()>>,
}

#[cfg(unix)]
impl UnixProcessTree {
    fn new(root_pid: u32) -> Result<Self> {
        let root_identity = read_process_record(root_pid)
            .context("failed to inspect the v3 engine process before process-tree tracking")?
            .identity;
        let known = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let tracker = spawn_process_tracker(root_pid, root_identity, Arc::clone(&known), Arc::clone(&stop))?;
        Ok(Self {
            root_pid,
            root_identity,
            known,
            stop,
            tracker: Some(tracker),
        })
    }

    fn restart_tracking(&mut self) -> Result<()> {
        self.tracker = Some(spawn_process_tracker(
            self.root_pid,
            self.root_identity,
            Arc::clone(&self.known),
            Arc::clone(&self.stop),
        )?);
        Ok(())
    }

    fn stop_tracking(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(tracker) = self.tracker.take() {
            tracker.thread().unpark();
            if tracker.join().is_err() {
                tracing::warn!("the v3 engine process-tree tracker panicked");
            }
        }
    }

    async fn terminate_and_reap(&mut self, child: &mut Child, child_already_exited: bool) -> Result<ExitStatus> {
        let mut reaped_status = None;
        let mut first_error = None;
        if child_already_exited {
            let status = tokio::time::timeout(KAS_EXIT_TIMEOUT, child.wait())
                .await
                .map_err(|_elapsed| eyre::eyre!("timed out reaping the v3 engine process after clean exit"))?
                .context("failed to reap the v3 engine process after clean exit")?;
            reaped_status = Some(status);

            if live_known_processes(&self.known).is_empty() && !process_group_has_live_members(self.root_pid) {
                self.stop_tracking();
                if live_known_processes(&self.known).is_empty() && !process_group_has_live_members(self.root_pid) {
                    return Ok(status);
                }
                if let Err(error) = self.restart_tracking() {
                    first_error = Some(error);
                }
            }
        }

        if let Err(error) = signal_process_group(self.root_pid, libc::SIGTERM) {
            first_error.get_or_insert(error);
        }
        if let Err(error) = signal_known_processes(&self.known, libc::SIGTERM) {
            first_error.get_or_insert(error);
        }
        tokio::time::sleep(KAS_TERMINATE_GRACE).await;
        self.stop_tracking();

        if let Err(error) = signal_process_group(self.root_pid, libc::SIGKILL) {
            first_error.get_or_insert(error);
        }
        if let Err(error) = signal_known_processes(&self.known, libc::SIGKILL) {
            first_error.get_or_insert(error);
        }
        if let Err(error) = child.start_kill()
            && !matches!(error.kind(), io::ErrorKind::InvalidInput | io::ErrorKind::NotFound)
        {
            first_error.get_or_insert(error.into());
        }

        let status = match reaped_status {
            Some(status) => status,
            None => tokio::time::timeout(KAS_EXIT_TIMEOUT, child.wait())
                .await
                .map_err(|_elapsed| eyre::eyre!("timed out reaping the v3 engine process after SIGKILL"))?
                .context("failed to reap the v3 engine process")?,
        };
        let deadline = Instant::now() + KAS_TERMINATE_GRACE;
        loop {
            let survivors = live_known_processes(&self.known);
            if survivors.is_empty() {
                break;
            }
            if Instant::now() >= deadline {
                bail!("failed to terminate descendant processes of the v3 engine: {survivors:?}");
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }

        if let Some(error) = first_error {
            return Err(error).context("failed to terminate the complete process tree for the v3 engine");
        }
        Ok(status)
    }
}

#[cfg(unix)]
impl Drop for UnixProcessTree {
    fn drop(&mut self) {
        self.stop_tracking();
    }
}

#[cfg(unix)]
fn spawn_process_tracker(
    root_pid: u32,
    root_identity: ProcessIdentity,
    known: Arc<std::sync::Mutex<HashMap<u32, ProcessIdentity>>>,
    stop: Arc<AtomicBool>,
) -> Result<std::thread::JoinHandle<()>> {
    stop.store(false, Ordering::Release);
    std::thread::Builder::new()
        .name("acp-auth-process-tree".into())
        .spawn(move || {
            while !stop.load(Ordering::Acquire) {
                refresh_descendants(root_pid, root_identity, &known);
                std::thread::park_timeout(PROCESS_TRACK_INTERVAL);
            }
            refresh_descendants(root_pid, root_identity, &known);
        })
        .context("failed to spawn the process-tree tracker for the v3 engine")
}

#[cfg(unix)]
fn refresh_descendants(
    root_pid: u32,
    root_identity: ProcessIdentity,
    known: &std::sync::Mutex<HashMap<u32, ProcessIdentity>>,
) {
    let known_snapshot = known.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
    let mut pending = Vec::new();
    if process_matches(root_pid, root_identity) {
        pending.push(root_pid);
    }
    pending.extend(
        known_snapshot
            .iter()
            .filter_map(|(&pid, &identity)| process_matches(pid, identity).then_some(pid)),
    );

    let mut visited = HashSet::new();
    let mut discovered = HashMap::new();
    while let Some(parent) = pending.pop() {
        if !visited.insert(parent) {
            continue;
        }
        for child in direct_child_pids(parent) {
            let Some(record) = read_process_record(child) else {
                continue;
            };
            if record.parent_pid != parent || record.zombie {
                continue;
            }
            discovered.insert(child, record.identity);
            pending.push(child);
        }
    }

    let mut known = known.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    known.retain(|&pid, &mut identity| process_matches(pid, identity));
    known.extend(discovered);
}

#[cfg(target_os = "linux")]
fn direct_child_pids(parent: u32) -> Vec<u32> {
    let Ok(tasks) = std::fs::read_dir(format!("/proc/{parent}/task")) else {
        return Vec::new();
    };
    let mut children = HashSet::new();
    for task in tasks.flatten() {
        let Ok(contents) = std::fs::read_to_string(task.path().join("children")) else {
            continue;
        };
        children.extend(
            contents
                .split_ascii_whitespace()
                .filter_map(|pid| pid.parse::<u32>().ok()),
        );
    }
    children.into_iter().collect()
}

#[cfg(target_os = "macos")]
fn direct_child_pids(parent: u32) -> Vec<u32> {
    // PROC_PPID_ONLY is the stable libproc ABI selector from <libproc.h>.
    const PROC_PPID_ONLY: u32 = 6;
    let required = unsafe { libc::proc_listpids(PROC_PPID_ONLY, parent, std::ptr::null_mut(), 0) };
    if required <= 0 {
        return Vec::new();
    }
    let slots = required as usize / std::mem::size_of::<u32>() + 16;
    let mut children = vec![0_u32; slots];
    let buffer_bytes = children
        .len()
        .checked_mul(std::mem::size_of::<u32>())
        .and_then(|bytes| libc::c_int::try_from(bytes).ok())
        .unwrap_or(libc::c_int::MAX);
    let written = unsafe { libc::proc_listpids(PROC_PPID_ONLY, parent, children.as_mut_ptr().cast(), buffer_bytes) };
    if written <= 0 {
        return Vec::new();
    }
    children.truncate((written as usize / std::mem::size_of::<u32>()).min(children.len()));
    children.retain(|pid| *pid != 0);
    children
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
fn direct_child_pids(_parent: u32) -> Vec<u32> {
    // There is no portable targeted Unix child-enumeration API. Keep process-
    // group containment without introducing periodic host-wide process scans.
    Vec::new()
}

#[cfg(target_os = "linux")]
fn read_process_record(pid: u32) -> Option<ProcessRecord> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    parse_linux_process_stat(&stat)
}

#[cfg(target_os = "linux")]
fn parse_linux_process_stat(stat: &str) -> Option<ProcessRecord> {
    let (parent_pid, start_time_ticks, zombie) = parse_linux_process_stat_fields(stat)?;
    Some(ProcessRecord {
        parent_pid,
        identity: ProcessIdentity { start_time_ticks },
        zombie,
    })
}

#[cfg(any(target_os = "linux", test))]
fn parse_linux_process_stat_fields(stat: &str) -> Option<(u32, u64, bool)> {
    let mut fields = stat.get(stat.rfind(')')? + 1..)?.split_ascii_whitespace();
    let state = fields.next()?.as_bytes().first().copied()?;
    let parent_pid = fields.next()?.parse().ok()?;
    let start_time_ticks = fields.nth(17)?.parse().ok()?;
    Some((parent_pid, start_time_ticks, state == b'Z'))
}

#[cfg(target_os = "macos")]
fn read_process_record(pid: u32) -> Option<ProcessRecord> {
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let size = std::mem::size_of::<libc::proc_bsdinfo>();
    let read = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size as libc::c_int,
        )
    };
    if read != size as libc::c_int {
        return None;
    }
    let info = unsafe { info.assume_init() };
    Some(ProcessRecord {
        parent_pid: info.pbi_ppid,
        identity: ProcessIdentity {
            start_time_seconds: info.pbi_start_tvsec,
            start_time_microseconds: info.pbi_start_tvusec,
        },
        zombie: info.pbi_status == libc::SZOMB,
    })
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
fn read_process_record(pid: u32) -> Option<ProcessRecord> {
    let pid_value = libc::pid_t::try_from(pid).ok()?;
    let result = unsafe { libc::kill(pid_value, 0) };
    if result != 0 && io::Error::last_os_error().raw_os_error() != Some(libc::EPERM) {
        return None;
    }
    Some(ProcessRecord {
        parent_pid: 0,
        identity: ProcessIdentity { pid },
        zombie: false,
    })
}

#[cfg(unix)]
fn process_matches(pid: u32, identity: ProcessIdentity) -> bool {
    read_process_record(pid).is_some_and(|record| record.identity == identity && !record.zombie)
}

#[cfg(unix)]
fn signal_known_processes(known: &std::sync::Mutex<HashMap<u32, ProcessIdentity>>, signal: libc::c_int) -> Result<()> {
    let known = known.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
    let mut first_error = None;
    for (pid, identity) in known {
        if let Err(error) = signal_process_if_matches(pid, identity, signal) {
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

#[cfg(unix)]
fn signal_process_if_matches(pid: u32, identity: ProcessIdentity, signal: libc::c_int) -> Result<()> {
    if !process_matches(pid, identity) {
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    if let Some(pidfd) = open_pidfd(pid) {
        if !process_matches(pid, identity) {
            return Ok(());
        }
        let result = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                pidfd.as_raw_fd(),
                signal,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        };
        if result == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if matches!(error.raw_os_error(), Some(libc::ESRCH | libc::EPERM)) {
            return Ok(());
        }
        return Err(error).context("failed to signal a tracked descendant process of the v3 engine by pidfd");
    }

    if !process_matches(pid, identity) {
        return Ok(());
    }
    let result = unsafe { libc::kill(pid as libc::pid_t, signal) };
    if result == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if matches!(error.raw_os_error(), Some(libc::ESRCH | libc::EPERM)) {
        return Ok(());
    }
    Err(error).context("failed to signal a tracked descendant process of the v3 engine")
}

#[cfg(target_os = "linux")]
fn open_pidfd(pid: u32) -> Option<OwnedFd> {
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::pid_t, 0) };
    if fd < 0 {
        return None;
    }
    Some(unsafe { OwnedFd::from_raw_fd(fd as libc::c_int) })
}

#[cfg(unix)]
fn live_known_processes(known: &std::sync::Mutex<HashMap<u32, ProcessIdentity>>) -> Vec<u32> {
    known
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .filter_map(|(&pid, &identity)| process_matches(pid, identity).then_some(pid))
        .collect()
}

#[cfg(unix)]
fn process_group_has_live_members(group: u32) -> bool {
    (unsafe { libc::killpg(group as libc::pid_t, 0) }) == 0
}

#[cfg(windows)]
struct WindowsJob {
    handle: usize,
}

#[cfg(windows)]
impl WindowsJob {
    fn new(child: &Child) -> Result<Self> {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::JobObjects::{
            AssignProcessToJobObject,
            CreateJobObjectW,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        };
        use windows::core::PCWSTR;

        let process = child
            .raw_handle()
            .context("the v3 engine exited before Job Object assignment")?;
        let process = HANDLE(process);
        let job = unsafe { CreateJobObjectW(None, PCWSTR::null()) }
            .context("failed to create a Job Object for the v3 engine")?;
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configure = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&info).cast(),
                std::mem::size_of_val(&info) as u32,
            )
        };
        if let Err(error) = configure {
            let _ = unsafe { windows::Win32::Foundation::CloseHandle(job) };
            return Err(error).context("failed to configure the v3 engine's Job Object");
        }
        if let Err(error) = unsafe { AssignProcessToJobObject(job, process) } {
            let _ = unsafe { windows::Win32::Foundation::CloseHandle(job) };
            return Err(error).context("failed to assign the v3 engine to its Job Object");
        }
        if let Err(error) = resume_process_threads(process) {
            let _ = unsafe { windows::Win32::Foundation::CloseHandle(job) };
            return Err(error).context("failed to resume the v3 engine after Job Object assignment");
        }
        Ok(Self { handle: job.0 as usize })
    }

    fn terminate(&self) -> Result<()> {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::JobObjects::TerminateJobObject;

        unsafe { TerminateJobObject(HANDLE(self.handle as *mut _), 1) }
            .context("failed to terminate the v3 engine through its Job Object")
    }
}

#[cfg(windows)]
fn resume_process_threads(process: windows::Win32::Foundation::HANDLE) -> io::Result<()> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot,
        TH32CS_SNAPTHREAD,
        THREADENTRY32,
        Thread32First,
        Thread32Next,
    };
    use windows::Win32::System::Threading::{
        GetProcessId,
        OpenThread,
        ResumeThread,
        THREAD_SUSPEND_RESUME,
    };

    let pid = unsafe { GetProcessId(process) };
    if pid == 0 {
        return Err(io::Error::last_os_error());
    }
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) }.map_err(io::Error::other)?;
    let result = (|| {
        let mut entry = THREADENTRY32 {
            dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        let mut next = unsafe { Thread32First(snapshot, &mut entry) };
        let mut resumed = false;
        while next.is_ok() {
            if entry.th32OwnerProcessID == pid {
                let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, false, entry.th32ThreadID) }
                    .map_err(io::Error::other)?;
                let resume_result = unsafe { ResumeThread(thread) };
                let close_result = unsafe { CloseHandle(thread) };
                if resume_result == u32::MAX {
                    return Err(io::Error::last_os_error());
                }
                close_result.map_err(io::Error::other)?;
                resumed = true;
            }
            next = unsafe { Thread32Next(snapshot, &mut entry) };
        }
        if !resumed {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "the v3 engine's main thread was not found",
            ));
        }
        Ok(())
    })();
    let close_result = unsafe { CloseHandle(snapshot) }.map_err(io::Error::other);
    result.and(close_result)
}

#[cfg(windows)]
impl Drop for WindowsJob {
    fn drop(&mut self) {
        use windows::Win32::Foundation::{
            CloseHandle,
            HANDLE,
        };
        let _ = unsafe { CloseHandle(HANDLE(self.handle as *mut _)) };
    }
}

/// Relay a piped KAS process until it exits or the external ACP client closes
/// stdin. KAS stderr remains inherited by the parent process.
pub(super) async fn run(mut child: Child, mut signals: ShutdownSignals, relay_io: RelayIo) -> Result<ExitCode> {
    let pid = child
        .id()
        .context("the v3 engine exited before the ACP relay started")?;
    let kas_stdin = match child.stdin.take() {
        Some(stdin) => stdin,
        None => {
            terminate_unmanaged_child(&mut child, pid).await;
            bail!("failed to capture the v3 engine's stdin");
        },
    };
    let kas_stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            terminate_unmanaged_child(&mut child, pid).await;
            bail!("failed to capture the v3 engine's stdout");
        },
    };
    let mut process_tree = match ProcessTree::new(&child, pid) {
        Ok(process_tree) => process_tree,
        Err(error) => {
            terminate_unmanaged_child(&mut child, pid).await;
            return Err(error);
        },
    };
    let kas_stdin = Arc::new(Mutex::new(Some(kas_stdin)));
    let response_owners = Arc::new(ResponseOwnership::default());
    let auth_tracker = Arc::new(AuthTracker::default());
    let RelayIo {
        client_stdin,
        external_input_end,
        client_stdout,
    } = relay_io;

    let client_to_kas = forward_client_input(client_stdin, Arc::clone(&kas_stdin), Arc::clone(&response_owners));
    let kas_to_client = forward_kas_output(
        kas_stdout,
        client_stdout,
        Arc::clone(&kas_stdin),
        Arc::clone(&response_owners),
        Arc::clone(&auth_tracker),
        |params| {
            chat_cli_v2::auth::kas_token::handle_ext_method(ExtRequest::new(
                chat_cli_v2::auth::kas_token::KAS_AUTH_EXT_METHOD,
                params,
            ))
        },
    );
    tokio::pin!(client_to_kas);
    tokio::pin!(kas_to_client);
    tokio::pin!(external_input_end);

    let relay_result: Result<RelayCompletion> = {
        let child_exit = observe_child_exit(&mut child, pid);
        tokio::pin!(child_exit);
        async {
            let mut external_end_seen = false;
            loop {
                tokio::select! {
                    input_result = &mut client_to_kas => {
                        input_result?;
                        tracing::debug!("external ACP stdin closed; draining accepted v3 engine responses");
                        let signal = signals.recv();
                        tokio::pin!(signal);
                        break finish_after_client_eof(
                            &kas_stdin,
                            &response_owners,
                            &mut kas_to_client,
                            &mut child_exit,
                            &mut signal,
                        ).await;
                    },
                    external_result = &mut external_input_end, if !external_end_seen => {
                        external_result
                            .context("ACP stdin reader stopped before reporting its result")?
                            .context("failed to read ACP stdin")?;
                        external_end_seen = true;
                    },
                    output_result = &mut kas_to_client => {
                        output_result?;
                        break finish_after_kas_output(&mut child_exit, &mut signals).await;
                    },
                    child_result = &mut child_exit => {
                        child_result?;
                        break finish_after_kas_exit(&mut kas_to_client, &mut signals).await;
                    },
                    exit_code = signals.recv() => break Ok(RelayCompletion::Signal(exit_code)),
                }
            }
        }
        .await
    };

    let child_already_exited = matches!(&relay_result, Ok(RelayCompletion::Child));
    let cleanup_result = process_tree.terminate_and_reap(&mut child, child_already_exited).await;
    let result = match (relay_result, cleanup_result) {
        (Ok(RelayCompletion::Child), Ok(status)) => Ok(to_exit_code(status)),
        (Ok(RelayCompletion::Signal(exit_code)), Ok(_)) => Ok(exit_code),
        (Ok(_), Err(error)) => Err(error),
        (Err(error), Ok(_)) => Err(error),
        (Err(error), Err(cleanup_error)) => {
            tracing::warn!(%cleanup_error, "failed to clean up the v3 engine process after relay failure");
            Err(error)
        },
    };

    finish_with_auth_drain(result, &auth_tracker).await
}

async fn terminate_unmanaged_child(child: &mut Child, pid: u32) {
    #[cfg(unix)]
    let _ = signal_process_group(pid, libc::SIGKILL);
    if let Err(error) = child.start_kill()
        && !matches!(error.kind(), io::ErrorKind::InvalidInput | io::ErrorKind::NotFound)
    {
        tracing::warn!(%error, "failed to terminate the v3 engine process after relay setup error");
    }
    if tokio::time::timeout(KAS_EXIT_TIMEOUT, child.wait()).await.is_err() {
        tracing::warn!("timed out reaping the v3 engine process after relay setup error");
    }
}

async fn finish_with_auth_drain<T>(result: Result<T>, auth_tracker: &AuthTracker) -> Result<T> {
    // Auth refresh can rotate a refresh token before persisting it. Keep the
    // task shielded through every expected critical section. The auth-owned
    // provider list and refresh budget determine this bound; the relay adds a
    // safety margin. Only work beyond every legitimate refresh window is cut
    // off so a wedged resolver cannot prevent process exit indefinitely.
    if tokio::time::timeout(AUTH_DRAIN_TIMEOUT, auth_tracker.wait_idle())
        .await
        .is_err()
    {
        tracing::warn!(
            timeout_secs = AUTH_DRAIN_TIMEOUT.as_secs(),
            "timed out draining CLI authentication tasks after relay shutdown"
        );
    }
    result
}

async fn finish_after_client_eof<FOutput, FExit, FSignal, W>(
    kas_stdin: &SharedKasStdin<W>,
    response_owners: &ResponseOwners,
    kas_to_client: &mut Pin<&mut FOutput>,
    child_exit: &mut Pin<&mut FExit>,
    signal: &mut Pin<&mut FSignal>,
) -> Result<RelayCompletion>
where
    FOutput: Future<Output = Result<()>>,
    FExit: Future<Output = Result<()>>,
    FSignal: Future<Output = ExitCode>,
    W: AsyncWrite + Unpin,
{
    let mut output_done = false;
    let mut child_done = false;
    {
        let drain_and_close = async {
            // Auth resolution can legitimately use the credential resolver's
            // full refresh window. Keep polling KAS and signals while every
            // response already accepted by this relay reaches its bounded
            // write, then deliver real EOF by dropping the writer.
            wait_for_pending_responses(response_owners).await;
            close_kas_stdin(kas_stdin).await;
        };
        tokio::pin!(drain_and_close);
        loop {
            tokio::select! {
                () = &mut drain_and_close => break,
                output_result = kas_to_client.as_mut(), if !output_done => {
                    output_result?;
                    output_done = true;
                },
                child_result = child_exit.as_mut(), if !child_done => {
                    child_result?;
                    child_done = true;
                },
                exit_code = signal.as_mut() => return Ok(RelayCompletion::Signal(exit_code)),
            }
        }
    }

    wait_for_terminal_peers(kas_to_client, child_exit, signal, output_done, child_done).await
}

async fn finish_after_kas_output<FExit>(
    child_exit: &mut Pin<&mut FExit>,
    signals: &mut ShutdownSignals,
) -> Result<RelayCompletion>
where
    FExit: Future<Output = Result<()>>,
{
    let deadline = tokio::time::sleep(KAS_EXIT_TIMEOUT);
    tokio::pin!(deadline);
    tokio::select! {
        child_result = child_exit.as_mut() => {
            child_result?;
            Ok(RelayCompletion::Child)
        },
        exit_code = signals.recv() => Ok(RelayCompletion::Signal(exit_code)),
        () = &mut deadline => bail!(
            "the v3 engine did not exit within {} seconds after closing stdout",
            KAS_EXIT_TIMEOUT.as_secs()
        ),
    }
}

async fn finish_after_kas_exit<FOutput>(
    kas_to_client: &mut Pin<&mut FOutput>,
    signals: &mut ShutdownSignals,
) -> Result<RelayCompletion>
where
    FOutput: Future<Output = Result<()>>,
{
    let deadline = tokio::time::sleep(KAS_EXIT_TIMEOUT);
    tokio::pin!(deadline);
    tokio::select! {
        output_result = kas_to_client.as_mut() => {
            output_result?;
            Ok(RelayCompletion::Child)
        },
        exit_code = signals.recv() => Ok(RelayCompletion::Signal(exit_code)),
        () = &mut deadline => bail!(
            "the v3 engine's stdout did not close within {} seconds after process exit",
            KAS_EXIT_TIMEOUT.as_secs()
        ),
    }
}

async fn wait_for_terminal_peers<FOutput, FExit, FSignal>(
    kas_to_client: &mut Pin<&mut FOutput>,
    child_exit: &mut Pin<&mut FExit>,
    signal: &mut Pin<&mut FSignal>,
    mut output_done: bool,
    mut child_done: bool,
) -> Result<RelayCompletion>
where
    FOutput: Future<Output = Result<()>>,
    FExit: Future<Output = Result<()>>,
    FSignal: Future<Output = ExitCode>,
{
    let deadline = tokio::time::sleep(KAS_GRACEFUL_EXIT_TIMEOUT);
    tokio::pin!(deadline);
    while !output_done || !child_done {
        tokio::select! {
            output_result = kas_to_client.as_mut(), if !output_done => {
                output_result?;
                output_done = true;
            },
            child_result = child_exit.as_mut(), if !child_done => {
                child_result?;
                child_done = true;
            },
            exit_code = signal.as_mut() => return Ok(RelayCompletion::Signal(exit_code)),
            () = &mut deadline => bail!(
                "the v3 engine did not exit within {} seconds after ACP stdin closed",
                KAS_GRACEFUL_EXIT_TIMEOUT.as_secs()
            ),
        }
    }
    Ok(RelayCompletion::Child)
}

async fn wait_for_pending_responses(response_owners: &ResponseOwners) {
    loop {
        let notified = response_owners.updates.notified();
        let pending_client_response = response_owners.lock().await.values().any(|owner| {
            matches!(
                owner,
                ResponseOwner::Publishing { queued: Some(_) } | ResponseOwner::Forwarding
            )
        });
        let pending_internal_response = response_owners.pending_internal_responses.load(Ordering::Acquire) != 0;
        if !pending_client_response && !pending_internal_response {
            return;
        }
        notified.await;
    }
}

#[cfg(unix)]
async fn observe_child_exit(_child: &mut Child, pid: u32) -> Result<()> {
    loop {
        if child_exited_without_reaping(pid)? {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[cfg(unix)]
fn child_exited_without_reaping(pid: u32) -> Result<bool> {
    let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
    let result = unsafe {
        libc::waitid(
            libc::P_PID,
            pid as libc::id_t,
            info.as_mut_ptr(),
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error())
            .context("failed to inspect the v3 engine's exit status without reaping the process");
    }
    let info = unsafe { info.assume_init() };
    Ok(unsafe { info.si_pid() } != 0)
}

#[cfg(not(unix))]
async fn observe_child_exit(child: &mut Child, _pid: u32) -> Result<()> {
    child.wait().await.context("failed to wait for the v3 engine process")?;
    Ok(())
}

#[cfg(unix)]
fn signal_process_group(pid: u32, signal: libc::c_int) -> Result<()> {
    let result = unsafe { libc::killpg(pid as libc::pid_t, signal) };
    if result == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if matches!(error.raw_os_error(), Some(libc::ESRCH | libc::EPERM)) {
        return Ok(());
    }
    Err(error).context("failed to signal the v3 engine process group")
}

fn to_exit_code(status: ExitStatus) -> ExitCode {
    status
        .code()
        .map_or(ExitCode::FAILURE, |code| ExitCode::from(code as u8))
}

async fn forward_client_input<R, W>(
    input: R,
    kas_stdin: SharedKasStdin<W>,
    response_owners: ResponseOwners,
) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut input = BufReader::new(input);
    let mut frame = Vec::new();

    while read_bounded_frame(&mut input, &mut frame, "ACP stdin", MAX_ACP_FRAME_BYTES).await? {
        match classify_client_frame(&frame) {
            ClientFrame::Forward => {
                write_to_kas(&kas_stdin, &frame, "failed to forward ACP input to the v3 engine").await?;
            },
            ClientFrame::Response(id) => {
                let action = {
                    let mut owners = response_owners.lock().await;
                    match owners.get_mut(&id) {
                        Some(ResponseOwner::Exposed) => {
                            owners.remove(&id);
                            ClientResponseAction::Forward
                        },
                        Some(ResponseOwner::Publishing { queued }) if queued.is_none() => {
                            *queued = Some(frame.clone());
                            ClientResponseAction::Queued
                        },
                        _ => ClientResponseAction::Drop,
                    }
                };
                match action {
                    ClientResponseAction::Forward => {
                        write_to_kas(
                            &kas_stdin,
                            &frame,
                            "failed to forward the ACP response to the v3 engine",
                        )
                        .await?;
                    },
                    ClientResponseAction::Queued => {},
                    ClientResponseAction::Drop => {
                        tracing::warn!("dropping external response without an exposed v3 engine request");
                    },
                }
            },
            ClientFrame::Drop => {
                tracing::warn!("dropping malformed external ACP frame");
            },
        }
        frame.clear();
    }

    Ok(())
}

enum ClientResponseAction {
    Forward,
    Queued,
    Drop,
}

async fn forward_kas_output<R, C, W, F, Fut>(
    output: R,
    mut client_stdout: C,
    kas_stdin: SharedKasStdin<W>,
    response_owners: ResponseOwners,
    auth_tracker: Arc<AuthTracker>,
    mut auth_handler: F,
) -> Result<()>
where
    R: AsyncRead + Unpin,
    C: FrameSink,
    W: AsyncWrite + Unpin,
    F: FnMut(Arc<RawValue>) -> Fut,
    Fut: Future<Output = AuthResult> + Send + 'static,
{
    let mut output = BufReader::new(output);
    let mut frame = Vec::new();
    let mut auth_responses = FuturesUnordered::new();
    let mut output_closed = false;

    while !output_closed || !auth_responses.is_empty() {
        tokio::select! {
            frame_result = read_bounded_frame(
                &mut output,
                &mut frame,
                "the v3 engine's stdout",
                MAX_ACP_FRAME_BYTES,
            ), if !output_closed => {
                if !frame_result? {
                    output_closed = true;
                    continue;
                }
                match classify_kas_frame(&frame) {
                    KasFrame::Auth { id, params } => {
                        let pending_response = response_owners.track_internal_response();
                        response_owners.lock().await.remove(&id);
                        response_owners.changed();
                        let auth_result = auth_tracker.spawn(auth_handler(params));
                        auth_responses.push(async move {
                            let auth_result = auth_result
                                .await
                                .context("CLI auth task ended without a response");
                            (id, pending_response, auth_result)
                        });
                    },
                    KasFrame::InvalidAuth => {
                        let _pending_response = response_owners.track_internal_response();
                        tracing::warn!("received a malformed v3 engine authentication callback request");
                        let response = encode_response(RequestId::Null, Err(AcpError::invalid_request()))?;
                        write_to_kas(
                            &kas_stdin,
                            &response,
                            "failed to return the invalid-request response to the v3 engine",
                        )
                        .await?;
                    },
                    KasFrame::Forward { request_id: Some(id) } => {
                        response_owners
                            .lock()
                            .await
                            .insert(id.clone(), ResponseOwner::Publishing { queued: None });
                        if let Err(error) = client_stdout.write_frame(&frame).await {
                            response_owners.lock().await.remove(&id);
                            return Err(error).context("failed to forward the v3 engine's output to ACP stdout");
                        }
                        let queued = {
                            let mut owners = response_owners.lock().await;
                            match owners.remove(&id) {
                                Some(ResponseOwner::Publishing { queued: Some(response) }) => {
                                    owners.insert(id.clone(), ResponseOwner::Forwarding);
                                    Some(response)
                                },
                                Some(ResponseOwner::Publishing { queued: None }) => {
                                    owners.insert(id.clone(), ResponseOwner::Exposed);
                                    None
                                },
                                Some(ResponseOwner::Exposed | ResponseOwner::Forwarding) | None => None,
                            }
                        };
                        response_owners.changed();
                        if let Some(response) = queued {
                            let result = write_to_kas(
                                &kas_stdin,
                                &response,
                                "failed to forward the queued ACP response to the v3 engine",
                            )
                            .await;
                            response_owners.lock().await.remove(&id);
                            response_owners.changed();
                            result?;
                        }
                    },
                    KasFrame::Forward { request_id: None } => {
                        client_stdout
                            .write_frame(&frame)
                            .await
                            .context("failed to forward the v3 engine's output to ACP stdout")?;
                    },
                }
                frame.clear();
            },
            Some((id, _pending_response, auth_result)) = auth_responses.next(),
                if !auth_responses.is_empty() => {
                let response = encode_response(id, auth_result?)?;
                write_to_kas(
                    &kas_stdin,
                    &response,
                    "failed to return the CLI authentication response to the v3 engine",
                )
                .await?;
            },
        }
    }

    client_stdout
        .flush_frames()
        .await
        .context("failed to flush ACP stdout")?;
    Ok(())
}

async fn read_bounded_frame<R>(
    reader: &mut R,
    frame: &mut Vec<u8>,
    source: &'static str,
    max_bytes: usize,
) -> Result<bool>
where
    R: AsyncBufRead + Unpin,
{
    if frame.is_empty() && frame.capacity() > MAX_RETAINED_FRAME_CAPACITY {
        frame.shrink_to(MAX_RETAINED_FRAME_CAPACITY);
    }

    loop {
        let available = reader
            .fill_buf()
            .await
            .with_context(|| format!("failed to read {source}"))?;
        if available.is_empty() {
            if frame.is_empty() {
                return Ok(false);
            }
            bail!("{source} closed with an unterminated NDJSON frame");
        }

        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |position| position + 1);
        if frame.len().saturating_add(consumed) > max_bytes {
            bail!("{source} frame exceeds the {max_bytes}-byte limit");
        }
        frame.extend_from_slice(&available[..consumed]);
        reader.consume(consumed);
        if newline.is_some() {
            return Ok(true);
        }
    }
}

async fn write_to_kas<W>(kas_stdin: &SharedKasStdin<W>, frame: &[u8], context: &'static str) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    match tokio::time::timeout(KAS_WRITE_TIMEOUT, async {
        let mut guard = kas_stdin.lock().await;
        let writer = guard.as_mut().context("the v3 engine's stdin is closed")?;
        writer.write_all(frame).await.context(context)
    })
    .await
    {
        Ok(result) => result,
        Err(_) => bail!(
            "timed out writing to the v3 engine's stdin after {} seconds",
            KAS_WRITE_TIMEOUT.as_secs()
        ),
    }
}

async fn close_kas_stdin<W>(kas_stdin: &SharedKasStdin<W>) {
    // ChildStdin::poll_shutdown is a no-op on Unix and does not close every
    // Windows handle. Dropping the sole writer is what delivers real EOF.
    kas_stdin.lock().await.take();
}

fn classify_client_frame(frame: &[u8]) -> ClientFrame {
    // Value intentionally follows JSON.parse's last-key-wins behavior. That
    // prevents duplicate `id` fields from bypassing response ownership checks.
    let Ok(Value::Object(object)) = serde_json::from_slice(frame) else {
        return ClientFrame::Drop;
    };

    match object.get("method") {
        Some(Value::String(method)) if method == KAS_AUTH_WIRE_METHOD => return ClientFrame::Drop,
        Some(Value::String(_)) => return ClientFrame::Forward,
        Some(_) => return ClientFrame::Drop,
        None => {},
    }

    if object.contains_key("result") == object.contains_key("error") {
        return ClientFrame::Drop;
    }
    let Some(id) = object
        .get("id")
        .cloned()
        .and_then(|id| serde_json::from_value::<RequestId>(id).ok())
        .filter(|id| !matches!(id, RequestId::Null))
    else {
        return ClientFrame::Drop;
    };
    ClientFrame::Response(id)
}

fn classify_kas_frame(frame: &[u8]) -> KasFrame {
    // Match KAS's JSON.parse behavior for duplicate keys so an auth callback
    // cannot become externally visible through a parser-semantics mismatch.
    let Ok(Value::Object(object)) = serde_json::from_slice(frame) else {
        return KasFrame::Forward { request_id: None };
    };

    let method = object.get("method").and_then(Value::as_str);
    let id = object
        .get("id")
        .cloned()
        .and_then(|id| serde_json::from_value::<RequestId>(id).ok());
    if method == Some(KAS_AUTH_WIRE_METHOD) {
        let Some(id) = id.filter(|id| !matches!(id, RequestId::Null)) else {
            return KasFrame::InvalidAuth;
        };
        let params = object
            .get("params")
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default()));
        let params = Arc::from(serde_json::value::to_raw_value(&params).expect("JSON value is serializable"));
        return KasFrame::Auth { id, params };
    }

    KasFrame::Forward {
        request_id: method
            .is_some()
            .then_some(id)
            .flatten()
            .filter(|id| !matches!(id, RequestId::Null)),
    }
}

fn encode_response(id: RequestId, result: AuthResult) -> Result<Vec<u8>> {
    let mut response = serde_json::to_vec(&JsonRpcMessage::wrap(Response::new(id, result)))
        .context("failed to serialize the authentication response for the v3 engine")?;
    response.push(b'\n');
    Ok(response)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;
    use std::sync::atomic::AtomicUsize;

    use serde_json::json;

    use super::*;

    #[test]
    fn parses_linux_process_stat_parent_identity_and_status() {
        let running = "4321 (kas worker (nested)) S 1234 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 987654 0";
        assert_eq!(parse_linux_process_stat_fields(running), Some((1234, 987654, false)));

        let zombie = "4321 (kas) helper) Z 99 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 42";
        assert_eq!(parse_linux_process_stat_fields(zombie), Some((99, 42, true)));
        assert_eq!(parse_linux_process_stat_fields("malformed"), None);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_proxied_process_is_suspended_assigned_resumed_and_reaped() {
        use tokio::io::{
            AsyncBufReadExt as _,
            BufReader,
        };
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::JobObjects::IsProcessInJob;

        let mut command = tokio::process::Command::new("cmd.exe");
        command
            .args(["/D", "/S", "/C", "echo ready & ping -n 31 127.0.0.1 >NUL"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        configure_proxied_command(&mut command);
        let mut child = command.spawn().expect("spawn suspended process");
        let pid = child.id().expect("suspended process id");
        let mut stdout = BufReader::new(child.stdout.take().expect("capture suspended stdout"));
        let mut line = String::new();
        assert!(
            tokio::time::timeout(Duration::from_millis(100), stdout.read_line(&mut line))
                .await
                .is_err(),
            "the proxied process executed before Job Object assignment"
        );

        let mut process_tree = ProcessTree::new(&child, pid).expect("assign Job Object and resume process");
        let process = HANDLE(child.raw_handle().expect("resumed process handle"));
        let job = HANDLE(process_tree.windows.handle as *mut _);
        let mut in_job = windows::core::BOOL::default();
        unsafe { IsProcessInJob(process, Some(job), &mut in_job) }.expect("query Job Object assignment");
        assert!(in_job.as_bool(), "proxied process was not assigned to its Job Object");

        tokio::time::timeout(Duration::from_secs(2), stdout.read_line(&mut line))
            .await
            .expect("resumed process did not execute")
            .expect("read resumed process output");
        assert_eq!(line.trim(), "ready");

        let status = process_tree
            .terminate_and_reap(&mut child, false)
            .await
            .expect("terminate and reap Job Object");
        assert!(
            !status.success(),
            "Job Object termination unexpectedly reported success"
        );
    }

    #[derive(Clone, Default)]
    struct RecordingWriter {
        bytes: Arc<StdMutex<Vec<u8>>>,
    }

    impl RecordingWriter {
        fn bytes(&self) -> Vec<u8> {
            self.bytes.lock().expect("recording lock").clone()
        }
    }

    impl AsyncWrite for RecordingWriter {
        fn poll_write(self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
            self.bytes.lock().expect("recording lock").extend_from_slice(buf);
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    struct GatedSink {
        published: RecordingWriter,
        started: Option<oneshot::Sender<()>>,
        release: Option<oneshot::Receiver<()>>,
    }

    impl FrameSink for GatedSink {
        async fn write_frame(&mut self, frame: &[u8]) -> io::Result<()> {
            self.published
                .bytes
                .lock()
                .expect("recording lock")
                .extend_from_slice(frame);
            if let Some(started) = self.started.take() {
                let _ = started.send(());
            }
            if let Some(release) = self.release.take() {
                let _ = release.await;
            }
            Ok(())
        }

        async fn flush_frames(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct SplitFrameReader {
        first: Vec<u8>,
        prefix: Vec<u8>,
        suffix: Vec<u8>,
        state: u8,
        waiting: Option<oneshot::Sender<()>>,
        release: oneshot::Receiver<()>,
    }

    impl AsyncRead for SplitFrameReader {
        fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
            let this = self.get_mut();
            match this.state {
                0 => {
                    buf.put_slice(&this.first);
                    this.state = 1;
                    Poll::Ready(Ok(()))
                },
                1 => {
                    buf.put_slice(&this.prefix);
                    this.state = 2;
                    Poll::Ready(Ok(()))
                },
                2 => {
                    if let Some(waiting) = this.waiting.take() {
                        let _ = waiting.send(());
                    }
                    match Pin::new(&mut this.release).poll(cx) {
                        Poll::Ready(_) => {
                            buf.put_slice(&this.suffix);
                            this.state = 3;
                            Poll::Ready(Ok(()))
                        },
                        Poll::Pending => Poll::Pending,
                    }
                },
                _ => Poll::Ready(Ok(())),
            }
        }
    }

    fn shared_writer(writer: RecordingWriter) -> SharedKasStdin<RecordingWriter> {
        Arc::new(Mutex::new(Some(writer)))
    }

    fn empty_response_owners() -> ResponseOwners {
        Arc::new(ResponseOwnership::default())
    }

    fn exposed_response_owners(ids: impl IntoIterator<Item = RequestId>) -> ResponseOwners {
        Arc::new(ResponseOwnership {
            owners: Mutex::new(ids.into_iter().map(|id| (id, ResponseOwner::Exposed)).collect()),
            pending_internal_responses: AtomicUsize::new(0),
            updates: Notify::new(),
        })
    }

    fn token_response() -> ExtResponse {
        let raw = serde_json::value::to_raw_value(&json!({
            "accessToken": "short-lived-access-token",
            "expiresAt": "2099-01-01T00:00:00Z",
            "profileArn": "arn:aws:codewhisperer:us-east-1:1:profile/test"
        }))
        .expect("serialize test token");
        ExtResponse::new(Arc::from(raw))
    }

    #[tokio::test]
    async fn intercepts_auth_without_exposing_request_or_token() {
        let normal = b" {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"ok\":true}} \n";
        let mut output = br#"{"jsonrpc":"2.0","id":"auth-7","method":"_kiro/auth/getAccessToken","params":{}}
"#
        .to_vec();
        output.extend_from_slice(normal);

        let client_stdout = RecordingWriter::default();
        let kas_stdin_writer = RecordingWriter::default();
        let allowed = exposed_response_owners([RequestId::Str("auth-7".into())]);
        forward_kas_output(
            output.as_slice(),
            AsyncFrameSink(client_stdout.clone()),
            shared_writer(kas_stdin_writer.clone()),
            Arc::clone(&allowed),
            Arc::new(AuthTracker::default()),
            |_| async { Ok(token_response()) },
        )
        .await
        .expect("relay KAS output");

        assert_eq!(client_stdout.bytes(), normal);
        let external = String::from_utf8(client_stdout.bytes()).expect("utf8 client output");
        assert!(!external.contains(KAS_AUTH_WIRE_METHOD));
        assert!(!external.contains("short-lived-access-token"));

        let response: Value = serde_json::from_slice(&kas_stdin_writer.bytes()).expect("auth response JSON");
        assert_eq!(response["id"], "auth-7");
        assert_eq!(response["result"]["accessToken"], "short-lived-access-token");
        assert!(response["result"].get("refreshToken").is_none());
        assert!(allowed.lock().await.is_empty());
    }

    #[tokio::test]
    async fn forwards_unrelated_output_while_auth_is_pending() {
        let auth_frame = b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"_kiro/auth/getAccessToken\",\"params\":{}}\n";
        let unrelated = b" {\"jsonrpc\":\"2.0\",\"id\":3,\"result\":{\"ok\":true}} \n";
        let mut output = auth_frame.to_vec();
        output.extend_from_slice(unrelated);
        let client_stdout = RecordingWriter::default();
        let kas_stdin_writer = RecordingWriter::default();
        let (started, started_rx) = oneshot::channel();
        let (release, release_rx) = oneshot::channel();
        let mut started = Some(started);
        let mut release_rx = Some(release_rx);
        let mut relay = Box::pin(forward_kas_output(
            output.as_slice(),
            AsyncFrameSink(client_stdout.clone()),
            shared_writer(kas_stdin_writer.clone()),
            empty_response_owners(),
            Arc::new(AuthTracker::default()),
            move |_| {
                let started = started.take().expect("one auth request");
                let release_rx = release_rx.take().expect("one auth request");
                async move {
                    let _ = started.send(());
                    let _ = release_rx.await;
                    Ok(token_response())
                }
            },
        ));

        tokio::select! {
            result = &mut relay => panic!("relay ended before auth started: {result:?}"),
            started = started_rx => started.expect("auth task started"),
        }
        tokio::select! {
            result = &mut relay => panic!("relay ended before forwarding unrelated output: {result:?}"),
            forwarded = tokio::time::timeout(Duration::from_secs(1), async {
                while client_stdout.bytes() != unrelated {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            }) => forwarded.expect("unrelated engine output stalled behind CLI auth"),
        }
        assert!(kas_stdin_writer.bytes().is_empty());

        release.send(()).expect("release auth task");
        relay.await.expect("relay KAS output");
        let response: Value = serde_json::from_slice(&kas_stdin_writer.bytes()).expect("auth response JSON");
        assert_eq!(response["id"], 2);
        assert_eq!(response["result"]["accessToken"], "short-lived-access-token");
    }

    #[tokio::test]
    async fn cancellation_during_split_auth_frame_preserves_interception() {
        let first = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"_kiro/auth/getAccessToken\",\"params\":{}}\n";
        let second = b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"_kiro/auth/getAccessToken\",\"params\":{}}\n";
        let split = second.len() / 2;
        let (waiting, waiting_rx) = oneshot::channel();
        let (release_frame, release_frame_rx) = oneshot::channel();
        let output = SplitFrameReader {
            first: first.to_vec(),
            prefix: second[..split].to_vec(),
            suffix: second[split..].to_vec(),
            state: 0,
            waiting: Some(waiting),
            release: release_frame_rx,
        };
        let client_stdout = RecordingWriter::default();
        let kas_stdin_writer = RecordingWriter::default();
        let (first_auth_release, first_auth_release_rx) = oneshot::channel();
        let first_auth_release_rx = Arc::new(StdMutex::new(Some(first_auth_release_rx)));
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_handler = Arc::clone(&calls);
        let release_for_handler = Arc::clone(&first_auth_release_rx);
        let (started, started_rx) = oneshot::channel();
        let mut started = Some(started);
        let mut relay = Box::pin(forward_kas_output(
            output,
            AsyncFrameSink(client_stdout.clone()),
            shared_writer(kas_stdin_writer.clone()),
            empty_response_owners(),
            Arc::new(AuthTracker::default()),
            move |_| {
                let call = calls_for_handler.fetch_add(1, Ordering::SeqCst);
                let release = (call == 0).then(|| {
                    release_for_handler
                        .lock()
                        .expect("release lock")
                        .take()
                        .expect("first auth release")
                });
                let started = (call == 0).then(|| started.take().expect("first auth request"));
                async move {
                    if let Some(started) = started {
                        let _ = started.send(());
                    }
                    if let Some(release) = release {
                        let _ = release.await;
                    }
                    Ok(token_response())
                }
            },
        ));

        tokio::select! {
            result = &mut relay => panic!("relay ended before auth started: {result:?}"),
            started = started_rx => started.expect("first auth task started"),
        }
        tokio::select! {
            result = &mut relay => panic!("relay ended before consuming the split frame prefix: {result:?}"),
            waiting = tokio::time::timeout(Duration::from_secs(1), waiting_rx) => waiting
                .expect("split frame prefix was not consumed")
                .expect("split reader stopped before waiting for the suffix"),
        }

        first_auth_release.send(()).expect("release first auth task");
        tokio::select! {
            result = &mut relay => panic!("relay ended before writing the first auth response: {result:?}"),
            response = tokio::time::timeout(Duration::from_secs(1), async {
                while kas_stdin_writer.bytes().is_empty() {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            }) => response.expect("first auth response did not complete the competing select branch"),
        }
        assert!(client_stdout.bytes().is_empty());

        release_frame.send(()).expect("release split frame suffix");
        relay.await.expect("relay KAS output");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(client_stdout.bytes().is_empty());
        let responses = kas_stdin_writer.bytes();
        let ids: Vec<_> = responses
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice::<Value>(line).expect("auth response JSON")["id"].clone())
            .collect();
        assert_eq!(ids, [json!(1), json!(2)]);
        let external = String::from_utf8(client_stdout.bytes()).expect("utf8 client output");
        assert!(!external.contains(KAS_AUTH_WIRE_METHOD));
        assert!(!external.contains("short-lived-access-token"));
    }

    #[tokio::test]
    async fn duplicate_method_keys_use_the_last_value_for_auth_interception() {
        let intercepted = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"_kiro/other\",\"method\":\"_kiro/auth/getAccessToken\",\"params\":{}}\n";
        let forwarded = b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"_kiro/auth/getAccessToken\",\"method\":\"_kiro/other\",\"params\":{}}\n";
        let mut output = intercepted.to_vec();
        output.extend_from_slice(forwarded);
        let client_stdout = RecordingWriter::default();
        let kas_stdin_writer = RecordingWriter::default();
        let owners = empty_response_owners();
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_handler = Arc::clone(&calls);

        forward_kas_output(
            output.as_slice(),
            AsyncFrameSink(client_stdout.clone()),
            shared_writer(kas_stdin_writer.clone()),
            Arc::clone(&owners),
            Arc::new(AuthTracker::default()),
            move |_| {
                calls_for_handler.fetch_add(1, Ordering::SeqCst);
                async { Ok(token_response()) }
            },
        )
        .await
        .expect("relay duplicate-method KAS output");

        assert_eq!(client_stdout.bytes(), forwarded);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let response: Value = serde_json::from_slice(&kas_stdin_writer.bytes()).expect("auth response JSON");
        assert_eq!(response["id"], 1);
        assert!(owners.lock().await.contains_key(&RequestId::Number(2)));
    }

    #[tokio::test]
    async fn forwards_other_reverse_requests_byte_for_byte() {
        let frames = b" {\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"_kiro/other\",\"params\":{\"x\":1}} \nnot-json\n";
        let client_stdout = RecordingWriter::default();
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_handler = Arc::clone(&calls);
        let allowed = empty_response_owners();

        forward_kas_output(
            frames.as_slice(),
            AsyncFrameSink(client_stdout.clone()),
            shared_writer(RecordingWriter::default()),
            Arc::clone(&allowed),
            Arc::new(AuthTracker::default()),
            move |_| {
                calls_for_handler.fetch_add(1, Ordering::SeqCst);
                async { Ok(token_response()) }
            },
        )
        .await
        .expect("relay KAS output");

        assert_eq!(client_stdout.bytes(), frames);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(allowed.lock().await.contains_key(&RequestId::Number(8)));
    }

    #[tokio::test]
    async fn queues_legitimate_response_while_reverse_request_is_being_published() {
        let reverse_request = b"{\"jsonrpc\":\"2.0\",\"id\":10,\"method\":\"_kiro/other\",\"params\":{}}\n";
        let racing_response = b"{\"jsonrpc\":\"2.0\",\"id\":10,\"result\":{\"ok\":true}}\n";
        let owners = empty_response_owners();
        let published = RecordingWriter::default();
        let kas_stdin_writer = RecordingWriter::default();
        let kas_stdin = shared_writer(kas_stdin_writer.clone());
        let (started, started_rx) = oneshot::channel();
        let (release, release_rx) = oneshot::channel();
        let mut output = Box::pin(forward_kas_output(
            reverse_request.as_slice(),
            GatedSink {
                published: published.clone(),
                started: Some(started),
                release: Some(release_rx),
            },
            Arc::clone(&kas_stdin),
            Arc::clone(&owners),
            Arc::new(AuthTracker::default()),
            |_| async { Ok(token_response()) },
        ));

        tokio::select! {
            result = &mut output => panic!("KAS output ended before the write gate: {result:?}"),
            started = started_rx => started.expect("reverse request write started"),
        }
        assert_eq!(published.bytes(), reverse_request);
        let (mut client_writer, client_reader) = tokio::io::duplex(racing_response.len() + 1);
        client_writer
            .write_all(racing_response)
            .await
            .expect("prime racing response");
        let mut client = Box::pin(forward_client_input(
            client_reader,
            Arc::clone(&kas_stdin),
            Arc::clone(&owners),
        ));
        tokio::select! {
            result = &mut client => panic!("client relay ended before publication: {result:?}"),
            _ = async {
                loop {
                    let queued = matches!(
                        owners.lock().await.get(&RequestId::Number(10)),
                        Some(ResponseOwner::Publishing { queued: Some(_) })
                    );
                    if queued {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            } => {},
        }
        release.send(()).expect("release reverse request write");
        output.await.expect("finish KAS output");
        drop(client_writer);
        client.await.expect("finish client relay");

        assert_eq!(kas_stdin_writer.bytes(), racing_response);
        assert!(owners.lock().await.is_empty());
    }

    #[tokio::test]
    async fn forwards_only_responses_for_exposed_reverse_requests() {
        let preplayed = b"{\"jsonrpc\":\"2.0\",\"id\":9,\"result\":{\"spoofed\":true}}\n";
        let duplicate_id = b"{\"jsonrpc\":\"2.0\",\"id\":10,\"id\":9,\"result\":{\"spoofed\":true}}\n";
        let duplicate_allowed = b"{\"jsonrpc\":\"2.0\",\"id\":9,\"id\":12,\"result\":{\"ok\":true}}\n";
        let request_same_id = b"{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"initialize\",\"params\":{}}\n";
        let injected_auth = b"{\"jsonrpc\":\"2.0\",\"id\":11,\"method\":\"_kiro/auth/getAccessToken\",\"params\":{}}\n";
        let allowed_response = b"{\"jsonrpc\":\"2.0\",\"id\":10,\"result\":{}}\n";
        let replayed_response = b"{\"jsonrpc\":\"2.0\",\"id\":10,\"result\":{\"again\":true}}\n";
        let mut input = preplayed.to_vec();
        input.extend_from_slice(duplicate_id);
        input.extend_from_slice(duplicate_allowed);
        input.extend_from_slice(request_same_id);
        input.extend_from_slice(injected_auth);
        input.extend_from_slice(allowed_response);
        input.extend_from_slice(replayed_response);

        let kas_stdin_writer = RecordingWriter::default();
        let allowed = exposed_response_owners([RequestId::Number(10), RequestId::Number(12)]);
        forward_client_input(
            input.as_slice(),
            shared_writer(kas_stdin_writer.clone()),
            Arc::clone(&allowed),
        )
        .await
        .expect("relay client input");

        let mut expected = duplicate_allowed.to_vec();
        expected.extend_from_slice(request_same_id);
        expected.extend_from_slice(allowed_response);
        assert_eq!(kas_stdin_writer.bytes(), expected);
        assert!(allowed.lock().await.is_empty());
    }

    #[tokio::test]
    async fn rejects_unterminated_client_frames_without_forwarding() {
        let kas_stdin_writer = RecordingWriter::default();
        let result = forward_client_input(
            b"{\"jsonrpc\":\"2.0\",\"id\":1".as_slice(),
            shared_writer(kas_stdin_writer.clone()),
            empty_response_owners(),
        )
        .await;

        let error = result.expect_err("unterminated input must fail");
        assert!(error.to_string().contains("unterminated NDJSON frame"));
        assert!(kas_stdin_writer.bytes().is_empty());
    }

    #[tokio::test]
    async fn clean_client_eof_leaves_kas_writer_for_supervisor() {
        let kas_stdin = shared_writer(RecordingWriter::default());
        forward_client_input(b"".as_slice(), Arc::clone(&kas_stdin), empty_response_owners())
            .await
            .expect("clean EOF");
        assert!(kas_stdin.lock().await.is_some());
    }

    #[tokio::test]
    async fn bounded_reader_rejects_oversized_frames() {
        let mut input = BufReader::new(b"12345678\n".as_slice());
        let mut frame = Vec::new();
        let error = read_bounded_frame(&mut input, &mut frame, "test input", 8)
            .await
            .expect_err("oversized frame must fail");
        assert!(error.to_string().contains("8-byte limit"));
    }

    #[tokio::test]
    async fn returns_auth_errors_to_kas_without_terminating_output_relay() {
        let normal = b"{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{}}\n";
        let mut output = br#"{"jsonrpc":"2.0","id":42,"method":"_kiro/auth/getAccessToken","params":{}}
"#
        .to_vec();
        output.extend_from_slice(normal);

        let client_stdout = RecordingWriter::default();
        let kas_stdin_writer = RecordingWriter::default();
        forward_kas_output(
            output.as_slice(),
            AsyncFrameSink(client_stdout.clone()),
            shared_writer(kas_stdin_writer.clone()),
            empty_response_owners(),
            Arc::new(AuthTracker::default()),
            |_| async {
                Err(AcpError::internal_error().data(json!({
                    "details": "You are not logged in. Please log in with `kiro-cli login`."
                })))
            },
        )
        .await
        .expect("auth errors stay on protocol");

        assert_eq!(client_stdout.bytes(), normal);
        let response: Value = serde_json::from_slice(&kas_stdin_writer.bytes()).expect("error response JSON");
        assert_eq!(response["id"], 42);
        assert_eq!(response["error"]["code"], -32603);
        assert!(
            response["error"]["data"]["details"]
                .as_str()
                .unwrap()
                .contains("kiro-cli login")
        );
    }

    #[tokio::test]
    async fn malformed_auth_ids_return_invalid_request_without_calling_handler() {
        let output = br#"{"jsonrpc":"2.0","method":"_kiro/auth/getAccessToken","params":{}}
{"jsonrpc":"2.0","id":null,"method":"_kiro/auth/getAccessToken","params":{}}
{"jsonrpc":"2.0","id":{},"method":"_kiro/auth/getAccessToken","params":{}}
"#;
        let client_stdout = RecordingWriter::default();
        let kas_stdin_writer = RecordingWriter::default();
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_handler = Arc::clone(&calls);

        forward_kas_output(
            output.as_slice(),
            AsyncFrameSink(client_stdout.clone()),
            shared_writer(kas_stdin_writer.clone()),
            empty_response_owners(),
            Arc::new(AuthTracker::default()),
            move |_| {
                calls_for_handler.fetch_add(1, Ordering::SeqCst);
                async { Ok(token_response()) }
            },
        )
        .await
        .expect("malformed auth requests stay on protocol");

        assert!(client_stdout.bytes().is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let responses: Vec<Value> = kas_stdin_writer
            .bytes()
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).expect("invalid-request response JSON"))
            .collect();
        assert_eq!(responses.len(), 3);
        for response in responses {
            assert!(response["id"].is_null());
            assert_eq!(response["error"]["code"], -32600);
        }
    }

    #[tokio::test]
    async fn concurrent_client_and_auth_writes_do_not_interleave() {
        let client_frame = format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{{\"padding\":\"{}\"}}}}\n",
            "x".repeat(32_768)
        );
        let auth_frame = b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"_kiro/auth/getAccessToken\",\"params\":{}}\n";
        let kas_stdin_writer = RecordingWriter::default();
        let kas_stdin = shared_writer(kas_stdin_writer.clone());
        let allowed = empty_response_owners();

        let (mut input_writer, input_reader) = tokio::io::duplex(client_frame.len() + 1);
        input_writer
            .write_all(client_frame.as_bytes())
            .await
            .expect("prime client input");
        let output = async {
            let result = forward_kas_output(
                auth_frame.as_slice(),
                AsyncFrameSink(RecordingWriter::default()),
                Arc::clone(&kas_stdin),
                Arc::clone(&allowed),
                Arc::new(AuthTracker::default()),
                |_| async {
                    tokio::task::yield_now().await;
                    Ok(token_response())
                },
            )
            .await;
            drop(input_writer);
            result
        };
        let (client_result, output_result) = tokio::join!(
            forward_client_input(input_reader, Arc::clone(&kas_stdin), Arc::clone(&allowed)),
            output,
        );
        client_result.expect("client relay");
        output_result.expect("KAS relay");

        let bytes = kas_stdin_writer.bytes();
        let lines: Vec<_> = bytes
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .collect();
        assert_eq!(lines.len(), 2);
        let messages: Vec<Value> = lines
            .into_iter()
            .map(|line| serde_json::from_slice(line).expect("non-interleaved JSON frame"))
            .collect();
        assert!(
            messages
                .iter()
                .any(|message| message.get("method") == Some(&json!("initialize")))
        );
        assert!(
            messages
                .iter()
                .any(|message| message["id"] == 2 && message.get("result").is_some())
        );
    }

    #[tokio::test]
    async fn client_eof_finisher_waits_for_an_accepted_auth_response() {
        let auth_frame = b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"_kiro/auth/getAccessToken\",\"params\":{}}\n";
        let owners = empty_response_owners();
        let kas_stdin_writer = RecordingWriter::default();
        let kas_stdin = shared_writer(kas_stdin_writer.clone());
        let (started, started_rx) = oneshot::channel();
        let (release, release_rx) = oneshot::channel();
        let mut started = Some(started);
        let mut release_rx = Some(release_rx);
        let output = forward_kas_output(
            auth_frame.as_slice(),
            AsyncFrameSink(RecordingWriter::default()),
            Arc::clone(&kas_stdin),
            Arc::clone(&owners),
            Arc::new(AuthTracker::default()),
            move |_| {
                let started = started.take().expect("one auth request");
                let release_rx = release_rx.take().expect("one auth request");
                async move {
                    let _ = started.send(());
                    let _ = release_rx.await;
                    Ok(token_response())
                }
            },
        );
        tokio::pin!(output);

        tokio::select! {
            result = &mut output => panic!("relay ended before auth started: {result:?}"),
            started = started_rx => started.expect("auth task started"),
        }
        let child_exit = async { Ok(()) };
        tokio::pin!(child_exit);
        let signal = std::future::pending::<ExitCode>();
        tokio::pin!(signal);
        let mut finish = Box::pin(finish_after_client_eof(
            &kas_stdin,
            &owners,
            &mut output,
            &mut child_exit,
            &mut signal,
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut finish)
                .await
                .is_err(),
            "client EOF finisher closed stdin before the accepted response completed"
        );
        assert!(kas_stdin.lock().await.is_some());

        release.send(()).expect("release auth task");
        assert!(matches!(
            finish.await.expect("finish client EOF"),
            RelayCompletion::Child
        ));

        assert!(kas_stdin.lock().await.is_none());
        let response: Value = serde_json::from_slice(&kas_stdin_writer.bytes()).expect("auth response JSON");
        assert_eq!(response["id"], 2);
        assert_eq!(response["result"]["accessToken"], "short-lived-access-token");
    }

    #[tokio::test]
    async fn production_finish_waits_for_in_flight_auth() {
        let tracker = Arc::new(AuthTracker::default());
        let (started, started_rx) = oneshot::channel();
        let (release, release_rx) = oneshot::channel();
        let _auth_result = tracker.spawn(async move {
            let _ = started.send(());
            let _ = release_rx.await;
            Ok(token_response())
        });
        started_rx.await.expect("auth task started");

        let mut finish = Box::pin(finish_with_auth_drain(Ok(()), &tracker));
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut finish)
                .await
                .is_err()
        );
        release.send(()).expect("release auth task");
        finish.await.expect("production finish drained auth");
    }

    #[tokio::test(start_paused = true)]
    async fn production_finish_bounds_a_stuck_auth_drain() {
        let tracker = Arc::new(AuthTracker::default());
        let _auth_result = tracker.spawn(std::future::pending());
        let started = tokio::time::Instant::now();

        finish_with_auth_drain(Ok(()), &tracker)
            .await
            .expect("production finish bounded auth drain");

        assert_eq!(started.elapsed(), AUTH_DRAIN_TIMEOUT);
    }

    #[tokio::test]
    async fn auth_task_survives_output_relay_cancellation() {
        let auth_frame = b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"_kiro/auth/getAccessToken\",\"params\":{}}\n";
        let tracker = Arc::new(AuthTracker::default());
        let (started, started_rx) = oneshot::channel();
        let (release, release_rx) = oneshot::channel();
        let completed = Arc::new(AtomicUsize::new(0));
        let completed_for_handler = Arc::clone(&completed);
        let mut started = Some(started);
        let mut release_rx = Some(release_rx);

        let mut relay = Box::pin(forward_kas_output(
            auth_frame.as_slice(),
            AsyncFrameSink(RecordingWriter::default()),
            shared_writer(RecordingWriter::default()),
            empty_response_owners(),
            Arc::clone(&tracker),
            move |_| {
                let started = started.take().expect("one auth request");
                let release_rx = release_rx.take().expect("one auth request");
                let completed = Arc::clone(&completed_for_handler);
                async move {
                    let _ = started.send(());
                    let _ = release_rx.await;
                    completed.fetch_add(1, Ordering::SeqCst);
                    Ok(token_response())
                }
            },
        ));

        tokio::select! {
            result = &mut relay => panic!("relay ended before auth started: {result:?}"),
            started = started_rx => started.expect("auth task started"),
        }
        drop(relay);
        release.send(()).expect("release auth task");
        tokio::time::timeout(Duration::from_secs(1), tracker.wait_idle())
            .await
            .expect("auth task drained");
        assert_eq!(completed.load(Ordering::SeqCst), 1);
    }
}
