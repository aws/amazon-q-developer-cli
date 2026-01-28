# E2E Testing Implementation

**Status:** In Progress  
**Started:** 2025-12-29

## Problem Statement
Implement comprehensive E2E testing that validates the complete Kiro CLI chat application (TUI + Rust backend) using real processes with mocked LLM API responses, following the same sequential test flow as integration tests.

## Requirements
- Sequential test flow API similar to existing `TestCase` structure
- File-based mock JSONL responses for LLM API calls
- IPC querying of both Zustand store (AppState) and ACP session state (AgentSnapshot)
- Shared test utilities package to eliminate code duplication
- Environment variable activation with clear separation between test modes
- Run actual `kiro-cli chat` command for authentic E2E testing

## Background
Current architecture has:
- TUI integration tests with PTY + IPC for Zustand state querying using `KIRO_TEST_MODE=true`
- Rust agent tests with mock API responses via JSONL files
- Duplicated `TestCommand`/`TestResponse` interfaces
- Type generation from Rust to TypeScript via typeshare

## Proposed Solution
Extend the TUI package with E2E testing capabilities while creating shared test utilities within the TUI package for PTY management and IPC communication. Use `KIRO_MOCK_ACP=true` for integration tests (mock ACP) and default behavior for E2E tests (real ACP with mocked API).

## Task Breakdown

### Task 1: Refactor shared test utilities within TUI package ✅
- Extract shared PTY utilities (creation, key sending, output assertion) from integration tests into `src/test-utils/shared/`
- Consolidate duplicated `TestCommand`/`TestResponse` interfaces into shared module
- Update integration tests to use `KIRO_MOCK_ACP=true` environment variable
- Demo: Integration tests still pass with refactored utilities and new environment variable

### Task 2: Add typeshare support to agent crate for E2E testing ✅
- Add `typeshare` dependency to `crates/agent/Cargo.toml`
- Add `#[typeshare]` annotation to `AgentSnapshot` struct
- Update type generation script to output agent types to `packages/tui/e2e_tests/types/`
- Add comment in script clarifying agent types are for E2E testing only
- Demo: `AgentSnapshot` TypeScript types are generated in E2E test directory

### Task 3: Implement Rust backend IPC support ✅
- Add `KIRO_TEST_MODE` environment variable detection in Rust backend
- Implement IPC socket server for test commands in agent
- Add `GET_AGENT_STATE` command to return current `AgentSnapshot`
- Demo: Rust backend responds to IPC test commands

### Task 4: Create E2E TestCase implementation ✅
- Create `packages/tui/e2e_tests/E2ETestCase.ts`
- Implement `E2ETestCase` class using shared PTY utilities
- Support IPC connection to Rust backend for agent state querying
- Demo: Basic E2E test can spawn CLI and establish IPC connections

### Task 5: Implement IPC-based mock model for dynamic response injection ✅
- Create `IpcModel` implementation of `Model` trait in Rust that receives responses via IPC
- Add IPC command to push mock responses from test to agent
- Enable tests to inject responses dynamically during execution (for testing delays, etc.)
- Demo: E2E test can inject mock response and verify agent processes it

### Task 6: Implement hello world E2E test ✅
- Create mock response JSONL file for "hello world" interaction
- Write test that sends "hello world\n", injects mock response, verifies output
- Assert on AgentSnapshot (session state)
- Demo: Complete E2E test passes with real CLI process and mock API responses

## Progress Log

### 2025-12-31 - Task 6 Complete
Completed E2E testing infrastructure cleanup:
- ✅ Updated `pushMockResponse` to use generated `StreamResult` type from typeshare
- ✅ Renamed `KIRO_LOG_FILE` to `KIRO_TUI_LOG_FILE` for TUI logging (optional - no logging if not set)
- ✅ Added `KIRO_CHAT_LOG_FILE` env var for Rust backend log file override
- ✅ Added `KIRO_TUI_LOG_LEVEL` env var for TUI log level (separate from Rust log level)
- ✅ Created shared `getTestPaths()` utility in `src/test-utils/shared/test-paths.ts`
  - Returns `baseDir`, `tuiLogFile`, `rustLogFile`, `tuiIpcSocket`, `agentIpcSocket`
  - Uses `{TMPDIR}/kiro-cli-tests/{testName}/` as base directory
- ✅ Updated `E2ETestCase` to establish dual IPC connections (TUI + Agent)
  - Added `getStore()` method for Zustand store assertions
  - Added `getAgentState()` method for AgentSnapshot assertions
- ✅ Updated `TestCase` (integration tests) to use shared test paths
- ✅ Fixed `TestModeProvider` to check for non-empty `KIRO_TEST_MODE` value
- ✅ All tests pass: chat_cli (458), agent (1), integration (1), E2E (2)

