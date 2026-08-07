//! `rmcp::ServerHandler` impl that proxies kiro-help → Taskei MCP gateway.
//!
//! The local stdio server speaks plain MCP to whatever client kiro-help
//! launches the shim under (`kiro-cli acp` in dev, `kiro-bot` in
//! production). On every `tools/call` it forwards to the IAD prod
//! Taskei gateway with a SigV4-signed JSON-RPC POST and translates the
//! response back. Read tools sign with the bridge's read provider
//! (cached read role or base task-role creds in the no-op case);
//! write tools take a *per-call* AssumeRole snapshot via
//! [`crate::sts_bridge::StsBridge::assume_write_once`], sign exactly
//! one HTTPS POST with it, and drop the snapshot before returning.
//!
//! See [`super::mcp_proxy`] for the wire-level details and
//! [`super::tools`] for the read/write classification + annotation
//! overlay.

use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams,
    CallToolResponse,
    ListToolsResult,
    PaginatedRequestParams,
    ServerCapabilities,
    ServerInfo,
    Tool,
};
use rmcp::service::{
    RequestContext,
    RoleServer,
};
use rmcp::{
    ErrorData,
    ServerHandler,
};
use serde_json::Value;
use tracing::warn;

use crate::families::taskei::{
    mcp_proxy,
    tools as taskei_tools,
};
use crate::sts_bridge::StsBridge;

/// Top-of-message-thread instructions surfaced to the upstream MCP
/// client (kiro-help). rmcp passes this through `initialize`'s
/// `serverInfo.instructions` field; the agent loop may surface it to
/// the model verbatim. Keep it short and operationally focused.
const INSTRUCTIONS: &str = "Read and write Amazon Taskei tasks for kiro-help. Read tools \
    (Taskei___list_tasks / _get_task / _get_room / _list_room_resource) are auto-approved when \
    the upstream agent's allowedTools permits them. Write tools (Taskei___create_task / \
    _update_task) MUST go through the kiro-bot reaction-approval gate. Always honor the \
    bot operator's roomId allowlist — calls outside it are refused locally before signing.";

/// Bundled-shim `ServerHandler` implementation. Cheap to clone — every
/// field is `Arc`-backed.
#[derive(Clone)]
pub struct TaskeiServer {
    bridge: Arc<StsBridge>,
    endpoint: Arc<str>,
    region: Arc<str>,
    allow_rooms: Arc<[String]>,
    /// Decorated catalog populated at startup by [`Self::with_catalog`].
    tools: Arc<Vec<Tool>>,
}

impl TaskeiServer {
    /// Construct a server with a pre-fetched, pre-decorated catalog.
    /// Production passes in the exact live catalog that schema-pin
    /// already validated; tests can supply a synthetic catalog without
    /// touching the network.
    pub fn with_catalog(
        bridge: Arc<StsBridge>,
        endpoint: impl Into<Arc<str>>,
        region: impl Into<Arc<str>>,
        allow_rooms: Vec<String>,
        tools: Vec<Tool>,
    ) -> Self {
        Self {
            bridge,
            endpoint: endpoint.into(),
            region: region.into(),
            allow_rooms: allow_rooms.into(),
            tools: Arc::new(tools),
        }
    }

    /// Return a new server whose catalog is filtered to read-only
    /// tools per [`super::tools::is_read_tool`]. Used by Phase 1d's
    /// `--scope=read-only` break-glass to refuse to surface write
    /// tools in `tools/list` regardless of what the gateway returned.
    pub fn filter_to_read_only(self) -> Self {
        let filtered: Vec<Tool> = self
            .tools
            .iter()
            .filter(|t| super::tools::is_read_tool(t.name.as_ref()))
            .cloned()
            .collect();
        Self {
            tools: Arc::new(filtered),
            ..self
        }
    }

    fn check_tool_exposed(&self, tool_name: &str) -> Result<(), ErrorData> {
        if self.tools.iter().any(|tool| tool.name.as_ref() == tool_name) {
            return Ok(());
        }
        warn!(
            target: "kiro_mcp",
            tool_name = %tool_name,
            "rejecting tool call because the tool is not in this process's exposed catalog"
        );
        Err(ErrorData::invalid_params(
            format!(
                "tool `{tool_name}` is not exposed by this kiro-mcp process. It may be outside \
                 the curated Taskei catalog or blocked by --scope=read-only."
            ),
            None,
        ))
    }

