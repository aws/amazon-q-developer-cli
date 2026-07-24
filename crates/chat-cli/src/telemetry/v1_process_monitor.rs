use std::time::{
    Duration,
    Instant,
};

use cpu_time::ProcessTime;
use sysinfo::{
    Pid,
    ProcessRefreshKind,
    ProcessesToUpdate,
    System,
};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use super::TelemetryThread;

const SAMPLE_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, PartialEq)]
struct RawProcessSample {
    wall_time: Duration,
    cpu_time: Duration,
    rss_bytes: u64,
    peak_rss_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ProcessMetrics {
    rss_bytes: f64,
    peak_rss_bytes: f64,
    cpu_utilization: f64,
}

trait ProcessSampler: Send + 'static {
    fn sample(&mut self) -> Option<RawProcessSample>;
}

struct SystemProcessSampler {
    system: System,
    pid: Pid,
    started_at: Instant,
}

impl SystemProcessSampler {
    fn new() -> Self {
        Self {
            system: System::new(),
            pid: Pid::from_u32(std::process::id()),
            started_at: Instant::now(),
        }
    }
}

impl ProcessSampler for SystemProcessSampler {
    fn sample(&mut self) -> Option<RawProcessSample> {
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[self.pid]),
            true,
            ProcessRefreshKind::nothing().with_memory(),
        );
        let process = self.system.process(self.pid)?;
        let rss_bytes = process.memory();
        Some(RawProcessSample {
            wall_time: self.started_at.elapsed(),
            cpu_time: ProcessTime::now().as_duration(),
            rss_bytes,
            peak_rss_bytes: peak_rss_bytes().unwrap_or(rss_bytes),
        })
    }
}

#[cfg(unix)]
fn peak_rss_bytes() -> Option<u64> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    // getrusage initializes the complete rusage structure on success.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
        return None;
    }
    let usage = unsafe { usage.assume_init() };
    let peak = u64::try_from(usage.ru_maxrss).ok()?;
    #[cfg(target_os = "macos")]
    return Some(peak);
    #[cfg(not(target_os = "macos"))]
    return peak.checked_mul(1024);
}

#[cfg(windows)]
fn peak_rss_bytes() -> Option<u64> {
    use windows::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo,
        PROCESS_MEMORY_COUNTERS,
    };
    use windows::Win32::System::Threading::GetCurrentProcess;

    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
        .ok()?;
    }
    Some(counters.PeakWorkingSetSize as u64)
}

#[cfg(not(any(unix, windows)))]
fn peak_rss_bytes() -> Option<u64> {
    None
}

#[derive(Default)]
struct ProcessMonitorState {
    previous: Option<RawProcessSample>,
    peak_rss_bytes: u64,
}

impl ProcessMonitorState {
    fn observe(&mut self, sample: RawProcessSample) -> ProcessMetrics {
        self.peak_rss_bytes = self.peak_rss_bytes.max(sample.peak_rss_bytes);
        let cpu_utilization = self.previous.map_or(0.0, |previous| {
            let wall_delta = sample.wall_time.saturating_sub(previous.wall_time).as_secs_f64();
            let cpu_delta = sample.cpu_time.saturating_sub(previous.cpu_time).as_secs_f64();
            if wall_delta > 0.0 { cpu_delta / wall_delta } else { 0.0 }
        });
        self.previous = Some(sample);

        ProcessMetrics {
            rss_bytes: sample.rss_bytes as f64,
            peak_rss_bytes: self.peak_rss_bytes as f64,
            cpu_utilization,
        }
    }
}

pub(crate) struct V1ProcessMonitor {
    stop_tx: Option<oneshot::Sender<()>>,
    handle: Option<JoinHandle<()>>,
}

impl V1ProcessMonitor {
    pub(crate) fn start(telemetry: TelemetryThread) -> Option<Self> {
        let emitter_telemetry = telemetry.clone();
        Self::start_with_sampler(
            telemetry.is_enabled(),
            SystemProcessSampler::new(),
            SAMPLE_INTERVAL,
            move |metrics| {
                let _ = emitter_telemetry.send_process_health(
                    metrics.rss_bytes,
                    metrics.peak_rss_bytes,
                    metrics.cpu_utilization,
                );
            },
        )
    }

