# Implementation Tasks: Spec-Mode Support in Kiro CLI TUI

## Overview

This task list implements spec-mode support in the Kiro CLI TUI (V2, KAS engine), enabling users to create, browse, resume, and execute specs entirely from the terminal via the `/spec` slash command and ACP integration with the KAS agent server.

---

## Task 1: Mode Change Propagation

**Validates: Requirements 1.1, 1.2, 1.3, 1.4**

- [x] 1.1 Handle `current_mode_update` in `KasAcpClient.wireSessionListeners` — update cache + broadcast `AgentSwitched`
- [x] 1.2 Guard against spurious broadcasts when mode is unchanged
- [x] 1.3 Include `previousAgentName` and `welcomeMessage` in the broadcast
- [x] 1.4 Add unit tests for the broadcast behavior
- [x] 1.5 Update base-class comment to clarify interception

---

## Task 2: Conditional `/spec` Command Registration

**Validates: Requirements 2.1, 2.2**

- [x] 2.1 Remove `/spec` from the static `slashCommands` array in `app-store.ts`
- [x] 2.2 Add a mechanism to detect when the KAS engine is active (check session client type or `KIRO_AGENT_ENGINE` env var)
- [x] 2.3 Dynamically add `/spec` to the slash command list when KAS is detected
- [x] 2.4 Remove `/spec` from the list if the engine changes away from KAS
- [x] 2.5 Add unit test: `/spec` not in command list when engine is not KAS
- [x] 2.6 Add unit test: `/spec` appears in command list when KAS engine is active

---

## Task 3: `/spec` Slash Command — Core

**Validates: Requirements 2.3, 2.4, 3.1, 3.2, 3.3, 5.1, 5.2, 5.3**

- [x] 3.1 Create `src/utils/spec-workspace.ts` utility (listSpecFeatures, findSpecFeature, describeSpecDocuments)
- [x] 3.2 Register `/spec` as a local slash command in app-store with subcommand metadata
- [x] 3.3 Add `runSpec` effect handler in `commands/effects.ts`
- [x] 3.4 Implement `/spec` (no args) — selection menu from workspace scan
- [x] 3.5 Implement `/spec new <name>` — setMode + prompt
- [x] 3.6 Implement `/spec <name>` — setMode + resume prompt
- [x] 3.7 Call `ctx.setCurrentAgent({ name: 'spec' })` after setMode to fix chip display

---

## Task 4: `/spec run <name>` — ACP Integration

**Validates: Requirements 4.1, 4.2, 4.3, 4.4, 4.5, 4.6**

- [x] 4.1 Define local spec ACP types (SpecInvokeRequest, SpecResolveSessionRequest, etc.)
- [x] 4.2 Add `resolveSpecSession()` method on KasAcpClient
- [x] 4.3 Add `invokeSpec()` method on KasAcpClient
- [x] 4.4 Expose `resolveSpecSession()` and `invokeSpec()` on Kiro class
- [x] 4.5 Implement `runSpecFeature()` helper in effects.ts
- [x] 4.6 Implement `/spec run <name>` flow (validate tasks.md → resolveSession → invoke)

---

## Task 5: Task Status Notifications

**Validates: Requirements 6.1, 6.2**

- [~] 5.1 Subscribe to `_kiro/spec/taskStatusChanged` ext notification in KasAcpClient
- [~] 5.2 Define a new `AgentEventType` or use transient alerts to surface task status changes
- [~] 5.3 Show task progress in the activity tray or as inline alerts
- [~] 5.4 Handle terminal statuses (completed, failed, aborted) with appropriate UX

---

## Task 6: Polish & Testing

**Validates: Requirements 7.1, 7.2, 7.3, 7.4**

- [~] 6.1 Add unit tests for `spec-workspace.ts` (listSpecFeatures, findSpecFeature edge cases)
- [~] 6.2 Add unit tests for the `runSpec` effect handler (mock kiro methods)
- [~] 6.3 Add integration test (cucumber BDD) for `/spec` command flow
- [~] 6.4 Run full `pr-check:quick` and fix any issues
- [~] 6.5 Invoke semantic reviewer for PR review

---

## Notes

- Task 1 is complete; Task 2 implements conditional command registration (KAS-only visibility)
- Tasks 3–4 are complete; Tasks 5–6 remain
- Each task references specific requirements for traceability
- Task 5 implements future-facing notification support (Requirement 6)
- Task 6 covers testing and validation for the workspace scanner utility (Requirement 7)

## Task Dependency Graph

```json
{
  "waves": [
    { "id": 0, "tasks": ["1.1", "2.1", "3.1"] },
    { "id": 1, "tasks": ["1.2", "1.3", "2.2", "2.3", "3.2"] },
    { "id": 2, "tasks": ["1.4", "1.5", "2.4", "2.5", "2.6", "3.3"] },
    { "id": 3, "tasks": ["3.4", "3.5", "3.6", "3.7"] },
    { "id": 4, "tasks": ["4.1"] },
    { "id": 5, "tasks": ["4.2", "4.3"] },
    { "id": 6, "tasks": ["4.4", "4.5"] },
    { "id": 7, "tasks": ["4.6"] },
    { "id": 8, "tasks": ["5.1"] },
    { "id": 9, "tasks": ["5.2", "5.3"] },
    { "id": 10, "tasks": ["5.4"] },
    { "id": 11, "tasks": ["6.1", "6.2"] },
    { "id": 12, "tasks": ["6.3"] },
    { "id": 13, "tasks": ["6.4"] },
    { "id": 14, "tasks": ["6.5"] }
  ]
}
```
