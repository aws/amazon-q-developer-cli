use kiro_telemetry::{
    MetricRecord,
    metric,
};
use sysinfo::{
    ProcessRefreshKind,
    ProcessesToUpdate,
    System,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub engine: metric::Engine,
    pub role: metric::ProcessRole,
}

impl ProcessIdentity {
    pub const fn new(engine: metric::Engine, role: metric::ProcessRole) -> Self {
        Self { engine, role }
    }
}

#[derive(Debug)]
pub struct ProcessSampler {
    system: System,
    pid: sysinfo::Pid,
}

impl ProcessSampler {
    pub fn new() -> Option<Self> {
        let pid = sysinfo::get_current_pid().ok()?;
        let mut sampler = Self {
            system: System::new(),
            pid,
        };
        sampler.refresh();
        Some(sampler)
    }

    pub fn sample(&mut self, identity: ProcessIdentity) -> Vec<MetricRecord> {
        self.records(identity, false)
    }

    pub fn final_sample(&mut self, identity: ProcessIdentity) -> Vec<MetricRecord> {
        self.records(identity, true)
    }

    fn records(&mut self, identity: ProcessIdentity, include_peak_rss: bool) -> Vec<MetricRecord> {
        self.refresh();
        let os_type = current_os_type();
        let mut records = Vec::with_capacity(6);

        if let Some(process) = self.system.process(self.pid) {
            records.extend(metric::record_process_memory_rss_bytes(
                process.memory() as f64,
                os_type,
                identity.engine,
                identity.role,
            ));
            records.extend(metric::record_process_cpu_utilization_ratio(
                f64::from(process.cpu_usage()) / 100.0,
                os_type,
                identity.engine,
                identity.role,
            ));
        }

        records.extend(open_file_descriptor_count().and_then(|count| {
            metric::record_process_open_file_descriptor_count(count as f64, os_type, identity.engine, identity.role)
        }));
        records.extend(
            process_handle_count()
                .and_then(|count| metric::record_process_handle_count(count as f64, identity.engine, identity.role)),
        );
        records.extend(process_thread_count().and_then(|count| {
            metric::record_process_thread_count(count as f64, os_type, identity.engine, identity.role)
        }));
        if include_peak_rss {
            records.extend(peak_rss_bytes().and_then(|bytes| {
                metric::record_process_peak_rss_bytes(bytes as f64, os_type, identity.engine, identity.role)
            }));
        }

        records
    }

    fn refresh(&mut self) {
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[self.pid]),
            true,
            ProcessRefreshKind::nothing().with_memory().with_cpu(),
        );
    }
}

fn current_os_type() -> metric::OsType {
    match std::env::consts::OS {
        "linux" => metric::OsType::Linux,
        "macos" => metric::OsType::Macos,
        "windows" => metric::OsType::Windows,
        _ => metric::OsType::Other,
    }
}

#[cfg(target_os = "linux")]
fn open_file_descriptor_count() -> Option<u64> {
    std::fs::read_dir("/proc/self/fd")
        .ok()
        .and_then(|entries| u64::try_from(entries.count()).ok())
}

#[cfg(target_os = "macos")]
fn open_file_descriptor_count() -> Option<u64> {
    let bytes = unsafe {
        libc::proc_pidinfo(
            std::process::id() as i32,
            libc::PROC_PIDLISTFDS,
            0,
            std::ptr::null_mut(),
            0,
        )
    };
    (bytes >= 0).then(|| bytes as u64 / std::mem::size_of::<libc::proc_fdinfo>() as u64)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn open_file_descriptor_count() -> Option<u64> {
    None
}

#[cfg(windows)]
fn process_handle_count() -> Option<u64> {
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess,
        GetProcessHandleCount,
    };

    let mut count = 0;
    let success = unsafe { GetProcessHandleCount(GetCurrentProcess(), &mut count) };
    (success != 0).then_some(u64::from(count))
}

#[cfg(not(windows))]
fn process_handle_count() -> Option<u64> {
    None
}

#[cfg(target_os = "linux")]
fn process_thread_count() -> Option<u64> {
    std::fs::read_dir("/proc/self/task")
        .ok()
        .and_then(|entries| u64::try_from(entries.count()).ok())
}

#[cfg(target_os = "macos")]
fn process_thread_count() -> Option<u64> {
    macos_task_info().and_then(|info| u64::try_from(info.pti_threadnum).ok())
}

#[cfg(windows)]
fn process_thread_count() -> Option<u64> {
    use windows_sys::Win32::Foundation::{
        CloseHandle,
        INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot,
        TH32CS_SNAPTHREAD,
        THREADENTRY32,
        Thread32First,
        Thread32Next,
    };

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return None;
    }

    let pid = std::process::id();
    let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
    let mut count = 0_u64;
    let mut has_entry = unsafe { Thread32First(snapshot, &mut entry) } != 0;
    while has_entry {
        if entry.th32OwnerProcessID == pid {
            count += 1;
        }
        has_entry = unsafe { Thread32Next(snapshot, &mut entry) } != 0;
    }
    unsafe {
        CloseHandle(snapshot);
    }
    Some(count)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn process_thread_count() -> Option<u64> {
    None
}

#[cfg(unix)]
fn peak_rss_bytes() -> Option<u64> {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
        return None;
    }
    let value = u64::try_from(usage.ru_maxrss).ok()?;
    if cfg!(target_os = "macos") {
        Some(value)
    } else {
        value.checked_mul(1024)
    }
}

#[cfg(windows)]
fn peak_rss_bytes() -> Option<u64> {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo,
        PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    let success = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
    (success != 0).then(|| counters.PeakWorkingSetSize as u64)
}

#[cfg(not(any(unix, windows)))]
fn peak_rss_bytes() -> Option<u64> {
    None
}

#[cfg(target_os = "macos")]
fn macos_task_info() -> Option<libc::proc_taskinfo> {
    let mut info = std::mem::MaybeUninit::<libc::proc_taskinfo>::uninit();
    let size = std::mem::size_of::<libc::proc_taskinfo>() as i32;
    let bytes = unsafe {
        libc::proc_pidinfo(
            std::process::id() as i32,
            libc::PROC_PIDTASKINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    (bytes == size).then(|| unsafe { info.assume_init() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_the_current_process_without_zero_filling_platform_specific_metrics() {
        let mut sampler = ProcessSampler::new().expect("current process should be visible");
        let identity = ProcessIdentity::new(metric::Engine::V2, metric::ProcessRole::Host);
        let records = sampler.final_sample(identity);

        assert!(
            records
                .iter()
                .any(|record| record.name == "kiro_cli_process_memory_rss_bytes")
        );
        assert!(
            records
                .iter()
                .any(|record| record.name == "kiro_cli_process_cpu_utilization_ratio")
        );
        assert!(
            records
                .iter()
                .any(|record| record.name == "kiro_cli_process_thread_count")
        );
        assert!(
            records
                .iter()
                .any(|record| record.name == "kiro_cli_process_peak_rss_bytes")
        );
        if cfg!(windows) {
            assert!(
                records
                    .iter()
                    .any(|record| record.name == "kiro_cli_process_handle_count")
            );
            assert!(
                records
                    .iter()
                    .all(|record| record.name != "kiro_cli_process_open_file_descriptor_count")
            );
        } else {
            assert!(
                records
                    .iter()
                    .any(|record| record.name == "kiro_cli_process_open_file_descriptor_count")
            );
            assert!(
                records
                    .iter()
                    .all(|record| record.name != "kiro_cli_process_handle_count")
            );
        }
    }
}
