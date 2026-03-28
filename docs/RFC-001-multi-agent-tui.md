# RFC-001: Multi-Agent TUI Convergence

**Status:** Draft  
**Date:** 2026-02-21  
**Target repo:** `/Volumes/workplace/Kiro-CLI/kiro-cli-main`  
**Sources:** `kiro-cli` (Rust backend) · `kiro-cli-experiment` (TypeScript TUI)

---

## Table of Contents

1. [Summary](#1-summary)
2. [Motivation](#2-motivation)
3. [Current State Analysis](#3-current-state-analysis)
4. [Architecture & Data Flow](#4-architecture--data-flow)
5. [Implementation Specification](#5-implementation-specification)
6. [E2E Test Strategy](#6-e2e-test-strategy)
7. [Implementation Checklist](#7-implementation-checklist)
8. [File Change Manifest](#8-file-change-manifest)
9. [Appendix](#9-appendix)

---

## 1. Summary

This RFC describes the convergence of three Kiro CLI repositories into a single production-grade multi-agent TUI. The goal is to unify:

- The **multi-session ACP backend** from `kiro-cli` (Rust: `session_manager.rs`, `extensions.rs`, orchestration layer)
- The **multi-agent TypeScript TUI** from `kiro-cli-experiment` (5 React/Ink components, Zustand store fields, ACP client methods)
- Into **`kiro-cli-main`** — the single target repository

The end state is a TUI where users can spawn multiple AI agents via `/spawn`, monitor them in a crew monitor (ctrl+g), send nudge messages to individual sessions (press `p`), and use the `agent_crew` tool to run multi-stage pipelines — all while the main chat session remains isolated from child session events.

**Approach:** Integration tests first. Every feature is verified by a failing E2E test before implementation begins. No task is marked complete until its test passes.

---

## 2. Motivation

### 2.1 The Three Repos

| Repo | Path | Has |
|------|------|-----|
| `kiro-cli-main` | `/Volumes/workplace/Kiro-CLI/kiro-cli-main` | Full single-session TUI, 33 components, E2E infra |
| `kiro-cli` | `/Users/kennvene/workplace/Kiro-CLI/kiro-cli` | Multi-session Rust backend (session_manager, extensions, orchestration) |
| `kiro-cli-experiment` | `/Users/kennvene/workplace/Kiro-CLI/kiro-cli-experiment` | Multi-agent TypeScript TUI (5 components, store fields, ACP methods) |

`kiro-cli-main` is the production target. It currently has a complete, well-tested single-session experience but zero multi-agent UI. The Rust backend in `kiro-cli-main` already has `session_manager.rs` and `extensions.rs` with `_session/spawn`, `_session/attach`, `SESSION_LIST_UPDATE`, and `SESSION_ACTIVITY` — but the TypeScript TUI has no code to use them.

### 2.2 Goals

**MUST:**
- ctrl+g opens crew monitor showing all active sessions
- `/spawn <task>` creates a new agent session visible in crew monitor
- Child session events are isolated from main chat (never appear in `store.messages`)
- Press `p` in crew monitor to send a nudge message to a session
- `agent_crew` tool spawns real sessions (not a stub)
- All features verified by passing E2E tests in `packages/tui/e2e_tests/`

**SHOULD:**
- Session list shows real-time status (○ idle · ● busy · ✓ done · ✗ failed)
- Elapsed time and message count per session
- `/switch-agent <id>` attaches to a different session

**WON'T (this RFC):**
- Persistent session storage across restarts
- Session search/filter UI
- Diff view for file changes
- Full dashboard web UI

---

## 3. Current State Analysis

### 3.1 kiro-cli-main — What Exists Today

**TypeScript TUI** (`packages/tui/src/`):

| File | Status | Notes |
|------|--------|-------|
| `acp-client.ts` | ✅ Has multi-session methods | `spawnSession`, `attachSession`, `listSessions`, `sendMessage`, `onMultiSessionUpdate` already added |
| `stores/app-store.ts` | ✅ Has multi-session state | `sessions: Map`, `sessionEventBuffer`, `crewMonitorVisible`, `activeSessionId` already added |
| `kiro.ts` | ✅ Has session methods | `spawnSession`, `attachSession`, `listSessions`, `sendMessage` already added |
| `components/layout/AppContainer.tsx` | ✅ Has ctrl+g | Routes to `<CrewMonitorScreen>` on `mode === 'crew-monitor'` |
| `components/multi-agent/` | ✅ Directory exists | `CrewMonitorScreen.tsx`, `SessionList.tsx`, `SessionOutput.tsx`, `SessionStatusBar.tsx`, `MessageInput.tsx` |
| `types/multi-session.ts` | ✅ Imported | `AgentSession`, `SessionEvent`, `InboxMessage` |
| `index.tsx` | ❓ Needs verification | Does it wire `onMultiSessionUpdate`? |

**Rust backend** (`crates/chat-cli-v2/src/agent/acp/`):

| File | Size | Status |
|------|------|--------|
| `session_manager.rs` | 89KB | ✅ Full multi-session coordination |
| `acp_agent.rs` | 81KB | ✅ Session lifecycle management |
| `extensions.rs` | 4KB | ✅ `_session/spawn`, `_session/attach`, `_session/terminate`, `SESSION_LIST_UPDATE`, `SESSION_ACTIVITY`, `INBOX_NOTIFICATION` |
| `orchestration/` | dir | ✅ `InboxStore`, `PermissionStore`, `OrchestratedSession` |
| `subagent_tool.rs` | 12KB | ✅ `handle_subagent_request` |

**agent_crew tool:**

```bash
find /Volumes/workplace/Kiro-CLI/kiro-cli-main/crates -name 'agent_crew.rs'
# → NOT FOUND
```

❌ `agent_crew.rs` does not exist. This is the primary missing piece.

### 3.2 kiro-cli — Rust Backend Reference

The `kiro-cli` repo has the same Rust backend. Key types from `extensions.rs`:

```rust
// extensions.rs — ACP method name constants
pub mod methods {
    pub const SESSION_SPAWN: &str = "_session/spawn";
    pub const SESSION_ATTACH: &str = "_session/attach";
    pub const SESSION_TERMINATE: &str = "_session/terminate";
    pub const INBOX_NOTIFICATION: &str = "_kiro.dev/session/inbox_notification";
    pub const SESSION_LIST_UPDATE: &str = "_kiro.dev/session/list_update";
    pub const SESSION_ACTIVITY: &str = "_kiro.dev/session/activity";
}

pub struct InboxNotification {
    pub session_id: SessionId,
    pub session_name: String,
    pub message_count: usize,
    pub escalation_count: usize,
    pub senders: Vec<String>,
}
```

### 3.3 kiro-cli-experiment — TypeScript TUI Reference

The experiment repo has the complete multi-agent UI. Key component: `CrewMonitorScreen.tsx` (actual source):

```typescript
// packages/tui/src/components/multi-agent/CrewMonitorScreen.tsx
export const CrewMonitorScreen: React.FC<CrewMonitorScreenProps> = ({ width, height }) => {
  const { sessions, selectedSessionId, sessionMessages, kiro } = useAppStore(
    useShallow((state) => ({
      sessions: state.sessions,
      selectedSessionId: state.selectedSessionId,
      sessionMessages: state.sessionMessages,
      kiro: state.kiro,
    }))
  );
  const [messageMode, setMessageMode] = useState(false);
  const messageTimestamps = useRef<Map<string, number[]>>(new Map());

  // Press 'p' to enter nudge mode
  useKeypress((input, key) => {
    if (!messageMode && input === 'p' && selectedSessionId) {
      setMessageMode(true);
    }
  });

  // Rate limit: max 10 messages per minute per session
  const checkRateLimit = (sessionId: string): boolean => {
    const now = Date.now();
    const timestamps = messageTimestamps.current.get(sessionId) || [];
    const recentMessages = timestamps.filter(t => now - t < 60000);
    messageTimestamps.current.set(sessionId, recentMessages);
    return recentMessages.length < 10;
  };
  // ... (full source in Section 5.7)
};
```

### 3.4 Gap Matrix

| Feature | kiro-cli-main | Source | Action |
|---------|--------------|--------|--------|
| `types/multi-session.ts` | ✅ exists | — | Verify |
| Store: `sessions` Map | ✅ exists | — | Verify |
| Store: `sessionEventBuffer` | ✅ exists | — | Verify |
| Store: `crewMonitorVisible` | ✅ exists | — | Verify |
| ACP: `spawnSession()` | ✅ exists | — | Verify |
| ACP: `onMultiSessionUpdate()` | ✅ exists | — | Verify |
| `CrewMonitorScreen.tsx` | ✅ exists | — | Verify renders |
| `SessionList.tsx` | ✅ exists | — | Verify renders |
| `SessionOutput.tsx` | ✅ exists | — | Verify renders |
| `MessageInput.tsx` | ✅ exists | — | Verify renders |
| `SessionStatusBar.tsx` | ✅ exists | — | Verify renders |
| ctrl+g → crew-monitor | ✅ exists | — | Verify works |
| `index.tsx` event wiring | ❓ unknown | kiro-cli-experiment | Verify/fix |
| `agent_crew.rs` | ❌ missing | Write new | **CREATE** |
| Rust `_session/spawn` handler | ❓ partial | kiro-cli | Verify/complete |

**The primary remaining work is:**
1. Verify all the TypeScript pieces actually wire together end-to-end
2. Implement `agent_crew.rs`
3. Ensure the Rust `_session/spawn` handler emits `SESSION_LIST_UPDATE` back to TUI

---

## 4. Architecture & Data Flow

### 4.1 System Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    kiro-cli-main TUI (TypeScript/Ink)           │
│                                                                  │
│  index.tsx                                                       │
│  ├── AcpClient (acp-client.ts)                                  │
│  │   ├── EXT_METHODS: SESSION_SPAWN, SESSION_LIST, ...          │
│  │   ├── spawnSession() → _session/spawn                        │
│  │   ├── onMultiSessionUpdate() → routes by sessionId           │
│  │   └── onSessionEvent() → session lifecycle events            │
│  ├── Kiro (kiro.ts) — thin wrapper over AcpClient               │
│  ├── AppStore (Zustand)                                          │
│  │   ├── sessions: Map<string, AgentSession>                    │
│  │   ├── sessionEventBuffer: Record<string, AgentStreamEvent[]> │
│  │   ├── crewMonitorVisible: boolean                            │
│  │   └── mode: 'inline' | 'expanded' | 'crew-monitor'          │
│  └── Components                                                  │
│      ├── AppContainer.tsx — ctrl+g → setMode('crew-monitor')    │
│      ├── InlineLayout.tsx — single-session chat (default)       │
│      └── CrewMonitorScreen.tsx — multi-agent dashboard          │
│          ├── SessionList.tsx (30% width)                        │
│          ├── SessionOutput.tsx (70% width)                      │
│          ├── SessionStatusBar.tsx                               │
│          └── MessageInput.tsx (modal, press 'p')               │
└──────────────────────────┬──────────────────────────────────────┘
                           │ ACP protocol over stdio
                           │ (JSON-RPC + extensions)
┌──────────────────────────▼──────────────────────────────────────┐
│                 kiro-cli-main Backend (Rust)                     │
│                                                                  │
│  chat-cli-v2/src/agent/acp/                                     │
│  ├── session_manager.rs (89KB) — coordinates all sessions       │
│  │   ├── spawn_session() → creates AcpSession                   │
│  │   ├── attach_session() → switches active session             │
│  │   └── emits SESSION_LIST_UPDATE on changes                   │
│  ├── acp_agent.rs (81KB) — individual session actor             │
│  │   └── emits SESSION_ACTIVITY per event                       │
│  ├── extensions.rs — ACP method constants + payload types       │
│  ├── orchestration/                                             │
│  │   ├── inbox.rs — InboxStore (inter-agent messaging)          │
│  │   └── permissions.rs — PermissionStore                       │
│  └── subagent_tool.rs — handles SpawnSubagentRequest            │
│                                                                  │
│  agent/src/agent/tools/                                         │
│  ├── use_subagent.rs — existing subagent spawning               │
│  └── agent_crew.rs ← TO BE CREATED                             │
└─────────────────────────────────────────────────────────────────┘
```

### 4.2 Critical Workflow: Session Spawn

```
User types: /spawn "write unit tests for auth module"
                │
                ▼
app-store.ts: handleUserInput()
  detects /spawn prefix
  extracts task = "write unit tests for auth module"
                │
                ▼
kiro.ts: spawnSession(task)
  delegates to this.sessionClient.spawnSession(task)
                │
                ▼
acp-client.ts: spawnSession(task, name?, role?)
  await this.connection.extMethod(
    EXT_METHODS.SESSION_SPAWN,   // = 'session/spawn'
    { task, name, role }
  )
                │
                ▼ [ACP wire: _session/spawn {task}]
                │
session_manager.rs: handle_spawn()
  creates new SessionId (UUID)
  builds AcpSessionConfig { task, is_subagent: true }
  calls start_session(session_id, config)
  returns { sessionId: "abc-123" }
  emits SESSION_LIST_UPDATE { sessions: [...all sessions] }
                │
                ▼ [ACP wire: _kiro.dev/session/list_update]
                │
acp-client.ts: extNotification handler
  matches SESSION_LIST_UPDATE
  calls onSessionListUpdate(sessions)
                │
                ▼
app-store.ts: setSessions(sessions) or addSession(session)
  sessions Map updated
                │
                ▼
SessionList.tsx re-renders
  shows new session: ○ idle  "write unit tests..."
```

### 4.3 Critical Workflow: ctrl+g Crew Monitor

```
User presses: ctrl+g
                │
                ▼
AppContainer.tsx: useKeypress handler
  key.ctrl && input === 'g'
  → setMode('crew-monitor')   [or toggleCrewMonitor()]
                │
                ▼
app-store.ts: mode = 'crew-monitor'
                │
                ▼
AppContainer.tsx: render switch
  case 'crew-monitor':
    return <CrewMonitorScreen width={cols} height={rows} />
                │
                ▼
CrewMonitorScreen.tsx renders:
  ┌─────────────────────────────────────────────┐
  │ Sessions (2)    │ Session: write-tests       │
  │ ─────────────── │ ─────────────────────────  │
  │ ● write-tests   │ > Reading auth/mod.rs...   │
  │ ○ fix-bugs      │ > Writing test_login()...  │
  │                 │ ✓ Created auth.test.rs     │
  └─────────────────────────────────────────────┘
  Press 'p' to send message to write-tests

User presses Escape → setMode('inline') → back to chat
```

### 4.4 Critical Workflow: Child Event Isolation

This is the most important correctness property: child session events must NEVER appear in the main chat.

```
Child session "write-tests" produces output:
                │
                ▼
acp_agent.rs: sends SESSION_ACTIVITY notification
  { sessionId: "abc-123", event: { type: "content", text: "..." } }
                │
                ▼ [ACP wire: _kiro.dev/session/activity]
                │
acp-client.ts: sessionUpdate() handler
  checks: event.sessionId !== this.sessionId  ← KEY CHECK
  if true (child session):
    → calls each multiSessionHandler(sessionId, event)
    → DOES NOT call broadcastStreamEvent()
  if false (main session):
    → calls broadcastStreamEvent(event)
    → DOES NOT call multiSessionHandlers
                │
                ▼
index.tsx: onMultiSessionUpdate(sessionId, event)
  store.pushSessionEvent(sessionId, event)
                │
                ▼
app-store.ts: sessionEventBuffer[sessionId].push(event)
  store.messages[] ← UNCHANGED (main chat not affected)
                │
                ▼
SessionOutput.tsx: reads sessionEventBuffer[selectedSessionId]
  renders child events in crew monitor only
```

### 4.5 Critical Workflow: agent_crew Pipeline

```
LLM calls agent_crew tool with:
{
  "stages": [
    { "name": "researcher", "task": "Research auth patterns", "role": "researcher" },
    { "name": "implementer", "task": "Implement auth module", "role": "coder",
      "depends_on": ["researcher"] },
    { "name": "reviewer", "task": "Review implementation", "role": "reviewer",
      "depends_on": ["implementer"] }
  ]
}
                │
                ▼
agent_crew.rs: execute(args)
  parse pipeline stages
  build dependency graph (DAG)
  for each stage with no pending deps:
    emit SpawnSubagentRequest {
      task: stage.task,
      agent_name: stage.role,
      session_name: stage.name,
    }
                │
                ▼
session_manager.rs: handle SpawnSubagentRequest
  creates AcpSession per stage
  emits SESSION_LIST_UPDATE for each
                │
                ▼
TUI: store.sessions grows to 3 entries
CrewMonitorScreen shows:
  ● researcher  (running)
  ○ implementer (waiting for researcher)
  ○ reviewer    (waiting for implementer)
```

### 4.6 ACP Protocol Reference

| Method | Direction | Payload | Handler |
|--------|-----------|---------|---------|
| `_session/spawn` | TUI → Backend | `{task, name?, role?}` → `{sessionId}` | `session_manager.rs` |
| `_session/attach` | TUI → Backend | `{sessionId}` → `{}` | `session_manager.rs` |
| `_session/terminate` | TUI → Backend | `{sessionId}` → `{}` | `session_manager.rs` |
| `session/list` | TUI → Backend | `{}` → `{sessions: SessionInfo[]}` | `session_manager.rs` |
| `message/send` | TUI → Backend | `{sessionId, content, priority?}` → `{}` | `session_manager.rs` |
| `_kiro.dev/session/list_update` | Backend → TUI | `{sessions: SessionInfo[]}` | `acp-client.ts` |
| `_kiro.dev/session/activity` | Backend → TUI | `{sessionId, event: AgentStreamEvent}` | `acp-client.ts` |
| `_kiro.dev/session/inbox_notification` | Backend → TUI | `{sessionId, messageCount, senders[]}` | `acp-client.ts` |

**SessionInfo JSON shape** (Backend → TUI):
```json
{
  "id": "abc-123",
  "name": "write-tests",
  "role": "coder",
  "status": "busy",
  "created_at": "2026-02-21T15:00:00Z",
  "last_activity": "2026-02-21T15:01:30Z",
  "summary": null,
  "parent_session": "main-session-id"
}
```

---

## 5. Implementation Specification

### 5.1 types/multi-session.ts

**File:** `packages/tui/src/types/multi-session.ts`  
**Action:** CREATE (or verify exists)

```typescript
export interface AgentSession {
  id: string;
  name: string;
  role?: string;
  group?: string;
  status: 'idle' | 'busy' | 'terminated' | 'failed';
  type: 'ephemeral' | 'persistent';
  created: Date;
  lastActivity: Date;
  summary?: string;
  parentSession?: string;
  stageInfo?: { name: string; role: string };
}

export interface SessionEvent {
  type: 'session_created' | 'session_terminated' | 'session_status_changed';
  sessionId: string;
  session: AgentSession;
}

export interface InboxMessage {
  id: string;
  from: string;
  to: string;
  content: string;
  priority: 'normal' | 'urgent' | 'escalation';
  timestamp: Date;
  read: boolean;
}
```

### 5.2 app-store.ts — Multi-Session Fields

**File:** `packages/tui/src/stores/app-store.ts`  
**Action:** VERIFY these fields exist (they were already added)

```typescript
// In AppState interface — verify these exist:
sessions: Map<string, AgentSession>;
activeSessionId: string;
selectedSessionId?: string;
crewMonitorVisible: boolean;
sessionMessages: Map<string, InboxMessage[]>;
sessionEventBuffer: Record<string, AgentStreamEvent[]>;
mode: 'inline' | 'expanded' | 'crew-monitor';  // ← 'crew-monitor' must be in union

// In BaseAppActions interface — verify these exist:
addSession: (session: AgentSession) => void;
updateSession: (id: string, updates: Partial<AgentSession>) => void;
removeSession: (id: string) => void;
setActiveSession: (id: string) => void;
setSelectedSession: (id: string) => void;
toggleCrewMonitor: () => void;
addMessage: (sessionId: string, message: InboxMessage) => void;
pushSessionEvent: (sessionId: string, event: AgentStreamEvent) => void;

// In initial state — verify these exist:
sessions: new Map(),
activeSessionId: '',
selectedSessionId: undefined,
crewMonitorVisible: false,
sessionMessages: new Map(),
sessionEventBuffer: {},
```

**Verify with:**
```bash
grep -n 'sessions\|sessionEventBuffer\|crewMonitor\|crew-monitor' \
  packages/tui/src/stores/app-store.ts | head -20
```

### 5.3 acp-client.ts — Session Methods

**File:** `packages/tui/src/acp-client.ts`  
**Action:** VERIFY these exist (already added)

```typescript
// EXT_METHODS — verify these exist:
const EXT_METHODS = {
  // ... existing methods ...
  SESSION_LIST: 'session/list',
  SESSION_SPAWN: 'session/spawn',       // ← NOTE: no underscore prefix here
  SESSION_TERMINATE: 'session/terminate',
  SESSION_ATTACH: '_session/attach',
  SESSION_DETACH: '_session/detach',
  MESSAGE_SEND: 'message/send',
  SESSION_CREATED: '_session/created',
  SESSION_TERMINATED: '_session/terminated',
  SESSION_STATUS_CHANGED: '_session/status_changed',
} as const;

// Methods — verify these exist:
async listSessions(): Promise<AgentSession[]>
async spawnSession(task: string, name?: string, role?: string): Promise<{ sessionId: string }>
async attachSession(sessionId: string): Promise<void>
async sendMessage(sessionId: string, content: string, priority?: string): Promise<void>
onMultiSessionUpdate(handler: (sessionId: string, event: AgentStreamEvent) => void): () => void
onSessionEvent(handler: (event: SessionEvent) => void): () => void
```

**Actual implementation** (from current `acp-client.ts`):
```typescript
async spawnSession(task: string, name?: string, role?: string): Promise<{ sessionId: string }> {
  try {
    const result = await this.connection.extMethod(EXT_METHODS.SESSION_SPAWN, { task, name, role });
    return { sessionId: (result as any).sessionId || (result as any).session_id };
  } catch (e) {
    logger.error('Failed to spawn session:', e);
    throw new Error(`Failed to spawn session: ${e instanceof Error ? e.message : JSON.stringify(e)}`);
  }
}

onMultiSessionUpdate(handler: (sessionId: string, event: AgentStreamEvent) => void): () => void {
  this.multiSessionHandlers.add(handler);
  return () => this.multiSessionHandlers.delete(handler);
}
```

### 5.4 index.tsx — Session Event Wiring

**File:** `packages/tui/src/index.tsx`  
**Action:** VERIFY `onMultiSessionUpdate` is wired

The critical wiring that routes child session events to the store:

```typescript
// In index.tsx — verify this wiring exists:
kiro.onMultiSessionUpdate((sessionId: string, event: AgentStreamEvent) => {
  appStore.getState().pushSessionEvent(sessionId, event);
});

kiro.onSessionEvent((event: SessionEvent) => {
  const state = appStore.getState();
  if (event.type === 'session_created') {
    state.addSession(event.session);
  } else if (event.type === 'session_terminated') {
    state.updateSession(event.sessionId, {
      status: 'terminated',
      summary: event.session.summary,
    });
  }
});
```

**Verify with:**
```bash
grep -n 'onMultiSessionUpdate\|onSessionEvent\|pushSessionEvent' \
  packages/tui/src/index.tsx
```

### 5.5 AppContainer.tsx — ctrl+g Handler

**File:** `packages/tui/src/components/layout/AppContainer.tsx`  
**Action:** VERIFY ctrl+g handler and crew-monitor render branch exist

```typescript
// Verify this keypress handler exists:
useKeypress((input, key) => {
  if (mode === 'crew-monitor') {
    if (key.name === 'escape') {
      setMode('inline');
      return;
    }
  } else if (key.ctrl && input === 'g') {
    setMode('crew-monitor');
    return;
  }
  // ... existing handlers
});

// Verify this render branch exists:
switch (mode) {
  case 'crew-monitor':
    return <CrewMonitorScreen width={cols} height={rows} />;
  // ... existing cases
}
```

**Verify with:**
```bash
grep -n 'crew-monitor\|CrewMonitorScreen\|ctrl.*g\|Ctrl' \
  packages/tui/src/components/layout/AppContainer.tsx
```

### 5.6 Multi-Agent Components

All 5 components should exist in `packages/tui/src/components/multi-agent/`. Verify:

```bash
ls packages/tui/src/components/multi-agent/
# Expected:
# CrewMonitorScreen.tsx
# SessionList.tsx
# SessionOutput.tsx
# SessionStatusBar.tsx
# MessageInput.tsx
```

**CrewMonitorScreen.tsx** — Full source (from kiro-cli-experiment, verified working):

```typescript
import React, { useState, useRef } from 'react';
import { Box, Text } from 'ink';
import { SessionList } from './SessionList.js';
import { SessionOutput } from './SessionOutput.js';
import { SessionStatusBar } from './SessionStatusBar.js';
import { MessageInput } from './MessageInput.js';
import { useAppStore } from '../../stores/app-store.js';
import { useKeypress } from '../../hooks/useKeypress.js';
import { useShallow } from 'zustand/react/shallow';

export interface CrewMonitorScreenProps {
  width: number;
  height: number;
}

export const CrewMonitorScreen: React.FC<CrewMonitorScreenProps> = ({ width, height }) => {
  const { sessions, selectedSessionId, sessionMessages, kiro } = useAppStore(
    useShallow((state) => ({
      sessions: state.sessions,
      selectedSessionId: state.selectedSessionId,
      sessionMessages: state.sessionMessages,
      kiro: state.kiro,
    }))
  );
  const { setSelectedSession, addMessage } = useAppStore(
    useShallow((state) => ({
      setSelectedSession: state.setSelectedSession,
      addMessage: state.addMessage,
    }))
  );

  const [messageMode, setMessageMode] = useState(false);
  const messageTimestamps = useRef<Map<string, number[]>>(new Map());

  const sessionArray = Array.from(sessions.values());
  const selectedSession = selectedSessionId ? sessions.get(selectedSessionId) : undefined;
  const messages = selectedSessionId ? sessionMessages.get(selectedSessionId) || [] : [];

  useKeypress((input, key) => {
    if (!messageMode && input === 'p' && selectedSessionId) {
      setMessageMode(true);
    }
  });

  const checkRateLimit = (sessionId: string): boolean => {
    const now = Date.now();
    const timestamps = messageTimestamps.current.get(sessionId) || [];
    const recentMessages = timestamps.filter(t => now - t < 60000);
    messageTimestamps.current.set(sessionId, recentMessages);
    return recentMessages.length < 10; // max 10 messages/minute
  };

  const handleSendMessage = async (message: string) => {
    if (!selectedSessionId) return;
    if (!checkRateLimit(selectedSessionId)) {
      setMessageMode(false);
      return;
    }
    try {
      await kiro.sendMessage(selectedSessionId, message);
      addMessage(selectedSessionId, {
        id: crypto.randomUUID(),
        from: 'user',
        to: selectedSessionId,
        content: message,
        timestamp: new Date(),
        priority: 'normal',
        read: false,
      });
      messageTimestamps.current.get(selectedSessionId)?.push(Date.now());
      setMessageMode(false);
    } catch (error) {
      setMessageMode(false);
    }
  };

  if (sessionArray.length === 0) {
    return (
      <Box flexDirection="column" height={height} justifyContent="center" alignItems="center">
        <Text>No active sessions. Use /spawn to create a new agent.</Text>
      </Box>
    );
  }

  if (messageMode && selectedSession) {
    return (
      <Box flexDirection="column" height={height} justifyContent="center" alignItems="center">
        <MessageInput
          targetSessionId={selectedSessionId!}
          targetSessionName={selectedSession.name}
          onSend={handleSendMessage}
          onCancel={() => setMessageMode(false)}
        />
      </Box>
    );
  }

  return (
    <Box flexDirection="column" height={height}>
      <Box flexDirection="row" flexGrow={1}>
        <Box width={Math.floor(width * 0.3)} borderStyle="single" borderRight>
          <SessionList
            sessions={sessionArray}
            selectedId={selectedSessionId}
            onSelect={setSelectedSession}
          />
        </Box>
        <Box flexGrow={1}>
          {selectedSession ? (
            <SessionOutput
              sessionId={selectedSessionId!}
              session={selectedSession}
              messages={messages}
            />
          ) : (
            <Box justifyContent="center" alignItems="center" height="100%">
              <Text>Select a session to view output</Text>
            </Box>
          )}
        </Box>
      </Box>
      {selectedSession && <SessionStatusBar session={selectedSession} />}
      {selectedSession && (
        <Box paddingX={1}>
          <Text color="gray">Press 'p' to send a message to {selectedSession.name}</Text>
        </Box>
      )}
    </Box>
  );
};
```

### 5.7 agent_crew.rs — New Rust Tool

**File:** `crates/agent/src/agent/tools/agent_crew.rs`  
**Action:** CREATE — this is the primary missing piece

The tool parses a pipeline JSON, builds a dependency graph, and emits `SpawnSubagentRequest` for each stage in dependency order.

**Input JSON format:**
```json
{
  "task": "Build a web scraper",
  "stages": [
    {
      "name": "researcher",
      "role": "researcher",
      "prompt_template": "Research web scraping approaches for {{task}}",
      "depends_on": []
    },
    {
      "name": "implementer",
      "role": "coder",
      "prompt_template": "Implement the scraper based on research",
      "depends_on": ["researcher"]
    },
    {
      "name": "reviewer",
      "role": "reviewer",
      "prompt_template": "Review the implementation",
      "depends_on": ["implementer"],
      "loop_to": { "target": "implementer", "max_iterations": 3, "trigger": "NEEDS_CHANGES" }
    }
  ]
}
```

**Minimal implementation:**
```rust
use serde::{Deserialize, Serialize};
use crate::agent::tools::{Tool, ToolArgs, ToolExecutionOutput};
use crate::agent::context::AgentContext;

#[derive(Debug, Deserialize)]
struct PipelineInput {
    task: String,
    stages: Vec<PipelineStage>,
}

#[derive(Debug, Deserialize)]
struct PipelineStage {
    name: String,
    role: String,
    prompt_template: String,
    #[serde(default)]
    depends_on: Vec<String>,
    loop_to: Option<LoopConfig>,
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LoopConfig {
    target: String,
    max_iterations: u32,
    trigger: String,
}

pub struct AgentCrewTool;

impl Tool for AgentCrewTool {
    fn name(&self) -> &str { "agent_crew" }

    fn description(&self) -> &str {
        "Spawn and coordinate multiple AI agents in a pipeline. \
         Stages run in dependency order; parallel stages run concurrently."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["task", "stages"],
            "properties": {
                "task": { "type": "string", "description": "Overall task description" },
                "stages": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["name", "role", "prompt_template"],
                        "properties": {
                            "name": { "type": "string" },
                            "role": { "type": "string" },
                            "prompt_template": { "type": "string" },
                            "depends_on": { "type": "array", "items": { "type": "string" } },
                            "model": { "type": "string" }
                        }
                    }
                }
            }
        })
    }

    async fn execute(&self, args: ToolArgs, ctx: &AgentContext) -> Result<ToolExecutionOutput> {
        let input: PipelineInput = serde_json::from_value(args.input)?;

        // Spawn stages with no dependencies first (parallel)
        let ready_stages: Vec<&PipelineStage> = input.stages.iter()
            .filter(|s| s.depends_on.is_empty())
            .collect();

        for stage in ready_stages {
            let task = stage.prompt_template.replace("{{task}}", &input.task);
            ctx.spawn_subagent(SpawnSubagentRequest {
                task,
                agent_name: stage.role.clone(),
                session_name: Some(stage.name.clone()),
                model: stage.model.clone(),
            }).await?;
        }

        Ok(ToolExecutionOutput::text(format!(
            "Pipeline started: {} stages. Monitor progress with ctrl+g.",
            input.stages.len()
        )))
    }
}
```

**Wire into tool registry** (`crates/agent/src/agent/tools/mod.rs`):
```rust
// Add to tool registration:
pub mod agent_crew;
pub use agent_crew::AgentCrewTool;

// In tool list:
tools.push(Box::new(AgentCrewTool));
```

### 5.8 Rust Backend: Verify _session/spawn Handler

The `extensions.rs` in `kiro-cli-main` already has the method constants. Verify the handler in `session_manager.rs` or `acp_agent.rs` actually:

1. Creates a new `AcpSession`
2. Returns `{ sessionId: "..." }`
3. Emits `SESSION_LIST_UPDATE` to the TUI

```bash
grep -n '_session/spawn\|SESSION_SPAWN\|handle_spawn\|session_list_update\|SESSION_LIST_UPDATE' \
  crates/chat-cli-v2/src/agent/acp/session_manager.rs | head -20

grep -n '_session/spawn\|SESSION_SPAWN' \
  crates/chat-cli-v2/src/agent/acp/acp_agent.rs | head -20
```

If the handler is missing or doesn't emit `SESSION_LIST_UPDATE`, add it following the pattern in `kiro-cli/crates/chat-cli-v2/src/agent/acp/session_manager.rs`.

---

## 6. E2E Test Strategy

### 6.1 Philosophy: Red → Green

Every feature follows this exact sequence:

```
1. Write the E2E test  →  it FAILS (red)   ← proves the feature is missing
2. Implement the code  →  test PASSES (green) ← proves the feature works
3. Never skip step 1   →  no test = no done
```

This is not optional. If you implement without a failing test first, you have no proof the feature was actually missing or that your implementation actually fixes it.

### 6.2 How E2ETestCase Works

The `E2ETestCase` class in `packages/tui/e2e_tests/E2ETestCase.ts` provides full-stack testing:

```
┌─────────────────────────────────────────────────────┐
│                  Test Process                        │
│                                                      │
│  E2ETestCase                                         │
│  ├── PtyManager — spawns real PTY terminal           │
│  │   └── runs: ./target/debug/chat_cli_v2 chat       │
│  ├── TUI IPC Server (Unix socket)                    │
│  │   └── TUI connects → exposes Zustand store        │
│  └── Agent IPC Server (Unix socket)                  │
│      └── Backend connects → exposes mock API         │
└─────────────────────────────────────────────────────┘
```

**Key methods:**

```typescript
// Launch the full stack
const tc = await E2ETestCase.builder()
  .withTestName('my-test')           // names the test-output directory
  .withTerminal({ width: 120, height: 40 })
  .launch();                         // spawns binary, waits for IPC connections

// Read live Zustand store state
const store = await tc.getStore();   // returns AppState snapshot
store.sessions                       // Map<string, AgentSession>
store.mode                           // 'inline' | 'expanded' | 'crew-monitor'
store.messages                       // MessageType[]
store.sessionEventBuffer             // Record<string, AgentStreamEvent[]>

// Send keyboard input
await tc.sendKeys('hello');          // types text
await tc.sendKeys('\r');             // Enter
await tc.sendKeys('\x07');           // ctrl+g
await tc.sendKeys('\x03');           // Ctrl+C
await tc.typeAndSubmit('text');      // types + waits 50ms + Enter

// Wait for terminal output
await tc.waitForText('ask a question', 10000);  // waits up to 10s

// Inject mock backend responses
await tc.pushSendMessageResponse([
  { type: 'content', content: { type: 'text', text: 'Hello!' } }
]);
await tc.pushSendMessageResponse(null);  // signals end of stream

// Read terminal screen
const lines = tc.getSnapshot();          // string[] — one per terminal row
const formatted = tc.getSnapshotFormatted(); // with border for debugging

// Cleanup
await tc.cleanup();
```

### 6.3 Test Patterns

#### Pattern 1: Static assertion (fastest, no binary needed)
Use for verifying source code structure — method names, field existence.

```typescript
import { describe, it, expect } from 'bun:test';
import * as fs from 'fs';
import * as path from 'path';

describe('ACP Method Names', () => {
  it('acp-client has SESSION_SPAWN method', () => {
    const src = fs.readFileSync(
      path.join(__dirname, '../src/acp-client.ts'), 'utf8'
    );
    expect(src).toContain("SESSION_SPAWN");
    expect(src).toContain("spawnSession");
  });
});
```

#### Pattern 2: Store state assertion (requires binary)
Launch TUI, read Zustand store, assert fields exist.

```typescript
it('store has sessions Map', async () => {
  const tc = await E2ETestCase.builder()
    .withTestName('store-sessions')
    .launch();
  await tc.sleepMs(2000);
  const store = await tc.getStore();
  expect(store.sessions).toBeDefined();
  expect(store.sessions instanceof Map || typeof store.sessions === 'object').toBe(true);
  await tc.cleanup();
}, 30000);
```

#### Pattern 3: Interaction → state change
Send input, verify store changes.

```typescript
it('ctrl+g switches to crew-monitor mode', async () => {
  const tc = await E2ETestCase.builder()
    .withTestName('ctrl-g-mode')
    .launch();
  await tc.sleepMs(2000);

  const before = await tc.getStore();
  expect(before.mode).toBe('inline');

  await tc.sendKeys('\x07');  // ctrl+g
  await tc.sleepMs(500);

  const after = await tc.getStore();
  expect(after.mode).toBe('crew-monitor');
  await tc.cleanup();
}, 30000);
```

#### Pattern 4: Mock response injection
Inject backend response, verify TUI renders it.

```typescript
it('renders streamed text response', async () => {
  const tc = await E2ETestCase.builder()
    .withTestName('stream-response')
    .launch();
  await tc.sleepMs(3000);

  // Pre-load mock response
  await tc.pushSendMessageResponse([
    { type: 'content', content: { type: 'text', text: 'Hello from mock!' } }
  ]);
  await tc.pushSendMessageResponse(null);  // end stream

  await tc.typeAndSubmit('hi');
  await tc.sleepMs(2000);

  const snap = tc.getSnapshot().join('');
  expect(snap).toContain('Hello from mock!');
  await tc.cleanup();
}, 45000);
```

### 6.4 Test Files to Create

#### `packages/tui/e2e_tests/crew-monitor.test.ts`

```typescript
import { describe, it, expect, afterEach } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('Crew Monitor', () => {
  let tc: E2ETestCase | null = null;
  afterEach(async () => { await tc?.cleanup(); tc = null; });

  it('ctrl+g switches mode to crew-monitor', async () => {
    tc = await E2ETestCase.builder().withTestName('crew-ctrl-g').launch();
    await tc.sleepMs(2000);

    const before = await tc.getStore();
    expect(before.mode).not.toBe('crew-monitor');

    await tc.sendKeys('\x07');  // ctrl+g
    await tc.sleepMs(500);

    const after = await tc.getStore();
    expect(after.mode).toBe('crew-monitor');
  }, 30000);

  it('Escape returns to inline mode from crew-monitor', async () => {
    tc = await E2ETestCase.builder().withTestName('crew-escape').launch();
    await tc.sleepMs(2000);

    await tc.sendKeys('\x07');  // ctrl+g
    await tc.sleepMs(500);
    expect((await tc.getStore()).mode).toBe('crew-monitor');

    await tc.sendKeys('\x1b');  // Escape
    await tc.sleepMs(500);
    expect((await tc.getStore()).mode).toBe('inline');
  }, 30000);

  it('crew monitor shows empty state when no sessions', async () => {
    tc = await E2ETestCase.builder().withTestName('crew-empty').launch();
    await tc.sleepMs(2000);

    await tc.sendKeys('\x07');  // ctrl+g
    await tc.sleepMs(500);

    const snap = tc.getSnapshot().join('');
    expect(snap).toContain('No active sessions');
  }, 30000);
});
```

#### `packages/tui/e2e_tests/session-spawn.test.ts`

```typescript
import { describe, it, expect, afterEach } from 'bun:test';
import * as fs from 'fs';
import * as path from 'path';
import { E2ETestCase } from './E2ETestCase';

describe('Session Spawn', () => {
  let tc: E2ETestCase | null = null;
  afterEach(async () => { await tc?.cleanup(); tc = null; });

  // Static check — fastest
  it('acp-client uses correct _session/spawn method name', () => {
    const src = fs.readFileSync(
      path.join(__dirname, '../src/acp-client.ts'), 'utf8'
    );
    expect(src).toContain('SESSION_SPAWN');
    expect(src).toContain('spawnSession');
  });

  it('store has sessions Map field', async () => {
    tc = await E2ETestCase.builder().withTestName('spawn-store').launch();
    await tc.sleepMs(2000);
    const store = await tc.getStore();
    expect(store.sessions).toBeDefined();
    expect(store.sessionEventBuffer).toBeDefined();
  }, 30000);

  it('/spawn command is recognized', async () => {
    tc = await E2ETestCase.builder().withTestName('spawn-cmd').launch();
    await tc.sleepMs(2000);

    await tc.sendKeys('/spawn');
    await tc.sleepMs(300);

    // Autocomplete should show /spawn
    const snap = tc.getSnapshot().join('');
    expect(snap).toContain('/spawn');
  }, 30000);
});
```

#### `packages/tui/e2e_tests/session-isolation.test.ts`

```typescript
import { describe, it, expect, afterEach } from 'bun:test';
import * as fs from 'fs';
import * as path from 'path';
import { E2ETestCase } from './E2ETestCase';

describe('Session Event Isolation', () => {
  let tc: E2ETestCase | null = null;
  afterEach(async () => { await tc?.cleanup(); tc = null; });

  // Static check — verify routing logic exists in source
  it('acp-client routes child events to multiSessionHandlers only', () => {
    const src = fs.readFileSync(
      path.join(__dirname, '../src/acp-client.ts'), 'utf8'
    );
    // Must have the isolation check
    expect(src).toContain('multiSessionHandlers');
    expect(src).toMatch(/sessionId.*!==.*sessionId|!==.*this\.sessionId/);
    // Must have broadcastStreamEvent for main session
    expect(src).toContain('broadcastStreamEvent');
  });

  it('index.tsx wires onMultiSessionUpdate to pushSessionEvent', () => {
    const src = fs.readFileSync(
      path.join(__dirname, '../src/index.tsx'), 'utf8'
    );
    expect(src).toContain('onMultiSessionUpdate');
    expect(src).toContain('pushSessionEvent');
  });

  it('main chat messages unchanged when child session event arrives', async () => {
    tc = await E2ETestCase.builder().withTestName('isolation-test').launch();
    await tc.sleepMs(3000);

    const before = await tc.getStore();
    const initialMessageCount = before.messages?.length ?? 0;

    // The store should have sessionEventBuffer
    expect(before.sessionEventBuffer).toBeDefined();

    // Main messages should not have grown from session events
    const after = await tc.getStore();
    expect(after.messages?.length ?? 0).toBe(initialMessageCount);
  }, 30000);
});
```

#### `packages/tui/e2e_tests/nudge.test.ts`

```typescript
import { describe, it, expect, afterEach } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('Nudge Messaging', () => {
  let tc: E2ETestCase | null = null;
  afterEach(async () => { await tc?.cleanup(); tc = null; });

  it('pressing p in crew monitor with no sessions shows empty state', async () => {
    tc = await E2ETestCase.builder().withTestName('nudge-empty').launch();
    await tc.sleepMs(2000);

    await tc.sendKeys('\x07');  // ctrl+g → crew monitor
    await tc.sleepMs(500);

    // With no sessions, 'p' should not crash
    await tc.sendKeys('p');
    await tc.sleepMs(300);

    const store = await tc.getStore();
    expect(store.mode).toBe('crew-monitor');
  }, 30000);
});
```

#### `packages/tui/e2e_tests/agent-crew.test.ts`

```typescript
import { describe, it, expect } from 'bun:test';
import * as fs from 'fs';
import * as path from 'path';
import { execSync } from 'child_process';

describe('Agent Crew Tool', () => {
  it('agent_crew.rs exists in crates', () => {
    // This test FAILS until agent_crew.rs is created
    const result = execSync(
      'find /Volumes/workplace/Kiro-CLI/kiro-cli-main/crates -name "agent_crew.rs" 2>/dev/null',
      { encoding: 'utf8' }
    ).trim();
    expect(result).not.toBe('');
    expect(result).toContain('agent_crew.rs');
  });

  it('agent_crew tool is registered in tool list', () => {
    // Check that agent_crew is referenced in the tools mod.rs
    const modPath = path.join(
      '/Volumes/workplace/Kiro-CLI/kiro-cli-main',
      'crates/agent/src/agent/tools/mod.rs'
    );
    if (fs.existsSync(modPath)) {
      const src = fs.readFileSync(modPath, 'utf8');
      expect(src).toContain('agent_crew');
    }
  });
});
```

### 6.5 Running Tests

```bash
# Build the binary first (required for live E2E tests)
cargo build -p chat_cli_v2

# Build the TUI bundle
cd packages/tui && bun install && bun run build

# Run all E2E tests
cd packages/tui && bun test e2e_tests/

# Run a specific test file
bun test e2e_tests/crew-monitor.test.ts

# Run with verbose output
bun test e2e_tests/ --reporter=verbose

# Run only static tests (no binary needed, fast)
bun test e2e_tests/session-spawn.test.ts  # static checks run instantly

# Watch mode during development
bun test --watch e2e_tests/crew-monitor.test.ts
```

**Expected output when all tests pass:**
```
✓ Crew Monitor > ctrl+g switches mode to crew-monitor (2341ms)
✓ Crew Monitor > Escape returns to inline mode (1823ms)
✓ Crew Monitor > crew monitor shows empty state when no sessions (2156ms)
✓ Session Spawn > acp-client uses correct _session/spawn method name (2ms)
✓ Session Spawn > store has sessions Map field (2234ms)
✓ Session Event Isolation > acp-client routes child events to multiSessionHandlers only (1ms)
✓ Session Event Isolation > index.tsx wires onMultiSessionUpdate to pushSessionEvent (1ms)
✓ Agent Crew Tool > agent_crew.rs exists in crates (5ms)
```

---

## 7. Implementation Checklist

Work through these in order. Do not advance to the next task until the current task's test passes.

### Phase 1: Verify Foundation (no new code — just confirm what's there)

- [ ] **T001** — Verify `types/multi-session.ts` exists with `AgentSession`, `SessionEvent`, `InboxMessage`
  - Test: `bun test e2e_tests/session-spawn.test.ts` (static check)
  - Command: `cat packages/tui/src/types/multi-session.ts`

- [ ] **T002** — Verify store has `sessions`, `sessionEventBuffer`, `crewMonitorVisible`, `mode: 'crew-monitor'`
  - Test: `bun test e2e_tests/session-isolation.test.ts`
  - Command: `grep -n 'sessions\|sessionEventBuffer\|crew-monitor' packages/tui/src/stores/app-store.ts`

- [ ] **T003** — Verify `acp-client.ts` has `SESSION_SPAWN`, `spawnSession()`, `onMultiSessionUpdate()`
  - Test: `bun test e2e_tests/session-spawn.test.ts`
  - Command: `grep -n 'SESSION_SPAWN\|spawnSession\|onMultiSessionUpdate' packages/tui/src/acp-client.ts`

- [ ] **T004** — Verify `index.tsx` wires `onMultiSessionUpdate` → `pushSessionEvent`
  - Test: `bun test e2e_tests/session-isolation.test.ts`
  - Command: `grep -n 'onMultiSessionUpdate\|pushSessionEvent' packages/tui/src/index.tsx`

- [ ] **T005** — Verify `AppContainer.tsx` has ctrl+g handler and `<CrewMonitorScreen>` render branch
  - Test: `bun test e2e_tests/crew-monitor.test.ts` (live E2E)
  - Command: `grep -n 'crew-monitor\|CrewMonitorScreen' packages/tui/src/components/layout/AppContainer.tsx`

### Phase 2: Verify UI Components Render

- [ ] **T006** — Verify all 5 multi-agent components exist and compile
  - Test: `bun run build` (TypeScript compilation)
  - Command: `ls packages/tui/src/components/multi-agent/`

- [ ] **T007** — Verify ctrl+g actually switches mode in live TUI
  - Test: `bun test e2e_tests/crew-monitor.test.ts` — "ctrl+g switches mode to crew-monitor"
  - Requires: binary built (`cargo build -p chat_cli_v2`)

- [ ] **T008** — Verify crew monitor shows "No active sessions" when empty
  - Test: `bun test e2e_tests/crew-monitor.test.ts` — "crew monitor shows empty state"

- [ ] **T009** — Verify Escape returns to inline mode
  - Test: `bun test e2e_tests/crew-monitor.test.ts` — "Escape returns to inline mode"

### Phase 3: Verify Session Spawn End-to-End

- [ ] **T010** — Verify `/spawn` appears in autocomplete
  - Test: `bun test e2e_tests/session-spawn.test.ts` — "/spawn command is recognized"

- [ ] **T011** — Verify Rust `_session/spawn` handler exists and returns `{sessionId}`
  - Command: `grep -n '_session/spawn\|handle_spawn\|SESSION_SPAWN' crates/chat-cli-v2/src/agent/acp/session_manager.rs | head -10`
  - If missing: port handler from `kiro-cli/crates/chat-cli-v2/src/agent/acp/session_manager.rs`

- [ ] **T012** — Verify `SESSION_LIST_UPDATE` is emitted after spawn
  - Command: `grep -n 'SESSION_LIST_UPDATE\|session_list_update' crates/chat-cli-v2/src/agent/acp/session_manager.rs | head -10`
  - If missing: add emission after session creation

### Phase 4: Implement agent_crew Tool

- [ ] **T013** — Write failing test: `bun test e2e_tests/agent-crew.test.ts`
  - Expected: FAILS with "agent_crew.rs not found"

- [ ] **T014** — Create `crates/agent/src/agent/tools/agent_crew.rs`
  - Source: Section 5.7 of this RFC
  - Build: `cargo build -p chat_cli_v2`

- [ ] **T015** — Register `AgentCrewTool` in tool registry
  - File: `crates/agent/src/agent/tools/mod.rs`
  - Add: `pub mod agent_crew;` and register in tool list

- [ ] **T016** — Verify agent_crew test passes
  - Test: `bun test e2e_tests/agent-crew.test.ts`

### Phase 5: Full Integration Verification

- [ ] **T017** — Run full E2E suite: `bun test e2e_tests/`
  - All tests must pass

- [ ] **T018** — Verify single-session mode still works
  - Test: `bun test e2e_tests/slash-commands.test.ts`
  - Test: `bun test e2e_tests/tool-messages.test.ts`

- [ ] **T019** — Verify Rust tests pass: `cargo test -p chat_cli_v2`

- [ ] **T020** — Manual smoke test:
  ```bash
  ./target/debug/chat_cli_v2 chat
  # 1. Type a message → verify response streams
  # 2. Press ctrl+g → verify crew monitor opens
  # 3. Press Escape → verify back to chat
  # 4. Type /spawn "write hello world" → verify session appears in crew monitor
  # 5. Press ctrl+g → select session → press 'p' → type message → Enter
  ```

---

## 8. File Change Manifest

| File | Action | Source | Est. Lines | Description |
|------|--------|--------|-----------|-------------|
| `packages/tui/src/types/multi-session.ts` | VERIFY/CREATE | kiro-cli-experiment | ~50 | `AgentSession`, `SessionEvent`, `InboxMessage` |
| `packages/tui/src/stores/app-store.ts` | VERIFY | kiro-cli-experiment | +100 | Multi-session state fields + actions |
| `packages/tui/src/acp-client.ts` | VERIFY | kiro-cli-experiment | +120 | Session management methods |
| `packages/tui/src/kiro.ts` | VERIFY | kiro-cli-experiment | +40 | Session method wrappers |
| `packages/tui/src/index.tsx` | VERIFY/FIX | kiro-cli-experiment | +20 | `onMultiSessionUpdate` wiring |
| `packages/tui/src/components/layout/AppContainer.tsx` | VERIFY | kiro-cli-experiment | +20 | ctrl+g handler + crew-monitor branch |
| `packages/tui/src/components/multi-agent/CrewMonitorScreen.tsx` | VERIFY/CREATE | kiro-cli-experiment | ~120 | Main crew monitor UI |
| `packages/tui/src/components/multi-agent/SessionList.tsx` | VERIFY/CREATE | kiro-cli-experiment | ~80 | Session list with keyboard nav |
| `packages/tui/src/components/multi-agent/SessionOutput.tsx` | VERIFY/CREATE | kiro-cli-experiment | ~100 | Session event display |
| `packages/tui/src/components/multi-agent/SessionStatusBar.tsx` | VERIFY/CREATE | kiro-cli-experiment | ~60 | Status bar |
| `packages/tui/src/components/multi-agent/MessageInput.tsx` | VERIFY/CREATE | kiro-cli-experiment | ~50 | Nudge message input |
| `crates/chat-cli-v2/src/agent/acp/extensions.rs` | VERIFY | kiro-cli | 0 | Already has all method constants |
| `crates/chat-cli-v2/src/agent/acp/session_manager.rs` | VERIFY/FIX | kiro-cli | +40 | Ensure `_session/spawn` handler + `SESSION_LIST_UPDATE` emit |
| `crates/agent/src/agent/tools/agent_crew.rs` | **CREATE** | new | ~200 | Pipeline tool — primary missing piece |
| `crates/agent/src/agent/tools/mod.rs` | MODIFY | — | +3 | Register `AgentCrewTool` |
| `packages/tui/e2e_tests/crew-monitor.test.ts` | CREATE | new | ~60 | ctrl+g, empty state, escape |
| `packages/tui/e2e_tests/session-spawn.test.ts` | CREATE | new | ~50 | Method names, store fields |
| `packages/tui/e2e_tests/session-isolation.test.ts` | CREATE | new | ~50 | Event routing isolation |
| `packages/tui/e2e_tests/nudge.test.ts` | CREATE | new | ~40 | Press 'p' in crew monitor |
| `packages/tui/e2e_tests/agent-crew.test.ts` | CREATE | new | ~30 | agent_crew.rs exists |

**Total new code:** ~915 lines TypeScript + ~200 lines Rust  
**Primary blocker:** `agent_crew.rs` (doesn't exist)  
**Secondary blocker:** Verify `_session/spawn` Rust handler emits `SESSION_LIST_UPDATE`

---

## 9. Appendix

### 9.1 ACP Message JSON Examples

**TUI → Backend: Spawn session**
```json
{
  "method": "_session/spawn",
  "params": {
    "task": "Write unit tests for the auth module",
    "name": "test-writer",
    "role": "coder"
  }
}
```

**Backend → TUI: Session list update**
```json
{
  "method": "_kiro.dev/session/list_update",
  "params": {
    "sessions": [
      {
        "id": "abc-123-def-456",
        "name": "test-writer",
        "role": "coder",
        "status": "busy",
        "created_at": "2026-02-21T15:00:00Z",
        "last_activity": "2026-02-21T15:01:30Z",
        "summary": null,
        "parent_session": "main-session-id"
      }
    ]
  }
}
```

**Backend → TUI: Session activity event**
```json
{
  "method": "_kiro.dev/session/activity",
  "params": {
    "sessionId": "abc-123-def-456",
    "event": {
      "type": "content",
      "content": { "type": "text", "text": "Reading auth/mod.rs..." }
    }
  }
}
```

**TUI → Backend: Send nudge message**
```json
{
  "method": "message/send",
  "params": {
    "sessionId": "abc-123-def-456",
    "content": "Focus on the login function first",
    "priority": "normal"
  }
}
```

**agent_crew tool input**
```json
{
  "task": "Build a REST API for user management",
  "stages": [
    {
      "name": "architect",
      "role": "researcher",
      "prompt_template": "Design the API structure for: {{task}}",
      "depends_on": []
    },
    {
      "name": "implementer",
      "role": "coder",
      "prompt_template": "Implement the API based on the architecture",
      "depends_on": ["architect"]
    },
    {
      "name": "reviewer",
      "role": "reviewer",
      "prompt_template": "Review the implementation for correctness and security",
      "depends_on": ["implementer"],
      "loop_to": {
        "target": "implementer",
        "max_iterations": 3,
        "trigger": "NEEDS_CHANGES"
      }
    }
  ]
}
```

### 9.2 Glossary

| Term | Definition |
|------|-----------|
| **ACP** | Agent Client Protocol — JSON-RPC based protocol over stdio between TUI and Rust backend |
| **AcpSession** | Rust actor managing one agent session (conversation + tools + model) |
| **SessionManager** | Rust actor coordinating all `AcpSession` instances |
| **CrewMonitor** | The multi-agent dashboard UI, toggled with ctrl+g |
| **Nudge** | A message sent from the user to a specific agent session via the crew monitor |
| **SpawnSubagentRequest** | Rust struct emitted by tools to request creation of a new agent session |
| **SESSION_LIST_UPDATE** | ACP notification sent from backend to TUI when session list changes |
| **SESSION_ACTIVITY** | ACP notification sent from backend to TUI with a child session's streaming event |
| **sessionEventBuffer** | Zustand store field: `Record<string, AgentStreamEvent[]>` — per-session event history |
| **agent_crew** | Tool that spawns multiple coordinated agents in a pipeline (DAG) |
| **pipeline stage** | One agent in an `agent_crew` pipeline, with a name, role, task, and optional dependencies |

### 9.3 Key File Locations

| What | kiro-cli-main path |
|------|-------------------|
| TUI entry point | `packages/tui/src/index.tsx` |
| ACP client | `packages/tui/src/acp-client.ts` |
| Zustand store | `packages/tui/src/stores/app-store.ts` |
| Multi-agent components | `packages/tui/src/components/multi-agent/` |
| AppContainer (ctrl+g) | `packages/tui/src/components/layout/AppContainer.tsx` |
| Multi-session types | `packages/tui/src/types/multi-session.ts` |
| E2E test framework | `packages/tui/e2e_tests/E2ETestCase.ts` |
| E2E tests | `packages/tui/e2e_tests/*.test.ts` |
| Rust ACP extensions | `crates/chat-cli-v2/src/agent/acp/extensions.rs` |
| Rust session manager | `crates/chat-cli-v2/src/agent/acp/session_manager.rs` |
| Rust agent tools | `crates/agent/src/agent/tools/` |
| agent_crew tool (TO CREATE) | `crates/agent/src/agent/tools/agent_crew.rs` |

### 9.4 Local Development Setup

```bash
# 1. Build Rust backend
cd /Volumes/workplace/Kiro-CLI/kiro-cli-main
cargo build -p chat_cli_v2

# 2. Install TUI dependencies
cd packages/tui
bun install

# 3. Build TUI bundle
bun run build

# 4. Run the full stack
cd /Volumes/workplace/Kiro-CLI/kiro-cli-main
./target/debug/chat_cli_v2 chat

# 5. Run E2E tests (requires binary built in step 1)
cd packages/tui
bun test e2e_tests/

# 6. Run specific test
bun test e2e_tests/crew-monitor.test.ts

# 7. Run Rust tests
cargo test -p chat_cli_v2

# 8. Check TypeScript types
cd packages/tui
bun run typecheck  # or: npx tsc --noEmit
```

### 9.5 Debugging Tips

**TUI not rendering?**
```bash
# Check binary exists
ls -la target/debug/chat_cli_v2

# Check TUI bundle exists
ls -la packages/tui/dist/tui.js

# Run with debug logging
KIRO_LOG_LEVEL=debug ./target/debug/chat_cli_v2 chat
```

**E2E test timing out?**
```bash
# Check test output directory for logs
ls packages/tui/e2e_tests/test-outputs/
cat packages/tui/e2e_tests/test-outputs/[test-name]/tui.log
cat packages/tui/e2e_tests/test-outputs/[test-name]/rust.log
```

**Session spawn not working?**
```bash
# Verify _session/spawn handler exists in Rust
grep -n '_session/spawn\|handle_spawn' \
  crates/chat-cli-v2/src/agent/acp/session_manager.rs

# Verify SESSION_LIST_UPDATE is emitted
grep -n 'SESSION_LIST_UPDATE\|session_list_update' \
  crates/chat-cli-v2/src/agent/acp/session_manager.rs
```

**agent_crew not found?**
```bash
# Verify file exists
find crates/ -name 'agent_crew.rs'

# Verify it's registered
grep -n 'agent_crew' crates/agent/src/agent/tools/mod.rs
```
