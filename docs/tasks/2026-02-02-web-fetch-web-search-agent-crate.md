# Implementation Plan: web_fetch and web_search Tools for Agent Crate

## Overview
Port the `web_fetch` and `web_search` tools from `crates/chat-cli` to `crates/agent`. The implementation follows the existing tool pattern in the agent crate.

## Phase 1: Add Dependencies
**File:** `crates/agent/Cargo.toml`

Add required dependencies:
- `html2text = "0.12"` (for HTML stripping in web_fetch)

Note: `reqwest` and `url` are already present in the agent crate.

## Phase 2: Create web_fetch Tool
**File:** `crates/agent/src/agent/tools/web_fetch.rs`

1. Create struct `WebFetch` with fields:
   - `url: String`
   - `mode: FetchMode` (enum: Selective, Truncated, Full)
   - `search_terms: Option<String>`

2. Implement `BuiltInToolTrait`:
   - `name()` → `BuiltInToolName::WebFetch`
   - `description()` → tool description
   - `input_schema()` → JSON schema
   - `aliases()` → `&["web_fetch"]`

3. Implement `execute()` method:
   - HTTP fetch with reqwest
   - HTML stripping with html2text
   - Content extraction based on mode (selective/truncated/full)
   - Return `ToolExecutionResult`

## Phase 3: Create web_search Tool
**File:** `crates/agent/src/agent/tools/web_search.rs`

1. Create struct `WebSearch` with field:
   - `query: String`

2. Implement `BuiltInToolTrait`:
   - `name()` → `BuiltInToolName::WebSearch`
   - `description()` → tool description
   - `input_schema()` → JSON schema
   - `aliases()` → `&["web_search"]`

3. Implement `execute()` method:
   - Call `model.invoke_mcp()` to perform search
   - Parse and return search results as JSON
   - Return `ToolExecutionResult`

## Phase 4: Register Tools in mod.rs
**File:** `crates/agent/src/agent/tools/mod.rs`

1. Add module declarations:
   ```rust
   pub mod web_fetch;
   pub mod web_search;
   ```

2. Add imports for `WebFetch` and `WebSearch`

3. Add to `BuiltInToolName` enum:
   ```rust
   WebFetch,
   WebSearch,
   ```

4. Add to `BuiltInTool` enum:
   ```rust
   WebFetch(WebFetch),
   WebSearch(WebSearch),
   ```

5. Update all match arms in `BuiltInTool` impl:
   - `from_parts()`
   - `generate_tool_spec()`
   - `tool_name()`
   - `canonical_tool_name()`
   - `aliases()`

6. Update `BuiltInToolName::aliases()` match arm

## Phase 5: Wire Up Tool Execution
**File:** `crates/agent/src/agent/mod.rs`

Add match arms in `start_tool_execution()`:
```rust
BuiltInTool::WebFetch(t) => Box::pin(async move { t.execute().await }),
BuiltInTool::WebSearch(t) => {
    let model = Arc::clone(&self.model);
    Box::pin(async move { t.execute(&*model).await })
},
```

Also update:
- `validate_tool()` match
- `permissions.rs` - add permission handling for both tools

## Phase 6: Add invoke_mcp to Model Trait
**File:** `crates/agent/src/agent/agent_loop/model.rs`

Add `invoke_mcp` method to `Model` trait with default implementation:
```rust
fn invoke_mcp(
    &self,
    tool_name: &str,
    arguments: serde_json::Value,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send + '_>>;
```

## Phase 7: Implement invoke_mcp in chat-cli-v2
**Files:**
- `crates/chat-cli-v2/src/api_client/mod.rs` - add `invoke_mcp` to `ApiClient`
- `crates/chat-cli-v2/src/api_client/error.rs` - add `Other` error variant
- `crates/chat-cli-v2/src/agent/rts/mod.rs` - implement `invoke_mcp` in `RtsModel`
- `crates/chat-cli-v2/src/agent/acp/acp_agent.rs` - add tool kind/title for new tools

Added helper functions:
- `json_to_document()` - convert serde_json to aws_smithy_types::Document
- `document_to_json()` - convert aws_smithy_types::Document to serde_json

## Files Modified

### Agent Crate
1. `crates/agent/Cargo.toml` - add html2text dependency
2. `crates/agent/src/agent/tools/web_fetch.rs` - new file
3. `crates/agent/src/agent/tools/web_search.rs` - new file
4. `crates/agent/src/agent/tools/mod.rs` - register tools
5. `crates/agent/src/agent/mod.rs` - wire up execution, validation
6. `crates/agent/src/agent/permissions.rs` - add permission handling
7. `crates/agent/src/agent/agent_loop/model.rs` - add invoke_mcp to Model trait

### Chat-CLI-V2 Crate
8. `crates/chat-cli-v2/src/api_client/mod.rs` - add invoke_mcp, endpoint storage, helper functions
9. `crates/chat-cli-v2/src/api_client/error.rs` - add Other error variant
10. `crates/chat-cli-v2/src/agent/rts/mod.rs` - implement invoke_mcp in RtsModel
11. `crates/chat-cli-v2/src/agent/acp/acp_agent.rs` - add tool kind/title mappings

## Progress

- [x] Phase 1: Add Dependencies
- [x] Phase 2: Create web_fetch Tool
- [x] Phase 3: Create web_search Tool
- [x] Phase 4: Register Tools in mod.rs
- [x] Phase 5: Wire Up Tool Execution
- [x] Phase 6: Add invoke_mcp to Model Trait
- [x] Phase 7: Implement invoke_mcp in chat-cli-v2
