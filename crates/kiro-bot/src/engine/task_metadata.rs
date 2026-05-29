//! Per-task self-identification.
//!
//! `DynamoCoordinator` writes the owning task's identifier into the leases /
//! approvals tables so peers know who to forward to. The id has to be a
//! reachable network endpoint — `forward()` builds `http://{peer}/dispatch`
//! verbatim — so on Fargate we resolve the container's private IPv4 from
//! `ECS_CONTAINER_METADATA_URI_V4` and return `<ip>:<dispatch_port>`. Outside
//! Fargate (local CLI, unit tests) we fall back to a hostname-based id; the
//! `NoopCoordinator` doesn't actually `forward()` so the URL never gets used.

use serde::Deserialize;

const METADATA_ENV: &str = "ECS_CONTAINER_METADATA_URI_V4";
const FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

#[derive(Debug, Deserialize)]
struct ContainerMetadata {
    #[serde(rename = "Networks", default)]
    networks: Vec<ContainerNetwork>,
}

#[derive(Debug, Deserialize)]
struct ContainerNetwork {
    #[serde(rename = "IPv4Addresses", default)]
    ipv4_addresses: Vec<String>,
}

/// Best-effort: read the container's own private IPv4 from
/// `ECS_CONTAINER_METADATA_URI_V4` (no `/task` suffix → container-level
/// metadata, which has the Networks block for this container directly).
/// Returns `None` outside Fargate or on network failure.
pub async fn fetch_container_ipv4() -> Option<String> {
    let base = std::env::var(METADATA_ENV).ok()?;
    let client = reqwest::Client::builder().timeout(FETCH_TIMEOUT).build().ok()?;
    let resp = client.get(&base).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: ContainerMetadata = resp.json().await.ok()?;
    body.networks
        .into_iter()
        .find_map(|n| n.ipv4_addresses.into_iter().next())
}

/// Identify this process for coordinator bookkeeping. On Fargate, returns
/// `<private-ipv4>:<dispatch_port>` so peers can `POST http://<id>/dispatch`
/// against this task directly. Falls back to `hostname-pid` outside Fargate
/// (or when the container-metadata Networks block is unavailable) so local
/// CLI runs still get a stable id; we never return a task ARN because peers
/// would build a malformed forward URL from it.
pub async fn resolve_self_id(dispatch_port: u16) -> String {
    if let Some(ip) = fetch_container_ipv4().await {
        return format!("{ip}:{dispatch_port}");
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
        assert!(fetch_container_ipv4().await.is_none());
    }

    #[tokio::test]
    async fn resolve_self_id_returns_non_empty_fallback() {
        unsafe {
            std::env::remove_var(METADATA_ENV);
        }
        let id = resolve_self_id(8080).await;
        assert!(!id.is_empty(), "fallback id must be non-empty");
        assert!(id.contains('-'), "fallback id has hostname-pid shape");
    }
}
