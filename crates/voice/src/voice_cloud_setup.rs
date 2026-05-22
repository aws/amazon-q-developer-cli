use std::process::ExitCode;
use std::time::Duration;

use crossterm::style::{
    Color,
    SetForegroundColor,
};
use crossterm::{
    execute,
    style,
};
use eyre::Result;
use tokio::process::Command;
use tokio::signal;

/// Run an SSH command in batch mode (no tty, no password prompt) and return stdout.
async fn ssh_batch(host: &str, identity: Option<&str>, remote_cmd: &str) -> Result<String> {
    let mut cmd = Command::new("ssh");
    cmd.args([
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=10",
        "-o",
        "StrictHostKeyChecking=accept-new",
    ]);
    if let Some(id) = identity {
        cmd.arg("-i").arg(id);
    }
    cmd.arg(host).arg(remote_cmd);

    let output = cmd.output().await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(eyre::eyre!("{}", stderr.trim()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Run an SSH command interactively (inheriting stdio for password/mfa prompts).
async fn ssh_interactive(host: &str, identity: Option<&str>, remote_cmd: &str) -> Result<()> {
    let mut cmd = Command::new("ssh");
    cmd.args(["-o", "ConnectTimeout=10", "-o", "StrictHostKeyChecking=accept-new"]);
    if let Some(id) = identity {
        cmd.arg("-i").arg(id);
    }
    cmd.arg(host).arg(remote_cmd);
    cmd.stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());

    let status = cmd.status().await?;
    if !status.success() {
        return Err(eyre::eyre!("SSH command failed with exit code {:?}", status.code()));
    }
    Ok(())
}

fn print_step(stderr: &mut std::io::Stderr, step: u8, total: u8, label: &str) {
    execute!(
        stderr,
        SetForegroundColor(Color::DarkGrey),
        style::Print(format!("[{step}/{total}] {label:<40}")),
        SetForegroundColor(Color::Reset),
    )
    .ok();
}

fn print_ok(stderr: &mut std::io::Stderr, detail: &str) {
    execute!(
        stderr,
        SetForegroundColor(Color::Green),
        style::Print("✓"),
        SetForegroundColor(Color::Reset),
    )
    .ok();
    if !detail.is_empty() {
        execute!(stderr, style::Print(format!(" ({detail})"))).ok();
    }
    execute!(stderr, style::Print("\n")).ok();
}

fn print_fail(stderr: &mut std::io::Stderr, msg: &str) {
    execute!(
        stderr,
        SetForegroundColor(Color::Red),
        style::Print("✗ "),
        SetForegroundColor(Color::Reset),
        style::Print(format!("{msg}\n")),
    )
    .ok();
}

/// Set up voice mode for a cloud desktop in one step.
///
/// Architecture:
///   [Local machine (has mic)] runs voice-serve on localhost:PORT
///   [Cloud desktop (no mic)]  kiro TUI connects to localhost:PORT via SSH tunnel
///   SSH reverse tunnel:       cloud's localhost:PORT → local's localhost:PORT
///
/// This command:
/// 1. Verifies SSH connectivity to cloud desktop
/// 2. Configures voice.serverUrl on the cloud desktop
/// 3. Starts voice-serve locally
/// 4. Opens SSH reverse tunnel so cloud desktop can reach local voice-serve
pub async fn run_voice_cloud_setup(
    host: &str,
    port: u16,
    remote_bin: Option<&str>,
    identity: Option<&str>,
) -> Result<ExitCode> {
    if host.starts_with('-') {
        return Err(eyre::eyre!("Invalid hostname: must not start with '-'"));
    }

    let mut stderr = std::io::stderr();
    let total_steps: u8 = 4;

    // ── Phase 1: SSH Auth Probe ──────────────────────────────────────────────
    print_step(&mut stderr, 1, total_steps, "Checking SSH connectivity...");

    let ssh_ok = ssh_batch(host, identity, "echo ok").await.is_ok();

    if !ssh_ok {
        let mwinit_available = Command::new("which")
            .arg("mwinit")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false);

        if mwinit_available {
            execute!(
                stderr,
                style::Print("\n"),
                SetForegroundColor(Color::Yellow),
                style::Print("SSH auth failed. Refreshing Midway credentials...\n"),
                SetForegroundColor(Color::Reset),
            )?;

            let mwinit_status = Command::new("mwinit")
                .arg("-o")
                .stdin(std::process::Stdio::inherit())
                .stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::inherit())
                .status()
                .await;

            if mwinit_status.is_ok() && ssh_batch(host, identity, "echo ok").await.is_err() {
                eprintln!("Midway refresh succeeded but SSH still failing. Trying interactive SSH...");
                if let Err(e) = ssh_interactive(host, identity, "echo ok").await {
                    print_fail(&mut stderr, &format!("SSH connection failed: {e}"));
                    return Ok(ExitCode::FAILURE);
                }
            }
        } else {
            execute!(
                stderr,
                style::Print("\n"),
                SetForegroundColor(Color::Yellow),
                style::Print("SSH batch mode failed. Trying interactive authentication...\n"),
                SetForegroundColor(Color::Reset),
            )?;
            if let Err(e) = ssh_interactive(host, identity, "echo ok").await {
                print_fail(&mut stderr, &format!("SSH connection failed: {e}"));
                return Ok(ExitCode::FAILURE);
            }
        }

        print_step(&mut stderr, 1, total_steps, "Checking SSH connectivity...");
    }

    print_ok(&mut stderr, "");

    // ── Phase 2: Configure voice.serverUrl on cloud desktop ──────────────────
    print_step(&mut stderr, 2, total_steps, "Configuring cloud desktop...");

    // Try to configure via remote kiro-cli binary
    let remote_bin_path = if let Some(bin) = remote_bin {
        bin.to_string()
    } else {
        let detect_cmd = "which kiro-cli 2>/dev/null || which chat_cli_v2 2>/dev/null || (test -x target/release/chat_cli_v2 && echo target/release/chat_cli_v2) || echo NOTFOUND";
        ssh_batch(host, identity, detect_cmd)
            .await
            .unwrap_or_else(|_| "NOTFOUND".into())
    };

    // Validate remote_bin_path contains no shell metacharacters
    if remote_bin_path.contains([';', '|', '&', '$', '`', '\'', '"', '\\', '\n', '\r']) {
        print_fail(&mut stderr, "Detected unsafe characters in remote binary path");
        return Ok(ExitCode::FAILURE);
    }

    if remote_bin_path != "NOTFOUND" && !remote_bin_path.is_empty() {
        let settings_cmd = format!("{remote_bin_path} settings voice.serverUrl http://localhost:{port}");
        match ssh_batch(host, identity, &settings_cmd).await {
            Ok(_) => print_ok(&mut stderr, "voice.serverUrl set"),
            Err(_) => {
                execute!(
                    stderr,
                    SetForegroundColor(Color::Yellow),
                    style::Print("\u{26a0}"),
                    SetForegroundColor(Color::Reset),
                    style::Print(" manual config needed\n")
                )
                .ok();
                eprintln!("  Run on {host}: {remote_bin_path} settings voice.serverUrl http://localhost:{port}");
            },
        }
    } else {
        execute!(
            stderr,
            SetForegroundColor(Color::Yellow),
            style::Print("\u{26a0}"),
            SetForegroundColor(Color::Reset),
            style::Print(" manual config needed\n")
        )
        .ok();
        eprintln!("  Run on {host}: kiro-cli settings voice.serverUrl http://localhost:{port}");
    }

    // ── Phase 3: Start voice-serve LOCALLY ───────────────────────────────────
    print_step(&mut stderr, 3, total_steps, "Starting local voice server...");

    let voice_serve = tokio::spawn(async move { super::voice_serve::run_voice_server("127.0.0.1", port).await });

    // Give it a moment to start
    tokio::time::sleep(Duration::from_millis(500)).await;
    print_ok(&mut stderr, &format!("localhost:{port}"));

    // ── Phase 4: SSH Reverse Tunnel (foreground) ─────────────────────────────
    print_step(&mut stderr, 4, total_steps, "Opening SSH tunnel...");
    print_ok(&mut stderr, "");

    eprintln!();
    execute!(
        stderr,
        SetForegroundColor(Color::Green),
        style::Print(format!("Voice ready! Cloud desktop {host} → localhost:{port}\n")),
        SetForegroundColor(Color::Reset),
        style::Print("Use /voice or hold Space in kiro on the cloud desktop.\n"),
        style::Print("Press Ctrl+C to stop.\n"),
    )?;

    // Reconnect loop
    let mut backoff = Duration::from_secs(1);
    let max_backoff = Duration::from_secs(30);

    loop {
        let mut cmd = Command::new("ssh");
        cmd.args([
            "-o",
            "StrictHostKeyChecking=accept-new",
            "-o",
            "ExitOnForwardFailure=yes",
            "-o",
            "ServerAliveInterval=30",
            "-o",
            "ServerAliveCountMax=3",
            "-N",
            "-R",
            &format!("{port}:localhost:{port}"),
        ]);
        if let Some(id) = identity {
            cmd.arg("-i").arg(id);
        }
        cmd.arg(host);
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        let mut child = cmd.spawn()?;

        let exit_reason = tokio::select! {
            status = child.wait() => {
                match status {
                    Ok(s) => format!("Tunnel exited: {s}"),
                    Err(e) => format!("Tunnel error: {e}"),
                }
            }
            _ = signal::ctrl_c() => {
                let _ = child.kill().await;
                "ctrl_c".to_string()
            }
        };

        if exit_reason == "ctrl_c" {
            break;
        }

        execute!(
            stderr,
            SetForegroundColor(Color::Yellow),
            style::Print(format!("\n{exit_reason}. Reconnecting in {}s...\n", backoff.as_secs())),
            SetForegroundColor(Color::Reset),
        )?;

        tokio::select! {
            _ = tokio::time::sleep(backoff) => {}
            _ = signal::ctrl_c() => { break; }
        }

        backoff = std::cmp::min(backoff * 2, max_backoff);
    }

    // ── Clean Shutdown ───────────────────────────────────────────────────────
    eprintln!();
    voice_serve.abort();
    execute!(
        stderr,
        SetForegroundColor(Color::DarkGrey),
        style::Print("Voice cloud setup stopped.\n"),
        SetForegroundColor(Color::Reset),
    )?;

    Ok(ExitCode::SUCCESS)
}
