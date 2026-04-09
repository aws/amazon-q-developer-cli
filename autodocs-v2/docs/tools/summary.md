---
doc_meta:
  title: summary
  description: Subagent tool for reporting task results back to the main agent
  category: tool
  keywords: [summary, subagent, task, result, report, agent-crew, pipeline]
  related: [subagent]
  validated: 2026-04-08
  commit: 1a984cb0
  status: validated
  testable_headless: true
---

## Overview

The summary tool allows a subagent to report its task results back to the main agent that spawned it. When the main agent spawns work via the `subagent` tool, the subagent uses `summary` to send back its findings, context, and final result.

> This tool is used by the AI assistant to fulfill your requests. You don't invoke it directly - simply ask questions naturally.

This tool is only available to subagents — it is excluded from the main agent's tool set and always included for subagents.

## Usage

The tool accepts three parameters:

- `taskDescription` (required) — Description of the task that was assigned to the subagent
- `contextSummary` (optional) — Relevant context and information gathered during task execution that aids subsequent actions
- `taskResult` (required) — The final result or outcome of the completed task

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
- The subagent may have errored before calling summary
- Check that the subagent task completed successfully

### Tool not available

The summary tool is only available to subagents. If you see it missing:
- This is expected for the main agent — it cannot call summary on itself
- Only agents spawned via the `subagent` tool have access to this tool

## Related

- [subagent](subagent.md) — The tool that spawns subagents which use summary to report back
