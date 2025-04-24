use std::io::{
    self,
    Read,
    Write,
};
use std::os::windows::io::{
    AsRawHandle,
    RawHandle,
};
use std::pin::Pin;
use std::sync::{
    Arc,
    Mutex,
};
use std::task::{
    Context,
    Poll,
};

use anyhow::{
    Context as _,
    Result,
};
use async_trait::async_trait;
use filedescriptor::{
    FileDescriptor,
    OwnedHandle,
    Pipe,
};
use flume::{
    Receiver,
    Sender,
    unbounded,
};
use portable_pty::{
    Child,
    ChildKiller,
    ExitStatus,
};
use tracing::error;
use winapi::shared::minwindef::DWORD;
use winapi::um::minwinbase::STILL_ACTIVE;
use winapi::um::processthreadsapi::*;
use winapi::um::synchapi::WaitForSingleObject;
use winapi::um::winbase::INFINITE;
use winapi::um::wincon::COORD;

use crate::pty::win::pseudocon::PseudoCon;
use crate::pty::{
    AsyncMasterPty,
    CommandBuilder,
    MasterPty,
    PtyPair,
    PtySize,
    SlavePty,
};

mod procthreadattr;
mod pseudocon;

#[derive(Debug)]
pub struct WinChild {
    proc: Mutex<OwnedHandle>,
}

impl WinChild {
    fn child_has_completed(&mut self) -> io::Result<Option<ExitStatus>> {
        let mut status: DWORD = 0;
        let proc = self.proc.lock().unwrap().try_clone().unwrap();
        let res = unsafe { GetExitCodeProcess(proc.as_raw_handle() as _, &mut status) };
        if res != 0 {
            if status == STILL_ACTIVE {
                Ok(None)
            } else {
                Ok(Some(ExitStatus::with_exit_code(status)))
            }
        } else {
            Ok(None)
        }
    }

    fn do_kill(&mut self) -> io::Result<()> {
        let proc = self.proc.lock().unwrap().try_clone().unwrap();
        let res = unsafe { TerminateProcess(proc.as_raw_handle() as _, 1) };
        let err = io::Error::last_os_error();
        if res != 0 { Err(err) } else { Ok(()) }
    }
}

impl ChildKiller for WinChild {
    fn kill(&mut self) -> io::Result<()> {
        self.do_kill().ok();
        Ok(())
    }

    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        let proc = self.proc.lock().unwrap().try_clone().unwrap();
        Box::new(WinChildKiller { proc })
    }
}

#[derive(Debug)]
pub struct WinChildKiller {
    proc: OwnedHandle,
}

impl ChildKiller for WinChildKiller {
    fn kill(&mut self) -> io::Result<()> {
        let res = unsafe { TerminateProcess(self.proc.as_raw_handle() as _, 1) };
        let err = io::Error::last_os_error();
        if res != 0 { Err(err) } else { Ok(()) }
    }

    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        let proc = self.proc.try_clone().unwrap();
        Box::new(WinChildKiller { proc })
    }
}

