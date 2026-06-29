use rmcp::model::ToolAnnotations as RmcpToolAnnotations;
use serde::{
    Deserialize,
    Serialize,
};

use crate::agent::agent_config::parse::CanonicalToolName;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    pub tool_name: String,
    pub server_name: String,
    /// Optional parameters to pass to the tool when invoking the method.
    pub params: Option<serde_json::Map<String, serde_json::Value>>,
    /// MCP tool annotations from `tools/list`. Populated when the
    /// agent's MCP catalog is built; surfaced to ACP clients via
    /// `RequestPermissionRequest._meta.mcpAnnotations` (Phase 2 of
    /// the kiro-bot Taskei integration). When ACP v2 grows a typed
    /// annotations field on `ToolCallUpdateFields`, this should
    /// move there — see `acp_agent::attach_mcp_annotations`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<McpToolAnnotations>,
}

impl McpTool {
    pub fn canonical_tool_name(&self) -> CanonicalToolName {
        CanonicalToolName::Mcp {
            server_name: self.server_name.clone(),
            tool_name: self.tool_name.clone(),
        }
    }
}

/// MCP tool behavior hints surfaced to ACP clients via `_meta.mcpAnnotations`.
///
/// Mirrors the subset of [`rmcp::model::ToolAnnotations`] that is
/// safety-relevant for approval-gating. The `title` field is intentionally
/// omitted — it is display-only and already covered by the tool's name.
///
/// All fields are MCP-spec hints (see the MCP `ToolAnnotations` schema): they
/// MUST NOT be trusted blindly from untrusted servers. Kiro-bot uses
/// `read_only_hint == Some(true)` only as a *gate-down* signal (allow
/// auto-approval); a missing or `Some(false)` hint always falls through to the
/// usual approval policy.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolAnnotations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
}

impl McpToolAnnotations {
    /// Returns `None` if every hint field is unset, so callers can elide the
    /// `_meta.mcpAnnotations` payload entirely when there's nothing to say.
    pub fn or_none(self) -> Option<Self> {
        if self.read_only_hint.is_none()
            && self.destructive_hint.is_none()
            && self.idempotent_hint.is_none()
            && self.open_world_hint.is_none()
        {
            None
        } else {
            Some(self)
        }
    }
}

impl From<&RmcpToolAnnotations> for McpToolAnnotations {
    fn from(value: &RmcpToolAnnotations) -> Self {
        Self {
            read_only_hint: value.read_only_hint,
            destructive_hint: value.destructive_hint,
            idempotent_hint: value.idempotent_hint,
            open_world_hint: value.open_world_hint,
        }
    }
}

impl From<RmcpToolAnnotations> for McpToolAnnotations {
    fn from(value: RmcpToolAnnotations) -> Self {
        Self::from(&value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn or_none_collapses_empty() {
        let a = McpToolAnnotations::default();
        assert!(a.or_none().is_none());
    }

    #[test]
    fn or_none_preserves_any_set_field() {
        let a = McpToolAnnotations {
            read_only_hint: Some(true),
            ..Default::default()
        };
        assert_eq!(
            a.or_none(),
            Some(McpToolAnnotations {
                read_only_hint: Some(true),
                ..Default::default()
            })
        );
    }

    #[test]
    fn from_rmcp_copies_hints() {
        let rmcp = RmcpToolAnnotations::from_raw(Some("ignored".into()), Some(true), Some(false), Some(true), None);
        let ours: McpToolAnnotations = (&rmcp).into();
        assert_eq!(ours.read_only_hint, Some(true));
        assert_eq!(ours.destructive_hint, Some(false));
        assert_eq!(ours.idempotent_hint, Some(true));
        assert_eq!(ours.open_world_hint, None);
    }

    #[test]
    fn camel_case_serde_roundtrip() {
        let a = McpToolAnnotations {
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            idempotent_hint: None,
            open_world_hint: Some(false),
        };
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("readOnlyHint"));
        assert!(json.contains("destructiveHint"));
        assert!(!json.contains("idempotentHint")); // skipped when None
        assert!(json.contains("openWorldHint"));
        let parsed: McpToolAnnotations = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, a);
    }

    #[test]
    fn mcp_tool_serde_omits_annotations_when_none() {
        let t = McpTool {
            tool_name: "Taskei___list_tasks".into(),
            server_name: "kiro-mcp".into(),
            params: None,
            annotations: None,
        };
        let json = serde_json::to_string(&t).unwrap();
        assert!(!json.contains("annotations"));
    }
}
