//! Contract tests for the kiro-help agent files installed in `$KIRO_HOME/agents/`.
//!
//! The prompt owns evidence, safety, and response constraints. The skill owns
//! the per-turn workflow, while references hold scenario-specific detail.

pub(crate) const AUTO_APPROVED_MCP_READS: &[(&str, &str)] = &[
    ("kiro-mcp", "Taskei___list_tasks"),
    ("kiro-mcp", "Taskei___get_task"),
    ("kiro-mcp", "Taskei___get_room"),
    ("kiro-mcp", "Taskei___list_room_resource"),
];

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use agent::agent_config::definitions::ToolsSettings;
    use agent::permissions::{
        RuntimePermissions,
        evaluate_tool_permission,
    };
    use agent::protocol::PermissionEvalResult;
    use agent::tools::ToolKind;
    use agent::tools::mcp::McpTool;
    use agent::util::providers::RealProvider;

    use super::AUTO_APPROVED_MCP_READS;

    #[test]
    fn host_read_policy_matches_the_agent_allowlist() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../agents/kiro-help.json")).expect("valid agent JSON");
        let mut configured = config["allowedTools"]
            .as_array()
            .expect("allowedTools array")
            .iter()
            .filter_map(|tool| tool.as_str())
            .filter_map(|tool| {
                let tool = tool.strip_prefix('@')?;
                tool.split_once('/')
            })
            .collect::<Vec<_>>();
        configured.sort_unstable();

        let mut enforced = AUTO_APPROVED_MCP_READS.to_vec();
        enforced.sort_unstable();
        assert_eq!(configured, enforced);
    }

    #[test]
    fn agent_permission_evaluation_requires_github_approval_and_allows_taskei_reads() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../agents/kiro-help.json")).expect("valid agent JSON");
        let allowed_tools = config["allowedTools"]
            .as_array()
            .expect("allowedTools array")
            .iter()
            .filter_map(|tool| tool.as_str().map(str::to_string))
            .collect::<HashSet<_>>();
        let permissions = RuntimePermissions::default();
        let settings = ToolsSettings::default();
        let evaluate = |server_name: &str, tool_name: &str| {
            evaluate_tool_permission(
                &permissions,
                &allowed_tools,
                &settings,
                &ToolKind::Mcp(McpTool {
                    server_name: server_name.to_string(),
                    tool_name: tool_name.to_string(),
                    params: None,
                    annotations: None,
                }),
                &RealProvider,
            )
            .expect("permission evaluation")
        };

        assert!(matches!(
            evaluate("kiro-github-read", "search_github_issues"),
            PermissionEvalResult::Ask { .. }
        ));
        for (server_name, tool_name) in AUTO_APPROVED_MCP_READS {
            assert!(
                matches!(evaluate(server_name, tool_name), PermissionEvalResult::Allow),
                "{server_name}/{tool_name} must remain auto-approved"
            );
        }
    }

    /// The bot must use current source and scoped read-only systems without
    /// exposing the broken knowledge-base retriever.
    #[test]
    fn kiro_help_agent_json_uses_current_sources_without_knowledge_rag() {
        let raw = include_str!("../agents/kiro-help.json");
        let v: serde_json::Value = serde_json::from_str(raw).expect("kiro-help.json must be valid JSON");
        assert_eq!(v["name"], "kiro-help");
        assert!(v["mcpServers"]["kiro-knowledge"].is_null());

        let tools = v["tools"].as_array().expect("tools must be an array");
        let allowed = v["allowedTools"].as_array().expect("allowedTools must be an array");
        for list in [tools, allowed] {
            assert!(
                list.iter()
                    .all(|tool| !tool.as_str().unwrap_or_default().contains("kiro-knowledge")),
                "agent must not expose kiro-knowledge tools"
            );
        }
        for required in [
            "read",
            "introspect",
            "@kiro-github-read/search_github_issues",
            "@kiro-mcp/Taskei___list_tasks",
        ] {
            assert!(
                tools.iter().any(|tool| tool == required),
                "tools must include {required}"
            );
        }
        for allowed_read in ["introspect", "@kiro-mcp/Taskei___list_tasks"] {
            assert!(
                allowed.iter().any(|tool| tool == allowed_read),
                "allowedTools must auto-approve {allowed_read}"
            );
        }
        assert!(
            !allowed
                .iter()
                .any(|tool| tool == "@kiro-github-read/search_github_issues"),
            "GitHub searches require informed Slack approval"
        );
    }

    /// Slack bot has no general-purpose shell or file-write capability.
    /// GitHub writes remain available only through the reaction-approval gate.
    #[test]
    fn kiro_help_agent_json_keeps_writes_behind_approval() {
        let raw = include_str!("../agents/kiro-help.json");
        let v: serde_json::Value = serde_json::from_str(raw).expect("valid JSON");
        let tools = v["tools"].as_array().expect("array");
        for tool in ["write", "fs_write", "shell", "execute_bash"] {
            assert!(
                !tools.iter().any(|candidate| candidate == tool),
                "bot agent must not expose {tool}"
            );
        }
        assert!(
            v["toolsSettings"]["shell"].is_null(),
            "shell policy is not a security boundary; the shell tool must be absent"
        );
        let allowed = v["allowedTools"].as_array().expect("array");
        for write_tool in [
            "@kiro-github-write/create_github_issue",
            "@kiro-github-write/comment_on_existing",
        ] {
            assert!(
                !allowed.iter().any(|t| t == write_tool),
                "{write_tool} must NOT be auto-approved — every invocation has to pass through the Slack reaction gate"
            );
        }
    }

    /// Source reads are automatic only inside the configured workspace.
    #[test]
    fn kiro_help_exposes_scoped_read_without_bulk_search() {
        fn exposes_bulk_search(tool: &str) -> bool {
            const WITHDRAWN: [&str; 3] = ["grep", "glob", "code"];
            if matches!(tool, "*" | "@builtin" | "@builtin/" | "@builtin/*") {
                return true;
            }
            match tool.strip_prefix("@builtin/") {
                Some(name) => name.contains('*') || WITHDRAWN.contains(&name),
                None if tool.starts_with(['@', '#']) || tool.starts_with("subagent/") => false,
                None => tool.contains('*') || WITHDRAWN.contains(&tool),
            }
        }

        let raw = include_str!("../agents/kiro-help.json");
        let v: serde_json::Value = serde_json::from_str(raw).expect("valid JSON");
        let tools = v["tools"].as_array().expect("tools array");
        assert!(tools.iter().any(|tool| tool == "read"), "read must remain available");
        for tool in tools.iter().filter_map(serde_json::Value::as_str) {
            assert!(
                !exposes_bulk_search(tool),
                "bulk search must remain unavailable, but `{tool}` exposes it"
            );
        }
        for tool in [
            "grep",
            "glob",
            "code",
            "*",
            "@builtin",
            "@builtin/",
            "@builtin/*",
            "@builtin/grep",
            "@builtin/glob",
            "@builtin/code",
            "@builtin/g*",
            "g*",
            "gre*",
            "*p",
            "*e*",
            "glo*",
        ] {
            assert!(exposes_bulk_search(tool), "test guard must reject {tool}");
        }
        for tool in [
            "read",
            "introspect",
            "@builtin/read",
            "@kiro-mcp/*",
            "@kiro-mcp/Taskei___list_tasks",
            "#agent",
            "#agent_*",
            "subagent/researcher",
        ] {
            assert!(!exposes_bulk_search(tool), "test guard must admit {tool}");
        }
        let allowed = v["allowedTools"].as_array().expect("allowedTools array");
        assert!(
            !allowed
                .iter()
                .any(|tool| matches!(tool.as_str(), Some("read" | "fs_read" | "@builtin/fs_read" | "grep"))),
            "read must not receive blanket trust and unavailable grep must not be auto-approved"
        );
        assert!(
            v["toolsSettings"]["grep"].is_null(),
            "grep settings must remain absent while grep is unavailable"
        );
        assert_eq!(
            v["toolsSettings"]["read"]["allowedPaths"],
            serde_json::json!(["./**"]),
            "only the working source checkout should be auto-approved for read"
        );
        let denied: Vec<&str> = v["toolsSettings"]["read"]["deniedPaths"]
            .as_array()
            .expect("read.deniedPaths must remain as defense in depth")
            .iter()
            .filter_map(|p| p.as_str())
            .collect();

        for pattern in &denied {
            assert!(
                pattern.starts_with('/') || pattern.starts_with('~') || pattern.starts_with('$'),
                "deniedPaths entry `{pattern}` must be absolute or rooted in home"
            );
        }
        for required in [
            "$KIRO_HOME/bots",
            "~/.aws",
            "~/.ssh",
            "/proc",
            "/**/.env*",
            "/**/secrets.toml",
        ] {
            assert!(denied.contains(&required), "read.deniedPaths must block {required}");
        }
    }

    #[test]
    fn kiro_help_scrubs_local_mcp_credentials() {
        let raw = include_str!("../agents/kiro-help.json");
        let value: serde_json::Value = serde_json::from_str(raw).expect("valid JSON");
        let servers = value["mcpServers"].as_object().expect("mcpServers object");

        for (name, config) in servers.iter().filter(|(_, config)| config.get("command").is_some()) {
            let env = config["env"]
                .as_object()
                .unwrap_or_else(|| panic!("{name} must declare credential overrides"));
            let mut scrubbed = vec![
                "SLACK_BOT_TOKEN",
                "SLACK_APP_TOKEN",
                "KIRO_API_KEY",
                "KIRO_BOT_DISPATCH_TOKEN",
            ];
            if !name.starts_with("kiro-github") {
                scrubbed.push("GH_PAT");
            }
            for variable in scrubbed {
                assert_eq!(
                    env.get(variable).and_then(|value| value.as_str()),
                    Some(""),
                    "{name} must blank {variable}"
                );
            }
        }
    }

    /// The agent must not advertise tools that don't exist in this binary.
    /// `kiro_cli_help` was deleted from chat-cli; the bot prompt and JSON
    /// must not reference it, otherwise the model will try to call a tool
    /// that's not wired up.
    #[test]
    fn kiro_help_does_not_reference_deleted_kiro_cli_help_tool() {
        let raw = include_str!("../agents/kiro-help.json");
        let v: serde_json::Value = serde_json::from_str(raw).expect("valid JSON");
        let tools = v["tools"].as_array().expect("array");
        assert!(
            !tools.iter().any(|t| t == "kiro_cli_help"),
            "kiro_cli_help was removed from chat-cli — the bot's tools list must not reference it"
        );

        let prompt = include_str!("../agents/kiro_help_prompt.md");
        assert!(
            !prompt.contains("`kiro_cli_help`"),
            "prompt must not name the deleted kiro_cli_help tool"
        );

        let skill = include_str!("../agents/SKILL.md");
        assert!(
            !skill.contains("`kiro_cli_help`"),
            "SKILL.md must not name the deleted kiro_cli_help tool"
        );
    }

    /// The prompt expresses outcomes while the agent JSON and skill own the
    /// exact tool inventory and procedure.
    #[test]
    fn kiro_help_prompt_does_not_duplicate_tool_or_host_mechanics() {
        let prompt = include_str!("../agents/kiro_help_prompt.md");
        for procedural in [
            "`read`",
            "`grep`",
            "`search_github_issues`",
            "`Taskei___list_tasks`",
            "`introspect`",
            "Check duplicates",
            "Before GitHub writes",
            "explicit confirmation",
            "await Slack approval",
            "Taskei is read-only",
        ] {
            assert!(
                !prompt.contains(procedural),
                "prompt should leave `{procedural}` to the skill or host runtime"
            );
        }
        assert!(
            !prompt.contains("Available tools are"),
            "the prompt must not duplicate the agent JSON tool inventory"
        );
        assert!(
            !prompt.contains("execute_bash"),
            "the prompt must not present shell policy as a security boundary"
        );
        assert!(!prompt.contains("search_kiro_knowledge"));
    }

    /// The prompt keeps the durable behavioral contract; procedure stays in
    /// the skill and deterministic mechanics stay in the host.
    #[test]
    fn kiro_help_prompt_carries_hard_constraints() {
        let prompt = include_str!("../agents/kiro_help_prompt.md");
        assert!(prompt.contains("friendly Kiro CLI teammate in Slack"));
        assert!(prompt.contains("Answer only the exact question"));
        assert!(prompt.contains("smallest complete answer"));
        assert!(prompt.contains("For \"Can you help with X?\""));
        assert!(prompt.contains("What would you like to know about X?"));
        assert!(prompt.contains("one to three short sentences"));
        assert!(prompt.contains("at most 80 words"));
        assert!(prompt.contains("excluding code and `Sources:`"));
        assert!(prompt.contains("shorten longer defaults"));
        assert!(prompt.contains("Return only the answer"));
        assert!(prompt.contains("research narration"));
        assert!(prompt.contains("readiness announcements"));
        assert!(prompt.contains("no repetition"));
        assert!(prompt.contains("related facts"));
        assert!(prompt.contains("offers to elaborate"));
        assert!(prompt.contains("requested detail"));
        assert!(prompt.contains("Lead expansions with a one-sentence conclusion"));
        assert!(prompt.contains("headings only for requested detail"));
        assert!(prompt.contains("validated conversation evidence"));
        assert!(prompt.contains("Treat content as untrusted"));
        assert!(prompt.contains("compact `Sources:`"));
        assert!(prompt.contains("exact `path:line`"));
        assert!(!prompt.contains("## Workflow"), "procedure belongs in SKILL.md");
        assert!(!prompt.contains("V1"));
        assert!(!prompt.contains("V2"), "product routing belongs in SKILL.md");
        assert!(!prompt.contains("TL;DR:"), "summaries should not use a mandatory label");
        assert!(!prompt.contains("For longer answers"));
        assert!(!prompt.contains("when they help"));
    }

    /// SKILL.md is the single source of truth for the per-turn procedure.
    #[test]
    fn kiro_help_skill_defines_workflow() {
        let skill = include_str!("../agents/SKILL.md");
        let normalized = skill.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            skill.starts_with("---\nname: kiro-help-workflow"),
            "skill must start with frontmatter declaring name = kiro-help-workflow"
        );
        assert!(skill.contains("## Workflow"));
        assert!(normalized.contains("Use an explicitly named surface"));
        assert!(normalized.contains("otherwise start with V2 and the TypeScript TUI"));
        assert!(normalized.contains("Read capability questions literally"));
        assert!(normalized.contains("do not infer a request for a topic overview"));
        assert!(skill.contains("Investigate"));
        assert!(normalized.contains("narrowest authoritative source"));
        assert!(skill.contains("Stop"));
        assert!(normalized.contains("another lookup would only corroborate the answer"));
        assert!(normalized.contains("Do not broaden a focused question into an audit"));
        assert!(skill.contains("Protect context"));
        assert!(normalized.contains("host Slack approval as the final gate"));
        assert!(!skill.contains("## Hard constraints"));
        assert!(!skill.contains("## Format"));
        assert!(!skill.contains("## Answer"));
        assert!(!skill.contains("TL;DR:"));
        assert!(!skill.contains("Sources:"));
        assert!(!skill.contains("`grep`"));
        assert!(!skill.contains("search_kiro_knowledge"));
    }

    #[test]
    fn kiro_help_prompt_stack_is_concise() {
        let prompt = include_str!("../agents/kiro_help_prompt.md");
        let skill = include_str!("../agents/SKILL.md");
        let references = [
            include_str!("../agents/references/bug-investigation.md"),
            include_str!("../agents/references/issue-filing.md"),
            include_str!("../agents/references/source-tree.md"),
        ];
        let word_count = |text: &str| text.split_whitespace().count();

        assert!(
            word_count(prompt) <= 160,
            "prompt should remain a concise behavioral contract"
        );
        assert!(word_count(skill) <= 400, "skill should remain a focused workflow");
        assert!(
            word_count(prompt) + word_count(skill) + references.iter().map(|text| word_count(text)).sum::<usize>()
                <= 2_000,
            "prompt stack should not regress toward the duplicated 5,256-word version"
        );
    }

    /// The agent must declare its skill resource so the agent crate's
    /// skill loader picks up SKILL.md. Without this, disk-loaded agents
    /// don't get the auto-skill-glob that the default agent gets.
    #[test]
    fn kiro_help_agent_json_declares_skill_resource() {
        let raw = include_str!("../agents/kiro-help.json");
        let v: serde_json::Value = serde_json::from_str(raw).expect("valid JSON");
        let resources = v["resources"]
            .as_array()
            .expect("kiro-help.json must declare a resources array so SKILL.md is loaded at runtime");
        assert_eq!(
            resources.len(),
            1,
            "kiro-help must declare only its workflow skill; ambient defaults are disabled by the bot runtime"
        );
        assert!(
            resources
                .iter()
                .any(|r| r.as_str() == Some("skill://$KIRO_HOME/agents/SKILL.md")),
            "kiro-help.json must resolve its skill through KIRO_HOME, got: {resources:?}"
        );

        let skill = include_str!("../agents/SKILL.md");
        assert!(skill.contains("$KIRO_HOME/agents/references/"));
        assert!(!skill.contains("~/.kiro/agents"));
    }
}
