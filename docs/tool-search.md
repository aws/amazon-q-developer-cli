# Tool Search

Tool Search is a feature that reduces context window usage by deferring MCP tool specs until they are needed. Instead of sending all MCP tool schemas to the model on every turn, the agent provides a compact list of tool names and descriptions, and the model uses the `tool_search` built-in tool to load specific tools on demand.

## Enabling Tool Search

Tool Search is disabled by default. Enable it via the `kiro-cli settings` command from your terminal:

```
kiro-cli settings toolSearch.enabled true
```

Once enabled, Tool Search activates automatically when MCP tool specs are large enough to benefit from deferral. The default thresholds (5% of context window or 50k tokens) cover most cases. To force activation whenever any MCP tools are present, set a threshold to `0`:

```
kiro-cli settings toolSearch.minPct 0
kiro-cli settings toolSearch.minTokens 0
```

### Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `toolSearch.enabled` | `false` | Master toggle for Tool Search. |
| `toolSearch.minPct` | `5` | Activate when MCP tool specs exceed this % of the context window. |
| `toolSearch.minTokens` | `50000` | Activate when MCP tool specs exceed this token count. |

When both thresholds are set, Tool Search activates if **either** is exceeded (OR logic). When neither threshold is set, Tool Search is always active.

## How It Works

1. **Indexing**: When MCP servers connect, all tool specs are indexed into a BM25 keyword search engine. Each tool's name, server name, description, and parameter descriptions are tokenized for search.

2. **Deferred tool list**: Instead of full JSON schemas, the model receives a compact `<available-deferred-tools>` block listing each tool as `server_name::tool_name: description`. Descriptions are truncated to 1KB.

3. **On-demand loading**: When the model needs a tool, it calls `tool_search` with either:
   - `tool_id` — exact match (e.g., `builder-mcp::InternalSearch`)
   - `query` — keyword search (e.g., `"search documents"`)

4. **Activation**: Matched tools are activated and their full schemas are included in subsequent requests to the model.

## The `tool_search` Built-in Tool

**Tool name**: `tool_search`

Finds and loads MCP tools. Automatically allowed without user permission prompts (read-only).

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `tool_id` | string | One of `tool_id` or `query` | Exact tool identifier in `server_name::tool_name` format. |
| `query` | string | One of `tool_id` or `query` | Keywords to search for matching tools. |
| `max_results` | integer | No | Maximum results to return (default: 5). |

Provide exactly one of `tool_id` or `query`, not both.

Matched tools are immediately activated and available for invocation. The model should use the `tool_name` value (not the `server_name::tool_name` composite) when calling the tool.

### BM25 Matching

Keyword search uses BM25 scoring with parameters `k1=0.9`, `b=0.4`. Tool names are split on casing boundaries (e.g., `ReadFile` → `read file`, `read_file` → `read file`) to improve matching.

Only results above the matching threshold are returned. The default threshold is `1.5`, configurable via the `KIRO_CLI_TOOL_SEARCH_MATCHING_THRESHOLD` environment variable.

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `KIRO_CLI_TOOL_SEARCH_MATCHING_THRESHOLD` | `1.5` | Minimum BM25 score for a tool to be returned by keyword search. |
