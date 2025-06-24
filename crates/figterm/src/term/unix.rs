use std::fs::OpenOptions;
use std::io::{
    Error as IoError,
    ErrorKind,
    Write,
    stdin,
    stdout,
};
use std::mem;
use std::os::fd::BorrowedFd;
use std::os::unix::io::AsRawFd;
use std::sync::atomic::{
    AtomicBool,
    Ordering,
};
use std::time::Duration;

use anyhow::{
    Context,
    Result,
    bail,
};
use bytes::BytesMut;
use filedescriptor::FileDescriptor;
use flume::{
    Receiver,
    bounded,
};
use nix::libc::{
    self,
    winsize,
};
use nix::sys::termios::{
    FlushArg,
    SetArg,
    Termios,
    cfmakeraw,
    tcdrain,
    tcflush,
    tcgetattr,
    tcsetattr,
};
use tokio::io::{
    self,
    AsyncReadExt,
};
use tokio::select;
use tokio::signal::unix::SignalKind;
use tokio::time::MissedTickBehavior;
use tracing::{
    error,
    trace,
    warn,
};

use super::InputEventResult;
use crate::input::{
    InputEvent,
    InputParser,
};
use crate::term::istty::IsTty;
use crate::term::{
    ScreenSize,
    Terminal,
    cast,
};

const BUF_SIZE: usize = 4096;

pub enum Purge {
    InputQueue,
    OutputQueue,
    InputAndOutputQueue,
}

pub enum SetAttributeWhen {
    /// changes are applied immediately
    Now,
    /// Apply once the current output queue has drained
    AfterDrainOutputQueue,
    /// Wait for the current output queue to drain, then
    /// discard any unread input
    AfterDrainOutputQueuePurgeInputQueue,
}

pub trait UnixTty {
    fn get_size(&mut self) -> Result<winsize>;
    fn set_size(&mut self, size: winsize) -> Result<()>;
    fn get_termios(&mut self) -> Result<Termios>;
    fn set_termios(&mut self, termios: &Termios, when: SetAttributeWhen) -> Result<()>;
    /// Waits until all written data has been transmitted.
    fn drain(&mut self) -> Result<()>;
    fn purge(&mut self, purge: Purge) -> Result<()>;
}

pub struct TtyWriteHandle {
    fd: FileDescriptor,
    write_buffer: Vec<u8>,
}

impl TtyWriteHandle {
    fn new(fd: FileDescriptor) -> Self {
        Self {
            fd,
            write_buffer: Vec::with_capacity(BUF_SIZE),
        }
    }

    fn flush_local_buffer(&mut self) -> std::result::Result<(), IoError> {
        if !self.write_buffer.is_empty() {
            self.fd.write_all(&self.write_buffer)?;
            self.write_buffer.clear();
        }
        Ok(())
    }
}

impl Write for TtyWriteHandle {
    fn write(&mut self, buf: &[u8]) -> std::result::Result<usize, IoError> {
        if self.write_buffer.len() + buf.len() > self.write_buffer.capacity() {
            self.flush()?;
        }
        if buf.len() >= self.write_buffer.capacity() {
            self.fd.write(buf)
        } else {
            self.write_buffer.write(buf)
        }
    }

    fn flush(&mut self) -> std::result::Result<(), IoError> {
        self.flush_local_buffer()?;
        self.drain()
            .map_err(|e| IoError::new(ErrorKind::Other, format!("{e}")))?;
        Ok(())
    }
}

/// SAFETY: the [FileDescriptor] is already guaranteed to have an owned file descriptor by
/// its contract, so we just borrow it with a lifetime which make the borrow safe.
fn borrow_fd(fd: &FileDescriptor) -> BorrowedFd<'_> {
    unsafe { BorrowedFd::borrow_raw(fd.as_raw_fd()) }
}

impl UnixTty for TtyWriteHandle {
    fn get_size(&mut self) -> Result<winsize> {
        let mut size: winsize = unsafe { mem::zeroed() };
        if unsafe { libc::ioctl(self.fd.as_raw_fd(), libc::TIOCGWINSZ as _, &mut size) } != 0 {
            bail!("failed to ioctl(TIOCGWINSZ): {}", IoError::last_os_error());
        }
        Ok(size)
    }

    fn set_size(&mut self, size: winsize) -> Result<()> {
        if unsafe { libc::ioctl(self.fd.as_raw_fd(), libc::TIOCSWINSZ as _, &size as *const _) } != 0 {
            bail!("failed to ioctl(TIOCSWINSZ): {:?}", IoError::last_os_error());
        }

        Ok(())
    }

    fn get_termios(&mut self) -> Result<Termios> {
        tcgetattr(borrow_fd(&self.fd)).context("get_termios failed")
    }

    fn set_termios(&mut self, termios: &Termios, when: SetAttributeWhen) -> Result<()> {
        let when = match when {
            SetAttributeWhen::Now => SetArg::TCSANOW,
            SetAttributeWhen::AfterDrainOutputQueue => SetArg::TCSADRAIN,
            SetAttributeWhen::AfterDrainOutputQueuePurgeInputQueue => SetArg::TCSAFLUSH,
        };
        tcsetattr(borrow_fd(&self.fd), when, termios).context("set_termios failed")
    }

    fn drain(&mut self) -> Result<()> {
        tcdrain(borrow_fd(&self.fd)).context("tcdrain failed")
    }

    fn purge(&mut self, purge: Purge) -> Result<()> {
        let param = match purge {
            Purge::InputQueue => FlushArg::TCIFLUSH,
            Purge::OutputQueue => FlushArg::TCOFLUSH,
            Purge::InputAndOutputQueue => FlushArg::TCIOFLUSH,
        };
        tcflush(borrow_fd(&self.fd), param).context("tcflush failed")
    }
}

