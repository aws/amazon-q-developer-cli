use std::io::{
    self,
    Read,
    Write,
};
use std::os::unix::io::{
    AsRawFd,
    FromRawFd,
    RawFd,
};
use std::os::unix::process::CommandExt;
use std::path::Path;

use anyhow::{
    Context,
    Result,
};
use async_trait::async_trait;
use filedescriptor::FileDescriptor;
use nix::fcntl::{
    FcntlArg,
    FdFlag,
    OFlag,
    fcntl,
    open,
};
use nix::libc;
use nix::pty::{
    PtyMaster,
    Winsize,
    grantpt,
    posix_openpt,
    ptsname,
    unlockpt,
};
use nix::sys::signal::{
    SigHandler,
    Signal,
    signal,
};
use nix::sys::stat::{
    Mode,
    umask,
};
use portable_pty::unix::close_random_fds;
use portable_pty::{
    Child,
    PtySize,
};
use tokio::io::unix::AsyncFd;

use crate::pty::{
    AsyncMasterPty,
    CommandBuilder,
    MasterPty,
    PtyPair,
    SlavePty,
};

nix::ioctl_write_ptr_bad!(ioctl_tiocswinsz, libc::TIOCSWINSZ, Winsize);

struct UnixSlavePty {
    name: String,
    fd: FileDescriptor,
}

struct UnixMasterPty {
    fd: PtyMaster,
}

struct UnixAsyncMasterPty {
    fd: AsyncFd<PtyMaster>,
}

/// Helper function to set the close-on-exec flag for a raw descriptor
fn cloexec(fd: RawFd) -> Result<()> {
    let flags = fcntl(fd, FcntlArg::F_GETFD)?;
    fcntl(
        fd,
        FcntlArg::F_SETFD(FdFlag::from_bits_truncate(flags) | FdFlag::FD_CLOEXEC),
    )?;
    Ok(())
}

/// Open a pseudoterminal
pub fn open_pty(pty_size: &PtySize) -> Result<PtyPair> {
    // Open a new pseudoterminal master
    // The pseudoterminal must be initialized with O_NONBLOCK since on macOS, the
    // it can not be safely set with fcntl() later on.
    // https://github.com/pkgw/stund/blob/master/tokio-pty-process/src/lib.rs#L127-L133
    cfg_if::cfg_if! {
        if #[cfg(any(target_os = "macos", target_os = "linux"))] {
            let oflag = OFlag::O_RDWR | OFlag::O_NONBLOCK;
        } else if #[cfg(target_os = "freebsd")] {
            let oflag = OFlag::O_RDWR;
        }
    }
    let master_pty = posix_openpt(oflag).context("Failed to openpt")?;

    // Allow pseudoterminal pair to be generated
    grantpt(&master_pty).context("Failed to grantpt")?;
    unlockpt(&master_pty).context("Failed to unlockpt")?;

    // Get the name of the pseudoterminal
    // SAFETY: This is done before any threads are spawned, thus it being
    // non thread safe is not an issue
    let pty_name = unsafe { ptsname(&master_pty) }?;
    let slave_pty = open(Path::new(&pty_name), OFlag::O_RDWR, Mode::empty())?;

    // let termios = tcgetattr(STDIN_FILENO)
    //    .context("Failed to get terminal attributes")?;
    // tcsetattr(slave_pty, SetArg::TCSANOW, termios)?;
    let winsize = Winsize {
        ws_row: pty_size.rows,
        ws_col: pty_size.cols,
        ws_xpixel: pty_size.pixel_width,
        ws_ypixel: pty_size.pixel_height,
    };
    unsafe { ioctl_tiocswinsz(slave_pty, &winsize) }?;

    #[cfg(target_os = "freebsd")]
    set_nonblocking(master_pty.as_raw_fd()).context("Failed to set nonblocking")?;

    let master = UnixMasterPty { fd: master_pty };
    let slave = UnixSlavePty {
        name: pty_name,
        fd: unsafe { FileDescriptor::from_raw_fd(slave_pty) },
    };

    // Ensure that these descriptors will get closed when we execute
    // the child process. This is done after constructing the Pty
    // instances so that we ensure that the Ptys get drop()'d if
    // the cloexec() functions fail (unlikely!).
    cloexec(master.fd.as_raw_fd())?;
    cloexec(slave.fd.as_raw_fd())?;

    Ok(PtyPair {
        master: Box::new(master),
        slave: Box::new(slave),
    })
}

