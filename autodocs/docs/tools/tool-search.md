---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: false
  category: tool
  title: tool_search
  description: Find and load MCP tools on demand to reduce context window usage
  keywords: [tool, search, mcp, discovery, load, bm25, deferred]
  related: [mcp-registry, tool-search-feature]
---

# tool_search

Find and load MCP tools on demand to reduce context window usage.

## Overview

> **Note**: This tool is used by the AI assistant to fulfill your requests. You don't invoke it directly - simply ask questions naturally, and the assistant will use this tool to discover and load MCP tools as needed.

The tool_search tool enables on-demand MCP tool discovery. When Tool Search is active, MCP tool schemas are deferred until needed. The assistant sees a compact list of available tools and uses tool_search to load specific tools before invoking them. This reduces context window consumption when many MCP tools are configured.

## How It Works

1. MCP tool specs are indexed into a BM25 keyword search engine
2. The assistant receives a compact list of `server_name::tool_name: description` entries
3. When a tool is needed, the assistant calls tool_search with either an exact tool_id or a keyword query
4. Matched tools are activated and their full schemas become available for subsequent turns

## Usage

> **Technical Reference**: The JSON examples below show the internal tool format used by the AI assistant. Users should not copy or type these - they are provided for developers and agent configuration authors only.

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `tool_id` | string | One of tool_id or query | Exact tool identifier in `server_name::tool_name` format |
| `query` | string | One of tool_id or query | Keywords to search for matching tools |
| `max_results` | integer | No | Maximum results to return (default: 5) |

Provide exactly one of `tool_id` or `query`, not both.

### Basic Usage

```json
{
  "tool_id": "builder-mcp::InternalSearch"
}
```

### Common Use Cases

#### Use Case 1: Load Tool by Exact ID

```json
{
  "tool_id": "filesystem::read_file"
}
```

**What this does**: Loads the read_file tool from the filesystem MCP server. The tool becomes immediately available for invocation.

#### Use Case 2: Search by Keywords

```json
{
  "query": "search documents"
}
```

**What this does**: Searches for tools matching "search documents" using BM25 keyword matching. Returns up to 5 matching tools above the score threshold.

#### Use Case 3: Search with Custom Limit

```json
{
  "query": "database",
  "max_results": 10
}
```

**What this does**: Searches for database-related tools, returning up to 10 matches.

## Output Format

Returns JSON with matched tools:

```json
{
  "tools": [
    {
      "tool_name": "InternalSearch",
      "server_name": "builder-mcp",
      "description": "Search internal documents...",
      "score": 3.45
    }
  ]
}
```

The `tool_name` value (not the composite `server_name::tool_name`) should be used when invoking the tool.

## Permissions

tool_search is automatically allowed without user permission prompts since it's a read-only discovery operation.

## Troubleshooting

### Tool Not Found

**Symptom**: Error "Tool 'X' not found"

**Cause**: The tool_id doesn't match any indexed tool.

**Solution**: Check the available-deferred-tools list in the context for valid `server_name::tool_name` entries.

### No Results from Query

**Symptom**: Empty tools array returned.

**Cause**: No tools matched above the BM25 score threshold.

**Solution**: Try different keywords or use exact tool_id if you know the tool name.

### Invalid tool_id Format

**Symptom**: Error about server_name::tool_name format.

**Cause**: tool_id must use the composite format with `::` separator.

**Solution**: Use `server_name::tool_name` format (e.g., `myserver::mytool`).

## Technical Details

- BM25 parameters: k1=0.9, b=0.4
- Tool names are tokenized on casing boundaries (ReadFile → read file)
- Default matching threshold: 1.5 (configurable via `KIRO_CLI_TOOL_SEARCH_MATCHING_THRESHOLD`)
- Descriptions truncated to 1KB in the deferred tools list

## Related

- [Tool Search Feature](../features/tool-search.md) - Complete feature documentation
- [MCP Registry](../features/mcp-registry.md) - MCP server configuration
