//! `refresh_mcp_registry` tests
//!
//! These exercise the `RefreshMcpRegistry` request variant added when the agent crate
//! took ownership of the MCP registry concept. The expectations they encode mirror the
//! guarantees we want before migrating SessionManager off the host-rewrite path:
//!
//!   1. The agent is paused while processing the request (implicit — actor model).
//!   2. If a turn is in progress, the request returns `NotIdle`. Hosts defer.
//!   3. Workspace + global `mcp.json` paths are reused from `Agent::new`; hosts no longer pass them
//!      on every refresh.
//!   4. Registry-driven invariants survive a subsequent `swap_agent`.
//!
//! And these properties are verified across the four governance scenarios:
//!   - MCP enabled, registry-controlled
//!   - MCP enabled, no registry on the request (no-op apply)
//!   - MCP disabled (governance short-circuit)
//!   - MCP enabled with a stored registry, then a swap

mod common;

use std::sync::Arc;
use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};
use std::time::Duration;

use agent::agent_config::definitions::{
    AgentConfig,
    AgentConfigV2025_08_22,
    LocalMcpServerConfig,
    McpServerConfig,
};
use agent::agent_config::{
    ConfigSource,
    LoadedAgentConfig,
    ResolvedGlobalPrompt,
};
use agent::agent_loop::model::MockResponse;
use agent::agent_loop::protocol::StreamResult;
use agent::agent_loop::types::{
    ContentBlockDelta,
    ContentBlockDeltaEvent,
    MessageStartEvent,
    MessageStopEvent,
    Role,
    StopReason,
    StreamEvent,
};
use agent::mcp::McpRegistry;
use agent::protocol::{
    AgentError,
    SwapAgentArgs,
};
use common::*;

/// Test registry that records `apply` invocations and optionally drops named MCP
/// servers from the agent config. Drops are idempotent — applying the same registry
/// twice produces the same result.
#[derive(Debug, Clone)]
struct TestRegistry {
    apply_count: Arc<AtomicUsize>,
    servers_to_drop: Vec<String>,
}

impl TestRegistry {
    fn new() -> Self {
        Self {
            apply_count: Arc::new(AtomicUsize::new(0)),
            servers_to_drop: Vec::new(),
        }
    }

    fn dropping(servers: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            apply_count: Arc::new(AtomicUsize::new(0)),
            servers_to_drop: servers.into_iter().map(Into::into).collect(),
        }
    }

    fn apply_count(&self) -> usize {
        self.apply_count.load(Ordering::SeqCst)
    }
}

impl McpRegistry for TestRegistry {
    fn apply(&self, agent_config: &mut LoadedAgentConfig) {
        self.apply_count.fetch_add(1, Ordering::SeqCst);
        let drop = self.servers_to_drop.clone();
        agent_config
            .config_mut()
            .retain_mcp_servers(|name| !drop.iter().any(|d| d == name));
    }
}

fn local_mcp_server(name: &str) -> (String, McpServerConfig) {
    (
        name.to_string(),
        McpServerConfig::Local(LocalMcpServerConfig {
            command: "/bin/echo".to_string(),
            args: vec![name.to_string()],
            env: None,
            timeout_ms: 30_000,
            disabled: false,
            disabled_tools: vec![],
        }),
    )
}

