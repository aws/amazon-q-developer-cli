use std::ffi::OsStr;
use std::mem::MaybeUninit;
use std::os::unix::prelude::OsStrExt;
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
        let pid = self.0;
        let mut info = MaybeUninit::<nix::libc::proc_bsdinfo>::zeroed();
        let ret = unsafe {
            nix::libc::proc_pidinfo(
                pid,
                nix::libc::PROC_PIDTBSDINFO,
                0,
                info.as_mut_ptr().cast(),
                std::mem::size_of::<nix::libc::proc_bsdinfo>() as _,
            )
        };
        if ret as usize != std::mem::size_of::<nix::libc::proc_bsdinfo>() {
            return None;
        }
        let info = unsafe { info.assume_init() };
        match info.pbi_ppid {
            0 => None,
            ppid => Some(Pid(ppid.try_into().ok()?)),
        }
    }

    fn exe(&self) -> Option<PathBuf> {
        let mut buffer = [0u8; 4096];
        let pid = self.0;
        let buffer_ptr = buffer.as_mut_ptr().cast::<std::ffi::c_void>();
        let buffer_size = buffer.len() as u32;
        let ret = unsafe { nix::libc::proc_pidpath(pid, buffer_ptr, buffer_size) };
        match ret {
            0 => None,
            len => Some(PathBuf::from(OsStr::from_bytes(&buffer[..len as usize]))),
        }
    }
}
