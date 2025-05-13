use std::path::PathBuf;
use std::sync::Weak;

use sysinfo::{
    ProcessesToUpdate,
    System,
};

use super::pid::{
    Pid,
    RawPid,
};
use crate::Context;

pub fn current(ctx: Weak<Context>) -> Pid {
    use std::process;
    Pid::Real(ctx, RawPid(process::id()))
}

pub fn parent(ctx: Weak<Context>, pid: &RawPid) -> Option<Box<Pid>> {
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);

    let sys_pid = sysinfo::Pid::from_u32(pid.as_u32());
    if let Some(process) = system.process(sys_pid) {
        if let Some(parent_pid) = process.parent() {
            return Some(Box::new(Pid::Real(ctx, RawPid(parent_pid.as_u32()))));
        }
    }
    None
}

pub fn exe(_ctx: Weak<Context>, pid: &RawPid) -> Option<PathBuf> {
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);

    let sys_pid = sysinfo::Pid::from_u32(pid.as_u32());
    if let Some(process) = system.process(sys_pid) {
        return Some(PathBuf::from(process.exe()?.to_string_lossy().to_string()));
    }
    None
}

pub fn cmdline(_ctx: Weak<Context>, pid: &RawPid) -> Option<String> {
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);

    let sys_pid = sysinfo::Pid::from_u32(pid.as_u32());
    if let Some(process) = system.process(sys_pid) {
        let cmd_parts: Vec<String> = process
            .cmd()
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();

        let cmd = cmd_parts.join(" ");

        if !cmd.is_empty() {
            return Some(cmd);
        }
    }
    None
}
