# Design Document: Spec-Mode Support in Kiro CLI TUI

## Architecture Overview

The spec-mode integration follows the existing TUI V2 architecture: a React/Twinki frontend communicates with the KAS agent over ACP (stdio). Spec-mode is a KAS-only feature — the V1 Rust engine has no equivalent.

```
┌─────────────────────────────────────────────────────────────────┐
│  TUI (TypeScript)                                               │
│                                                                 │
│  /spec command ──► effects.ts ──► Kiro class ──► KasAcpClient   │
│       │                              │                │         │
│       │  spec-workspace.ts           │                │         │
│       │  (scan .kiro/specs/)         │                │         │
│       ▼                              ▼                ▼         │
│  Selection Menu              setMode('spec')   resolveSpecSession│
│  or sendMessage()            setCurrentAgent   invokeSpec        │
└─────────────────────────────────────────────────────────────────┘
                              │ ACP (stdio)
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  KAS Agent (kiro-agent-server)                                  │
│                                                                 │
│  session/set_config_option ──► current_mode_update notification  │
│  _kiro/spec/resolveSession ──► returns sessionId                │
│  _kiro/spec/invoke         ──► fires execution, streams updates │
│  _kiro/spec/taskStatusChanged ──► notification to client        │
└─────────────────────────────────────────────────────────────────┘
```

## Component Design

### 1. `spec-workspace.ts` — Workspace Scanner

**Location:** `packages/tui/src/utils/spec-workspace.ts`

Pure synchronous utility. No state, no side effects, no network calls.

- `listSpecFeatures(workspaceRoot)` → `SpecFeatureSummary[]`
- `findSpecFeature(workspaceRoot, name)` → `SpecFeatureSummary | undefined`
- `describeSpecDocuments(summary)` → `string`

Scans `.kiro/specs/*/` for well-known documents (`requirements.md`, `design.md`, `tasks.md`, `bugfix.md`). Returns sorted results. Never throws.

### 2. `KasAcpClient` — ACP Transport Layer

**Location:** `packages/tui/src/acp-client.ts`

Extended with:

- **`wireSessionListeners` enhancement:** Intercepts `current_mode_update` notifications. Updates the local `modesState` cache and broadcasts an `AgentSwitched` stream event when the mode actually changes. Includes a guard to suppress no-op broadcasts.

- **`resolveSpecSession(request)`:** Calls `_kiro/spec/resolveSession` via `kiroClient.sendExtMethod`. Gates on `extensionMethods` capability set. Returns `{ sessionId }`.

- **`invokeSpec(request)`:** Calls `_kiro/spec/invoke` via `kiroClient.sendExtMethod`. Gates on `extensionMethods` capability set. Returns `{ sessionId, executionId? }`.

- **Local type definitions:** `SpecResolveSessionRequest`, `SpecInvokeRequest`, `SpecInvokeResponse`, etc. Defined locally to avoid adding `@kiro/acp-type-covenant` as a direct dependency.

### 3. `Kiro` Class — Session Lifecycle Facade

**Location:** `packages/tui/src/kiro.ts`

Thin pass-through methods:

- `resolveSpecSession(request)` — delegates to `sessionClient.resolveSpecSession()`. Throws if the engine doesn't support it (V1).
- `invokeSpec(request)` — delegates to `sessionClient.invokeSpec()`. Throws if the engine doesn't support it (V1).

Uses `'resolveSpecSession' in this.sessionClient` duck-typing to gate KAS-only methods without modifying the `SessionClient` interface.

### 4. `/spec` Effect Handler — Command Orchestration

**Location:** `packages/tui/src/commands/effects.ts`

Registered as `runSpec` in the effect map (`spec: 'runSpec'`). Handles four branches:

| Input | Behavior |
|-------|----------|
| `/spec` (no args) | Scan workspace → show selection menu |
| `/spec new <name>` | `setMode('spec')` + `setCurrentAgent` + send start prompt |
| `/spec <name>` | `setMode('spec')` + `setCurrentAgent` + send resume prompt |
| `/spec run <name>` | Validate `tasks.md` → `resolveSpecSession` → `invokeSpec(runAllTasks)` |

The `runSpecFeature()` helper encapsulates the resolve-then-invoke flow with loading state and error handling.

### 5. App Store — Command Registration

**Location:** `packages/tui/src/stores/app-store.ts`

`/spec` is registered as a local slash command (same pattern as `/spawn`, `/editor`):