/// Scenario: MCP enabled, registry-controlled.
///
/// Refresh runs `apply` on the agent's current config and the resulting MCP server
/// set reflects the registry's filtering. This is the primary happy path the
/// SessionManager migration depends on.
#[tokio::test]
async fn refresh_applies_registry_to_current_config() {
    let _ = tracing_subscriber::fmt::try_init();

    let test = TestCase::builder()
        .test_name("refresh_applies_registry_to_current_config")
        .with_default_agent_config()
        .with_mcp_server("keep", local_mcp_server("keep").1)
        .with_mcp_server("drop", local_mcp_server("drop").1)
        .build()
        .await
        .unwrap();

    // Sanity: both servers are present before refresh.
    let snapshot = test.create_snapshot().await;
    let names: std::collections::HashSet<&String> = snapshot.agent_config.config().mcp_servers().keys().collect();
    assert!(names.contains(&"keep".to_string()));
    assert!(names.contains(&"drop".to_string()));

    let registry = TestRegistry::dropping(["drop"]);
    let observer = registry.clone();
    test.refresh_mcp_registry(Box::new(registry))
        .await
        .expect("refresh should succeed when agent is idle");

    assert_eq!(
        observer.apply_count(),
        1,
        "registry.apply() should have been called exactly once"
    );

    let snapshot = test.create_snapshot().await;
    let names: Vec<&String> = snapshot.agent_config.config().mcp_servers().keys().collect();
    assert_eq!(names.len(), 1, "filtered server should be removed; got {:?}", names);
    assert!(
        names.iter().any(|n| n.as_str() == "keep"),
        "kept server must remain; got {:?}",
        names
    );
}

/// Scenario: MCP enabled, registry-controlled, refresh sent twice with same data.
///
/// `McpRegistry::apply` is contractually idempotent. Two refreshes with the same
/// registry behaviour must converge to the same config.
#[tokio::test]
async fn refresh_is_idempotent() {
    let _ = tracing_subscriber::fmt::try_init();

    let test = TestCase::builder()
        .test_name("refresh_is_idempotent")
        .with_default_agent_config()
        .with_mcp_server("keep", local_mcp_server("keep").1)
        .with_mcp_server("drop", local_mcp_server("drop").1)
        .build()
        .await
        .unwrap();

    let registry = TestRegistry::dropping(["drop"]);
    test.refresh_mcp_registry(Box::new(registry.clone())).await.unwrap();
    test.refresh_mcp_registry(Box::new(registry.clone())).await.unwrap();

    let snapshot = test.create_snapshot().await;
    let names: Vec<&String> = snapshot.agent_config.config().mcp_servers().keys().collect();
    assert_eq!(names.len(), 1);
    assert!(names.iter().any(|n| n.as_str() == "keep"));
}

/// Scenario: MCP disabled by governance.
///
/// The handler must short-circuit: governance wins. The registry is stored
/// (so it would apply if MCP were re-enabled), but `apply` is **not** called and
/// the config's MCP server set stays empty (it was already cleared at construction
/// by the same governance check).
#[tokio::test]
async fn refresh_short_circuits_when_mcp_disabled() {
    let _ = tracing_subscriber::fmt::try_init();

    let settings = agent::types::AgentSettings {
        mcp_enabled: false,
        ..Default::default()
    };

    // Build with MCP servers in the config; they should be cleared at construction
    // because mcp_enabled=false. We verify the registry refresh doesn't accidentally
    // resurrect them.
    let test = TestCase::builder()
        .test_name("refresh_short_circuits_when_mcp_disabled")
        .with_default_agent_config()
        .with_settings(settings)
        .with_mcp_server("would-be-launched", local_mcp_server("would-be-launched").1)
        .build()
        .await
        .unwrap();

    // Confirm governance already stripped the server at construction time.
    let snapshot = test.create_snapshot().await;
    assert!(
        snapshot.agent_config.config().mcp_servers().is_empty(),
        "mcp_enabled=false at construction must clear mcp_servers"
    );

    // A registry that *would* mutate config (drop "anything") — but apply must not
    // be called when MCP is disabled.
    let registry = TestRegistry::dropping(["anything"]);
    let observer = registry.clone();
    test.refresh_mcp_registry(Box::new(registry))
        .await
        .expect("refresh under mcp_enabled=false should still return Success");

    assert_eq!(
        observer.apply_count(),
        0,
        "registry.apply() must not run when settings.mcp_enabled == false"
    );

    let snapshot = test.create_snapshot().await;
    assert!(
        snapshot.agent_config.config().mcp_servers().is_empty(),
        "mcp_servers must remain empty after refresh under mcp_enabled=false"
    );
}

