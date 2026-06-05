//! Canonical home for the kiro-help Slack bot's agent definition.
//!
//! These files don't ship in the public kiro-cli binary — they're packed into
//! the bot's container image at build time and dropped into `~/.kiro/agents/`
//! so chat-cli-v2's `load_agents()` picks them up by disk scan when the
//! container runs `kiro-cli acp --agent kiro-help`.
//!
//! This module exists primarily as a test harness: we pin the JSON's tool
//! wiring, the prompt's hard constraints, and the SKILL.md's per-turn
//! workflow. The prompt and skill have a single-source-of-truth split:
//! identity + tool inventory live in the prompt, the per-turn procedure
//! lives in the skill. If either drifts the bot regresses to ungrounded
//! answers.

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

    /// Slack bot is read-only by default; write capabilities live behind a
    /// reaction-approval gate. Assert the agent JSON does not expose write
    /// tools, does not auto-approve the GitHub write tools, and that the
    /// shell tool (when present) is locked down with denyByDefault + a
    /// scoped allowlist that excludes git writes and network commands.
    #[test]
    fn kiro_help_agent_json_keeps_writes_behind_approval() {
        let raw = include_str!("../agents/kiro-help.json");
        let v: serde_json::Value = serde_json::from_str(raw).expect("valid JSON");
        let tools = v["tools"].as_array().expect("array");
        for forbidden in ["fs_write"] {
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

        // If execute_bash is exposed, it must be locked down: denyByDefault
        // on, an explicit allowlist, and no write/network commands sneaking
        // in. The agent's shell-permission machinery enforces these — but
        // pin the config so a careless edit can't loosen the policy.
        if tools.iter().any(|t| t == "execute_bash") {
            // CRITICAL: execute_bash MUST NOT appear in allowedTools. When
            // a tool is in allowedTools, the shell-permission decider's
            // step 2 short-circuits with Allow before the denyByDefault +
            // allowedCommands checks ever run (see
            // crates/agent/src/agent/shell_permission/decider.rs:148-153).
            // The whole point of the toolsSettings.shell policy below is
            // to be enforced — it only is when is_tool_allowed = false,
            // i.e. when execute_bash is NOT auto-approved.
            assert!(
                !allowed.iter().any(|t| t == "execute_bash"),
                "execute_bash MUST NOT be in allowedTools — that bypasses the \
                 denyByDefault + allowedCommands enforcement in the shell \
                 permission decider (decider.rs step 2). Keep execute_bash in \
                 the `tools` array but NOT in `allowedTools`."
            );

            let shell = &v["toolsSettings"]["shell"];
            assert_eq!(
                shell["denyByDefault"], true,
                "shell must run with denyByDefault: true so unlisted commands are denied"
            );
            let allowlist: Vec<&str> = shell["allowedCommands"]
                .as_array()
                .expect("allowedCommands array")
                .iter()
                .filter_map(|p| p.as_str())
                .collect();
            assert!(
                !allowlist.is_empty(),
                "execute_bash with denyByDefault must declare a non-empty allowlist"
            );
            for forbidden_substr in [
                "git push",
                "git pull",
                "git fetch",
                "git commit",
                "git reset",
                "rm ",
                "curl ",
                "wget ",
                "sudo ",
                "bash -c",
                "sh -c",
            ] {
                assert!(
                    !allowlist.iter().any(|p| p.contains(forbidden_substr)),
                    "shell allowlist must not include `{forbidden_substr}` — that's a write/network command"
                );
            }
            let denylist: Vec<&str> = shell["deniedCommands"]
                .as_array()
                .expect("deniedCommands array")
                .iter()
                .filter_map(|p| p.as_str())
                .collect();
            assert!(
                !denylist.is_empty(),
                "shell denylist must explicitly block writes/network as defense-in-depth"
            );
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

    /// Every tool the prompt names must appear in the JSON's tools list.
    /// This is the contract that prevents the agent from being told about
    /// tools it cannot call.
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

        // Tools the prompt's "Tools you can call" section explicitly names.
        // For MCP-namespaced tools we strip the @server/ prefix in the prompt
        // for readability, so we check the unqualified name maps to one in
        // the JSON via suffix match.
        let prompt = include_str!("../agents/kiro_help_prompt.md");
        for named in [
            "search_kiro_knowledge",
            "search_github_issues",
            "create_github_issue",
            "comment_on_existing",
            "introspect",
            "read",
            "execute_bash",
        ] {
            assert!(
                prompt.contains(&format!("`{named}`")),
                "prompt should still introduce {named}"
            );
            assert!(
                tools.iter().any(|t| t == named || t.ends_with(&format!("/{named}"))),
                "prompt names `{named}` but it's not in kiro-help.json's tools: {tools:?}"
            );
        }
    }

    /// The prompt is the bot's identity + hard-constraint contract. It must
    /// retain retrieval-mandatory + citation rules and reference the skill
    /// for the per-turn procedure rather than restating it.
    #[test]
    fn kiro_help_prompt_carries_hard_constraints() {
        let prompt = include_str!("../agents/kiro_help_prompt.md");
        assert!(
            prompt.contains("Read the source for behavior questions"),
            "prompt must hard-require reading source for kiro-touching questions"
        );
        assert!(
            prompt.contains("Cite every non-trivial claim"),
            "prompt must require citations on retrieved answers"
        );
        assert!(
            prompt.contains("Never invent doc paths, file paths, issue numbers, or Taskei task IDs"),
            "prompt must keep the no-fabrication rule"
        );
        assert!(
            prompt.contains("kiro-help-workflow"),
            "prompt must reference the kiro-help-workflow skill so the agent loads it for the per-turn procedure"
        );
    }

    /// SKILL.md is the per-turn recipe and the single source of truth for
    /// the workflow. Pin its load-bearing pieces.
    #[test]
    fn kiro_help_skill_defines_workflow() {
        let skill = include_str!("../agents/SKILL.md");
        assert!(
            skill.starts_with("---\nname: kiro-help-workflow"),
            "skill must start with frontmatter declaring name = kiro-help-workflow"
        );
        for clause in ["search_kiro_knowledge", "search_github_issues", "Sources:"] {
            assert!(
                skill.contains(clause),
                "skill must keep the '{clause}' clause that defines the workflow"
            );
        }
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
        assert!(
            resources
                .iter()
                .any(|r| r.as_str().unwrap_or_default().starts_with("skill://")
                    && r.as_str().unwrap().ends_with("SKILL.md")),
            "kiro-help.json must declare a skill:// resource pointing at SKILL.md, got: {resources:?}"
        );
    }
}
