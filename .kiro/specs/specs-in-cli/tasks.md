# Tasks — Spec-Mode Support in Kiro CLI TUI

## Phase 1: Mode Change Propagation (Requirement 1)

- [x] Handle `current_mode_update` in `KasAcpClient.wireSessionListeners` — update cache + broadcast `AgentSwitched`
- [x] Guard against spurious broadcasts when mode is unchanged
- [x] Include `previousAgentName` and `welcomeMessage` in the broadcast
- [x] Add unit tests for the broadcast behavior
- [x] Update base-class comment to clarify interception

## Phase 2: `/spec` Slash Command — Core (Requirements 2, 3, 5)

- [x] Create `src/utils/spec-workspace.ts` utility (listSpecFeatures, findSpecFeature, describeSpecDocuments)
- [x] Register `/spec` as a local slash command in app-store with subcommand metadata
- [x] Add `runSpec` effect handler in `commands/effects.ts`
- [x] Implement `/spec` (no args) — selection menu from workspace scan
- [x] Implement `/spec new <name>` — setMode + prompt
- [x] Implement `/spec <name>` — setMode + resume prompt
- [x] Call `ctx.setCurrentAgent({ name: 'spec' })` after setMode to fix chip display

## Phase 3: `/spec run <name>` — ACP Integration (Requirement 4)

- [x] Define local spec ACP types (SpecInvokeRequest, SpecResolveSessionRequest, etc.)
- [x] Add `resolveSpecSession()` method on KasAcpClient
- [x] Add `invokeSpec()` method on KasAcpClient
- [x] Expose `resolveSpecSession()` and `invokeSpec()` on Kiro class
- [x] Implement `runSpecFeature()` helper in effects.ts
- [x] Implement `/spec run <name>` flow (validate tasks.md → resolveSession → invoke)

## Phase 4: Task Status Notifications (Requirement 6) — NOT STARTED

- [ ] Subscribe to `_kiro/spec/taskStatusChanged` ext notification in KasAcpClient
- [ ] Define a new `AgentEventType` or use transient alerts to surface task status changes
- [ ] Show task progress in the activity tray or as inline alerts
- [ ] Handle terminal statuses (completed, failed, aborted) with appropriate UX

## Phase 5: Polish & Testing — NOT STARTED

- [ ] Add unit tests for `spec-workspace.ts` (listSpecFeatures, findSpecFeature edge cases)
- [ ] Add unit tests for the `runSpec` effect handler (mock kiro methods)
- [ ] Add integration test (cucumber BDD) for `/spec` command flow
- [ ] Run full `pr-check:quick` and fix any issues
- [ ] Invoke semantic reviewer for PR review
