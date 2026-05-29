//! MCP registry abstraction.
//!
//! The `agent` crate is intentionally agnostic to *what* an MCP registry is or
//! where its data comes from. The host implements [`McpRegistry`] to describe
//! how a registry should transform a [`LoadedAgentConfig`] before MCP servers
//! are launched.
//!
//! ## Lifecycle
//!
//! 1. The host constructs an implementation of [`McpRegistry`] from whatever source it wants (HTTP
//!    fetch, embedded JSON, in-memory test fixture, …).
//! 2. The host hands a `Box<dyn McpRegistry>` to the agent at construction time (via `Agent::new` /
//!    `AcpSessionBuilder`). The agent applies it to its current config before launching MCP
//!    servers.
//! 3. When the host wants a running agent to pick up a new registry snapshot, it pushes a fresh
//!    `Box<dyn McpRegistry>` to the agent through the agent request channel. The agent re-applies
//!    it to its current config and refreshes MCP state. Hosts no longer pre-rewrite the agent
//!    config.
//!
//! ## Why a trait, not a concrete type?
//!
//! Different hosts disagree about what a registry *is* (Kiro's HTTP-fetched
//! JSON, a local file for offline development, a synthetic fixture in tests).
//! The shape of the registry response, caching, governance, and refresh
//! cadence are all host concerns. The agent only cares about the result of
//! applying the registry to a config.
//!
//! ## Why sync?
//!
//! The expected responsibilities are pure in-memory transformations of a
//! [`LoadedAgentConfig`]. Implementations are expected to operate on data
//! they have already fetched and cached. Keeping [`McpRegistry::apply`] sync
//! lets it be called from anywhere in the agent (including hot paths like
//! `swap_agent`) without contaminating call sites with `async`.
//!
//! ## Why is the trait object cloneable?
//!
//! The agent and its host pass a registry handle through several layers
//! (session manager → builder → agent → swap_agent paths) and frequently
//! need to clone it without an [`std::sync::Arc`] wrapper. We use
//! [`dyn_clone`] so `Box<dyn McpRegistry>` itself is `Clone`. Implementations
//! get this for free as long as they implement [`Clone`].

use std::fmt::Debug;

use dyn_clone::DynClone;

use crate::agent_config::LoadedAgentConfig;

/// Describes how an MCP registry should be applied to a
/// [`LoadedAgentConfig`].
///
/// The trait is `Clone` (via [`dyn_clone`]) so a `Box<dyn McpRegistry>` can
/// be cloned and passed through layers without an [`std::sync::Arc`].
/// Implementations are expected to be cheap to clone — typically they wrap
/// their data in an `Arc` internally and clone the `Arc`, not the data.
///
/// ## Required behaviour of [`apply`](Self::apply)
///
/// An implementation is expected to perform some combination of the
/// following in-place mutations of `agent_config`:
///
/// - **Filter `mcp_servers`**: drop entries whose name is not present in the registry.
/// - **Resolve placeholders**: replace any
///   [`McpServerConfig::Registry`](crate::agent_config::definitions::McpServerConfig::Registry)
///   entry — including those implicitly referenced via the agent's `tools` list — with the concrete
///   `Local` or `Remote` variant from the registry.
/// - **Filter `tools`**: drop tool entries that reference servers no longer available after the
///   above steps.
///
/// ## Idempotency
///
/// Implementations must be idempotent: calling `apply` twice with the same
/// underlying registry data must produce the same result as calling it once.
/// The agent relies on this to safely re-apply a registry on refresh without
/// tracking which configs have already been transformed.
///
/// ## No-op fallback
///
/// Hosts that don't have a registry (e.g. unmanaged users, tests) can pass
/// `None` where an `Option<Box<dyn McpRegistry>>` is accepted. This keeps
/// the "no registry" case visible at call sites rather than routing through
/// a stub implementation.
pub trait McpRegistry: Send + Sync + Debug + DynClone {
    /// Apply registry-driven transformations to `agent_config` in place.
    ///
    /// See the trait-level documentation for the expected behaviour.
    fn apply(&self, agent_config: &mut LoadedAgentConfig);
}

dyn_clone::clone_trait_object!(McpRegistry);