```ts
{
  name: '/spec',
  description: 'List specs, switch to spec mode, or run spec tasks',
  source: 'local',
  meta: {
    local: true,
    subcommands: ['new', 'run'],
    subcommandHints: { new: '<feature-name>', run: '<feature-name>' },
  },
}
```

## Data Flow

### Mode Switch Flow (`/spec new` or `/spec <name>`)

```
User types "/spec new auth"
  → effects.ts: runSpec handler
  → kiro.setMode('spec')
    → KasAcpClient.setMode()
      → kiroClient.setSessionConfigOption({configId:'mode', value:'spec'})
  → ctx.setCurrentAgent({name:'spec'})  ← immediate UI update
  → ctx.sendMessage("Start a new spec...")
  → Agent processes prompt in spec mode
  → Agent emits current_mode_update (redundant, but harmless — guard skips)
```

### Run All Tasks Flow (`/spec run`)

```
User types "/spec run auth"
  → effects.ts: runSpec handler
  → spec-workspace.ts: findSpecFeature('auth') → validates tasks.md exists
  → kiro.resolveSpecSession({featureName:'auth', strategy:'reuse'})
    → KasAcpClient.resolveSpecSession()
      → kiroClient.sendExtMethod('_kiro/spec/resolveSession', ...)
      → returns {sessionId: 'sess_xxx'}
  → kiro.invokeSpec({operation:'runAllTasks', sessionId:'sess_xxx', ...})
    → KasAcpClient.invokeSpec()
      → kiroClient.sendExtMethod('_kiro/spec/invoke', ...)
      → Agent starts autonomous execution
  → TUI shows success alert
  → Agent streams tool calls / content via session updates (normal flow)
```

### Agent-Initiated Mode Switch (e.g. spec → execution handoff)

```
Agent internally switches mode
  → Emits session notification: {sessionUpdate:'current_mode_update', currentModeId:'vibe'}
  → KasAcpClient.wireSessionListeners handler:
    → Updates modesState cache
    → Detects newModeId !== previousModeId
    → Broadcasts AgentSwitched event {agentName:'vibe', previousAgentName:'spec'}
  → Kiro.ts global handler receives AgentSwitched
    → Calls agentHandler({name:'vibe'})
  → App store setCurrentAgent({name:'vibe'})
    → React re-renders agent chip
```

## Design Decisions

1. **Prompt-based workflow initiation** (not direct `_kiro/spec/invoke generateDocument`): For `/spec new` and `/spec <name>`, we switch mode and send a natural-language prompt rather than calling the structured ACP method. This is simpler, more flexible, and matches what a user would type manually. The agent's spec mode handles the workflow orchestration.

2. **Immediate `setCurrentAgent` after `setMode`**: The `current_mode_update` notification arrives asynchronously. To avoid a visible lag in the agent chip, we optimistically update the store right after `setMode` resolves. The subsequent notification is a no-op (guard catches same-mode).

3. **Local type definitions**: Rather than adding `@kiro/acp-type-covenant` to `package.json`, we define the spec ACP types locally in `acp-client.ts`. The shapes are stable ACP contracts and unlikely to drift. This keeps the dependency graph lean.

