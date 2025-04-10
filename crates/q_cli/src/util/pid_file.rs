use std::fs::{
    File,
    OpenOptions,
};
use std::io::{
    Error,
    ErrorKind,
    Seek,
    SeekFrom,
    Write,
};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

use eyre::Result;
use nix::fcntl::{
    Flock,
    FlockArg,
};
use nix::sys::signal::{
    Signal,
    kill,
};
use nix::unistd::Pid;
use tokio::fs::read_to_string;
use tokio::time::sleep;
use tracing::{
    debug,
    error,
    info,
    instrument,
    warn,
};

/// A file-based process lock that ensures only one instance of a process is running.
///
/// `PidLock` works by:
/// 1. Creating/opening a PID file at the specified path
/// 2. Attempting to acquire an exclusive lock on the file
/// 3. Writing the current process ID to the file
/// 4. If another process holds the lock, attempts to terminate that process first
///
/// The lock is automatically released when the `PidLock` instance is dropped.
#[derive(Debug)]
pub struct PidLock {
    lock: Flock<File>,
    pid_path: PathBuf,
}

impl PidLock {
    #[instrument(name = "PidLock::new")]
    pub async fn new(pid_path: PathBuf) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o644)
            .open(&pid_path)
            .inspect_err(|err| error!(%err, "Failed to open pid file"))?;

        // Try to get exclusive lock
        let mut lock = match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
            Ok(lock) => lock,
            Err((file, err)) => {
                debug!(%err, "Failed to acquire lock, trying to handle existing process");

                // Read existing PID
                match read_to_string(&pid_path).await {
                    Ok(content) => match content.trim().parse::<i32>() {
                        Ok(pid) => {
                            debug!(%pid, "Found existing process ID");
                            if let Err(err) = kill_process(pid).await {
                                error!(%err, %pid, "Failed to kill existing process");
                            } else {
                                info!(%pid, "Successfully killed existing process");
                            }
                        },
                        Err(err) => {
                            warn!(%err, %content, "Failed to parse PID from lockfile");
                        },
                    },
                    Err(err) => warn!(%err, "Failed to read PID from lockfile"),
                }

                Flock::lock(file, FlockArg::LockExclusiveNonblock).map_err(|(_, err)| {
                    error!(%err, "Failed to acquire lock after handling existing process");
                    err
                })?
            },
        };

        // Write current PID
        let current_pid = std::process::id();
        lock.set_len(0)
            .inspect_err(|err| error!(%err, "Failed to truncate lock file"))?;
        lock.seek(SeekFrom::Start(0))
            .inspect_err(|err| error!(%err, "Failed to seek to start of file"))?;
        lock.write_all(current_pid.to_string().as_bytes())
            .inspect_err(|err| error!(%err, "Failed to write PID to file"))?;
        lock.flush()
            .inspect_err(|err| error!(%err, "Failed to flush lock file"))?;

        info!(%current_pid, "Successfully created and locked PID file");
        Ok(PidLock { lock, pid_path })
    }

    #[instrument(name = "PidLock::release", skip(self), fields(pid_path =? self.pid_path))]
    pub fn release(mut self) -> Result<(), Error> {
        debug!("Releasing PID lock");
        self.lock
            .set_len(0)
            .inspect_err(|err| error!(%err, "Failed to truncate lock file during release"))?;
        self.lock
            .flush()
            .inspect_err(|err| error!(%err, "Failed to flush lock file during release"))?;
        self.lock.unlock().map_err(|(_, err)| {
            error!(%err, "Failed to unlock file during release");
            err
        })?;
        Ok(())
    }
}

#[instrument(level = "debug")]
fn process_exists(pid: i32) -> bool {
    let exists = kill(Pid::from_raw(pid), None).is_ok();
    debug!(%pid, %exists, "Checked if process exists");
    exists
}

#[instrument(level = "debug")]
async fn kill_process(pid: i32) -> Result<()> {
    if !process_exists(pid) {
        error!(%pid, "Process not found");
        return Err(Error::new(ErrorKind::NotFound, format!("Process already running with PID {pid}")).into());
    }

    info!(%pid, "Attempting to terminate process");
    match kill(Pid::from_raw(pid), Signal::SIGINT) {
        Ok(_) => {
            debug!(%pid, "Sent SIGINT signal");

            // Wait for the process to terminate
            for i in 0..50 {
                if !process_exists(pid) {
                    info!(%pid, "Process terminated successfully");
                    return Ok(());
                }
                debug!(%pid, attempt = i, "Process still running, waiting");
                sleep(std::time::Duration::from_millis(100)).await;
            }

            if process_exists(pid) {
                warn!(%pid, "Process didn't terminate gracefully, sending SIGKILL");
                let _ = kill(Pid::from_raw(pid), Signal::SIGKILL)
                    .inspect_err(|err| error!(%err, %pid, "Failed to send SIGKILL"));
                sleep(std::time::Duration::from_millis(100)).await;
            }
            Ok(())
        },
        Err(err) => {
            error!(%err, %pid, "Failed to send SIGINT");
            Err(Error::new(ErrorKind::Other, format!("Failed to terminate existing process: {err}")).into())
        },
    }
}
