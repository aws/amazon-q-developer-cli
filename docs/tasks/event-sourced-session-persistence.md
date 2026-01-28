# Event-Sourced Session Persistence - Implementation Plan

## Problem Statement
Implement an event-sourced state management system for agent sessions that captures all conversation state changes via an append-only log, persists sessions to the filesystem, and supports `session/load` functionality.

## Requirements
1. Event log as source of truth - `Vec<Message>` derived from log entries, not directly mutable
2. Log entry types: `Prompt`, `AssistantMessage`, `ToolResult`, `Compaction`, `ResetTo`
3. Each `ToolResult` stores the `Tool` for potential future undo operations
4. Filesystem persistence with JSON metadata and JSONL log files
5. Save on every log append event
6. Compaction entries store summary + messages snapshot after compaction (natural snapshot point)
7. Versioned log entries using `#[serde(tag = "version")]`

## Background
- Current `ConversationState` has `messages: Vec<Message>` that's directly mutated in 6 places in `agent/mod.rs`
- `Tool` struct already contains all info needed to re-execute (for future redo support)
- TUI already has `loadSession(sessionId)` stub in `acp-client.ts`

## Proposed Solution

### Event-Sourced Architecture
1. `ConversationState` owns an `EventLog` internally
2. All mutations go through log entry append operations
3. `messages()` returns a derived view by replaying the log
4. Compaction entries serve as snapshot points to avoid full replay

### Filesystem Storage
Sessions stored under `~/.kiro/sessions/cli/`:
```
~/.kiro/sessions/cli/
  {session_id}.json      # { session_id, cwd, created_at, updated_at, session_state }
  {session_id}.jsonl     # one LogEntry per line (append-only)
  {session_id}.lock      # { pid, started_at } - only when session is active
```

### Access Patterns
- **By ID**: Direct file read
- **By CWD**: Scan `*.json` files, filter by `cwd` field, return all matches (consumer sorts)

### Locking
- Atomic create with `O_CREAT | O_EXCL` for lock file
- Lock contains PID + timestamp
- Stale lock detection via PID liveness check
- UX: error + prompt to start new session if locked

## Progress

### Task 1: Define LogEntry types and EventLog structure ✅
- Created `crates/agent/src/agent/event_log.rs`
- Added `ToolCallResult::to_tool_result_block()` method in `protocol.rs`
- Exported `event_log` and `compact` modules in `mod.rs`
- All log entry types store `message_id` for proper message reconstruction

### Task 2: Refactor ConversationState to use EventLog internally ✅
- `ConversationState` now has `event_log: EventLog` and `messages_cache: Option<Vec<Message>>`
- `messages_cache` is `#[serde(skip)]` - derived lazily or updated incrementally via `LogEntry::apply()`
- Append methods return `(LogEntry, usize)` for event emission

### Task 3: Update agent/mod.rs mutation sites to use new API ✅
- All 15 `.messages` field accesses updated to use new API methods
- Helper methods `append_user_message()` and `append_assistant_message()` encapsulate event emission

### Task 4: Add AgentEvent for log appended ✅
- Added `AgentEvent::LogEntryAppended { entry: LogEntry, index: usize }` variant
- Integration test verifies events are emitted with sequential indices

### Task 5: Add sessions_dir to paths.rs and define session types ✅
- Added `sessions_dir() -> Result<PathBuf>` to `paths.rs`
- Created `crates/chat-cli/src/agent/session/mod.rs`
- Defined `SessionData` (metadata), `SessionState` (versioned), `SessionDb` (active session with lock)
- Added `is_pid_alive()` with platform-specific implementations
- Unit tests for serialization and CRUD operations

## Task Breakdown

### Task 6: Implement session lock management ✅
**Objective:** Prevent concurrent access to the same session

**Implementation:**
- `SessionLockGuard` struct holding lock file path, implements `Drop` to delete lock file
- `acquire_lock_impl(lock_path, is_pid_alive)` - testable with closure for PID checking
- Atomic create with `OpenOptions::new().write(true).create_new(true)`
- Stale lock detection and cleanup

**Test:**
- Acquire lock succeeds on fresh session
- Acquire lock fails when lock file exists with "alive" PID
- Stale lock cleanup works

---

### Task 7: Implement SessionDb with CRUD operations ✅
**Objective:** Encapsulate session lifecycle with lock guard

**Implementation:**
- `SessionDb::new(session_id, cwd, state)` - creates session, acquires lock
- `SessionDb::load(session_id, cwd)` - loads session, optionally updates cwd
- `append_log_entry(&self, entry)` - appends to JSONL
- `update_state(&mut self, state)` - atomic metadata update
- `load_log_entries(&self)` - reads JSONL, skips malformed lines

**Test:**
- Create session, verify files exist
- Load session, verify metadata matches
- Append entries, load back, verify content
- Malformed last line skipped

---

### Task 8: Implement list_sessions_by_cwd ✅
**Objective:** Find all sessions for a given directory

**Implementation:**
- `list_sessions_by_cwd(cwd)` - scans `*.json`, filters by canonicalized cwd
- Returns all matches (consumer sorts)

