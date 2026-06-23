---
doc_meta:
  title: subagent
  description: Spawn and coordinate multiple AI agents in a pipeline (DAG) with dependency management
  category: tool
  keywords: [subagent, agent_crew, crew, pipeline, DAG, stages, parallel, blocking, use_subagent, trust, turn-limit, fail-fast]
  related: [summary, session-management, agent-configuration]
  validated: 2026-06-22
  commit: eb5a9bbac
  status: validated
  testable_headless: true
---

## Overview

> This tool is used by the AI assistant to fulfill your requests. You don't invoke it directly - simply ask questions naturally.

The subagent tool (also known as `agent_crew`) spawns and coordinates multiple AI agents in a pipeline (DAG). Each stage runs as a persistent session. Stages with no dependencies start immediately in parallel, while dependent stages wait for their prerequisites to complete.

**Naming**: This tool is called `subagent` (canonical), with `agent_crew` and `use_subagent` as legacy aliases. All names work in agent configs.

Use this when you need multi-step work with specialized agents:
- Research → Implement → Review pipelines
- Parallel research tracks that feed into a single implementer
- Any workflow where stages have dependencies

Each stage becomes a session you can monitor via `ctrl+g` in the TUI.

## Usage

### Parameters

- `task` (string, required) — Overall task description
- `mode` (string, optional) — Execution mode. Currently only `blocking` (wait for all stages to complete). Default: `blocking`
- `stages` (array, required) — Pipeline stages, each with:
  - `name` (string, required) — Unique stage name
  - `role` (string, required) — Agent config name to use for this stage
  - `prompt_template` (string, required) — Task for this stage. Use `{task}` to reference the overall task
  - `depends_on` (array of strings, optional) — Names of stages that must complete before this one starts
  - `model` (string, optional) — Override the model for this stage

### How Dependencies Work

- Stages with no `depends_on` (or empty array) start immediately in parallel
- Stages with `depends_on` wait until all named stages complete before starting
- This forms a DAG (directed acyclic graph) — no circular dependencies allowed
- **Fail-fast**: If any stage fails, all still-running sibling stages in the group are cancelled and the error is immediately returned to the parent agent. The pipeline does not wait for remaining stages to finish.

## Examples

### Simple parallel research

```json
{
  "task": "Compare testing frameworks for our Node.js project",
  "stages": [
    {"name": "jest-research", "role": "research-agent", "prompt_template": "Research Jest for {task}"},
    {"name": "vitest-research", "role": "research-agent", "prompt_template": "Research Vitest for {task}"},
    {"name": "mocha-research", "role": "research-agent", "prompt_template": "Research Mocha for {task}"}
  ]
}
```

All three stages run in parallel since none have dependencies.

### Pipeline with dependencies

```json
{
  "task": "Add CSV export to the reports page",
  "stages": [
    {"name": "research", "role": "research-agent", "prompt_template": "Gather requirements for {task}"},
    {"name": "implement", "role": "code-agent", "prompt_template": "Implement {task}", "depends_on": ["research"]},
    {"name": "review", "role": "review-agent", "prompt_template": "Review the implementation of {task}", "depends_on": ["implement"]}
  ]
}
```

`research` starts immediately → when done, `implement` starts → when done, `review` starts.

### Fan-out / fan-in pattern

```json
{
  "task": "Audit the authentication module",
  "stages": [
    {"name": "security-scan", "role": "security-agent", "prompt_template": "Scan for vulnerabilities in {task}"},
    {"name": "perf-analysis", "role": "perf-agent", "prompt_template": "Analyze performance of {task}"},
    {"name": "report", "role": "report-agent", "prompt_template": "Compile findings for {task}", "depends_on": ["security-scan", "perf-analysis"]}
  ]
}
```

`security-scan` and `perf-analysis` run in parallel → both must complete before `report` starts.

## Configuration

Control which agents can be used as stages via `toolsSettings` in your agent configuration:

```json
{
  "toolsSettings": {
    "crew": {
      "availableAgents": ["research-agent", "code-agent", "test-*"],
      "trustedAgents": ["research-agent"]
    }
  }
}
```

- `availableAgents` (array, optional) — Controls which agents can be used as stage roles. Supports exact names and glob patterns. If empty, all agents are available.
- `trustedAgents` (array, optional) — Agents that are auto-approved without user confirmation. Supports glob patterns.

The config key `agent_crew` is also accepted as an alias for `crew`.

## FAQ

### Do subagents inherit trust from the parent agent?

No. Subagents use their own agent configuration's `allowedTools` for tool permissions. The parent agent's trust settings do not transfer. To auto-approve tools for subagents:
- Add tools to the subagent's agent config `allowedTools`
- Or add the agent to `trustedAgents` in the parent's crew settings (this trusts the agent spawn, not its tools)

### What is the turn limit for subagents?

Subagents have a default turn limit to prevent runaway execution. The limit is not currently user-configurable. If a subagent hits the limit, it will complete with whatever progress it made.

### Can subagents spawn sub-subagents?

No. Subagents cannot spawn additional subagents. The crew tool is only available to the parent agent. This prevents unbounded recursion.

### Do subagent sessions persist after the task completes?

No. Subagent sessions terminate when their task completes. The results are returned to the parent agent via the summary tool. You cannot resume a subagent session later.

### What happens when a stage fails?

The pipeline uses fail-fast semantics. If any stage encounters an error, all sibling stages still running in the same group are immediately cancelled, and the error is reported back to the parent agent. This prevents wasted work and ensures the parent can retry or adjust its approach without waiting for other stages to finish.

### What happens if a subagent returns an empty response?

Empty responses are handled gracefully. If a subagent produces an empty response (e.g., due to a model issue), the pipeline degrades to the subagent's last available message instead of treating it as a stage failure. This avoids failing the entire crew for transient issues.

### Why do I see MCP errors after a subagent completes?

When a subagent session terminates, its MCP server connections close. If the parent agent tries to reference MCP state from the subagent, you may see connection errors. This is expected — each subagent has its own isolated MCP connections that don't outlive the session.

## Troubleshooting

### "Agents not available for crew stages: X"

The stage's `role` doesn't match any entry in `availableAgents`. Add the agent name or a matching glob pattern.

### Pipeline seems stuck

Press `ctrl+g` to monitor stage progress. A stage may be waiting for tool approval or processing a large task. If a stage has failed, the pipeline will now fail fast and report the error immediately — you should not see indefinite hangs due to stage failures.

### Stage doesn't start

Check that all stages listed in its `depends_on` have completed. Use `ctrl+g` to see which stages are still running.

### Subagent requires tool approval

Subagents prompt for tool approval based on their own agent config. To avoid prompts:
- Add tools to the subagent's `allowedTools` in its agent configuration
- Or run the parent with `--trust-all-tools` (trusts all tools for all agents)

### MCP tools not available in subagent

Each subagent loads MCP servers from its own agent configuration. If a subagent needs MCP tools, add the `mcpServers` section to that agent's config file.

## Related

- [summary](summary.md) — How subagents report results back
- [session-management](session-management.md) — Lower-level session orchestration
- [Agent Configuration](../features/agent-configuration.md) — Creating specialized agents for pipeline stages
