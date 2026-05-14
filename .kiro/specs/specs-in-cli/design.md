# Design — Spec-Mode Support in Kiro CLI TUI

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

## Future Considerations

- **Task status UI**: Phase 4 will subscribe to `_kiro/spec/taskStatusChanged` notifications. The natural UX is either the existing activity tray (task list) or inline transient alerts showing "Task 3/7 completed".
- **Per-task execution**: `/spec execute <feature> <taskId>` could be added as a subcommand, using `_kiro/spec/invoke` with `operation: 'executeTask'`.
- **Spec session persistence**: The agent's `SpecSessionTracker` persists feature→session mappings. The TUI doesn't need its own persistence — `resolveSession(strategy:'reuse')` handles it.