### 2025-12-31 - Task 5.5: Add typeshare to StreamResult and standardize on `kind` tag
Added typeshare support for StreamResult types and standardized tagged union discriminator:
- ✅ Added `#[typeshare]` to `StreamResult`, `StreamEvent`, and related types in agent crate
- ✅ Changed `StreamEvent` to use `#[serde(tag = "kind", content = "data")]` (adjacently tagged for typeshare)
- ✅ Changed `StreamErrorKind`, `ContentBlockStart`, `ContentBlockDelta` to use same format
- ✅ Changed `MetadataUsage` fields from `u64` to `u32` for typeshare compatibility
- ✅ Updated mock response JSONL files to use new adjacently tagged format
- ✅ Renamed `type` to `kind` in `TestCommand`, `TestResponse`, `TestMessageCommand`, `TestMessageResponse`
- ✅ Updated TypeScript IPC types and all usages to use `kind` instead of `type`
- ✅ Updated E2E test to use new StreamResult/StreamEvent format
- ✅ All tests pass: agent (67), chat_cli (458), integration (1), E2E (2)

### 2025-12-31 - Task 5 Complete
Implemented IPC-based mock model for dynamic response injection:
- ✅ Created `IpcModel` in `crates/chat-cli/src/agent/ipc_model.rs` with actor-based design
- ✅ Actor uses `VecDeque<Option<StreamResult>>` buffer to queue multiple responses
- ✅ Added `PUSH_MOCK_RESPONSE` IPC command with `Option<Vec<StreamResult>>` (None = end of response)
- ✅ `AcpSession` uses `IpcModel` instead of `RtsModel` when `KIRO_TEST_MODE` is set
- ✅ Added `pushMockResponse()` method to `E2ETestCase`
- ✅ Added `async-stream` dependency for stream creation in sync context
- ✅ E2E test demonstrates mock response injection: pushes events, sends prompt, verifies output
- ✅ All E2E tests pass (2/2)

### 2025-12-30 - Task 4 Complete
Created E2E TestCase implementation:
- ✅ Created `E2ETestCase` class in `packages/tui/e2e_tests/E2ETestCase.ts`
- ✅ Uses shared `PtyManager` and `TuiIpcConnection` utilities
- ✅ Spawns real `kiro-cli chat` command with proper environment variables
- ✅ Establishes IPC connection to Rust backend for `getAgentState()`
- ✅ Builder pattern API matching integration test `TestCase`
- ✅ Basic E2E test validates connectivity and agent state retrieval
- ✅ All E2E tests pass (2/2), integration tests pass (1/1)

Note: Mock response injection requires new Task 5 to implement `IpcModel` in Rust
that can receive responses dynamically via IPC during test execution.

### 2025-12-30 - Task 3 Refactored
Refactored IPC architecture to proper actor pattern:
- ✅ Renamed `IpcClient` to `IpcServer` (it's a server accepting connections, not a client)
- ✅ `IpcServer` is now a proper actor that spawns and returns a message receiver
- ✅ `AcpSession` refactored to actor pattern with `new()` + `spawn_acp_session()` returning handle
- ✅ `AcpSession::main_loop()` uses `tokio::select!` over:
  1. ACP requests from handle
  2. IPC messages from `IpcServer`
  3. Active prompt task completion
- ✅ `handle_prompt_request` runs in separate spawned task, isolated from main loop
- ✅ Message types use proper enum for `type` field matching TypeScript IPC types
- ✅ All E2E and integration tests pass

### 2025-12-30 - Task 3 Complete
Implemented Rust backend IPC support with full E2E connectivity:
- ✅ Added `KIRO_TEST_MODE` environment variable detection in AcpSession
- ✅ Created IPC client with message-passing architecture using tokio actors
- ✅ Implemented `GET_AGENT_STATE` command returning current `AgentSnapshot`
- ✅ Added `KIRO_TEST_TUI_JS_PATH` environment variable support for E2E testing
- ✅ Updated embedded TUI logic to use system bun + provided JS path in test mode
- ✅ Created `TuiAssetPaths` struct for clean path management
- ✅ Full E2E test passes: TUI launches agent, agent connects via IPC, responds to commands
- ✅ AgentSnapshot successfully serialized and transmitted over IPC

### 2025-12-30 - Task 2 Complete
Added typeshare support to agent crate for E2E testing:
- ✅ Added `typeshare` dependency to workspace and agent crate
- ✅ Added `#[typeshare]` annotation to `AgentSnapshot` struct
- ✅ Updated type generation script to output agent types to `packages/tui/e2e_tests/types/agent.ts`
- ✅ Added comment clarifying agent types are for E2E testing only
- ✅ AgentSnapshot TypeScript interface generated successfully (though dependent types need future work)

### 2025-12-30 - Task 1 Complete
Refactored shared test utilities within TUI package:
- ✅ Created shared IPC types with proper tagged unions (TestMessage, TestCommand, TestResponse)
- ✅ Created TuiIpcConnection for centralized newline-delimited JSON handling
- ✅ Created PtyManager for PTY lifecycle management
- ✅ Updated TestCase and TestModeProvider to use shared utilities
- ✅ Added timing delays to fix race conditions in integration tests
- ✅ Integration tests pass consistently (5/5 runs)

### 2025-12-29 - Task 1 In Progress
Refactored shared test utilities within TUI package:
- ✅ Created shared IPC types with proper tagged unions (TestMessage, TestCommand, TestResponse)
- ✅ Created TestIpcConnection for centralized newline-delimited JSON handling
- ✅ Updated TestModeProvider to use shared utilities
- ⏳ Updating TestCase to use shared utilities and run integration tests
