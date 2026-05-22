//! Canonical home for the kiro-help Slack bot's agent definition.
//!
//! These files don't ship in the public kiro-cli binary — they're packed into
//! the bot's container image at build time and dropped into `~/.kiro/agents/`
//! so chat-cli-v2's `load_agents()` picks them up by disk scan when the
//! container runs `kiro-cli acp --agent kiro-help`.
//!
//! This module exists primarily as a test harness: we pin the JSON's tool
//! wiring, the prompt's MUST-language, and the SKILL.md's per-turn workflow.
//! If any of these drift the bot will silently regress to ungrounded answers.

#[cfg(test)]
mod tests {
    /// The agent JSON must declare the kiro-knowledge MCP server, expose
    /// search_kiro_knowledge, and auto-approve the read-only tools so users
    /// don't see permission prompts.
    #[test]
    fn kiro_help_agent_json_wires_search_kiro_knowledge() {
        let raw = include_str!("../agents/kiro-help.json");
        let v: serde_json::Value = serde_json::from_str(raw).expect("kiro-help.json must be valid JSON");
        assert_eq!(v["name"], "kiro-help");
        let mcp = &v["mcpServers"]["kiro-knowledge"];
        assert!(!mcp.is_null(), "kiro-help.json must declare the kiro-knowledge MCP server");
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

    /// Slack bot is read-only by default; write capabilities live behind a
    /// reaction-approval gate. Assert the agent JSON does not auto-approve
    /// write tools and does not expose execute_bash/fs_write/shell.
    #[test]
    fn kiro_help_agent_json_keeps_writes_behind_approval() {
        let raw = include_str!("../agents/kiro-help.json");
        let v: serde_json::Value = serde_json::from_str(raw).expect("valid JSON");
        let tools = v["tools"].as_array().expect("array");
        for forbidden in ["fs_write", "execute_bash", "shell"] {
            assert!(
                !tools.iter().any(|t| t == forbidden),
                "bot agent must not expose write tool {forbidden}"
            );
        }
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

    /// The prompt is the contract: MUST-language for retrieval, GitHub issue
    /// fallback for bug reports, and citation requirement.
    #[test]
    fn kiro_help_prompt_mandates_retrieval_and_citations() {
        let prompt = include_str!("../agents/kiro_help_prompt.md");
        assert!(
            prompt.contains("MUST be `search_kiro_knowledge`")
                || prompt.contains("MUST call `search_kiro_knowledge`"),
            "prompt must hard-require search_kiro_knowledge as the first action"
        );
        assert!(
            prompt.contains("search_github_issues"),
            "prompt must instruct the agent to also call search_github_issues for issue/bug queries"
        );
        assert!(
            prompt.contains("citation") || prompt.contains("Sources:") || prompt.contains("Cite "),
            "prompt must require citations on retrieved answers"
        );
    }

    /// The worked example anchors the model on a concrete output shape.
    /// Removing it craters retrieval reliability — keep it pinned.
    #[test]
    fn kiro_help_prompt_carries_worked_example() {
        let prompt = include_str!("../agents/kiro_help_prompt.md");
        assert!(
            prompt.contains("Worked example"),
            "prompt must include a worked example showing the expected output shape"
        );
        assert!(
            prompt.contains("Sources: `autodocs/docs/slash-commands/model.md`"),
            "worked example must show a concrete Sources: line so the model has a copy-this-shape target"
        );
        assert!(
            prompt.contains("STOP, call `search_kiro_knowledge`, and rewrite"),
            "prompt must keep the violation-recovery clause that tells the model what to do when it caught itself skipping"
        );
    }

    /// SKILL.md is the per-turn recipe. Pin the workflow's load-bearing steps.
    #[test]
    fn kiro_help_skill_defines_workflow() {
        let skill = include_str!("../agents/SKILL.md");
        assert!(
            skill.starts_with("---\nname: kiro-help-workflow"),
            "skill must start with frontmatter declaring name = kiro-help-workflow"
        );
        for clause in [
            "search_kiro_knowledge",
            "search_github_issues",
            "Sources:",
        ] {
            assert!(
                skill.contains(clause),
                "skill must keep the '{clause}' clause that defines the workflow"
            );
        }
    }
}