impl SlavePty for UnixSlavePty {
    fn spawn_command(&self, builder: CommandBuilder) -> anyhow::Result<Box<dyn Child + Send + Sync>> {
        let configured_mask = builder.umask;
        let mut cmd = builder.as_command()?;

        cmd.stdin(self.fd.as_stdio()?)
            .stdout(self.fd.as_stdio()?)
            .stderr(self.fd.as_stdio()?);

        let pre_exec_fn = move || {
            // Clean up a few things before we exec the program
            // Clear out any potentially problematic signal
            // dispositions that we might have inherited
            for signo in [
                Signal::SIGCHLD,
                Signal::SIGHUP,
                Signal::SIGINT,
                Signal::SIGQUIT,
                Signal::SIGTERM,
                Signal::SIGALRM,
            ] {
                unsafe { signal(signo, SigHandler::SigDfl) }?;
            }

            // Establish ourselves as a session leader.
            nix::unistd::setsid()?;

            // Clippy wants us to explicitly cast TIOCSCTTY using
            // type::from(), but the size and potentially signedness
            // are system dependent, which is why we're using `as _`.
            // Suppress this lint for this section of code.
            {
                // Set the pty as the controlling terminal.
                // Failure to do this means that delivery of
                // SIGWINCH won't happen when we resize the
                // terminal, among other undesirable effects.
                if unsafe { libc::ioctl(0, libc::TIOCSCTTY as _, 0) == -1 } {
                    return Err(io::Error::last_os_error());
                }
            }

            close_random_fds();

            if let Some(mode) = configured_mask {
                umask(mode);
            }

            Ok(())
        };

        unsafe { cmd.pre_exec(pre_exec_fn) };

        let mut child = cmd.spawn()?;

        // Ensure that we close out the slave fds that Child retains;
        // they are not what we need (we need the master side to reference
        // them) and won't work in the usual way anyway.
        // In practice these are None, but it seems best to be move them
        // out in case the behavior of Command changes in the future.
        child.stdin.take();
        child.stdout.take();
        child.stderr.take();

        Ok(Box::new(child))
    }

    fn get_name(&self) -> Option<String> {
        Some(self.name.clone())
    }
}

#[async_trait]
impl AsyncMasterPty for UnixAsyncMasterPty {
    async fn read(&mut self, buff: &mut [u8]) -> io::Result<usize> {
        loop {
            let mut guard = self.fd.readable_mut().await?;

            match guard.try_io(|inner| inner.get_mut().read(buff)) {
                Ok(result) => return result,
                Err(_would_block) => continue,
            }
        }
    }

    async fn write(&mut self, buff: &[u8]) -> io::Result<usize> {
        loop {
            let mut guard = self.fd.writable_mut().await?;

            match guard.try_io(|inner| inner.get_mut().write(buff)) {
                Ok(result) => return result,
                Err(_would_block) => continue,
            }
        }
    }

    fn resize(&self, size: PtySize) -> Result<()> {
        let ws_size = Winsize {
            ws_row: size.rows,
            ws_col: size.cols,
            ws_xpixel: size.pixel_width,
            ws_ypixel: size.pixel_height,
        };

        let fd = self.fd.as_raw_fd();
        let res = unsafe { libc::ioctl(fd, libc::TIOCSWINSZ as _, &ws_size as *const _) };

        if res != 0 {
            anyhow::bail!("failed to ioctl(TIOCSWINSZ): {:?}", io::Error::last_os_error());
        }

        Ok(())
    }
}

impl MasterPty for UnixMasterPty {
    fn get_async_master_pty(self: Box<Self>) -> Result<Box<dyn AsyncMasterPty + Send + Sync>> {
        Ok(Box::new(UnixAsyncMasterPty {
            fd: AsyncFd::new(self.fd).context("Failed to create AsyncFd")?,
        }))
    }
}

impl AsRawFd for UnixMasterPty {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

/// Set `fd` into non-blocking mode using `O_NONBLOCKING`
#[cfg(target_os = "freebsd")]
fn set_nonblocking(fd: RawFd) -> Result<()> {
    use nix::fcntl;

    let old_oflag_c_int =
        fcntl::fcntl(fd, FcntlArg::F_GETFL).with_context(|| format!("Failed to get flags for fd {fd:?}"))?;

    let old_oflag = OFlag::from_bits_truncate(old_oflag_c_int);

    fcntl::fcntl(fd, FcntlArg::F_SETFL(old_oflag | OFlag::O_NONBLOCK))
        .with_context(|| format!("Failed to set O_NONBLOCK for fd {fd:?}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openpty() {
        open_pty(&PtySize::default()).unwrap();
    }
}
