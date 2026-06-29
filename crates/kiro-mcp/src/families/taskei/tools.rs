//! Annotation overlay for the Taskei tool family.
//!
//! Phase 1d does not invent the Taskei tool catalog — the gateway is
//! itself an MCP server and `tools/list` returns the canonical set
//! (see [`crate::families::taskei::mcp_proxy::list_tools_remote`]).
//! What this module owns is the *annotation overlay*: per-tool
//! `readOnlyHint` / `destructiveHint` values pinned by plan
//! §377-384. We stamp them onto the gateway's Tool structs at startup
//! before handing the catalog to upstream clients.
//!
//! Why annotate locally instead of trusting the gateway:
//!
//! 1. The gateway may or may not return annotations today — Phase 0 didn't capture an explicit
//!    `readOnlyHint` field for every tool. Pinning here gives the Phase-2 kiro-bot agent a stable,
//!    reviewer-blessed annotation surface regardless of what the gateway emits.
//! 2. The plan §377-384 entry IS the contract for kiro-bot's approval policy. If the gateway ever
//!    flips a hint silently, our overlay *wins* and the schema-pin assertion (separate, hashes the
//!    inputSchema) will surface the drift.
//! 3. A new tool the gateway adds without an entry in our table will surface to upstream clients as
//!    un-annotated; combined with the schema-pin's name-set check, that's a fail-closed: kiro-bot's
//!    approval policy treats an annotation-less tool the same as `readOnlyHint: false` (per plan
//!    §270 second-layer `allowedTools`), and the schema-pin will refuse boot until the operator
//!    updates the fixture.
//!
//! `x_amz_bedrock_agentcore_search` is intentionally absent from the
//! overlay table: plan §383-384 says "annotation TBD per Phase-0
//! schema review." The schema-pin fixture pins whatever the gateway
//! returns for it; this overlay leaves the annotations untouched.

use std::collections::HashMap;

use rmcp::model::{
    Tool,
    ToolAnnotations,
};

/// One row of the overlay — what plan §377-384 says each tool's hints
/// must be. Stored as plain bools because every entry in the overlay
/// is opinionated; tools we don't know about (the
/// `x_amz_bedrock_agentcore_search` helper) don't appear here.
#[derive(Debug, Clone, Copy)]
struct Overlay {
    read_only: bool,
    destructive: bool,
    /// Idempotent: read tools are trivially idempotent; the writes
    /// pinned in Phase 1d are NOT idempotent on their own (every
    /// `Taskei___create_task` call yields a new task) so this is
    /// `false` for them. Phase 4b's DDB-backed idempotency layer is
    /// what makes the bot's *Slack-thread-keyed* dedup safe — that's
    /// orthogonal to whether the underlying tool itself is
    /// idempotent.
    idempotent: bool,
}

/// Plan §377-384 overlay table. The function builds a fresh `HashMap`
/// per call because rmcp's `Tool` itself isn't `Hash` and the cost is
/// negligible — kiro-mcp constructs the catalog once at startup.
fn overlay_table() -> HashMap<&'static str, Overlay> {
    HashMap::from([
        // Reads: trivially idempotent, never destructive.
        ("Taskei___list_tasks", Overlay {
            read_only: true,
            destructive: false,
            idempotent: true,
        }),
        ("Taskei___get_task", Overlay {
            read_only: true,
            destructive: false,
            idempotent: true,
        }),
        ("Taskei___get_room", Overlay {
            read_only: true,
            destructive: false,
            idempotent: true,
        }),
        ("Taskei___list_room_resource", Overlay {
            read_only: true,
            destructive: false,
            idempotent: true,
        }),
        // Writes: not destructive (creates new state, doesn't delete);
        // not idempotent at the Taskei layer (Phase 4b adds the DDB
        // dedup that makes Slack-thread-triggered creates safe).
        ("Taskei___create_task", Overlay {
            read_only: false,
            destructive: false,
            idempotent: false,
        }),
        // `Taskei___update_task` covers both field updates AND the
        // `addComment` sub-field. Comments aren't strictly destructive
        // and field updates *can* be (overwriting a description) — we
        // pin destructiveHint: false because:
        //   (a) the Phase-2 reaction-gate is keyed off `readOnlyHint`,
        //       not `destructiveHint`, so the policy outcome is the
        //       same either way (Ask + reaction approval);
        //   (b) the addComment path is the high-traffic Phase 3c
        //       use case and `destructiveHint: true` would mislabel
        //       it for any future client that *does* read the hint.
        // The plan-doc text (§381-382) does not pin destructiveHint for
        // `update_task` explicitly; the false choice is documented
        // here so a future change has a starting point.
        ("Taskei___update_task", Overlay {
            read_only: false,
            destructive: false,
            idempotent: false,
        }),
    ])
}