    /// Validate that any `roomId` argument in the call is on the
    /// allowlist. Empty allowlist (operator left `--allow-rooms`
    /// unset) means no fence — same Phase 1c behavior. Plan §299-300
    /// pins this contract.
    fn check_room_id(&self, args: &Value) -> Result<(), ErrorData> {
        if self.allow_rooms.is_empty() {
            return Ok(());
        }
        // The gateway's argument shape varies by tool: some take
        // `roomId` at the top level (list_tasks, get_room), some
        // nest it under `task.roomId` (create_task per Phase-0
        // capture). The schema-pin assertion is what lets us trust
        // those exact shapes; here we walk the JSON tree and reject
        // if *any* `roomId` field is set to a value not in the
        // allowlist. Walking is fine: the argument tree is shallow
        // and the alternative is duplicating each tool's schema.
        let mut found_outside_allowlist: Option<String> = None;
        walk_room_ids(args, &mut |room| {
            if !self.allow_rooms.iter().any(|allowed| allowed == room) {
                found_outside_allowlist = Some(room.to_string());
            }
        });
        match found_outside_allowlist {
            Some(room) => Err(ErrorData::invalid_params(
                format!(
                    "roomId `{room}` not on --allow-rooms allowlist; refusing to forward to gateway. \
                     Configured allowlist: {:?}",
                    self.allow_rooms
                ),
                None,
            )),
            None => Ok(()),
        }
    }
}

impl ServerHandler for TaskeiServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.instructions = Some(INSTRUCTIONS.to_string());
        info
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        // The catalog is pre-decorated at startup, so there's no
        // per-call gateway hit here. Plan §319 retry policy applies
        // only on call_tool; list_tools serves cached data and never
        // 429s.
        Ok(ListToolsResult::with_all_items((*self.tools).clone()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let tool_name = request.name.to_string();
        let args_value: Value = match request.arguments {
            Some(map) => Value::Object(map.into_iter().collect()),
            None => Value::Object(Default::default()),
        };

        self.check_tool_exposed(&tool_name)?;

        // Local roomId allowlist check (--allow-rooms). Skipped when
        // the operator left the list empty.
        self.check_room_id(&args_value)?;

        let result = if taskei_tools::is_read_tool(&tool_name) {
            let view = self.bridge.read_only();
            mcp_proxy::call_read_tool_remote(&view, &self.endpoint, &self.region, &tool_name, &args_value).await
        } else if taskei_tools::is_write_tool(&tool_name) {
            mcp_proxy::call_write_tool_remote(&self.bridge, &self.endpoint, &self.region, &tool_name, &args_value).await
        } else {
            // Unknown tool — the upstream client called something
            // not in the curated catalog. Reject locally rather than
            // forwarding so the gateway doesn't see traffic for
            // tools we haven't reviewed. Phase 1d is fail-closed on
            // unknowns; if the gateway adds a new tool we want to
            // expose, the schema-pin fixture has to be refreshed AND
            // `is_read_tool`/`is_write_tool` updated in the same PR.
            warn!(
                target: "kiro_mcp",
                tool_name = %tool_name,
                "rejecting unknown tool — not in curated catalog"
            );
            Err(ErrorData::invalid_params(
                format!(
                    "unknown tool: {tool_name}. The kiro-mcp shim only exposes the curated \
                     Taskei catalog reviewed at deploy time."
                ),
                None,
            ))
        }?;
        Ok(result.into())
    }
}

