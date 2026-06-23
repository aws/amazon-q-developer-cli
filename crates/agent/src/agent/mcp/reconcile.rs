//! Pure MCP server reconciliation.
//!
//! Given the servers currently applied and a new desired set, compute the
//! minimal [`McpReconcilePlan`] — which servers to launch, stop, or restart —
//! so that only servers whose presence or config actually changed are touched.
//! Servers present in both with an unchanged config are left running.
//!
//! This is the surgical alternative to tearing down every server and relaunching
//! (what a full agent swap does). It is a pure function: no I/O, no async, no
//! manager state — trivially unit-testable. The caller is responsible for:
//!   - passing only servers that *should be running* in `desired` (i.e. filter out `disabled`
//!     entries before calling), and
//!   - applying the returned plan via the MCP manager's
//!     [`stop_server`](super::McpManagerHandle::stop_server) /
//!     [`launch_server`](super::McpManagerHandle::launch_server).

use std::collections::HashMap;

use crate::agent::agent_config::definitions::McpServerConfig;

/// The minimal set of MCP lifecycle actions to move from the current running
/// set to the desired set. A server present in both with an unchanged config
/// appears in none of these lists — it is left untouched.
///
/// Vecs are sorted by server name for deterministic, testable output.
#[derive(Debug, Default)]
pub struct McpReconcilePlan {
    /// Servers in `desired` but not currently running — start them.
    pub launch: Vec<(String, McpServerConfig)>,
    /// Servers currently running but absent from `desired` — stop them.
    pub stop: Vec<String>,
    /// Servers in both whose config changed — stop the old, then start the new.
    pub restart: Vec<(String, McpServerConfig)>,
}

impl McpReconcilePlan {
    /// Whether the plan would change anything. An empty plan means the running
    /// set already matches desired, so the caller can skip cache invalidation.
    pub fn is_empty(&self) -> bool {
        self.launch.is_empty() && self.stop.is_empty() && self.restart.is_empty()
    }
}

/// Compare `current` (applied) against `desired` and produce the minimal plan.
///
/// Config equality is by serialized JSON value, since [`McpServerConfig`] does
/// not implement `PartialEq`. Comparing `serde_json::Value`s is order
/// independent, so a reordered `env` map does NOT count as a change. Any real
/// difference (command, args, env contents, url, *or* metadata such as
/// `disabledTools`) yields a restart.
///
/// ponytail: restart-on-any-change. A metadata-only change (e.g. toggling
/// `disabledTools`) reconnects rather than updating tools in place. Upgrade
/// path: split into connection-vs-metadata classification like KAS's
/// `classifyConfigChange` if reconnection churn ever becomes a problem.
pub fn reconcile_mcp(
    current: &HashMap<String, McpServerConfig>,
    desired: &HashMap<String, McpServerConfig>,
) -> McpReconcilePlan {
    let mut plan = McpReconcilePlan::default();

    for (name, cfg) in desired {
        match current.get(name) {
            None => plan.launch.push((name.clone(), cfg.clone())),
            Some(old) if config_changed(old, cfg) => plan.restart.push((name.clone(), cfg.clone())),
            Some(_) => { /* unchanged — leave running */ },
        }
    }

    for name in current.keys() {
        if !desired.contains_key(name) {
            plan.stop.push(name.clone());
        }
    }

    // Deterministic ordering keeps the plan stable and unit tests simple.
    plan.launch.sort_by(|a, b| a.0.cmp(&b.0));
    plan.stop.sort();
    plan.restart.sort_by(|a, b| a.0.cmp(&b.0));
    plan
}

/// Whether two server configs differ in effective content. Uses
/// `serde_json::Value` equality (order independent for maps). A serialization
/// failure on either side is treated conservatively as "changed".
fn config_changed(a: &McpServerConfig, b: &McpServerConfig) -> bool {
    match (serde_json::to_value(a), serde_json::to_value(b)) {
        (Ok(av), Ok(bv)) => av != bv,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::agent::agent_config::definitions::LocalMcpServerConfig;

    fn local(command: &str) -> McpServerConfig {
        McpServerConfig::Local(LocalMcpServerConfig {
            command: command.to_string(),
            args: vec![],
            env: None,
            timeout_ms: 1000,
            disabled: false,
            disabled_tools: vec![],
        })
    }

    fn local_env(command: &str, env: HashMap<String, String>) -> McpServerConfig {
        McpServerConfig::Local(LocalMcpServerConfig {
            command: command.to_string(),
            args: vec![],
            env: Some(env),
            timeout_ms: 1000,
            disabled: false,
            disabled_tools: vec![],
        })
    }

    fn map(pairs: &[(&str, McpServerConfig)]) -> HashMap<String, McpServerConfig> {
        pairs.iter().map(|(n, c)| ((*n).to_string(), c.clone())).collect()
    }

    fn names(servers: &[(String, McpServerConfig)]) -> Vec<&str> {
        servers.iter().map(|(n, _)| n.as_str()).collect()
    }

    #[test]
    fn empty_when_identical() {
        let cur = map(&[("a", local("x")), ("b", local("y"))]);
        let plan = reconcile_mcp(&cur, &cur.clone());
        assert!(plan.is_empty());
    }

    #[test]
    fn launches_new_servers() {
        let cur = map(&[("a", local("x"))]);
        let desired = map(&[("a", local("x")), ("b", local("y"))]);
        let plan = reconcile_mcp(&cur, &desired);
        assert_eq!(names(&plan.launch), vec!["b"]);
        assert!(plan.stop.is_empty());
        assert!(plan.restart.is_empty());
    }

    #[test]
    fn stops_removed_servers() {
        let cur = map(&[("a", local("x")), ("b", local("y"))]);
        let desired = map(&[("a", local("x"))]);
        let plan = reconcile_mcp(&cur, &desired);
        assert_eq!(plan.stop, vec!["b".to_string()]);
        assert!(plan.launch.is_empty());
        assert!(plan.restart.is_empty());
    }

    #[test]
    fn restarts_changed_config() {
        let cur = map(&[("a", local("old"))]);
        let desired = map(&[("a", local("new"))]);
        let plan = reconcile_mcp(&cur, &desired);
        assert_eq!(names(&plan.restart), vec!["a"]);
        assert!(plan.launch.is_empty());
        assert!(plan.stop.is_empty());
    }

    #[test]
    fn env_reorder_is_not_a_change() {
        // Two env maps with identical content but different insertion order must
        // NOT trigger a restart — config comparison is order independent.
        let mut e1 = HashMap::new();
        e1.insert("A".to_string(), "1".to_string());
        e1.insert("B".to_string(), "2".to_string());
        let mut e2 = HashMap::new();
        e2.insert("B".to_string(), "2".to_string());
        e2.insert("A".to_string(), "1".to_string());

        let cur = map(&[("a", local_env("x", e1))]);
        let desired = map(&[("a", local_env("x", e2))]);
        let plan = reconcile_mcp(&cur, &desired);
        assert!(plan.is_empty(), "env reorder should not trigger a restart");
    }

    #[test]
    fn mixed_plan_launch_stop_restart_keep() {
        let cur = map(&[("keep", local("k")), ("change", local("old")), ("drop", local("d"))]);
        let desired = map(&[("keep", local("k")), ("change", local("new")), ("add", local("a"))]);
        let plan = reconcile_mcp(&cur, &desired);
        assert_eq!(names(&plan.launch), vec!["add"]);
        assert_eq!(plan.stop, vec!["drop".to_string()]);
        assert_eq!(names(&plan.restart), vec!["change"]);
    }
}