/// Stamp the Phase 1d annotation overlay onto the gateway's tool
/// catalog. Tools missing from the overlay (e.g.
/// `x_amz_bedrock_agentcore_search`, plus any new tool the gateway
/// adds before this overlay is updated) pass through with whatever
/// annotations the gateway returned — the schema-pin assertion is
/// what catches "new tool appeared, overlay needs updating."
///
/// We always *replace* the annotations object rather than merge field
/// by field, because the overlay represents the canonical local
/// contract per the plan. If the gateway starts returning a richer
/// annotation set we want to opt into, the overlay table is the place
/// to add it.
pub fn decorate_with_overlay(tools: Vec<Tool>) -> Vec<Tool> {
    let table = overlay_table();
    tools
        .into_iter()
        .map(|mut tool| {
            let name = tool.name.as_ref();
            match table.get(name) {
                Some(overlay) => {
                    let annotations = ToolAnnotations::new()
                        .read_only(overlay.read_only)
                        .destructive(overlay.destructive)
                        .idempotent(overlay.idempotent);
                    tool.annotations = Some(annotations);
                    tool
                },
                // Unknown tools — gateway helper or a new addition.
                // Pass through unchanged. The schema-pin fixture
                // captures what the gateway returns for them, so
                // drift is caught at deploy time (plan §312-313).
                None => tool,
            }
        })
        .collect()
}

/// Tools the overlay treats as read-only. Used by the
/// [`crate::families::taskei::server::TaskeiServer`] to route reads
/// through `families/taskei/read` (which only sees a `ReadOnlyView`)
/// vs. writes through `families/taskei/write` (which holds the full
/// `StsBridge`). Tools missing from this set are routed to the read
/// path by default — annotation-less helpers like
/// `x_amz_bedrock_agentcore_search` are read-only-shaped per Phase-0
/// observation. If a future tool needs the write path, add it to
/// `is_write_tool` below.
pub fn is_read_tool(name: &str) -> bool {
    matches!(
        name,
        "Taskei___list_tasks"
            | "Taskei___get_task"
            | "Taskei___get_room"
            | "Taskei___list_room_resource"
            | "x_amz_bedrock_agentcore_search"
    )
}

/// Tools the overlay treats as write-shaped (route through the STS
/// per-call write path). Mirror of [`is_read_tool`]; one of them is
/// always `true` for any tool we know about.
pub fn is_write_tool(name: &str) -> bool {
    matches!(name, "Taskei___create_task" | "Taskei___update_task")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rmcp::model::Tool;
    use serde_json::json;

    use super::*;

    fn raw_tool(name: &str) -> Tool {
        let schema = json!({ "type": "object" });
        Tool::new_with_raw(
            name.to_string(),
            None,
            Arc::new(serde_json::from_value(schema).unwrap()),
        )
    }

    fn ann_of(tool: &Tool) -> &ToolAnnotations {
        tool.annotations.as_ref().expect("annotations expected")
    }

    #[test]
    fn read_tools_get_read_only_true() {
        let decorated = decorate_with_overlay(vec![
            raw_tool("Taskei___list_tasks"),
            raw_tool("Taskei___get_task"),
            raw_tool("Taskei___get_room"),
            raw_tool("Taskei___list_room_resource"),
        ]);
        for tool in &decorated {
            let ann = ann_of(tool);
            assert_eq!(ann.read_only_hint, Some(true), "{} read_only", tool.name);
            assert_eq!(ann.destructive_hint, Some(false), "{} destructive", tool.name);
            assert_eq!(ann.idempotent_hint, Some(true), "{} idempotent", tool.name);
        }
    }

    #[test]
    fn write_tools_get_read_only_false() {
        let decorated = decorate_with_overlay(vec![raw_tool("Taskei___create_task"), raw_tool("Taskei___update_task")]);
        for tool in &decorated {
            let ann = ann_of(tool);
            assert_eq!(ann.read_only_hint, Some(false), "{} read_only", tool.name);
            assert_eq!(ann.destructive_hint, Some(false), "{} destructive", tool.name);
            assert_eq!(ann.idempotent_hint, Some(false), "{} idempotent", tool.name);
        }
    }

    #[test]
    fn helper_tool_passes_through_without_annotation() {
        // x_amz_bedrock_agentcore_search isn't in the overlay table
        // (plan §383 says annotation TBD per Phase-0 schema review).
        // It should pass through with whatever the gateway returned.
        let decorated = decorate_with_overlay(vec![raw_tool("x_amz_bedrock_agentcore_search")]);
        assert_eq!(decorated.len(), 1);
        assert!(decorated[0].annotations.is_none(), "helper tool should pass through");
    }

    #[test]
    fn unknown_new_tool_passes_through_so_schema_pin_can_catch_it() {
        // If the gateway ships `Taskei___new_tool` before the overlay
        // table is updated, we must NOT silently stamp it as
        // read-only-true. Pass-through means the schema-pin fixture
        // (separate concern) will catch the unknown name and refuse
        // boot.
        let decorated = decorate_with_overlay(vec![raw_tool("Taskei___gizmo")]);
        assert!(decorated[0].annotations.is_none());
    }

    #[test]
    fn read_and_write_classification_is_disjoint() {
        for name in [
            "Taskei___list_tasks",
            "Taskei___get_task",
            "Taskei___get_room",
            "Taskei___list_room_resource",
            "Taskei___create_task",
            "Taskei___update_task",
            "x_amz_bedrock_agentcore_search",
        ] {
            let r = is_read_tool(name);
            let w = is_write_tool(name);
            assert!(r ^ w, "{name} must be exactly one of read/write (read={r}, write={w})");
        }
    }
}
