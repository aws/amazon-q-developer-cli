use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{
    Arc,
    Weak,
};

use super::{
    Pid,
    RawPid,
};
use crate::Context;

pub fn current(ctx: Weak<Context>) -> Pid {
    Pid::Real(ctx.clone(), nix::unistd::getpid().into())
}

pub fn parent(ctx: Weak<Context>, pid: &RawPid) -> Option<Box<Pid>> {
    if let Some(ctx) = ctx.upgrade() {
        let fs = ctx.fs();
        let raw_pid = fs
            .read_to_string_sync(format!("/proc/{pid}/status"))
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|line| line.starts_with("PPid:"))
                    .and_then(|line| line.strip_prefix("PPid:"))
                    .map(|line| line.trim())
                    .and_then(|pid_str| RawPid::from_str(pid_str).ok())
            });
        raw_pid.map(|raw_pid| Box::new(Pid::Real(Arc::downgrade(&ctx), raw_pid)))
    } else {
        None
    }
}

pub fn exe(_: Weak<Context>, pid: &RawPid) -> Option<PathBuf> {
    // TODO: add links to the fake file system
    std::path::PathBuf::from(format!("/proc/{pid}/exe")).read_link().ok()
}

pub fn cmdline(ctx: Weak<Context>, pid: &RawPid) -> Option<String> {
    if let Some(ctx) = ctx.upgrade() {
        let fs = ctx.fs();
        fs.read_to_string_sync(format!("/proc/{pid}/cmdline"))
            .ok()
            .map(|s| s.replace('\0', " "))
    } else {
        None
    }
}
