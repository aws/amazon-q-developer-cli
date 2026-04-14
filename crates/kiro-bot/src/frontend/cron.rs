//! Headless cron frontend — run prompts on a schedule or once.
//!
//! - `kiro-bot run <name>` — single execution, then exit
//! - `kiro-bot start <name>` — daemon loop on `every` interval or cron expression

use std::str::FromStr;
use std::time::Duration;

use anyhow::{
    Context,
    Result,
};
use tokio::sync::{
    mpsc,
    oneshot,
};
use tracing::info;

use crate::config::{
    self,
    Config,
    CronOutput,
    FrontendConfig,
    Secrets,
};
use crate::engine::acp::{
    self,
    AcpConfig,
    ApprovalPolicy,
};

/// Scheduling mode.
enum Schedule {
    Every(Duration),
    Cron(Box<cron::Schedule>),
}

/// Parse a duration string like "30s", "5m", "1h", "2h30m".
fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim();
    let mut total_secs: u64 = 0;
    let mut num_buf = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() {
            num_buf.push(c);
        } else {
            let n: u64 = num_buf.parse().context("invalid number in duration")?;
            num_buf.clear();
            total_secs += match c {
                's' => n,
                'm' => n * 60,
                'h' => n * 3600,
                'd' => n * 86400,
                _ => anyhow::bail!("unknown duration unit '{c}' — use s/m/h/d"),
            };
        }
    }
    if total_secs == 0 {
        anyhow::bail!("duration must be > 0");
    }
    Ok(Duration::from_secs(total_secs))
}

fn parse_time(s: &str) -> Result<std::time::SystemTime> {
    let dt = chrono::DateTime::parse_from_rfc3339(s)
        .or_else(|_| chrono::DateTime::parse_from_rfc3339(&format!("{s}Z")))
        .with_context(|| format!("invalid ISO 8601 time: {s}"))?;
    Ok(std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(dt.timestamp() as u64))
}

/// Run once and exit.
pub async fn run_once(name: &str) -> Result<()> {
    let (cfg, prompt, output, slack_token) = load_cron_config(name)?;
    set_working_directory(&cfg)?;

    let (work_tx, acp_info) = spawn_acp(&cfg).await?;
    let response = send_prompt(&work_tx, &prompt).await;
    emit(&output, &slack_token, &response).await?;
    drop(work_tx);
    drop(acp_info);
    Ok(())
}

/// Run as a scheduled daemon loop.
pub async fn run_scheduled(name: &str) -> Result<()> {
    let (cfg, prompt, output, slack_token) = load_cron_config(name)?;

    let (schedule, start_at, stop_at) = match &cfg.frontend {
        FrontendConfig::Cron {
            every,
            schedule,
            start_at,
            stop_at,
            ..
        } => {
            let sched = match (every.as_deref(), schedule.as_deref()) {
                (Some(e), None) => Schedule::Every(parse_duration(e)?),
                (None, Some(expr)) => Schedule::Cron(Box::new(
                    cron::Schedule::from_str(expr)
                        .map_err(|e| anyhow::anyhow!("invalid cron expression '{expr}': {e}"))?,
                )),
                (Some(_), Some(_)) => anyhow::bail!("cannot set both 'every' and 'schedule'"),
                (None, None) => Schedule::Every(Duration::from_secs(300)),
            };
            let start = start_at.as_deref().map(parse_time).transpose()?;
            let stop = stop_at.as_deref().map(parse_time).transpose()?;
            (sched, start, stop)
        },
        _ => anyhow::bail!("not a cron frontend"),
    };

    set_working_directory(&cfg)?;

    // Wait for start_at
    if let Some(start) = start_at {
        let now = std::time::SystemTime::now();
        if start > now {
            let wait = start.duration_since(now).unwrap_or_default();
            info!(wait_secs = wait.as_secs(), "Waiting for start_at");
            tokio::time::sleep(wait).await;
        }
    }

    let (work_tx, _acp_info) = spawn_acp(&cfg).await?;

    match &schedule {
        Schedule::Every(every) => {
            info!(every_secs = every.as_secs(), "Starting interval loop");
            loop {
                if past_stop(stop_at) {
                    info!("Reached stop_at, exiting");
                    break;
                }
                let response = send_prompt(&work_tx, &prompt).await;
                emit(&output, &slack_token, &response).await?;
                let sleep = if let Some(stop) = stop_at {
                    let remaining = stop.duration_since(std::time::SystemTime::now()).unwrap_or_default();
                    (*every).min(remaining)
                } else {
                    *every
                };
                tokio::time::sleep(sleep).await;
            }
        },
        Schedule::Cron(cron_sched) => {
            info!("Starting cron schedule loop");
            loop {
                // Find next fire time
                let next = cron_sched
                    .upcoming(chrono::Utc)
                    .next()
                    .context("no upcoming cron time")?;
                let now = chrono::Utc::now();
                let wait = (next - now).to_std().unwrap_or_default();

                if let Some(stop) = stop_at {
                    let stop_chrono = chrono::DateTime::<chrono::Utc>::from(stop);
                    if next > stop_chrono {
                        info!("Next cron fire is past stop_at, exiting");
                        break;
                    }
                }

                info!(next = %next, wait_secs = wait.as_secs(), "Waiting for next cron fire");
                tokio::time::sleep(wait).await;

                if past_stop(stop_at) {
                    break;
                }

                let response = send_prompt(&work_tx, &prompt).await;
                emit(&output, &slack_token, &response).await?;
            }
        },
    }

    Ok(())
}

