//! Daemon lifecycle via PID files.
//!
//! `start` spawns the bot as a detached child and writes a PID file.
//! `stop` reads the PID file and sends SIGTERM.
//! `status` scans all installed instances and checks liveness.

use std::path::PathBuf;

use anyhow::{
    Context,
    Result,
};
use kiro_bot::config;

fn pid_path(name: &str) -> Result<PathBuf> {
    let dir = config::state_dir(name)?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("bot.pid"))
}

fn log_path(name: &str) -> Result<PathBuf> {
    let dir = config::state_dir(name)?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("bot.log"))
}

fn read_pid(name: &str) -> Option<u32> {
    std::fs::read_to_string(pid_path(name).ok()?).ok()?.trim().parse().ok()
}

fn is_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Spawn the bot as a detached background process.
pub fn cmd_start(name: &str) -> Result<()> {
    if let Some(pid) = read_pid(name)
        && is_alive(pid)
    {
        println!("Already running (PID {pid})");
        return Ok(());
    }

    let exe = std::env::current_exe().context("cannot determine executable path")?;
    let log = log_path(name)?;
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .with_context(|| format!("cannot open log: {}", log.display()))?;

    let child = std::process::Command::new(exe)
        .args(["start", "--foreground", name])
        .env("RUST_LOG", std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()))
        .stdin(std::process::Stdio::null())
        .stdout(log_file.try_clone()?)
        .stderr(log_file)
        .spawn()
        .context("failed to spawn daemon")?;

    let pid = child.id();
    std::fs::write(pid_path(name)?, pid.to_string())?;
    println!("Started '{name}' (PID {pid})");
    println!("  Log: {}", log.display());
    Ok(())
}

