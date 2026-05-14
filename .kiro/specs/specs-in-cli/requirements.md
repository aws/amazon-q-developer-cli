# Spec-Mode Support in Kiro CLI TUI

## Introduction

The Kiro CLI TUI (V2, KAS engine) needs first-class support for the spec workflow — the structured requirements → design → tasks development flow powered by the kiro-agent-server. This spec covers wiring the existing KAS spec-mode ACP machinery into the TUI so users can create, browse, resume, and execute specs entirely from the terminal.

## Glossary

- **Spec**: A structured feature development workflow that produces `requirements.md`, `design.md`, and `tasks.md` under `.kiro/specs/<featureName>/`.
- **KAS**: Kiro Agent Server — the TypeScript agent engine that implements spec-mode.
- **ACP**: Agent Client Protocol — the JSON-RPC protocol between the TUI and the agent.
- **Mode**: An ACP session config option (`configId: 'mode'`) that switches the agent's behavior (vibe, spec, autonomous).
- **`current_mode_update`**: An ACP session notification emitted by the agent when the active mode changes.
- **`_kiro/spec/resolveSession`**: ACP ext method to get/create the session the agent uses for a spec feature.
- **`_kiro/spec/invoke`**: ACP ext method to trigger spec operations (executeTask, runAllTasks, generateDocument).
- **`_kiro/spec/taskStatusChanged`**: ACP notification emitted when a task's execution status changes.

## Requirements

### Requirement 1: Mode Change Propagation

**User Story:** As a user, I want the TUI to reflect the current agent mode in real-time so I always know which mode I'm working in.

#### Acceptance Criteria

1. WHEN the agent emits a `current_mode_update` notification with a new `currentModeId`, THE TUI SHALL update the agent chip (bottom bar) to display the new mode name.
2. WHEN the mode changes, THE TUI SHALL broadcast an `AgentSwitched` event with `agentName`, `previousAgentName`, and `welcomeMessage` (if the mode has one in `_meta`).
3. WHEN the `currentModeId` in the notification matches the already-cached mode, THE TUI SHALL NOT broadcast a spurious `AgentSwitched` event.
4. WHEN the user explicitly switches mode via `/spec` or `/agent spec`, THE TUI SHALL update the agent chip immediately (not wait for the async notification).

### Requirement 2: `/spec` Slash Command — Feature Discovery

**User Story:** As a user, I want to type `/spec` to see all my existing specs so I can quickly pick one to work on.

#### Acceptance Criteria

1. WHEN the user types `/spec` with no arguments, THE TUI SHALL scan `.kiro/specs/` in the workspace root for feature directories.
2. THE TUI SHALL display a selection menu listing each feature name with a description of which documents exist (e.g. "requirements, design, tasks").
3. WHEN no specs exist, THE TUI SHALL show a warning alert with usage guidance (`/spec new <name>`).
4. WHEN the user selects a feature from the menu, THE TUI SHALL switch to spec mode and send a resume prompt to the agent.

### Requirement 3: `/spec new <name>` — Create New Spec

**User Story:** As a user, I want to start a new spec workflow by name so the agent creates the feature directory and begins drafting requirements.

#### Acceptance Criteria

1. WHEN the user types `/spec new <name>`, THE TUI SHALL switch the session mode to `spec`.
2. THE TUI SHALL send a prompt instructing the agent to create `.kiro/specs/<name>/` and draft the initial requirements document.
3. WHEN `<name>` is not provided, THE TUI SHALL show a usage error.

### Requirement 4: `/spec run <name>` — Execute All Tasks

**User Story:** As a user, I want to run all tasks in a spec so the agent implements the feature autonomously.

#### Acceptance Criteria

1. WHEN the user types `/spec run <name>`, THE TUI SHALL verify that `.kiro/specs/<name>/tasks.md` exists.
2. THE TUI SHALL call `_kiro/spec/resolveSession` to obtain the spec session ID.
3. THE TUI SHALL call `_kiro/spec/invoke` with `operation: 'runAllTasks'`, passing the `tasksFilePath` and `specDocuments`.
4. THE TUI SHALL show a success alert indicating the agent is working autonomously.
5. WHEN `tasks.md` does not exist, THE TUI SHALL show an error prompting the user to generate it first.
6. WHEN the agent does not support `_kiro/spec/invoke`, THE TUI SHALL show a clear error message.

### Requirement 5: `/spec <name>` — Resume Existing Spec

**User Story:** As a user, I want to resume work on an existing spec by name so the agent picks up where it left off.

#### Acceptance Criteria

1. WHEN the user types `/spec <name>` (where `<name>` matches an existing feature directory), THE TUI SHALL switch to spec mode.
2. THE TUI SHALL send a prompt telling the agent which documents exist for the feature.
3. WHEN the feature directory does not exist, THE TUI SHALL show an error.

### Requirement 6: Task Status Notifications (Future)

**User Story:** As a user, I want to see task progress in the TUI so I know which tasks have completed, are in progress, or failed.

#### Acceptance Criteria

1. WHEN the agent emits `_kiro/spec/taskStatusChanged`, THE TUI SHALL surface the change (e.g. transient alert or activity tray update).
2. THE notification payload includes `tasksFilePath`, `sessionId`, and an array of `changes` with `taskId`, `executionStatus`, `lastSessionId`, and `lastExecutionId`.

### Requirement 7: Spec Workspace Scanning

**User Story:** As a developer, I want a pure utility module for scanning `.kiro/specs/` so the command layer stays slim and the logic is easy to unit-test.

#### Acceptance Criteria

1. THE utility SHALL enumerate directories under `.kiro/specs/` and report which well-known documents exist (`requirements.md`, `design.md`, `tasks.md`, `bugfix.md`).
2. THE utility SHALL skip dotfiles and non-directories.
3. THE utility SHALL never throw — it returns an empty array when the directory doesn't exist or can't be read.
4. THE utility SHALL sort features alphabetically.
