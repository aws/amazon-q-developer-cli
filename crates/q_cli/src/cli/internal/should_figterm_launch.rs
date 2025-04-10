use std::borrow::Cow;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::io::{
    Write,
    stdout,
};
use std::process::ExitCode;

use fig_os_shim::Context;
use fig_util::Terminal;

const Q_FORCE_FIGTERM_LAUNCH: &str = "Q_FORCE_FIGTERM_LAUNCH";
const Q_TERM_DISABLED: &str = "Q_TERM_DISABLED";
const INSIDE_EMACS: &str = "INSIDE_EMACS";
const TERM_PROGRAM: &str = "TERM_PROGRAM";
const WARP_TERMINAL: &str = "WarpTerminal";

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[allow(dead_code)]
struct ProcessInfo {
    pid: Box<fig_os_shim::process_info::Pid>,
    exe_name: String,
    cmdline_name: Option<String>,
    is_valid: bool,
    is_special: bool,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
enum Status {
    Launch(Cow<'static, str>),
    DontLaunch(Cow<'static, str>),
    Process(ProcessInfo),
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl Status {
    fn exit_status(self, quiet: bool) -> Result<ProcessInfo, u8> {
        match self {
            Status::Process(info) => Ok(info),
            Status::Launch(s) => {
                if !quiet {
                    writeln!(stdout(), "✅ {s}").ok();
                }
                Err(0)
            },
            Status::DontLaunch(s) => {
                if !quiet {
                    writeln!(stdout(), "❌ {s}").ok();
                }
                Err(1)
            },
        }
    }
}

fn parent_status(ctx: &Context, current_pid: fig_os_shim::process_info::Pid) -> Status {
    use fig_util::env_var::Q_TERM;
    let env = ctx.env();

    let parent_pid = match current_pid.parent() {
        Some(pid) => pid,
        None => return Status::DontLaunch("No parent PID".into()),
    };

    let parent_path = match parent_pid.exe() {
        Some(path) => path,
        None => return Status::DontLaunch("No parent path".into()),
    };

    let parent_name = match parent_path.file_name() {
        Some(name) => match name.to_str() {
            Some(name) => name,
            None => return Status::DontLaunch("Parent name is not valid unicode".into()),
        },
        None => return Status::DontLaunch("No parent name".into()),
    };

    let valid_parent = ["zsh", "bash", "fish", "nu"].contains(&parent_name);

    if env.in_ssh() && env.get_os(Q_TERM).is_none() {
        return Status::Launch(format!("In SSH and {Q_TERM} is not set").into());
    }

    if env.in_codespaces() {
        return match env.get_os(Q_TERM) {
            Some(_) => Status::DontLaunch(format!("In Codespaces and {Q_TERM} is set").into()),
            None => Status::Launch(format!("In Codespaces and {Q_TERM} is not set").into()),
        };
    }

    Status::Process(ProcessInfo {
        pid: parent_pid,
        exe_name: parent_name.into(),
        cmdline_name: None,
        is_valid: valid_parent,
        is_special: false,
    })
}

fn grandparent_status(ctx: &Context, parent_pid: fig_os_shim::process_info::Pid) -> Status {
    let current_os = ctx.platform().os();

    let Some(grandparent_pid) = parent_pid.parent() else {
        return Status::DontLaunch("No grandparent PID".into());
    };

    let Some(grandparent_path) = grandparent_pid.exe() else {
        return Status::DontLaunch("No grandparent path".into());
    };

    let grandparent_name = match grandparent_path.file_name() {
        Some(name) => match name.to_str() {
            Some(name) => name,
            None => return Status::DontLaunch("Grandparent name is not a valid utf8 str".into()),
        },
        None => return Status::DontLaunch("No grandparent name".into()),
    };

    let grandparent_cmdline_name = if let Some(cmdline) = grandparent_pid.cmdline() {
        cmdline
            .split(' ')
            .take(1)
            .next()
            .and_then(|cmd| cmd.split('/').last())
            .map(str::to_string)
    } else {
        None
    };

    // The terminals the platform supports
    let terminals = match current_os {
        fig_os_shim::Os::Mac => fig_util::terminal::MACOS_TERMINALS,
        fig_os_shim::Os::Linux => fig_util::terminal::LINUX_TERMINALS,
        _ => panic!("unsupported os"),
    }
    .iter()
    .chain(fig_util::terminal::SPECIAL_TERMINALS)
    .cloned()
    .collect::<Vec<_>>();

    // Try to find if any of the supported terminals matches the current grandparent process
    let valid_grandparent = match current_os {
        fig_os_shim::Os::Mac => terminals
            .iter()
            .find(|terminal| terminal.executable_names().contains(&grandparent_name)),
        fig_os_shim::Os::Linux => terminals.iter().find(|terminal| {
            terminal.executable_names().contains(&grandparent_name)
                || grandparent_pid
                    .cmdline()
                    .is_some_and(|cmdline| Terminal::try_from_cmdline(&cmdline, &terminals).is_some())
        }),
        _ => panic!("unsupported os"),
    };

    Status::Process(ProcessInfo {
        pid: grandparent_pid,
        exe_name: grandparent_name.into(),
        cmdline_name: grandparent_cmdline_name,
        is_valid: valid_grandparent.is_some(),
        is_special: valid_grandparent.is_some_and(|term| term.is_special()),
    })
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn should_launch(ctx: &Context, quiet: bool) -> u8 {
    use fig_os_shim::Os;

    let process_info = ctx.process_info();
    let current_pid = process_info.current_pid();

    let parent_info = match parent_status(ctx, current_pid).exit_status(quiet) {
        Ok(info) => info,
        Err(i) => return i,
    };
    let grandparent_info = match grandparent_status(ctx, *parent_info.pid.clone()).exit_status(quiet) {
        Ok(info) => info,
        Err(i) => return i,
    };

    if !quiet {
        let ancestry = format!(
            "{} {} | {} ({}) <- {} {} ({})",
            if grandparent_info.is_valid { "✅" } else { "❌" },
            grandparent_info.exe_name,
            grandparent_info.cmdline_name.unwrap_or_default(),
            grandparent_info.pid.clone(),
            if parent_info.is_valid { "✅" } else { "❌" },
            parent_info.exe_name,
            parent_info.pid.clone(),
        );

        writeln!(stdout(), "{ancestry}").ok();
    }

    if matches!(ctx.platform().os(), Os::Mac) && !grandparent_info.is_special {
        if !quiet {
            writeln!(stdout(), "🟡 Falling back to old mechanism since on macOS").ok();
        }
        return 2;
    }

    u8::from(!(grandparent_info.is_valid && parent_info.is_valid))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn should_figterm_launch_exit_status(ctx: &Context, quiet: bool) -> u8 {
    use fig_util::env_var::{
        PROCESS_LAUNCHED_BY_Q,
        Q_PARENT,
    };

    let env = ctx.env();

    if env.get_os(Q_FORCE_FIGTERM_LAUNCH).is_some() {
        if !quiet {
            writeln!(stdout(), "✅ {Q_FORCE_FIGTERM_LAUNCH}").ok();
        }
        return 0;
    }

    if env.get_os(Q_TERM_DISABLED).is_some() {
        if !quiet {
            writeln!(stdout(), "❌ {Q_TERM_DISABLED}").ok();
        }
        return 1;
    }

    if env.get_os(PROCESS_LAUNCHED_BY_Q).is_some() {
        if !quiet {
            writeln!(stdout(), "❌ {PROCESS_LAUNCHED_BY_Q}").ok();
        }
        return 1;
    }

    // Check if inside Emacs
    if env.get_os(INSIDE_EMACS).is_some() {
        if !quiet {
            writeln!(stdout(), "❌ INSIDE_EMACS").ok();
        }
        return 1;
    }

    // Check for Warp Terminal
    if let Ok(term_program) = env.get(TERM_PROGRAM) {
        if term_program == WARP_TERMINAL {
            if !quiet {
                writeln!(stdout(), "❌ {TERM_PROGRAM} = {WARP_TERMINAL}").ok();
            }
            return 1;
        }
    }

    // Check for SecureCRT
    if let Ok(cf_bundle_identifier) = env.get("__CFBundleIdentifier") {
        if cf_bundle_identifier == "com.vandyke.SecureCRT" {
            if !quiet {
                writeln!(stdout(), "❌ __CFBundleIdentifier = com.vandyke.SecureCRT").ok();
            }
            return 1;
        }
    }

    // PWSH var is set when launched by `pwsh -Login`, in which case we don't want to init.
    if env.get_os("__PWSH_LOGIN_CHECKED").is_some() {
        if !quiet {
            writeln!(stdout(), "❌ __PWSH_LOGIN_CHECKED").ok();
        }
        return 1;
    }

    // Make sure we're not in CI
    if env.in_ci() {
        if !quiet {
            writeln!(stdout(), "❌ In CI").ok();
        }
        return 1;
    }

    // If we are in SSH and there is no Q_PARENT dont launch
    if env.in_ssh() && env.get_os(Q_PARENT).is_none() {
        if !quiet {
            writeln!(stdout(), "❌ In SSH without {Q_PARENT}").ok();
        }
        return 1;
    }

    if fig_util::system_info::in_wsl() {
        if !quiet {
            writeln!(stdout(), "🟡 Falling back to old mechanism since in WSL").ok();
        }
        2
    } else {
        should_launch(ctx, quiet)
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn should_figterm_launch(ctx: &Context) -> ExitCode {
    ExitCode::from(should_figterm_launch_exit_status(ctx, false))
}

#[cfg(target_os = "windows")]
pub fn should_qterm_launch() -> ExitCode {
    use std::os::windows::io::AsRawHandle;

    use winapi::um::consoleapi::GetConsoleMode;

    let mut mode = 0;
    let stdin_ok = unsafe { GetConsoleMode(std::io::stdin().as_raw_handle() as *mut _, &mut mode) };
    ExitCode::from(if stdin_ok == 1 { 2 } else { 1 });
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub fn should_qterm_launch() -> ExitCode {
    ExitCode::from(2);
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]

    use std::path::PathBuf;

    use fig_os_shim::process_info::{
        FakePid,
        Pid,
        ProcessInfo,
    };
    use fig_os_shim::{
        ContextBuilder,
        Env,
        Os,
        Platform,
    };
    use fig_util::env_var::{
        PROCESS_LAUNCHED_BY_Q,
        Q_PARENT,
        Q_TERM,
    };

    use super::*;

    macro_rules! assert_exit_code {
        ($ctx:expr, $exit_code:expr, $msg:expr) => {
            assert_eq!(
                should_figterm_launch_exit_status(&$ctx, true),
                $exit_code,
                "{}",
                $msg
            );
            assert_eq!(
                should_figterm_launch_exit_status(&$ctx, false),
                $exit_code,
                "{}",
                $msg
            );
        };
    }

    #[derive(Default, Debug)]
    struct Test {
        name: String,
        os: Option<Os>,
        env: Option<Env>,
        parent: Option<FakePid>,
        grandparent_exe: Option<PathBuf>,
        grandparent_cmdline: Option<String>,
        expected_exit_code: u8,
    }

    impl Test {
        fn os(mut self, os: Os) -> Self {
            self.os = Some(os);
            self
        }

        fn env(mut self, slice: &[(&str, &str)]) -> Self {
            self.env = Some(Env::from_slice(slice));
            self
        }

        fn parent_exe(mut self, exe: impl Into<PathBuf>) -> Self {
            self.parent = Some(FakePid {
                exe: Some(exe.into()),
                ..Default::default()
            });
            self
        }

        fn grandparent_exe(mut self, exe: impl Into<PathBuf>) -> Self {
            self.grandparent_exe = Some(exe.into());
            self
        }

        fn grandparent_cmdline(mut self, cmdline: impl Into<String>) -> Self {
            self.grandparent_cmdline = Some(cmdline.into());
            self
        }

        fn expect(mut self, expected_exit_code: u8) -> Self {
            self.expected_exit_code = expected_exit_code;
            self
        }

        fn run(self) {
            let ctx = {
                let mut ctx = ContextBuilder::new().with_env(self.env.unwrap_or(Env::from_slice(&[])));

                // Create fake ProcessInfo
                let mut current_pid = FakePid::default();
                if let Some(mut parent) = self.parent {
                    if let Some(grandparent_exe) = self.grandparent_exe {
                        let grandparent = FakePid {
                            exe: Some(grandparent_exe),
                            cmdline: self.grandparent_cmdline,
                            ..Default::default()
                        };
                        parent.parent = Some(Box::new(Pid::new_fake(grandparent)));
                    }
                    current_pid.parent = Some(Box::new(Pid::new_fake(parent)));
                }
                ctx = ctx.with_process_info(ProcessInfo::new_fake(current_pid));

                let os = self.os.unwrap_or(Os::Mac);
                ctx = ctx.with_platform(Platform::new_fake(os));

                ctx.build()
            };

            assert_exit_code!(ctx, self.expected_exit_code, self.name);
        }
    }

    fn test(name: impl Into<String>) -> Test {
        Test {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Tests for basic override logic where the state of the parent
    /// and grandparent processes don't matter.
    #[test]
    fn override_tests() {
        let tests = [
            test(Q_FORCE_FIGTERM_LAUNCH)
                .env(&[(Q_FORCE_FIGTERM_LAUNCH, "1")])
                .expect(0),
            test(Q_TERM_DISABLED).env(&[(Q_TERM_DISABLED, "1")]).expect(1),
            test(PROCESS_LAUNCHED_BY_Q)
                .env(&[(PROCESS_LAUNCHED_BY_Q, "1")])
                .expect(1),
            test(INSIDE_EMACS).env(&[(INSIDE_EMACS, "1")]).expect(1),
            test(WARP_TERMINAL).env(&[(TERM_PROGRAM, WARP_TERMINAL)]).expect(1),
            test(WARP_TERMINAL).env(&[(TERM_PROGRAM, WARP_TERMINAL)]).expect(1),
            test("In CI").env(&[("CI", "1")]).expect(1),
            test(format!("In ssh without {Q_PARENT}"))
                .env(&[("SSH_CLIENT", "1")])
                .expect(1),
            test("__PWSH_LOGIN_CHECKED")
                .env(&[("__PWSH_LOGIN_CHECKED", "1")])
                .expect(1),
            test("SecureCRT")
                .env(&[("__CFBundleIdentifier", "com.vandyke.SecureCRT")])
                .expect(1),
        ];
        for test in tests {
            test.run();
        }
    }

    /// Tests for when the parent process is not valid.
    #[test]
    fn invalid_parent_tests() {
        let tests = [
            test("no parent id").expect(1),
            test("invalid parent").parent_exe("/usr/bin/invalid").expect(1),
            test(format!("In Codespaces with {Q_TERM}"))
                .parent_exe("/usr/bin/zsh")
                .env(&[("CODESPACES", "1")])
                .env(&[(Q_TERM, "1")])
                .expect(1),
        ];
        for test in tests {
            test.run();
        }
    }

    /// Tests for when the grandparent process is not valid.
    #[test]
    fn invalid_grandparent_tests() {
        let tests = [
            test("no grandparentparent id").parent_exe("/usr/bin/zsh").expect(1),
            test("on mac with valid parent and invalid grandparent")
                .os(Os::Mac)
                .parent_exe("/usr/bin/zsh")
                .grandparent_exe("/usr/bin/invalid")
                .expect(2),
            test("on linux with valid parent and invalid grandparent")
                .os(Os::Linux)
                .parent_exe("/usr/bin/zsh")
                .grandparent_exe("/usr/bin/invalid")
                .expect(1),
        ];
        for test in tests {
            test.run();
        }
    }

    /// Tests for when the app should launch, ignoring any of the overrides
    /// captured under override_tests below.
    #[test]
    fn should_launch_tests() {
        let tests = [
            test("on linux with valid parent and valid grandparent exe")
                .os(Os::Linux)
                .parent_exe("/usr/bin/zsh")
                .grandparent_exe("/usr/bin/wezterm")
                .expect(0),
            test("on linux with valid parent, invalid grandparent exe, and valid grandparent cmdline")
                .os(Os::Linux)
                .parent_exe("/usr/bin/zsh")
                .grandparent_exe("/usr/bin/ld-2.26.so")
                .grandparent_cmdline("/usr/bin/tmux random args here")
                .expect(0),
            test("on mac with valid parent and special grandparent")
                .os(Os::Mac)
                .parent_exe("/usr/bin/zsh")
                .grandparent_exe("/usr/bin/tmux")
                .expect(0),
            test(format!("In Codespaces without {Q_TERM}"))
                .parent_exe("/usr/bin/zsh")
                .env(&[("CODESPACES", "1")])
                .expect(0),
            test(format!("In ssh and {Q_TERM} is not set"))
                .parent_exe("/usr/bin/zsh")
                .env(&[("SSH_CLIENT", "1"), (Q_PARENT, "1")])
                .expect(0),
        ];
        for test in tests {
            test.run();
        }
    }
}
