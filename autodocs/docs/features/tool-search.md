---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: false
  category: feature
  title: Tool Search
  description: On-demand MCP tool discovery to reduce context window usage
  keywords: [tool, search, mcp, discovery, context, deferred, bm25]
  related: [mcp-registry, tool-search-tool]
---

# Tool Search

On-demand MCP tool discovery to reduce context window usage.

## Overview

Tool Search defers MCP tool schemas until they are needed. Instead of sending all MCP tool schemas to the model on every turn, the assistant receives a compact list of tool names and descriptions. When a tool is needed, the assistant uses the `tool_search` built-in tool to load specific tools on demand.

This is useful when you have many MCP servers configured with large tool schemas that would otherwise consume significant context window space.

## How It Works

1. **Indexing**: When MCP servers connect, all tool specs are indexed into a BM25 keyword search engine. Each tool's name, server name, description, and parameter descriptions are tokenized.

2. **Deferred tool list**: Instead of full JSON schemas, the model receives a compact list:
   ```
   server_name::tool_name: description (truncated to 1KB)
   ```

3. **On-demand loading**: When the assistant needs a tool, it calls `tool_search` with either:
   - `tool_id` — exact match (e.g., `builder-mcp::InternalSearch`)
   - `query` — keyword search (e.g., `"search documents"`)

4. **Activation**: Matched tools are activated and their full schemas are included in subsequent requests.

## Configuration

### Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `toolSearch.enabled` | `false` | Master toggle for Tool Search |
| `toolSearch.minPct` | `5` | Activate when MCP tool specs exceed this % of context window |
| `toolSearch.minTokens` | `50000` | Activate when MCP tool specs exceed this token count |

When both thresholds are set, Tool Search activates if **either** is exceeded (OR logic).

### Enable Tool Search

```bash
kiro-cli settings toolSearch.enabled true
```

### Force Always Active

To activate whenever any MCP tools are present:

```bash
kiro-cli settings toolSearch.enabled true
kiro-cli settings toolSearch.minPct 0
kiro-cli settings toolSearch.minTokens 0
```

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `KIRO_CLI_TOOL_SEARCH_MATCHING_THRESHOLD` | `1.5` | Minimum BM25 score for keyword search results |

## Examples

### Example 1: Basic Setup

Enable Tool Search with default thresholds:

```bash
kiro-cli settings toolSearch.enabled true
```

Tool Search activates automatically when MCP tool specs exceed 5% of context window or 50k tokens.

### Example 2: Lower Threshold

Activate Tool Search with smaller MCP tool sets:

```bash
kiro-cli settings toolSearch.enabled true
kiro-cli settings toolSearch.minPct 2
kiro-cli settings toolSearch.minTokens 10000
```

### Example 3: Adjust Matching Threshold

Lower the BM25 score threshold for more permissive keyword matching:

```bash
export KIRO_CLI_TOOL_SEARCH_MATCHING_THRESHOLD=1.0
kiro-cli chat
```

## Troubleshooting

### Tool Search Not Activating

**Symptom**: MCP tools still appear in full, tool_search not available.

**Cause**: MCP tool specs don't exceed configured thresholds.

**Solution**: Lower thresholds or set both to 0 to force activation:
```bash
kiro-cli settings toolSearch.minPct 0
kiro-cli settings toolSearch.minTokens 0
```

### Assistant Can't Find Tools

**Symptom**: tool_search returns empty results for keyword queries.

**Cause**: BM25 score below matching threshold.

**Solution**: Try different keywords, use exact tool_id, or lower the matching threshold via environment variable.

### Tool Invocation Fails After Loading

**Symptom**: Tool loaded via tool_search but invocation fails.

**Cause**: Using composite name instead of tool_name.

**Solution**: Use only the `tool_name` value from tool_search results (e.g., `InternalSearch`), not the composite `server_name::tool_name`.

## Technical Details

- BM25 parameters: k1=0.9, b=0.4
- Tool names tokenized on casing boundaries (ReadFile → read file, read_file → read file)
- Descriptions truncated to 1KB in deferred list
- Token estimation: (name + description + schema JSON) / 4 bytes per token
- tool_search is auto-allowed (no permission prompt)

## Limitations

- Only applies to MCP tools; built-in tools are always available
- Keyword search may miss tools with unusual naming
- Activated tools persist for the session (no way to deactivate)

## Related

- [tool_search](../tools/tool-search.md) - The built-in tool for loading MCP tools
- [MCP Registry](./mcp-registry.md) - MCP server configuration
- [toolSearch.enabled](../settings/tool-search-enabled.md) - Master toggle setting
- [toolSearch.minPct](../settings/tool-search-min-pct.md) - Percentage threshold setting
- [toolSearch.minTokens](../settings/tool-search-min-tokens.md) - Token threshold setting
