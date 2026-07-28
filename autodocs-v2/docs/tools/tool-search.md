---
doc_meta:
  validated: 2026-07-27
  commit: 482981ad0
  status: validated
  testable_headless: true
  category: tool
  title: tool_search
  description: Find and load MCP tools on demand to reduce context window usage
  keywords: [tool, search, mcp, discovery, load, context, bm25]
  related: [mcp, context]
---

# tool_search

Find and load MCP tools on demand to reduce context window usage.

## Overview

> **Note**: This tool is used by the AI assistant to fulfill your requests. You don't invoke it directly - simply ask questions naturally, and the assistant will use this tool to discover and load MCP tools as needed.

The tool_search tool enables on-demand MCP tool discovery. When Tool Search is enabled, MCP tool schemas are deferred until needed. The assistant sees a compact list of tool names and descriptions, then uses tool_search to load specific tools when required. This reduces context window usage when many MCP tools are configured.

## Usage

> **Technical Reference**: The JSON examples below show the internal tool format used by the AI assistant. Users should not copy or type these - they are provided for developers and agent configuration authors only.

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

#### Use Case 2: Search for Tools by Keyword

```json
{
  "query": "search documents"
}
```

**What this does**: Searches for tools matching "search documents" using BM25 keyword matching. All matching tools above the score threshold are loaded.

#### Use Case 3: Search with Result Limit

```json
{
  "query": "database",
  "max_results": 3
}
```

**What this does**: Searches for database-related tools, returning at most 3 matches.

## Configuration

Tool Search is enabled by default. It activates automatically when MCP tool specs exceed configured thresholds (3% of context window or 10,000 tokens, whichever comes first).

To disable Tool Search:

```bash
kiro-cli settings toolSearch.enabled false
```

To adjust activation thresholds:

```bash
kiro-cli settings toolSearch.minPct 5      # Activate at 5% of context window
kiro-cli settings toolSearch.minTokens 50000  # Activate at 50k tokens
```

To force activation whenever any MCP tools are present:

```bash
kiro-cli settings toolSearch.minPct 0
kiro-cli settings toolSearch.minTokens 0
```

## Parameters

### tool_id

Exact tool identifier to load.

**Type**: string  
**Required**: One of `tool_id` or `query` must be provided  
**Format**: `server_name::tool_name`

The tool_id must match an entry from the `<available-deferred-tools>` list provided in the context. The tool is loaded and immediately available for invocation.

### query

Keywords to search for matching tools.

**Type**: string  
**Required**: One of `tool_id` or `query` must be provided

Uses BM25 keyword matching against tool names, descriptions, and parameter descriptions. All matching tools above the score threshold are loaded.

### max_results

Maximum number of results to return.

**Type**: integer  
**Required**: No  
**Default**: 5

Limits the number of tools returned by keyword search.

## Examples

### Example 1: Load Specific Tool

```json
{
  "tool_id": "testdb::database_query"
}
```

**Response**:
```json
{
  "tools": [
    {
      "tool_name": "database_query",
      "server_name": "testdb",
      "description": "Execute SQL database queries",
      "score": 3.4028235e38
    }
  ]
}
```

### Example 2: Keyword Search

```json
{
  "query": "file read"
}
```

**Response**:
```json
{
  "tools": [
    {
      "tool_name": "read_file",
      "server_name": "filesystem",
      "description": "Read contents of a file from disk",
      "score": 2.45
    }
  ]
}
```

### Example 3: No Matches Found

```json
{
  "query": "xyzzy gibberish"
}
```

**Response**:
```json
{
  "tools": []
}
```

## Troubleshooting

### Issue: "Tool not found"

**Symptom**: Error message "Tool 'X' not found"  
**Cause**: The tool_id doesn't match any indexed tool  
**Solution**: Check the `<available-deferred-tools>` list for valid `server_name::tool_name` entries.

### Issue: "Provide either tool_id or query"

**Symptom**: Error when calling tool_search  
**Cause**: Neither parameter was provided  
**Solution**: Provide exactly one of `tool_id` or `query`.

### Issue: "Provide either tool_id or query, not both"

**Symptom**: Error when calling tool_search  
**Cause**: Both parameters were provided  
**Solution**: Use only one parameter per call.

### Issue: Empty Search Results

**Symptom**: Query returns no tools  
**Cause**: No tools match the query above the score threshold  
**Solution**: Try different search terms. The default BM25 threshold is 1.5.

### Issue: Tool Search Not Available

**Symptom**: tool_search not in available tools  
**Cause**: Tool Search is disabled or MCP tool specs are below activation thresholds  
**Solution**: Verify it's enabled with `kiro-cli settings toolSearch.enabled` (default: true). If enabled but not activating, lower the thresholds or add more MCP tools.

## Related Features

- [/context](../slash-commands/context.md) - View context usage including tool token breakdown
- [MCP](../commands/mcp.md) - MCP server management
- [/tools](../slash-commands/tools.md) - View available tools

## How It Works

1. **Indexing**: When MCP servers connect, tool specs are indexed into a BM25 search engine. Tool names, server names, descriptions, and parameter descriptions are tokenized.

2. **Deferred List**: Instead of full JSON schemas, the model receives a compact `<available-deferred-tools>` block listing each tool as `server_name::tool_name: description`.

3. **On-Demand Loading**: When the model needs a tool, it calls tool_search with either an exact tool_id or a keyword query.

4. **Activation**: Matched tools are activated and their full schemas are included in subsequent requests.

## Technical Details

**Aliases**: `tool_search`

**Permissions**: Automatically allowed without user permission prompts (read-only operation).

**BM25 Parameters**: `k1=0.9`, `b=0.4`

**Matching Threshold**: Default 1.5, configurable via `KIRO_CLI_TOOL_SEARCH_MATCHING_THRESHOLD` environment variable.

**Tool Name Tokenization**: Names are split on casing boundaries (e.g., `ReadFile` → `read file`) to improve matching.