impl Child for WinChild {
    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child_has_completed()
    }

    fn wait(&mut self) -> io::Result<ExitStatus> {
        if let Ok(Some(status)) = self.try_wait() {
            return Ok(status);
        }
        let proc = self.proc.lock().unwrap().try_clone().unwrap();
        unsafe {
            WaitForSingleObject(proc.as_raw_handle() as _, INFINITE);
        }
        let mut status: DWORD = 0;
        let res = unsafe { GetExitCodeProcess(proc.as_raw_handle() as _, &mut status) };
        if res != 0 {
            Ok(ExitStatus::with_exit_code(status))
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn process_id(&self) -> Option<u32> {
        let res = unsafe { GetProcessId(self.proc.lock().unwrap().as_raw_handle() as _) };
        if res == 0 { None } else { Some(res) }
    }

    fn as_raw_handle(&self) -> Option<std::os::windows::io::RawHandle> {
        let proc = self.proc.lock().unwrap();
        Some(proc.as_raw_handle())
    }
}

impl std::future::Future for WinChild {
    type Output = anyhow::Result<ExitStatus>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<anyhow::Result<ExitStatus>> {
        match self.child_has_completed() {
            Ok(Some(status)) => Poll::Ready(Ok(status)),
            Err(err) => Poll::Ready(Err(err).context("Failed to retrieve process exit status")),
            Ok(None) => {
                struct PassRawHandleToWaiterThread(pub RawHandle);
                unsafe impl Send for PassRawHandleToWaiterThread {}

                let proc = self.proc.lock().unwrap().try_clone()?;
                let handle = PassRawHandleToWaiterThread(proc.as_raw_handle());

                let waker = cx.waker().clone();
                std::thread::spawn(move || {
                    let handle = handle;
                    unsafe {
                        WaitForSingleObject(handle.0 as _, INFINITE);
                    }
                    waker.wake();
                });
                Poll::Pending
            },
        }
    }
}

pub fn open_pty(size: &PtySize) -> anyhow::Result<PtyPair> {
    let stdin = Pipe::new()?;
    let stdout = Pipe::new()?;

    let con = PseudoCon::new(
        COORD {
            X: size.cols as i16,
            Y: size.rows as i16,
        },
        stdin.read,
        stdout.write,
    )?;

    let master = ConPtyMasterPty {
        inner: Arc::new(Mutex::new(Inner {
            con,
            readable: stdout.read,
            writable: stdin.write,
            size: *size,
        })),
    };

    let slave = ConPtySlavePty {
        inner: master.inner.clone(),
    };

    Ok(PtyPair {
        master: Box::new(master),
        slave: Box::new(slave),
    })
}

struct Inner {
    con: PseudoCon,
    readable: FileDescriptor,
    writable: FileDescriptor,
    size: PtySize,
}

impl Inner {
    pub fn resize(&mut self, num_rows: u16, num_cols: u16, pixel_width: u16, pixel_height: u16) -> Result<()> {
        self.con.resize(COORD {
            X: num_cols as i16,
            Y: num_rows as i16,
        })?;
        self.size = PtySize {
            rows: num_rows,
            cols: num_cols,
            pixel_width,
            pixel_height,
        };
        Ok(())
    }
}

#[derive(Clone)]
pub struct ConPtyMasterPty {
    inner: Arc<Mutex<Inner>>,
}

pub struct ConPtySlavePty {
    inner: Arc<Mutex<Inner>>,
}

impl ConPtyMasterPty {
    // fn get_size(&self) -> Result<PtySize> {
    // let inner = self.inner.lock().unwrap();
    // Ok(inner.size.clone())
    // }
}

struct ConPtyAsyncMasterPty {
    inner: Arc<Mutex<Inner>>,
    write_request_tx: Sender<Vec<u8>>,
    write_result_rx: Receiver<Result<usize, io::Error>>,
    read_result_rx: Receiver<Result<Vec<u8>, io::Error>>,
}

impl ConPtyAsyncMasterPty {
    fn new(inner: Arc<Mutex<Inner>>) -> Result<Self> {
        let (write_request_tx, write_request_rx) = unbounded::<Vec<u8>>();
        let (write_result_tx, write_result_rx) = unbounded::<Result<usize, io::Error>>();
        let (read_result_tx, read_result_rx) = unbounded::<Result<Vec<u8>, io::Error>>();

        {
            // spawn threads, initialize incoming receiver and transmitter channels.
            let inner_lock = inner.lock().unwrap();
            let mut writable = inner_lock.writable.try_clone()?;
            let mut readable = inner_lock.readable.try_clone()?;

            tokio::task::spawn_blocking(move || {
                while let Ok(bytes) = write_request_rx.recv() {
                    if let Err(e) = write_result_tx.send(writable.write(&bytes)) {
                        error!("Error writing {e}");
                        break;
                    }
                }
            });

            tokio::task::spawn_blocking(move || {
                let mut read_buffer = [0u8; 4096];
                loop {
                    let result = readable.read(&mut read_buffer).map(|size| read_buffer[..size].to_vec());
                    if let Err(e) = read_result_tx.send(result) {
                        error!("Error writing {e}");
                        break;
                    }
                }
            });
        }

        Ok(ConPtyAsyncMasterPty {
            inner,
            read_result_rx,
            write_result_rx,
            write_request_tx,
        })
    }
}

#[async_trait]
impl AsyncMasterPty for ConPtyAsyncMasterPty {
    async fn read(&mut self, buff: &mut [u8]) -> io::Result<usize> {
        match self.read_result_rx.recv_async().await {
            Ok(Ok(res)) => {
                buff[..res.len()].clone_from_slice(&res);
                io::Result::Ok(res.len())
            },
            Ok(Err(e)) => Err(e),
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e)),
        }
    }

    async fn write(&mut self, buff: &[u8]) -> io::Result<usize> {
        match self.write_request_tx.send_async(buff.to_vec()).await {
            Ok(()) => match self.write_result_rx.recv_async().await {
                Ok(res) => res,
                Err(e) => Err(io::Error::new(io::ErrorKind::Other, e)),
            },
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e)),
        }
    }

    fn resize(&self, size: PtySize) -> anyhow::Result<()> {
        let mut inner = self.inner.lock().unwrap();
        inner.resize(size.rows, size.cols, size.pixel_width, size.pixel_height)
    }
}

impl MasterPty for ConPtyMasterPty {
    fn get_async_master_pty(self: Box<Self>) -> Result<Box<dyn AsyncMasterPty + Send + Sync>> {
        Ok(Box::new(ConPtyAsyncMasterPty::new(self.inner)?))
    }
}

impl SlavePty for ConPtySlavePty {
    fn spawn_command(&self, builder: CommandBuilder) -> anyhow::Result<Box<dyn Child + Send + Sync>> {
        let inner = self.inner.lock().unwrap();
        let child = inner.con.spawn_command(builder)?;
        Ok(Box::new(child))
    }

    fn get_name(&self) -> Option<String> {
        None
    }
}