/// Scenario: MCP enabled, refresh + subsequent swap.
///
/// The stored registry survives the swap and is automatically re-applied to the
/// new agent config. This is the invariant that lets the host stop pre-rewriting
/// configs before every `swap_agent` call.
#[tokio::test]
async fn registry_persists_across_agent_swap() {
    let _ = tracing_subscriber::fmt::try_init();

    let test = TestCase::builder()
        .test_name("registry_persists_across_agent_swap")
        .with_default_agent_config()
        .with_mcp_server("base-keep", local_mcp_server("base-keep").1)
        .with_mcp_server("base-drop", local_mcp_server("base-drop").1)
        .build()
        .await
        .unwrap();

    // Push a registry that filters out anything named "*-drop" (across both the
    // initial config and any swap target).
    let registry = TestRegistry::dropping(["base-drop", "swapped-drop"]);
    let observer = registry.clone();
    test.refresh_mcp_registry(Box::new(registry)).await.unwrap();

    assert_eq!(observer.apply_count(), 1, "registry should apply on initial refresh");

    // Swap to a different agent that *also* contains a server the registry would
    // drop. After swap, the agent must have re-applied the stored registry.
    let mut target_inner = AgentConfigV2025_08_22 {
        name: "swap-target".to_string(),
        ..Default::default()
    };
    target_inner
        .mcp_servers
        .insert("swapped-keep".to_string(), local_mcp_server("swapped-keep").1);
    target_inner
        .mcp_servers
        .insert("swapped-drop".to_string(), local_mcp_server("swapped-drop").1);

    test.swap_agent(SwapAgentArgs {
        agent_config: LoadedAgentConfig::new(
            AgentConfig::V2025_08_22(target_inner),
            ConfigSource::Ephemeral,
            ResolvedGlobalPrompt::None,
        ),
        force: false,
        knowledge_provider: None,
    })
    .await
    .expect("swap should succeed");

    assert_eq!(
        observer.apply_count(),
        2,
        "swap_agent must re-apply the stored registry; expected 2 apply calls (refresh + swap)"
    );

    let snapshot = test.create_snapshot().await;
    let names: Vec<&String> = snapshot.agent_config.config().mcp_servers().keys().collect();
    assert_eq!(
        names.len(),
        1,
        "after swap, registry should drop swapped-drop; got {:?}",
        names
    );
    assert!(
        names.iter().any(|n| n.as_str() == "swapped-keep"),
        "swapped-keep should remain after swap; got {:?}",
        names
    );
}

/// Behavior 2: refresh sent while a turn is in flight returns `NotIdle`.
///
/// We use `MockResponse::with_delay` so the agent stays in `ExecutingRequest` long
/// enough for us to race a refresh against it. The agent does not block-wait; it
/// rejects, and the host is expected to defer (the `pending_mcp_refresh` pattern in
/// `acp_agent.rs::main_loop`).
#[tokio::test]
async fn refresh_returns_not_idle_during_streaming_turn() {
    let _ = tracing_subscriber::fmt::try_init();

    // Mock model emits a single message, but only after a delay long enough that
    // we can inject a refresh request mid-stream.
    let stream_items: Vec<StreamResult> = vec![
        StreamResult::Ok(StreamEvent::MessageStart(MessageStartEvent { role: Role::Assistant })),
        StreamResult::Ok(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
            content_block_index: Some(0),
            delta: ContentBlockDelta::Text("hi".to_string()),
        })),
        StreamResult::Ok(StreamEvent::MessageStop(MessageStopEvent {
            stop_reason: StopReason::EndTurn,
        })),
    ];
    let delayed = MockResponse::with_delay(stream_items, Duration::from_millis(400));

    let mut test = TestCase::builder()
        .test_name("refresh_returns_not_idle_during_streaming_turn")
        .with_default_agent_config()
        .with_mock_response(delayed)
        .build()
        .await
        .unwrap();

    // Kick off a prompt — the agent transitions to ExecutingRequest and stays there
    // until the delayed mock stream begins.
    test.send_prompt("hello".to_string()).await;

    // Race a refresh against the in-flight turn. We allow a tiny grace window for
    // the actor to dequeue the prompt and update its active state.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let result = test.refresh_mcp_registry(Box::new(TestRegistry::new())).await;
    assert!(
        matches!(result, Err(AgentError::NotIdle)),
        "refresh during streaming must return NotIdle; got {:?}",
        result
    );

    // Drain the in-flight turn so the test exits cleanly.
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();
}

