# Implementation Plan - `use_aws` Tool

## Problem Statement

Implement the `use_aws` tool for the agent runtime in `crates/agent` that allows executing AWS CLI commands with permission controls. Read-only operations should be auto-approved while write operations require user confirmation.

## Requirements

- Tool location: `crates/agent/src/agent/tools/use_aws.rs`
- Data files: `crates/agent/src/data/aws_readonly_operations.json` and `aws_readonly_additions.json`
- Embed JSON directly using `include_str!()` (no compression)
- Permission settings struct with `allowed_services`, `denied_services`, `auto_allow_readonly`
- Unit tests matching original categories

## Background

- Existing tools follow `BuiltInToolTrait` pattern with `name()`, `description()`, `input_schema()`, `aliases()`
- Tools are registered in `mod.rs` via `BuiltInToolName` enum and `BuiltInTool` enum
- Permission evaluation happens in `permissions.rs` via `evaluate_tool_permission()`
- Original readonly list has ~7,000 operations embedded via `include_str!()`

## Proposed Solution

1. Copy JSON data files to `crates/agent/src/data/`
2. Create `use_aws.rs` tool with `LazyLock<HashSet>` for readonly operations
3. Add `UseAwsSettings` to `ToolsSettings` for permission configuration
4. Integrate into permission evaluation system via generic `ReadonlyChecker` trait

## Task Breakdown

### Task 1: Set up data files

- [x] Complete

**Objective:** Copy JSON files to agent crate

**Implementation:**
- Copy `aws_readonly_operations.json` and `aws_readonly_additions.json` from `crates/chat-cli/src/data/` to `crates/agent/src/data/`

---

### Task 2: Create `use_aws.rs` tool with readonly detection

- [x] Complete

**Objective:** Implement `UseAws` struct with deserialization, `BuiltInToolTrait`, and readonly detection

**Implementation:**
- Created `use_aws.rs` with `UseAws` struct (service_name, operation_name, parameters, region, profile_name, label)
- Added `UseAwsRaw` for validation (reject service names starting with `-`)
- Implemented `BuiltInToolTrait` (name, description, input_schema, aliases)
- Added `cli_parameters()` method for kebab-case conversion
- Created `LazyLock<HashSet<&'static str>>` for `AWS_READONLY_OPS` and `AWS_READONLY_ADDITIONS` using `include_str!()`
- Implemented `ReadonlyChecker` trait for readonly detection (shared with `ExecuteCmd`)

**Tests:** `test_is_readonly`, `test_use_aws_deser`, `test_service_name_validation`

---

### Task 3: Implement tool execution

- [x] Complete

**Objective:** Execute AWS CLI commands and return results

**Implementation:**
- Implemented `execute()` method that spawns `aws` CLI process
- Set up environment variables with user agent metadata
- Handle stdout/stderr truncation (MAX_OUTPUT_SIZE / 3)
- Return `ToolExecutionOutput` with JSON result containing exit_status, stdout, stderr

---

### Task 4: Add permission settings and evaluation

- [x] Complete

**Objective:** Integrate with permission system

**Implementation:**
- Added `UseAwsSettings` struct to `definitions.rs` with `allowed_services`, `denied_services`, `auto_allow_readonly` (defaults to `true`)
- Added `use_aws` field to `ToolsSettings`
- Created generic `ReadonlyChecker` trait in `permissions.rs`
- Made `evaluate_permission_for_command` generic over `ReadonlyChecker`
- Implemented `ReadonlyChecker` for both `ExecuteCmd` and `UseAws`
- Added permission evaluation case in `permissions.rs` for `BuiltInTool::UseAws`

---

### Task 5: Register tool in agent system

- [x] Complete

**Objective:** Make tool available to the agent

**Implementation:**
- Added `pub mod use_aws;` to `tools/mod.rs`
- Added `UseAws` to `BuiltInToolName` enum with aliases `["use_aws", "aws"]`
- Added `UseAws` variant to `BuiltInTool` enum
- Wired up `from_parts()`, `generate_tool_spec()`, `tool_name()`, `canonical_tool_name()`, `aliases()`
- Added `UseAws` to `validate_tool()` and `execute_tool()` in `mod.rs`
- Added `UseAws` to `get_tool_kind()` in `chat_cli/acp_agent.rs`
- Added `convert_case` dependency to agent crate

**Tests:** `cargo test -p agent --lib use_aws` passes (3 tests)
