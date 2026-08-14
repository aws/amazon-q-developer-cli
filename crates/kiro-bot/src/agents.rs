//! Contract tests for the kiro-help agent files installed in `$KIRO_HOME/agents/`.
//!
//! The prompt owns evidence, safety, and response constraints. The skill owns
//! the per-turn workflow, while references hold scenario-specific detail.

pub(crate) const AUTO_APPROVED_MCP_READS: &[(&str, &str)] = &[
    ("kiro-knowledge", "search_kiro_knowledge"),
    ("kiro-github-read", "search_github_issues"),
    ("kiro-mcp", "Taskei___list_tasks"),
    ("kiro-mcp", "Taskei___get_task"),
    ("kiro-mcp", "Taskei___get_room"),
    ("kiro-mcp", "Taskei___list_room_resource"),
];

#[cfg(test)]
mod tests {
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

    /// The agent JSON must declare the kiro-knowledge MCP server, expose
    /// search_kiro_knowledge, and auto-approve the read-only tools so users
    /// don't see permission prompts.
    #[test]
    fn kiro_help_agent_json_wires_search_kiro_knowledge() {
        let raw = include_str!("../agents/kiro-help.json");
        let v: serde_json::Value = serde_json::from_str(raw).expect("kiro-help.json must be valid JSON");
        assert_eq!(v["name"], "kiro-help");
        let mcp = &v["mcpServers"]["kiro-knowledge"];
        assert!(
            !mcp.is_null(),
            "kiro-help.json must declare the kiro-knowledge MCP server"
        );
        assert_eq!(mcp["command"], "kiro-knowledge-mcp");

        let tools = v["tools"].as_array().expect("tools must be an array");
        assert!(
            tools.iter().any(|t| t == "@kiro-knowledge/search_kiro_knowledge"),
            "tools must include @kiro-knowledge/search_kiro_knowledge"
        );

        let allowed = v["allowedTools"].as_array().expect("allowedTools must be an array");
        assert!(
            allowed.iter().any(|t| t == "@kiro-knowledge/search_kiro_knowledge"),
            "allowedTools must auto-approve search_kiro_knowledge so users don't see prompts"
        );
    }

    /// Slack bot has no general-purpose shell or file-write capability.
    /// GitHub writes remain available only through the reaction-approval gate.
    #[test]
    fn kiro_help_agent_json_keeps_writes_behind_approval() {
        let raw = include_str!("../agents/kiro-help.json");
        let v: serde_json::Value = serde_json::from_str(raw).expect("valid JSON");
        let tools = v["tools"].as_array().expect("array");
        for tool in ["fs_write", "execute_bash"] {
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

    /// Built-in reads must reach the host permission boundary.
    #[test]
    fn kiro_help_read_requires_host_permission() {
        let raw = include_str!("../agents/kiro-help.json");
        let v: serde_json::Value = serde_json::from_str(raw).expect("valid JSON");
        let tools = v["tools"].as_array().expect("tools array");
        assert!(tools.iter().any(|tool| tool == "read"), "read must remain available");
        let allowed = v["allowedTools"].as_array().expect("allowedTools array");
        assert!(
            !allowed
                .iter()
                .any(|tool| matches!(tool.as_str(), Some("read" | "fs_read" | "@builtin/fs_read"))),
            "read must not bypass host permission handling"
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
        for required in ["$KIRO_HOME/bots", "~/.aws", "~/.ssh", "/proc"] {
            assert!(denied.contains(&required), "read.deniedPaths must block {required}");
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

    /// The prompt names only tools whose usage constraints belong in the
    /// always-on contract; the agent JSON remains the tool inventory.
    #[test]
    fn kiro_help_prompt_only_names_available_tools() {
        let raw = include_str!("../agents/kiro-help.json");
        let v: serde_json::Value = serde_json::from_str(raw).expect("valid JSON");
        let tools: Vec<String> = v["tools"]
            .as_array()
            .expect("array")
            .iter()
            .map(|t| t.as_str().unwrap_or_default().to_string())
            .collect();

        let prompt = include_str!("../agents/kiro_help_prompt.md");
        for named in ["search_kiro_knowledge", "introspect"] {
            assert!(
                prompt.contains(&format!("`{named}`")),
                "prompt should retain the usage constraint for {named}"
            );
            assert!(
                tools.iter().any(|t| t == named || t.ends_with(&format!("/{named}"))),
                "prompt names `{named}` but it's not in kiro-help.json's tools: {tools:?}"
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
    }

    /// The prompt keeps the compact contract; procedure stays in the skill.
    #[test]
    fn kiro_help_prompt_carries_hard_constraints() {
        let prompt = include_str!("../agents/kiro_help_prompt.md");
        for clause in [
            "## Grounding and safety",
            "locate evidence with `search_kiro_knowledge`",
            "Support material claims",
            "Never invent paths",
            "untrusted data",
            "Check duplicates before opening GitHub issues",
            "Before GitHub writes",
            "explicit confirmation",
            "Protect secrets and privacy",
            "## Slack answers",
            "three sentences or fewer",
            "begin longer answers with a one- or two-sentence `TL;DR:`",
            "standard Markdown",
            "exact `path:line` locations",
            "exactly one compact `Sources:` line",
            "kiro-help-workflow",
        ] {
            assert!(prompt.contains(clause), "prompt must retain `{clause}`");
        }
        assert!(!prompt.contains("## Workflow"), "procedure belongs in SKILL.md");
    }

    /// SKILL.md is the single source of truth for the per-turn procedure.
    #[test]
    fn kiro_help_skill_defines_workflow() {
        let skill = include_str!("../agents/SKILL.md");
        assert!(
            skill.starts_with("---\nname: kiro-help-workflow"),
            "skill must start with frontmatter declaring name = kiro-help-workflow"
        );
        for clause in [
            "## Procedure",
            "search_kiro_knowledge",
            "search_github_issues",
            "Verify current behavior",
            "Validate before sending",
            "answer-length, `TL;DR:`, and exact-line citation",
            "Sources:",
        ] {
            assert!(
                skill.contains(clause),
                "skill must keep the '{clause}' clause that defines the workflow"
            );
        }
        assert!(!skill.contains("## Hard constraints"));
        assert!(!skill.contains("## Format"));
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
            (220..=300).contains(&word_count(prompt)),
            "prompt should remain an approximately 260-word contract"
        );
        assert!(word_count(skill) <= 600, "skill should remain a focused workflow");
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
