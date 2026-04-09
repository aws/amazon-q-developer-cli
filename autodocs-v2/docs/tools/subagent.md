---
doc_meta:
  title: subagent
  description: Spawn and coordinate multiple AI agents in a pipeline (DAG) with dependency management
  category: tool
  keywords: [subagent, agent_crew, crew, pipeline, DAG, stages, parallel, blocking, use_subagent]
  related: [summary, session-management, agent-configuration]
  validated: 2026-04-08
  commit: 1a984cb0
  status: validated
  testable_headless: true
---

## Overview

> This tool is used by the AI assistant to fulfill your requests. You don't invoke it directly - simply ask questions naturally.

The subagent tool (also known as `agent_crew`) spawns and coordinates multiple AI agents in a pipeline (DAG). Each stage runs as a persistent session. Stages with no dependencies start immediately in parallel, while dependent stages wait for their prerequisites to complete.

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

## Troubleshooting

### "Agents not available for crew stages: X"

The stage's `role` doesn't match any entry in `availableAgents`. Add the agent name or a matching glob pattern.

### Pipeline seems stuck

Press `ctrl+g` to monitor stage progress. A stage may be waiting for tool approval or processing a large task.

### Stage doesn't start

Check that all stages listed in its `depends_on` have completed. Use `ctrl+g` to see which stages are still running.

## Related

- [summary](summary.md) — How subagents report results back
- [session-management](session-management.md) — Lower-level session orchestration
- [Agent Configuration](../features/agent-configuration.md) — Creating specialized agents for pipeline stages
