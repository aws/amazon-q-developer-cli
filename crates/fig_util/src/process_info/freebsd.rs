use std::path::PathBuf;

use super::{
    Pid,
    PidExt,
};

impl PidExt for Pid {
    fn current() -> Self {
        nix::unistd::getpid().into()
    }

    fn parent(&self) -> Option<Pid> {
        None
    }

    fn exe(&self) -> Option<PathBuf> {
        None
    }
}
