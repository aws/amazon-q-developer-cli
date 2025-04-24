use std::io::{
    self,
    ErrorKind,
};

use anyhow::Result;
use async_trait::async_trait;
use portable_pty::{
    Child,
    PtySize,
};
pub mod cmdbuilder;
pub use cmdbuilder::CommandBuilder;

#[cfg(unix)]
pub mod unix;

#[cfg(windows)]
pub mod win;

#[async_trait]
pub trait AsyncMasterPty {
    async fn read(&mut self, buff: &mut [u8]) -> io::Result<usize>;
    async fn write(&mut self, buff: &[u8]) -> io::Result<usize>;
    fn resize(&self, size: PtySize) -> Result<()>;
}

#[async_trait]
pub trait AsyncMasterPtyExt: AsyncMasterPty {
    async fn write_all(&mut self, mut buff: &[u8]) -> io::Result<()> {
        while !buff.is_empty() {
            match self.write(buff).await {
                Ok(0) => {
                    return Err(std::io::Error::new(
                        ErrorKind::WriteZero,
                        "failed to write whole buffer",
                    ));
                },
                Ok(n) => buff = &buff[n..],
                Err(ref e) if e.kind() == ErrorKind::Interrupted => {},
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

impl<T: AsyncMasterPty + ?Sized> AsyncMasterPtyExt for T {}

pub trait MasterPty {
    fn get_async_master_pty(self: Box<Self>) -> Result<Box<dyn AsyncMasterPty + Send + Sync>>;
}

pub trait SlavePty {
    fn spawn_command(&self, builder: CommandBuilder) -> Result<Box<dyn Child + Send + Sync>>;
    fn get_name(&self) -> Option<String>;
}

pub struct PtyPair {
    // slave is listed first so that it is dropped first.
    // The drop order is stable and specified by rust rfc 1857
    pub slave: Box<dyn SlavePty + Send>,
    pub master: Box<dyn MasterPty + Send>,
}