/// A unix style terminal
pub struct UnixTerminal {
    write: TtyWriteHandle,
    saved_termios: Termios,
}

impl UnixTerminal {
    /// Attempt to create an instance from the stdin and stdout of the
    /// process.  This will fail unless both are associated with a tty.
    /// Note that this will duplicate the underlying file descriptors
    /// and will no longer participate in the stdin/stdout locking
    /// provided by the rust standard library.
    pub fn new_from_stdio() -> Result<UnixTerminal> {
        Self::new_with(&stdin(), &stdout())
    }

    pub fn new_with<A: AsRawFd, B: AsRawFd>(read: &A, write: &B) -> Result<UnixTerminal> {
        if !read.is_tty() || !write.is_tty() {
            anyhow::bail!("stdin and stdout must both be tty handles");
        }

        let mut write = TtyWriteHandle::new(FileDescriptor::dup(write)?);
        let saved_termios = write.get_termios()?;

        Ok(UnixTerminal { write, saved_termios })
    }

    /// Attempt to explicitly open a handle to the terminal device
    /// (/dev/tty) and build a `UnixTerminal` from there.  This will
    /// yield a terminal even if the stdio streams have been redirected,
    /// provided that the process has an associated controlling terminal.
    pub fn new() -> Result<UnixTerminal> {
        let file = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
        Self::new_with(&file, &file)
    }
}

static IMMEDIATE_MODE: AtomicBool = AtomicBool::new(true);

impl Terminal for UnixTerminal {
    fn set_raw_mode(&mut self) -> Result<()> {
        let mut raw = self.write.get_termios()?;
        cfmakeraw(&mut raw);
        self.write
            .set_termios(&raw, SetAttributeWhen::AfterDrainOutputQueuePurgeInputQueue)
            .context("failed to set raw mode")?;
        self.write.flush()?;

        Ok(())
    }

    fn set_cooked_mode(&mut self) -> Result<()> {
        self.write.set_termios(&self.saved_termios, SetAttributeWhen::Now)
    }

    fn get_screen_size(&mut self) -> Result<ScreenSize> {
        let size = self.write.get_size()?;
        Ok(ScreenSize {
            rows: cast(size.ws_row)?,
            cols: cast(size.ws_col)?,
            xpixel: cast(size.ws_xpixel)?,
            ypixel: cast(size.ws_ypixel)?,
        })
    }

    fn set_screen_size(&mut self, size: ScreenSize) -> Result<()> {
        let size = winsize {
            ws_row: cast(size.rows)?,
            ws_col: cast(size.cols)?,
            ws_xpixel: cast(size.xpixel)?,
            ws_ypixel: cast(size.ypixel)?,
        };

        self.write.set_size(size)
    }

    fn flush(&mut self) -> Result<()> {
        self.write.flush().context("flush failed")
    }

    fn set_immediate_mode(&mut self, immediate: bool) -> Result<()> {
        IMMEDIATE_MODE.store(immediate, Ordering::SeqCst);
        Ok(())
    }

    fn read_input(&mut self) -> Result<Receiver<InputEventResult>> {
        let mut window_change_signal = tokio::signal::unix::signal(SignalKind::window_change())?;
        let (input_tx, input_rx) = bounded::<InputEventResult>(1);

        tokio::spawn(async move {
            let mut stdin = io::stdin();
            let mut parser = InputParser::new();
            let mut buf = BytesMut::with_capacity(crate::BUFFER_SIZE);
            let mut read_timeout = tokio::time::interval(Duration::from_millis(1));
            read_timeout.set_missed_tick_behavior(MissedTickBehavior::Delay);

            macro_rules! parse_buf {
                () => {
                    async {
                        let mut events = vec![];
                        parser.parse(&buf, |raw, evt| events.push(Ok((raw, evt))), false);
                        buf.clear();
                        if let Err(err) = input_tx.send_async(events).await {
                            error!(%err, "Error sending event");
                        }
                    }
                }
            }

            loop {
                select! {
                    biased;
                    res = stdin.read_buf(&mut buf) => {
                        match res {
                            Ok(n) => {
                                trace!(buf =? &buf[0..n], "Read input");
                                let full = buf.capacity() == buf.len();
                                if full {
                                    buf.reserve(buf.len());
                                }
                                if IMMEDIATE_MODE.load(Ordering::SeqCst) {
                                    parse_buf!().await;
                                }
                                read_timeout.reset();
                            }
                            Err(err) => {
                                if let Err(err) = input_tx.send_async(vec![Err(anyhow::anyhow!(err))]).await {
                                    error!(%err, "Error sending event");
                                }
                            }
                        }
                    }
                    _ = read_timeout.tick(), if !buf.is_empty() && !IMMEDIATE_MODE.load(Ordering::SeqCst) => {
                        tracing::debug!(?buf, "Parsing input");
                        parse_buf!().await;
                    }
                    _ = window_change_signal.recv() => {
                        let event = InputEvent::Resized;
                        if let Err(err) = input_tx.send_async(vec![Ok((None, event))]).await {
                            warn!(%err, "Error sending event");
                        }
                    }
                }
            }
        });

        Ok(input_rx)
    }
}

impl Drop for UnixTerminal {
    fn drop(&mut self) {
        self.write.flush().unwrap();
        self.write
            .set_termios(&self.saved_termios, SetAttributeWhen::Now)
            .expect("failed to restore original termios state");
    }
}
