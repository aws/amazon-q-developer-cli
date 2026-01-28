# Grep and Glob Tools Implementation

## Problem Statement
Implement grep and glob tools in the new agent crate architecture, migrating the core functionality from the old `chat-cli` implementation while adapting to the new tool patterns.

## Requirements
- Grep and glob each have their own settings struct in `definitions.rs`
- Add `allow_read_only` field to settings for both tools
- Return JSON output format for structured parsing
- Preserve hard constraints (max limits) from old implementation
- Integrate with centralized permission evaluation in `permissions.rs`
- Cover the same test categories as the old implementation

## Hard Constraints (from old implementation)

### Grep
- `MAX_ALLOWED_MATCHES_PER_FILE=30`
- `MAX_ALLOWED_FILES=400`
- `MAX_ALLOWED_TOTAL_LINES=300`
- `MAX_ALLOWED_DEPTH=50`
- `MAX_LINE_LENGTH=500`
- `DEFAULT_MAX_MATCHES_PER_FILE=5`
- `DEFAULT_MAX_FILES=100`
- `DEFAULT_MAX_TOTAL_LINES=100`
- `DEFAULT_MAX_DEPTH=30`

### Glob
- `DEFAULT_MAX_RESULTS=200`
- `MAX_ALLOWED_DEPTH=50`
- `DEFAULT_MAX_DEPTH=30`

## Task Breakdown

- [x] **Task 1**: Add settings structs to definitions.rs
- [x] **Task 2**: Implement Grep tool struct and BuiltInToolTrait
- [x] **Task 3**: Implement Grep validate and execute methods
- [x] **Task 4**: Implement Glob tool struct and BuiltInToolTrait
- [x] **Task 5**: Implement Glob validate and execute methods
- [x] **Task 6**: Integrate tools into mod.rs and permissions.rs
- [x] **Task 7**: Wire up tool execution in agent mod.rs

## Task Details

### Task 1: Add settings structs to definitions.rs
- Objective: Define `GrepSettings` and `GlobSettings` structs with `allowed_paths`, `denied_paths`, `allow_read_only` fields
- Add fields to `ToolsSettings` struct with appropriate serde aliases
- Test: Verify deserialization works with a unit test

### Task 2: Implement Grep tool struct and BuiltInToolTrait
- Objective: Create `Grep` struct with all parameters (pattern, path, include, case_sensitive, output_mode, limits)
- Implement `BuiltInToolTrait` with description, schema, aliases
- Add constants for hard limits and defaults

### Task 3: Implement Grep validate and execute methods
- Objective: Implement `validate()` for regex/path validation and `execute()` for search logic
- Support three output modes: content, files_with_matches, count
- Include line truncation for long lines
- Tests: Content mode, files_with_matches mode, count mode, case insensitivity, include filter, no matches, long line truncation

### Task 4: Implement Glob tool struct and BuiltInToolTrait
- Objective: Create `Glob` struct with parameters (pattern, path, limit, max_depth)
- Implement `BuiltInToolTrait` with description, schema, aliases
- Add constants for hard limits and defaults

### Task 5: Implement Glob validate and execute methods
- Objective: Implement `validate()` for pattern validation and `execute()` for file discovery
- Handle pattern normalization (directory prefix extraction)
- Tests: Basic file finding, recursive patterns, path prefix patterns, truncation, no matches

### Task 6: Integrate tools into mod.rs and permissions.rs
- Objective: Register Grep/Glob in `BuiltInToolName`, `BuiltInTool` enum
- Wire up `from_parts`, `generate_tool_spec`, `tool_name`, `canonical_tool_name`, `aliases`
- Add permission evaluation cases in `permissions.rs` using `evaluate_permission_for_paths`
- Tests: Permission evaluation (path inside/outside CWD, allow_read_only, denied paths)

### Task 7: Wire up tool execution in agent mod.rs
- Objective: Add execution paths in `start_tool_execution` and validation in `validate_tool`