/// Stop a bot instance by PID.
pub fn cmd_stop(name: &str) -> Result<()> {
    let Some(pid) = read_pid(name) else {
        println!("'{name}' is not running");
        return Ok(());
    };

    if !is_alive(pid) {
        let _ = std::fs::remove_file(pid_path(name)?);
        println!("'{name}' was not running (stale PID {pid})");
        return Ok(());
    }

    std::process::Command::new("kill")
        .arg(pid.to_string())
        .status()
        .context("failed to send SIGTERM")?;

    for _ in 0..50 {
        if !is_alive(pid) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    if is_alive(pid) {
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .status();
    }

    let _ = std::fs::remove_file(pid_path(name)?);
    println!("Stopped '{name}' (PID {pid})");
    Ok(())
}

/// List all installed instances and their running status.
pub fn cmd_status() -> Result<()> {
    let base = config::base_dir()?;
    if !base.exists() {
        println!("No instances installed.");
        return Ok(());
    }

    let mut found = false;
    println!("{:<20} {:<10} {:<8} AGENT", "NAME", "STATUS", "PID");
    for entry in std::fs::read_dir(&base)? {
        let entry = entry?;
        if !entry.path().join("config.toml").exists() {
            continue;
        }
        found = true;

        let (name, agent) = match config::load_config(&entry.path()) {
            Ok(c) => (c.name, c.agent.default_mode.unwrap_or_else(|| "—".into())),
            Err(_) => (entry.file_name().to_string_lossy().into_owned(), "?".into()),
        };

        let (status, pid_str) = match read_pid(&name) {
            Some(pid) if is_alive(pid) => ("running", pid.to_string()),
            Some(pid) => ("dead", format!("{pid}?")),
            None => ("stopped", "—".into()),
        };

        println!("{name:<20} {status:<10} {pid_str:<8} {agent}");
    }

    if !found {
        println!("No instances installed.");
    }
    Ok(())
}

/// Rich monitoring view — capacity, workers, uptime, recent activity.
pub fn cmd_monitor() -> Result<()> {
    let base = config::base_dir()?;
    if !base.exists() {
        println!("No instances installed.");
        return Ok(());
    }

    let mut instances: Vec<InstanceInfo> = Vec::new();
    let mut total_workers = 0u32;
    let mut total_max = 0u32;
    let mut running = 0u32;
    let mut stopped = 0u32;

    for entry in std::fs::read_dir(&base)? {
        let entry = entry?;
        if !entry.path().join("config.toml").exists() {
            continue;
        }
        let info = collect_instance_info(&entry.path());
        if info.is_running {
            running += 1;
        } else {
            stopped += 1;
        }
        total_workers += info.active_workers;
        total_max += info.max_workers;
        instances.push(info);
    }

    // Header
    println!("┌─────────────────────────────────────────────────────────┐");
    println!(
        "│  🤖 kiro-bot fleet   {} running  {} stopped  {:>3}/{} workers │",
        running, stopped, total_workers, total_max
    );
    println!("├─────────────────────────────────────────────────────────┤");

    if instances.is_empty() {
        println!("│  No instances installed.                                │");
        println!("└─────────────────────────────────────────────────────────┘");
        return Ok(());
    }

    for inst in &instances {
        let status = if inst.is_running { "●" } else { "○" };
        let uptime = inst.uptime_secs.map(format_uptime).unwrap_or_else(|| "—".into());
        println!(
            "│  {} {:<18} {:<8} {:>5} PID {:<7} {:>2}/{:<2} workers │",
            status,
            inst.name,
            inst.frontend_type,
            uptime,
            inst.pid.map(|p| p.to_string()).unwrap_or_else(|| "—".into()),
            inst.active_workers,
            inst.max_workers,
        );
        if !inst.last_activity.is_empty() {
            println!("│    └─ {}│", pad_right(&inst.last_activity, 53));
        }
    }

    println!("└─────────────────────────────────────────────────────────┘");
    Ok(())
}

struct InstanceInfo {
    name: String,
    frontend_type: String,
    is_running: bool,
    pid: Option<u32>,
    uptime_secs: Option<u64>,
    active_workers: u32,
    max_workers: u32,
    last_activity: String,
}

fn collect_instance_info(instance_dir: &std::path::Path) -> InstanceInfo {
    let name = instance_dir
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    let (frontend_type, max_workers) = match config::load_config(instance_dir) {
        Ok(cfg) => {
            let ft = match &cfg.frontend {
                config::FrontendConfig::Slack { .. } => "slack",
                config::FrontendConfig::Cron { every, .. } => {
                    if every.is_some() {
                        "cron"
                    } else {
                        "oneshot"
                    }
                },
            };
            (ft.to_string(), cfg.agent.max_workers as u32)
        },
        Err(_) => ("?".into(), 0),
    };

    let pid = read_pid(&name);
    let is_running = pid.map(is_alive).unwrap_or(false);

    let uptime_secs = if is_running {
        pid.and_then(get_process_uptime)
    } else {
        None
    };

    let (active_workers, last_activity) = if is_running {
        parse_log_for_metrics(&name)
    } else {
        (0, String::new())
    };

    InstanceInfo {
        name,
        frontend_type,
        is_running,
        pid,
        uptime_secs,
        active_workers,
        max_workers,
        last_activity,
    }
}

fn get_process_uptime(pid: u32) -> Option<u64> {
    let output = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "etimes="])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

fn parse_log_for_metrics(name: &str) -> (u32, String) {
    let log_path = config::state_dir(name)
        .ok()
        .map(|d| d.join("bot.log"))
        .unwrap_or_default();
    let content = std::fs::read_to_string(&log_path).unwrap_or_default();

    // Find last "Posted to Slack" or "Workers:" line
    let mut workers = 0u32;
    let mut last = String::new();
    for line in content.lines().rev().take(50) {
        let clean = strip_ansi(line);
        if workers == 0
            && let Some(pos) = clean.find("Workers:")
            && let Some(n) = clean[pos..]
                .split('/')
                .next()
                .and_then(|s| s.trim_start_matches("Workers:").trim().parse::<u32>().ok())
        {
            workers = n;
        }
        if last.is_empty()
            && (clean.contains("Posted to Slack")
                || clean.contains("Bot started")
                || clean.contains("Reached stop_at")
                || clean.contains("Message from"))
        {
            // Extract timestamp
            if let Some(ts) = clean.split_whitespace().next() {
                last = ts.to_string();
            }
        }
        if workers > 0 && !last.is_empty() {
            break;
        }
    }
    (
        workers,
        if last.is_empty() {
            String::new()
        } else {
            format!("last: {last}")
        },
    )
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_escape = false;
    for c in s.chars() {
        if c == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if c.is_ascii_alphabetic() {
                in_escape = false;
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn format_uptime(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d{}h", secs / 86400, (secs % 86400) / 3600)
    }
}

fn pad_right(s: &str, width: usize) -> String {
    if s.len() >= width {
        s[..width].to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - s.len()))
    }
}