fn past_stop(stop_at: Option<std::time::SystemTime>) -> bool {
    stop_at.is_some_and(|stop| std::time::SystemTime::now() >= stop)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load_cron_config(name: &str) -> Result<(Config, String, CronOutput, Option<String>)> {
    let instance_dir = config::config_dir(name)?;
    let cfg = config::load_config(&instance_dir)?;

    let (prompt, output) = match &cfg.frontend {
        FrontendConfig::Cron { prompt, output, .. } => (prompt.clone(), output.clone()),
        _ => anyhow::bail!("Instance '{name}' is not a cron frontend"),
    };

    let slack_token = if matches!(output, CronOutput::Slack { .. }) {
        let secrets = config::load_secrets(&instance_dir)?;
        let Secrets::Slack(s) = secrets;
        Some(s.bot_token)
    } else {
        None
    };

    Ok((cfg, prompt, output, slack_token))
}

async fn spawn_acp(
    cfg: &Config,
) -> Result<(
    mpsc::UnboundedSender<acp::Work>,
    std::sync::Arc<std::sync::Mutex<acp::AcpInfo>>,
)> {
    let acp_cfg = AcpConfig {
        command: cfg.agent.command.clone(),
        model_id: cfg.agent.model.clone(),
        bot_user: "cron".into(),
        mcp_wait_ms: cfg.agent.mcp_wait_ms,
        default_mode: cfg.agent.default_mode.clone(),
        max_workers: 1,
        idle_timeout_secs: 3600,
        approval_policy: ApprovalPolicy::Approve,
        approval_tx: None,
    };

    let (work_tx, work_rx) = mpsc::unbounded_channel();
    let (ready_tx, ready_rx) = oneshot::channel();
    let acp_info = acp::spawn_acp_thread(work_rx, ready_tx, acp_cfg);
    ready_rx.await.context("ACP thread died")?;
    info!("ACP ready");
    Ok((work_tx, acp_info))
}

async fn send_prompt(work_tx: &mpsc::UnboundedSender<acp::Work>, prompt: &str) -> String {
    let (reply_tx, reply_rx) = oneshot::channel();
    let (progress_tx, _) = mpsc::unbounded_channel();
    let _ = work_tx.send(acp::Work::Prompt {
        text: prompt.to_string(),
        context: vec![],
        conversation: "cron".into(),
        channel: String::new(),
        thread_ts: None,
        user: "cron".into(),
        slack_user_id: String::new(),
        reply_tx,
        progress_tx,
    });
    reply_rx.await.unwrap_or_else(|_| "Error: no response".into())
}

async fn emit(output: &CronOutput, slack_token: &Option<String>, text: &str) -> Result<()> {
    match output {
        CronOutput::Stdout => println!("{text}"),
        CronOutput::Slack { channel } => {
            let token = slack_token.as_deref().context("no slack token for slack output")?;
            let client = reqwest::Client::new();
            let resp = client
                .post("https://slack.com/api/chat.postMessage")
                .header("Authorization", format!("Bearer {token}"))
                .json(&serde_json::json!({ "channel": channel, "text": text, "unfurl_links": false }))
                .send()
                .await
                .context("Failed to post to Slack")?;
            let body: serde_json::Value = resp.json().await?;
            if body["ok"].as_bool() != Some(true) {
                anyhow::bail!("Slack API error: {body}");
            }
            info!(channel, "Posted to Slack");
        },
    }
    Ok(())
}

fn set_working_directory(cfg: &Config) -> Result<()> {
    if let Some(wd) = &cfg.working_directory {
        let expanded = if let Some(suffix) = wd.strip_prefix("~/") {
            dirs::home_dir()
                .context("Cannot determine home directory")?
                .join(suffix)
        } else {
            std::path::PathBuf::from(wd)
        };
        std::env::set_current_dir(&expanded)
            .with_context(|| format!("failed to set working_directory: {}", expanded.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_seconds() {
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
    }

    #[test]
    fn parse_minutes() {
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
    }

    #[test]
    fn parse_compound() {
        assert_eq!(parse_duration("1h30m").unwrap(), Duration::from_secs(5400));
    }

    #[test]
    fn parse_zero_fails() {
        assert!(parse_duration("0s").is_err());
    }
}