/// Test registry that resolves named `Registry` placeholders into concrete
/// `Local` configs. Stands in for whatever registry implementation the host
/// supplies — verifies the agent crate's contract independently of any
/// specific host adapter.
#[derive(Debug, Clone)]
struct ResolvingTestRegistry {
    resolutions: std::collections::HashMap<String, McpServerConfig>,
}

impl ResolvingTestRegistry {
    fn new(entries: impl IntoIterator<Item = (String, McpServerConfig)>) -> Self {
        Self {
            resolutions: entries.into_iter().collect(),
        }
    }
}

impl McpRegistry for ResolvingTestRegistry {
    fn apply(&self, agent_config: &mut LoadedAgentConfig) {
        // Find every Registry placeholder we know how to resolve, then overwrite
        // those entries with concrete Local/Remote configs. Idempotent because
        // already-resolved entries no longer match `Registry(_)`.
        let placeholders: Vec<String> = agent_config
            .config()
            .mcp_servers()
            .iter()
            .filter(|(_, v)| matches!(v, McpServerConfig::Registry(_)))
            .map(|(k, _)| k.clone())
            .collect();

        let updates: Vec<(String, McpServerConfig)> = placeholders
            .into_iter()
            .filter_map(|name| self.resolutions.get(&name).cloned().map(|cfg| (name, cfg)))
            .collect();

        agent_config.config_mut().insert_mcp_servers(updates);
    }
}

fn registry_placeholder() -> McpServerConfig {
    McpServerConfig::Registry(agent::agent_config::definitions::RegistryMcpServerConfig {
        server_type: "registry".to_string(),
        env: None,
        headers: None,
        timeout: None,
        oauth_scopes: vec![],
        oauth: None,
    })
}

/// Scenario: agent.json declares `"type": "registry"` for a server, and the
/// agent is constructed with a registry that resolves it.
///
/// Verifies the `Registry` placeholder is replaced with a concrete `Local`
/// variant *before* MCP launch — i.e. the agent's `mcp::McpRegistry::apply`
/// hook is the resolution point. Without this guarantee, the agent would
/// hit the `"Registry server '...' was not resolved before launch. This is
/// a bug."` path in `mcp::service::launch`.
#[tokio::test]
async fn registry_resolves_placeholder_at_construction() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut config_inner = AgentConfigV2025_08_22 {
        name: "registry_resolves_placeholder".to_string(),
        tools: vec!["*".to_string(), "@github/issues".to_string()],
        ..Default::default()
    };
    config_inner
        .mcp_servers
        .insert("github".to_string(), registry_placeholder());

    // Concrete config the registry will substitute for the `github` placeholder.
    let resolved_config = local_mcp_server("github").1;

    let registry = ResolvingTestRegistry::new([("github".to_string(), resolved_config)]);

    let test = TestCase::builder()
        .test_name("registry_resolves_placeholder_at_construction")
        .with_agent_config(AgentConfig::V2025_08_22(config_inner))
        .with_mcp_registry(Box::new(registry))
        .build()
        .await
        .unwrap();

    let snapshot = test.create_snapshot().await;
    let github = snapshot
        .agent_config
        .config()
        .mcp_servers()
        .get("github")
        .expect("github server should be present after registry resolution");
    assert!(
        matches!(github, McpServerConfig::Local(_)),
        "registry should resolve placeholder into Local; got {:?}",
        github
    );

    // The tools list still references the resolved server — registry resolution
    // must not strip tool entries that point at servers it just resolved.
    assert!(
        snapshot.agent_config.tools().iter().any(|t| t == "@github/issues"),
        "tool reference to resolved server should survive; got {:?}",
        snapshot.agent_config.tools()
    );
}