/// Walk a JSON value recursively, calling `f` for every string-typed
/// field whose key is exactly `roomId` (case-sensitive — Taskei MCP
/// uses camelCase per Phase 0). Used by [`TaskeiServer::check_room_id`]
/// to enforce the `--allow-rooms` allowlist regardless of where in
/// the tool's argument shape the `roomId` sits.
fn walk_room_ids<F: FnMut(&str)>(value: &Value, f: &mut F) {
    match value {
        Value::Object(map) => {
            for (key, val) in map {
                if key == "roomId"
                    && let Value::String(room) = val
                {
                    f(room);
                }
                walk_room_ids(val, f);
            }
        },
        Value::Array(items) => {
            for item in items {
                walk_room_ids(item, f);
            }
        },
        _ => {},
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::families::taskei::tools::decorate_with_overlay;
    use crate::sts_bridge::{
        StsBridge,
        StsBridgeConfig,
    };

    /// The lightest possible `StsBridge` — no STS, no real region —
    /// good enough to construct a `TaskeiServer` for in-process tests
    /// that exercise check_room_id without touching the network.
    async fn dummy_bridge() -> Arc<StsBridge> {
        use std::time::{
            Duration,
            SystemTime,
        };

        use aws_credential_types::Credentials;
        use aws_credential_types::provider::SharedCredentialsProvider;
        let creds = Credentials::new(
            "AKIDTEST",
            "secret",
            None,
            Some(SystemTime::now() + Duration::from_secs(3600)),
            "test",
        );
        let base = aws_config::SdkConfig::builder()
            .credentials_provider(SharedCredentialsProvider::new(creds))
            .region(aws_config::Region::new("us-east-1"))
            .build();
        let bridge = StsBridge::from_base(base, StsBridgeConfig::default()).await.unwrap();
        Arc::new(bridge)
    }

    fn raw_tool(name: &str) -> Tool {
        Tool::new_with_raw(
            name.to_string(),
            None,
            Arc::new(serde_json::from_value(json!({ "type": "object" })).unwrap()),
        )
    }

    #[tokio::test]
    async fn check_room_id_passes_when_allowlist_empty() {
        let server = TaskeiServer::with_catalog(
            dummy_bridge().await,
            "https://example/mcp",
            "us-east-1",
            Vec::new(),
            Vec::new(),
        );
        let args = json!({ "roomId": "any-id-allowed" });
        server.check_room_id(&args).expect("empty allowlist == no fence");
    }

    #[tokio::test]
    async fn check_room_id_accepts_listed_room() {
        let server = TaskeiServer::with_catalog(
            dummy_bridge().await,
            "https://example/mcp",
            "us-east-1",
            vec!["room-A".into(), "room-B".into()],
            Vec::new(),
        );
        server
            .check_room_id(&json!({ "roomId": "room-A" }))
            .expect("listed room should pass");
    }

    #[tokio::test]
    async fn check_room_id_rejects_unlisted_room_at_top_level() {
        let server = TaskeiServer::with_catalog(
            dummy_bridge().await,
            "https://example/mcp",
            "us-east-1",
            vec!["room-A".into()],
            Vec::new(),
        );
        let err = server
            .check_room_id(&json!({ "roomId": "room-X" }))
            .expect_err("unlisted room must reject");
        assert!(err.message.contains("room-X"), "got: {err:?}");
    }

    #[tokio::test]
    async fn check_room_id_walks_into_nested_objects() {
        // Plan §313-318: the gateway's argument shape varies by tool.
        // `Taskei___create_task` (per Phase-0 capture) nests the
        // roomId under a `task` object. The walker must catch it
        // regardless.
        let server = TaskeiServer::with_catalog(
            dummy_bridge().await,
            "https://example/mcp",
            "us-east-1",
            vec!["room-A".into()],
            Vec::new(),
        );
        let err = server
            .check_room_id(&json!({ "task": { "roomId": "room-X", "title": "x" } }))
            .expect_err("nested unlisted room must reject");
        assert!(err.message.contains("room-X"), "got: {err:?}");
    }

    #[tokio::test]
    async fn check_room_id_walks_into_arrays() {
        // Defensive — no Phase-0 tool currently nests roomId in an
        // array, but a future tool could. The walker must not skip
        // arrays.
        let server = TaskeiServer::with_catalog(
            dummy_bridge().await,
            "https://example/mcp",
            "us-east-1",
            vec!["room-A".into()],
            Vec::new(),
        );
        let err = server
            .check_room_id(&json!({
                "items": [{ "roomId": "room-X" }]
            }))
            .expect_err("array-nested unlisted room must reject");
        assert!(err.message.contains("room-X"), "got: {err:?}");
    }

    #[tokio::test]
    async fn check_room_id_ignores_calls_with_no_room_id() {
        // Some tools (e.g. `x_amz_bedrock_agentcore_search`) may not
        // take a roomId at all. They should pass the allowlist check
        // unconditionally; the gateway is the source of truth for
        // whether the call is valid.
        let server = TaskeiServer::with_catalog(
            dummy_bridge().await,
            "https://example/mcp",
            "us-east-1",
            vec!["room-A".into()],
            Vec::new(),
        );
        server
            .check_room_id(&json!({ "query": "anything" }))
            .expect("no roomId == no check");
    }

    #[tokio::test]
    async fn filtered_read_only_server_rejects_write_tool_names() {
        let tools = decorate_with_overlay(vec![raw_tool("Taskei___list_tasks"), raw_tool("Taskei___create_task")]);
        let server = TaskeiServer::with_catalog(
            dummy_bridge().await,
            "https://example/mcp",
            "us-east-1",
            Vec::new(),
            tools,
        )
        .filter_to_read_only();

        server
            .check_tool_exposed("Taskei___list_tasks")
            .expect("read tool remains exposed");
        let err = server
            .check_tool_exposed("Taskei___create_task")
            .expect_err("write tool must not be callable after read-only filtering");
        assert!(err.message.contains("read-only"), "got: {err:?}");
    }
}
