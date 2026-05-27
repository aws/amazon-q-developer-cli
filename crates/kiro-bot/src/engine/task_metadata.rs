//! Per-task self-identification.
//!
//! `DynamoCoordinator` writes the owning task's identifier into the leases /
//! approvals tables so peers know who to forward to. On Fargate, the runtime
//! injects the metadata endpoint via `ECS_CONTAINER_METADATA_URI_V4`; querying
//! `<URI>/task` returns the task ARN. Outside Fargate (local CLI, unit tests),
//! callers should fall through to a hostname-based fallback.

use serde::Deserialize;

const METADATA_ENV: &str = "ECS_CONTAINER_METADATA_URI_V4";
const FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

#[derive(Debug, Deserialize)]
struct TaskMetadata {
    #[serde(rename = "TaskARN")]
    task_arn: String,
}

/// Best-effort: read `ECS_CONTAINER_METADATA_URI_V4` and GET `<URI>/task`.
/// Returns `None` if the env var is missing or the HTTP call fails.
pub async fn fetch_task_arn() -> Option<String> {
    let base = std::env::var(METADATA_ENV).ok()?;
    let url = format!("{base}/task");
    let client = reqwest::Client::builder().timeout(FETCH_TIMEOUT).build().ok()?;
    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: TaskMetadata = resp.json().await.ok()?;
    Some(body.task_arn)
}

/// Identify this process for coordinator bookkeeping. Prefers the ECS task
/// ARN; falls back to `hostname-pid` so local CLI runs still get a stable id.
pub async fn resolve_self_id() -> String {
    if let Some(arn) = fetch_task_arn().await {
        return arn;
    }
    let host = hostname().unwrap_or_else(|| "unknown".to_string());
    format!("{host}-{}", std::process::id())
}

fn hostname() -> Option<String> {
    std::env::var("HOSTNAME").ok().or_else(|| {
        // gethostname via libc on unix; on other platforms HOSTNAME usually
        // covers the CLI case.
        #[cfg(unix)]
        {
            let mut buf = [0u8; 256];
            // SAFETY: gethostname writes at most buf.len()-1 bytes and null-terminates.
            let rc = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
            if rc != 0 {
                return None;
            }
            let nul = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            std::str::from_utf8(&buf[..nul]).ok().map(|s| s.to_string())
        }
        #[cfg(not(unix))]
        {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fetch_returns_none_without_env_var() {
        // SAFETY: serial test; we don't care about cross-test races for the
        // missing-env-var case because we explicitly remove it.
        unsafe {
            std::env::remove_var(METADATA_ENV);
        }
        assert!(fetch_task_arn().await.is_none());
    }

    #[tokio::test]
    async fn resolve_self_id_returns_non_empty_fallback() {
        unsafe {
            std::env::remove_var(METADATA_ENV);
        }
        let id = resolve_self_id().await;
        assert!(!id.is_empty(), "fallback id must be non-empty");
        assert!(id.contains('-'), "fallback id has hostname-pid shape");
    }
}