/// Scenario: agent.json declares `"type": "registry"` for a server, but
/// `Agent::new` was given no registry.
///
/// The placeholder must survive — there's no way for the agent crate to
/// resolve it on its own. The agent's MCP launch path has a defensive
/// `eyre::bail!("...not resolved before launch. This is a bug.")` that
/// catches this at runtime; this test pins the *config-shape* invariant
/// that gets us there.
#[tokio::test]
async fn registry_placeholder_survives_when_no_registry() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut config_inner = AgentConfigV2025_08_22 {
        name: "registry_placeholder_no_registry".to_string(),
        tools: vec!["*".to_string()],
        ..Default::default()
    };
    config_inner
        .mcp_servers
        .insert("github".to_string(), registry_placeholder());

    let test = TestCase::builder()
        .test_name("registry_placeholder_survives_when_no_registry")
        // Note: no .with_mcp_registry(...) — agent gets None.
        .with_agent_config(AgentConfig::V2025_08_22(config_inner))
        .build()
        .await
        .unwrap();

    let snapshot = test.create_snapshot().await;
    let github = snapshot
        .agent_config
        .config()
        .mcp_servers()
        .get("github")
        .expect("github placeholder should still be present");
    assert!(
        matches!(github, McpServerConfig::Registry(_)),
        "Registry placeholder must survive when no registry is set; got {:?}",
        github
    );
}

/// Scenario: a placeholder resolved at construction is preserved across an
/// agent swap, because the stored registry re-applies on the swapped-in
/// config too.
///
/// Without this, `/agent <name>` would re-introduce un-resolved placeholders
/// every time the user switched modes — exactly the bug the
/// `apply-on-swap` hook in `handle_swap_agent` exists to prevent.
#[tokio::test]
async fn registry_resolves_placeholder_on_swap() {
    let _ = tracing_subscriber::fmt::try_init();

    let initial = AgentConfigV2025_08_22 {
        name: "initial".to_string(),
        ..Default::default()
    };

    let resolved_config = local_mcp_server("github").1;
    let registry = ResolvingTestRegistry::new([("github".to_string(), resolved_config)]);

    let test = TestCase::builder()
        .test_name("registry_resolves_placeholder_on_swap")
        .with_agent_config(AgentConfig::V2025_08_22(initial))
        .with_mcp_registry(Box::new(registry))
        .build()
        .await
        .unwrap();

    // Swap to a target that itself contains a Registry placeholder. The agent's
    // stored registry should resolve it during swap.
    let mut target_inner = AgentConfigV2025_08_22 {
        name: "swap-target".to_string(),
        tools: vec!["*".to_string(), "@github/issues".to_string()],
        ..Default::default()
    };
    target_inner
        .mcp_servers
        .insert("github".to_string(), registry_placeholder());

    test.swap_agent(SwapAgentArgs {
        agent_config: LoadedAgentConfig::new(
            AgentConfig::V2025_08_22(target_inner),
            ConfigSource::Ephemeral,
            ResolvedGlobalPrompt::None,
        ),
        force: false,
        knowledge_provider: None,
    })
    .await
    .expect("swap should succeed");

    let snapshot = test.create_snapshot().await;
    let github = snapshot
        .agent_config
        .config()
        .mcp_servers()
        .get("github")
        .expect("github should be present after swap");
    assert!(
        matches!(github, McpServerConfig::Local(_)),
        "stored registry must resolve placeholder on swap; got {:?}",
        github
    );
}
