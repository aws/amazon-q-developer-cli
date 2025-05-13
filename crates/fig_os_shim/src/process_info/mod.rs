mod pid;

use std::path::PathBuf;
use std::sync::{
    Arc,
    Weak,
};

pub use pid::{
    FakePid,
    Pid,
    RawPid,
};

use crate::{
    Context,
    Shim,
};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

/// Represents the interface to accessing info about the currently running process tree.
#[derive(Debug, Clone)]
pub struct ProcessInfo(inner::Inner);

mod inner {
    use super::*;

    #[derive(Debug, Clone)]
    pub(super) enum Inner {
        Real(Weak<Context>),
        Fake(Pid),
    }
}

impl ProcessInfo {
    /// Creates a new [ProcessInfo]. This takes a [Weak] pointer since accessing process info on
    /// platforms like Linux requires file system access, and allows this instance to be embedded
    /// within a single [Context] instance.
    pub fn new(ctx: Weak<Context>) -> Self {
        Self(inner::Inner::Real(ctx.clone()))
    }

    /// Creates a new fake implementation by using the supplied [FakePid] as the process id of the
    /// currently running process.
    pub fn new_fake(pid: FakePid) -> Self {
        Self(inner::Inner::Fake(Pid::Fake(pid)))
    }

    /// Creates a new fake implementation by creating a process hierarchy according to `exes`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use fig_os_shim::ProcessInfo;
    ///
    /// // Creates a slice with the following process hierarchy:
    /// // q <- bash <- wezterm
    /// let process_info = ProcessInfo::from_exes(vec!["q", "bash", "wezterm"]);
    /// ```
    pub fn from_exes<T: Into<TestExe>>(exes: Vec<T>) -> Self {
        let exes = exes.into_iter().map(|v| v.into()).collect::<Vec<TestExe>>();
        let test_exe = exes.first().expect("exes cannot be empty").clone();
        Self::new_fake(FakePid {
            parent: Some(parents(exes.into_iter().skip(1).collect())),
            exe: test_exe.exe,
            cmdline: test_exe.cmdline,
            ..Default::default()
        })
    }

    /// Returns a [Pid] representing the currently running process.
    pub fn current_pid(&self) -> Pid {
        use inner::Inner;
        match &self.0 {
            Inner::Real(ctx) => Pid::current(ctx.clone()),
            Inner::Fake(fake) => fake.clone(),
        }
    }
}

impl Shim for ProcessInfo {
    fn is_real(&self) -> bool {
        matches!(self.0, inner::Inner::Real(_))
    }
}

/// Test helper type for creating a fake [ProcessInfo] through [ProcessInfo::from_exes].
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct TestExe {
    exe: Option<PathBuf>,
    cmdline: Option<String>,
}

impl From<&str> for TestExe {
    fn from(value: &str) -> Self {
        Self {
            exe: Some(value.into()),
            ..Default::default()
        }
    }
}

impl From<(Option<&str>, Option<&str>)> for TestExe {
    fn from(value: (Option<&str>, Option<&str>)) -> Self {
        Self {
            exe: value.0.map(|s| s.into()),
            cmdline: value.1.map(|v| v.to_string()),
        }
    }
}

/// Returns the [MyPid::exe] of the current process's parent, with extra logic to account for
/// `toolbox-exec`.
pub fn get_parent_process_exe(ctx: &Arc<Context>) -> Option<PathBuf> {
    let mut pid = Pid::current(Arc::downgrade(ctx));
    loop {
        pid = *pid.parent()?;
        match pid.exe() {
            // We ignore toolbox-exec since we never want to know if that is the parent process
            Some(pid) if pid.file_name().is_some_and(|s| s == "toolbox-exec") => {},
            other => return other,
        }
    }
}

fn parents(mut exes: Vec<TestExe>) -> Box<Pid> {
    exes.reverse();
    let mut prev = fake_pid(exes.first().unwrap().clone(), None);
    for exe in exes.into_iter().skip(1) {
        let curr = fake_pid(exe, Some(prev));
        prev = curr;
    }
    prev
}

fn fake_pid(exe: TestExe, parent: Option<Box<Pid>>) -> Box<Pid> {
    Box::new(Pid::new_fake(FakePid {
        parent,
        exe: exe.exe,
        cmdline: exe.cmdline,
        ..Default::default()
    }))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_get_parent_process_exe() {
        let ctx = Context::new();
        get_parent_process_exe(&ctx);
    }

    #[test]
    fn test_from_exe_slice() {
        let info = ProcessInfo::from_exes(vec!["q", "bash", "wezterm"]);
        let current = info.current_pid();
        assert_eq!(current.exe().unwrap(), PathBuf::from_str("q").unwrap());
        let parent = current.parent().unwrap();
        assert_eq!(parent.exe().unwrap(), PathBuf::from_str("bash").unwrap());
        let grandparent = parent.parent().unwrap();
        assert_eq!(grandparent.exe().unwrap(), PathBuf::from_str("wezterm").unwrap());
        assert!(grandparent.parent().is_none());
    }

    #[test]
    fn test_from_exe_slice_with_cmdline() {
        let info = ProcessInfo::from_exes(vec![
            (Some("q"), None),
            (Some("bash"), None),
            (Some("python"), Some("terminator")),
        ]);
        let current = info.current_pid();
        assert_eq!(current.exe().unwrap(), PathBuf::from_str("q").unwrap());
        assert_eq!(current.cmdline(), None);
        let parent = current.parent().unwrap();
        assert_eq!(parent.exe().unwrap(), PathBuf::from_str("bash").unwrap());
        assert_eq!(parent.cmdline(), None);
        let grandparent = parent.parent().unwrap();
        assert_eq!(grandparent.exe().unwrap(), PathBuf::from_str("python").unwrap());
        assert_eq!(grandparent.cmdline().unwrap(), "terminator");
        assert!(grandparent.parent().is_none());
    }
}