    fn start_with_sampler<S, F>(enabled: bool, mut sampler: S, interval: Duration, mut emit: F) -> Option<Self>
    where
        S: ProcessSampler,
        F: FnMut(ProcessMetrics) + Send + 'static,
    {
        if !enabled {
            return None;
        }

        let (stop_tx, mut stop_rx) = oneshot::channel();
        let handle = tokio::spawn(async move {
            let mut state = ProcessMonitorState::default();
            if let Some(sample) = sampler.sample() {
                state.observe(sample);
            }

            let mut ticker = tokio::time::interval_at(tokio::time::Instant::now() + interval, interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        if let Some(sample) = sampler.sample() {
                            emit(state.observe(sample));
                        }
                    },
                    _ = &mut stop_rx => {
                        if let Some(sample) = sampler.sample() {
                            emit(state.observe(sample));
                        }
                        break;
                    },
                }
            }
        });

        Some(Self {
            stop_tx: Some(stop_tx),
            handle: Some(handle),
        })
    }

    pub(crate) async fn stop(&mut self) {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{
        Arc,
        Mutex,
    };

    use super::*;

    struct FakeSampler {
        samples: Arc<Mutex<VecDeque<RawProcessSample>>>,
        calls: Arc<Mutex<usize>>,
    }

    impl ProcessSampler for FakeSampler {
        fn sample(&mut self) -> Option<RawProcessSample> {
            *self.calls.lock().unwrap() += 1;
            self.samples.lock().unwrap().pop_front()
        }
    }

    fn sample(wall_seconds: u64, cpu_seconds: u64, rss_bytes: u64) -> RawProcessSample {
        RawProcessSample {
            wall_time: Duration::from_secs(wall_seconds),
            cpu_time: Duration::from_secs(cpu_seconds),
            rss_bytes,
            peak_rss_bytes: rss_bytes,
        }
    }

    #[test]
    fn computes_cpu_time_to_wall_time_ratio_and_tracks_peak_rss() {
        let mut state = ProcessMonitorState::default();
        state.observe(sample(0, 0, 100));

        let first = state.observe(sample(60, 30, 200));
        assert_eq!(first.rss_bytes, 200.0);
        assert_eq!(first.peak_rss_bytes, 200.0);
        assert_eq!(first.cpu_utilization, 0.5);

        let second = state.observe(RawProcessSample {
            peak_rss_bytes: 400,
            ..sample(120, 150, 150)
        });
        assert_eq!(second.rss_bytes, 150.0);
        assert_eq!(second.peak_rss_bytes, 400.0);
        assert_eq!(second.cpu_utilization, 2.0);
    }

    #[tokio::test(start_paused = true)]
    async fn emits_every_sixty_seconds_and_once_at_shutdown() {
        assert_eq!(SAMPLE_INTERVAL, Duration::from_secs(60));
        let samples = Arc::new(Mutex::new(VecDeque::from([
            sample(0, 0, 100),
            sample(60, 30, 200),
            sample(75, 45, 150),
        ])));
        let calls = Arc::new(Mutex::new(0));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let sampler = FakeSampler {
            samples,
            calls: calls.clone(),
        };
        let mut monitor = V1ProcessMonitor::start_with_sampler(true, sampler, SAMPLE_INTERVAL, move |metrics| {
            tx.send(metrics).unwrap();
        })
        .unwrap();
        tokio::task::yield_now().await;
        assert!(rx.try_recv().is_err());

        tokio::time::advance(SAMPLE_INTERVAL).await;
        let periodic = rx.recv().await.unwrap();
        assert_eq!(periodic.cpu_utilization, 0.5);

        monitor.stop().await;
        let final_sample = rx.recv().await.unwrap();
        assert_eq!(final_sample.rss_bytes, 150.0);
        assert_eq!(final_sample.peak_rss_bytes, 200.0);
        assert_eq!(final_sample.cpu_utilization, 1.0);

        monitor.stop().await;
        assert_eq!(*calls.lock().unwrap(), 3);
    }

    #[tokio::test]
    async fn stays_stopped_when_telemetry_is_disabled() {
        let calls = Arc::new(Mutex::new(0));
        let sampler = FakeSampler {
            samples: Arc::new(Mutex::new(VecDeque::new())),
            calls: calls.clone(),
        };

        let monitor = V1ProcessMonitor::start_with_sampler(false, sampler, SAMPLE_INTERVAL, |_| {});

        assert!(monitor.is_none());
        assert_eq!(*calls.lock().unwrap(), 0);
    }
}
