---
doc_meta:
  title: summary
  description: Mandatory subagent tool for reporting task results back to the main agent
  category: tool
  keywords: [summary, subagent, task, result, report, agent-crew, pipeline, mandatory]
  related: [subagent]
  validated: 2026-06-25
  commit: 84fded8f7
  status: validated
  testable_headless: true
---

## Overview

The summary tool is the **mandatory** mechanism for a subagent to deliver task results back to the main agent that spawned it. Subagents must always call this tool before ending their turn — ending with a plain text response instead of calling summary will fail to deliver results to the parent agent.

> This tool is used by the AI assistant to fulfill your requests. You don't invoke it directly - simply ask questions naturally.

This tool is only available to subagents — it is excluded from the main agent's tool set and always included for subagents. It is prioritized first in the subagent's tool list to maximize the model's attention on calling it.

## Usage

The tool accepts three parameters:

- `taskDescription` (required) — Description of the task that was assigned to the subagent
- `contextSummary` (optional) — Relevant context and information gathered during task execution that aids subsequent actions
- `taskResult` (required) — The final result or outcome of the completed task

### Calling behavior

The summary tool **must** be called before the subagent's turn ends. This is enforced through:

1. **Positional priority** — The summary tool is placed first in the subagent's tool list, ahead of even the code tool, to maximize model attention on it.
2. **Mandatory language in the tool description** — The tool description explicitly states the subagent must call it before ending.
3. **Embedded instructions** — The subagent's system prompt reinforces the requirement to call summary rather than ending with plain text.

If the subagent ends without calling summary, its results are not delivered to the parent agent.

## Examples

### Basic task completion

```
Ask: "Search the codebase for all TODO comments and summarize them"
```

The main agent spawns a subagent, which searches and then calls summary:

```json
{
  "taskDescription": "Search codebase for all TODO comments",
  "taskResult": "Found 12 TODO comments across 8 files. Most are in src/auth/ (5) and src/api/ (4)."
}
```

### With context summary

```json
{
  "taskDescription": "Analyze test coverage for the auth module",
  "contextSummary": "The auth module has 15 public functions across 3 files. Test files exist for login.rs and token.rs but not for session.rs.",
  "taskResult": "Test coverage is approximately 67%. session.rs has no tests and contains 5 public functions that need coverage."
}
```

### Pipeline stage result

When used in a multi-stage pipeline via the `subagent` tool:

```json
{
  "taskDescription": "Stage 1: Gather requirements from issue #42",
  "contextSummary": "Issue requests adding CSV export to the reports page. Acceptance criteria: support filtering by date range, include all report columns.",
  "taskResult": "Requirements gathered. Ready for implementation stage."
}
```

## Troubleshooting

### Summary not received by main agent

The summary tool emits an `AgentEvent::SubagentSummary` event. If the main agent doesn't receive results:
- The subagent may have ended its turn without calling summary (ended with plain text instead)
- The subagent may have errored before reaching the summary call
- Check that the subagent task completed successfully

### Subagent ended without calling summary

This can happen when the model produces a text response instead of a tool call. The positional prioritization and mandatory language in the tool description are designed to minimize this. If it occurs consistently:
- The task prompt may be ambiguous — make it clearer that a concrete result is expected
- The subagent may have hit a context limit before completing

### Tool not available

The summary tool is only available to subagents. If you see it missing:
- This is expected for the main agent — it cannot call summary on itself
- Only agents spawned via the `subagent` tool or orchestrated sessions have access to this tool

## Related

- [subagent](subagent.md) — The tool that spawns subagents which use summary to report back