4. **No session switching on `/spec <name>`**: The `_kiro/spec/resolveSession` method returns a *different* session ID (the agent's internal spec session). We don't switch the TUI's active session to it — that would disrupt the user's conversation. Instead, we stay on the current session (now in spec mode) and let the agent route internally.

5. **`/spec run` uses `resolveSession` + `invokeSpec`**: Unlike `/spec new` which is prompt-driven, `/spec run` needs the structured ACP path because `runAllTasks` is a fire-and-forget operation that the agent drives autonomously without further user prompts.

## Correctness Properties

*A property is a characteristic or behavior that should hold true across all valid executions of a system — essentially, a formal statement about what the system should do. Properties serve as the bridge between human-readable specifications and machine-verifiable correctness guarantees.*

### Property 1: Mode Change Propagation

*For any* valid `currentModeId` value received in a `current_mode_update` notification that differs from the currently cached mode, the TUI SHALL update the store to reflect the new mode AND broadcast exactly one `AgentSwitched` event containing the correct `agentName` and `previousAgentName`.

**Validates: Requirements 1.1, 1.2**

### Property 2: No Spurious Mode Events

*For any* `current_mode_update` notification where the `currentModeId` matches the already-cached mode, the TUI SHALL NOT broadcast an `AgentSwitched` event. Processing the same mode ID N times (N ≥ 2) produces exactly one event (on the first occurrence only).

**Validates: Requirements 1.3**

### Property 3: Spec Discovery Completeness

*For any* workspace directory structure, the scanner SHALL return exactly the set of non-dotfile subdirectories under `.kiro/specs/` that contain at least one entry, reporting the correct subset of well-known documents (`requirements.md`, `design.md`, `tasks.md`, `bugfix.md`) for each, sorted alphabetically by feature name.

**Validates: Requirements 7.1, 7.2, 7.4**

### Property 4: Command Routing Correctness

*For any* valid feature name string, `/spec new <name>` SHALL produce a prompt containing that name and referencing requirements creation, and `/spec <name>` (for an existing feature) SHALL produce a resume prompt that accurately describes exactly the documents that exist for that feature.

**Validates: Requirements 3.2, 5.2**

### Property 5: Workspace Scanner Never Throws

*For any* input string as `workspaceRoot` (including non-existent paths, empty strings, paths to files, and paths with permission errors), `listSpecFeatures` SHALL return an array (possibly empty) and SHALL NOT throw an exception.

**Validates: Requirements 7.3**

## Testing Strategy

### Unit Tests

1. **`spec-workspace.ts` (Workspace Scanner)**
   - `listSpecFeatures` returns correct features with document summaries
   - Skips dotfiles and non-directory entries
   - Returns empty array for non-existent `.kiro/specs/` directory
   - Returns empty array for unreadable directories (permission errors)
   - Results are sorted alphabetically
   - `findSpecFeature` returns matching feature or undefined
   - `describeSpecDocuments` produces human-readable document list

2. **`runSpec` Effect Handler (`effects.ts`)**
   - `/spec` with no args triggers workspace scan and shows selection menu
   - `/spec` with no args and no specs shows warning alert
   - `/spec new <name>` calls `setMode('spec')` and sends start prompt
   - `/spec new` without name shows usage error
   - `/spec <name>` for existing feature switches mode and sends resume prompt
   - `/spec <name>` for non-existent feature shows error
   - `/spec run <name>` validates `tasks.md` exists before calling ACP methods
   - `/spec run <name>` with missing `tasks.md` shows error
   - `/spec run <name>` calls `resolveSpecSession` then `invokeSpec` with correct params
   - `/spec run <name>` shows error when agent doesn't support `_kiro/spec/invoke`

3. **Mode Change Notification Handler (`KasAcpClient`)**
   - Processes `current_mode_update` and updates `modesState` cache
   - Broadcasts `AgentSwitched` event when mode actually changes
   - Suppresses duplicate broadcast when mode matches cached value

### Property-Based Tests

Using `fast-check` for property-based testing (minimum 100 iterations per property):

1. **Discovery Completeness** (Property 3)
   - Generate random directory structures with varying combinations of well-known documents
   - Verify scanner reports exactly the correct documents for each feature
   - Tag: `Feature: specs-in-cli, Property 3: Spec Discovery Completeness`

2. **Scanner Never Throws** (Property 5)
   - Generate arbitrary strings as workspace root inputs
   - Verify function always returns an array without throwing
   - Tag: `Feature: specs-in-cli, Property 5: Workspace Scanner Never Throws`

3. **No Spurious Mode Events** (Property 2)
   - Generate random mode IDs, set as current, then re-process same ID
   - Verify exactly one event is emitted per unique mode transition
   - Tag: `Feature: specs-in-cli, Property 2: No Spurious Mode Events`

### Integration Tests

1. **`/spec` Command Flow (end-to-end)**
   - Full flow: user types `/spec` → scanner runs → menu displayed → user selects → mode switches → prompt sent
   - Full flow: `/spec run auth` → `resolveSpecSession` called → `invokeSpec` called → success alert shown
   - Error flow: `/spec run auth` with unsupported agent → clear error message displayed

2. **ACP Integration**
   - `resolveSpecSession` sends correct `_kiro/spec/resolveSession` ext method call
   - `invokeSpec` sends correct `_kiro/spec/invoke` ext method call with operation and params
   - Graceful error handling when ACP methods are not available (V1 engine)

## Future Considerations

- **Task status UI**: Phase 4 will subscribe to `_kiro/spec/taskStatusChanged` notifications. The natural UX is either the existing activity tray (task list) or inline transient alerts showing "Task 3/7 completed".
- **Per-task execution**: `/spec execute <feature> <taskId>` could be added as a subcommand, using `_kiro/spec/invoke` with `operation: 'executeTask'`.
- **Spec session persistence**: The agent's `SpecSessionTracker` persists feature→session mappings. The TUI doesn't need its own persistence — `resolveSession(strategy:'reuse')` handles it.