**Test:**
- Multiple sessions with different CWDs
- List returns only matching sessions

---

### Task 9: Wire up auto-save on log append ✅
**Objective:** Save to filesystem whenever a log entry is appended

**Implementation:**
- `RtsModelState` refactored to use interior mutability with `RtsModelStateInner` (serializable)
- `RtsModel` holds `Arc<RtsModelState>` shared with `AcpSession`
- `SessionDb` uses interior mutability for `&self` API
- `AcpSession` creates `SessionDb` on startup with session_id from ACP and cwd from `NewSessionRequest`
- `conversation_id` in RTS is now the same as ACP session_id (single source of truth)
- `LogEntryAppended` events trigger `session_db.append_log_entry()`
- `EndTurn` events trigger `session_db.update_state()` with snapshot from agent

**Test:** Integration test: run agent, verify entries appear in `.jsonl`

**Demo:** Session automatically persists as conversation progresses

---

### Task 10: Implement session/load with Historical Notifications ✅
**Objective:** Load a session from filesystem, restore agent state, and emit historical notifications so TUI displays conversation history.

#### Refactoring Changes

1. **`AcpSessionConfig`** - Made `session_id` and `cwd` required, added `load: bool` flag
   - Added builder-style methods for optional fields
   - Whether session is loaded determined by `config.load`

2. **`SessionManagerRequestData`** - Consolidated `NewSession` and `LoadSession` into `StartSession`
   - Single `StartSession { config, connection_cx, resp_sender }` variant
   - Returns `(AcpSessionHandle, oneshot::Receiver<()>)` - ready receiver signals when historical notifications complete

3. **`AcpSessionBuilder::spawn`** → renamed to **`start_session`**
   - Returns `(AcpSessionHandle, oneshot::Receiver<()>)`

4. **`InnerSender`** - No changes needed (intentional design to prevent circular references through channel)

5. **`EventLog::new`** - Updated to accept `Vec<LogEntry>` parameter

6. **`ConversationState::new`** - Updated to accept `Uuid` and `Vec<LogEntry>` parameters
   - Added TODO to remove Default implementation

#### Core Implementation

7. **Added `emit_historical_notifications` helper in `AcpSession`**
   - Called in `main_loop` right before entering the `loop {}`
   - Loads entries from `session_db.load_log_entries()`
   - Converts `LogEntry` → `SessionUpdate` notifications via `log_entry_to_session_updates`:
     - `Prompt` → `UserMessageChunk` (one per content block)
     - `AssistantMessage` → `AgentMessageChunk` (one per content block)
     - `ToolResult` → `ToolCall` (with completed/failed status)
     - Ignores `Compaction` and `ResetTo`

8. **Updated `AcpSession::with_builder`** to handle loading:
   - If `config.load` is true, calls `SessionDb::load()` and reconstructs state from log entries
   - Otherwise creates new session with `SessionDb::new()`

9. **Updated `main_loop`** to accept `ready_tx: oneshot::Sender<()>`
   - Emits historical notifications before entering loop
   - Signals ready after notifications complete

10. **LoadSession ACP handler** waits on `ready_rx` before responding
    - Ensures all historical notifications are sent before client receives response

#### Files Modified

- `crates/agent/src/agent/event_log.rs` - `EventLog::new` accepts entries
- `crates/agent/src/agent/types.rs` - `ConversationState::new` accepts id and entries
- `crates/chat-cli/src/agent/acp/acp_agent.rs` - Config, builder, load handling, historical notifications
- `crates/chat-cli/src/agent/acp/session_manager.rs` - Consolidated to `StartSession`
- `crates/chat-cli/src/agent/acp/subagent_tool.rs` - Updated to use new API

**Demo:** After loading a session, the TUI immediately displays the full conversation history without needing to replay the conversation.

#### Integration Test Infrastructure

11. **Documented mock data recording** in `crates/chat-cli/README.md`
    - `KIRO_RECORD_API_RESPONSES_PATH=tests/mock_responses/{test_name}.jsonl cargo run -- chat --legacy-mode`
    - Records all API response events to JSONL file

12. **Added `push_mock_responses_from_file` to `AcpTestHarness`**
    - Loads mock responses from JSONL file
    - Parses format: JSON events per line, blank lines separate response streams, `//` comments

13. **Test data stored in `tests/mock_responses/`**
    - `hello.jsonl` - fs_write tool call for "write hello world in bash"

14. **Updated test skeleton in `acp.rs`** with file-based approach
    - TODO: Implement `load_session_emits_history` test using mock data

---

## Future Work (Not in Scope)

The following features are deferred:

- **Session cleanup/expiry policy** - Automatic deletion of old sessions
- **Max sessions limit** - Cap on number of sessions per directory
- **Virtual filesystem for undo validation** - Track touched files, validate undo feasibility
- **Tool undo operations** - Add `can_undo()` and `undo()` to Tool/BuiltInTool
- **ResetTo with undo support** - Reset conversation and optionally undo filesystem changes
- **Undo of ResetTo (redo)** - Re-apply original tools when undoing a ResetTo
- **Session export/import** - Share sessions between machines
